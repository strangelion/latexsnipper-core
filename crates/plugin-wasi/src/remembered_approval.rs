//! Authenticated host-owned policy snapshots. Metadata is not execution authority.
//! Protected key/checkpoint storage and atomic publication belong to the host.
use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::approval::{payload_bytes, same_target};
use crate::{ApprovalRequest, ApprovalScope, SensitiveAction, WasiDiagnostic, WasiDiagnosticCode};

const MAGIC: &[u8; 5] = b"LSAP\x01";
const HEADER_BYTES: usize = 9;
const TAG_BYTES: usize = 32;
const MAX_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;
const MAX_POLICIES: usize = 128;
const MAX_ACTION_BYTES: usize = 8192;
const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
const MAX_POLICY_SECONDS: u64 = 30 * 24 * 3600;
const MAC_DOMAIN: &[u8] = b"latexsnipper-remembered-approvals-v1\0";
type Clock = Arc<dyn Fn() -> Result<u64, WasiDiagnostic> + Send + Sync>;

/// Dedicated 256-bit host secret. Never serialize or log it. Export only to a
/// protected host key store; zeroization here cannot wipe caller-owned copies.
pub struct ApprovalPolicyKey(Zeroizing<[u8; 32]>);

impl ApprovalPolicyKey {
    pub fn generate() -> Result<Self, WasiDiagnostic> {
        let mut bytes = Zeroizing::new([0; 32]);
        getrandom::fill(bytes.as_mut())
            .map_err(|_| failed("cannot generate remembered policy key"))?;
        Ok(Self(bytes))
    }

    /// The host is responsible for supplying independently generated secret bytes.
    pub fn from_secret_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub fn secret_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn mac(&self) -> Result<Hmac<Sha256>, WasiDiagnostic> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.0.as_slice())
            .map_err(|_| failed("cannot initialize remembered policy authentication"))?;
        mac.update(MAC_DOMAIN);
        Ok(mac)
    }
}

impl fmt::Debug for ApprovalPolicyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApprovalPolicyKey([REDACTED])")
    }
}

/// The host must obtain this from separately protected latest-state storage,
/// not from the untrusted snapshot itself. HMAC alone does not prevent rollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalPolicyCheckpoint {
    pub store_id: String,
    pub revision: u64,
    pub snapshot_sha256: String,
    pub observed_unix_secs: u64,
}

/// Snapshot metadata for display/selection only. Not a capability or use permit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RememberedApprovalPolicy {
    pub id: String,
    pub binding_sha256: String,
    pub scope: ApprovalScope,
    pub action: SensitiveAction,
    pub approved_at_unix_secs: u64,
    pub expires_at_unix_secs: u64,
}

pub struct SealedApprovalPolicies {
    /// Authenticated, not encrypted: paths and route metadata can be sensitive.
    pub bytes: Vec<u8>,
    pub checkpoint: ApprovalPolicyCheckpoint,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    schema_version: u16,
    store_id: String,
    revision: u64,
    observed_unix_secs: u64,
    policies: Vec<RememberedApprovalPolicy>,
}

struct State {
    store_id: String,
    revision: u64,
    observed_unix_secs: u64,
    policies: BTreeMap<String, RememberedApprovalPolicy>,
}

/// Bounded, explicit host policies. No disk IO, guest consent interface, automatic
/// grant restoration, policy renewal or OS secret-store integration is provided.
pub struct RememberedApprovalPolicies {
    key: ApprovalPolicyKey,
    state: Mutex<State>,
    clock: Clock,
}

impl RememberedApprovalPolicies {
    pub fn new(key: ApprovalPolicyKey) -> Result<Self, WasiDiagnostic> {
        Self::new_with_clock(key, Arc::new(system_time))
    }

    fn new_with_clock(key: ApprovalPolicyKey, clock: Clock) -> Result<Self, WasiDiagnostic> {
        let mut identity = [0; 16];
        getrandom::fill(&mut identity)
            .map_err(|_| failed("cannot generate remembered policy store identity"))?;
        Ok(Self {
            key,
            state: Mutex::new(State {
                store_id: hex::encode(identity),
                revision: 0,
                observed_unix_secs: clock()?,
                policies: BTreeMap::new(),
            }),
            clock,
        })
    }

    pub fn restore(
        key: ApprovalPolicyKey,
        bytes: &[u8],
        trusted_checkpoint: &ApprovalPolicyCheckpoint,
    ) -> Result<Self, WasiDiagnostic> {
        Self::restore_with_clock(key, bytes, trusted_checkpoint, Arc::new(system_time))
    }

    fn restore_with_clock(
        key: ApprovalPolicyKey,
        bytes: &[u8],
        checkpoint: &ApprovalPolicyCheckpoint,
        clock: Clock,
    ) -> Result<Self, WasiDiagnostic> {
        if bytes.len() < HEADER_BYTES + TAG_BYTES
            || bytes.len() > MAX_SNAPSHOT_BYTES
            || &bytes[..MAGIC.len()] != MAGIC
        {
            return Err(invalid("invalid remembered policy snapshot framing"));
        }
        let payload_length = u32::from_be_bytes(bytes[5..9].try_into().unwrap()) as usize;
        if payload_length != bytes.len() - HEADER_BYTES - TAG_BYTES {
            return Err(invalid("invalid remembered policy snapshot length"));
        }
        let tag_offset = bytes.len() - TAG_BYTES;
        let mut mac = key.mac()?;
        mac.update(&bytes[..tag_offset]);
        mac.verify_slice(&bytes[tag_offset..])
            .map_err(|_| invalid("remembered policy snapshot authentication failed"))?;
        // Authenticate bounded bytes before parsing attacker-controlled JSON.
        let snapshot: Snapshot = serde_json::from_slice(&bytes[HEADER_BYTES..tag_offset])
            .map_err(|_| invalid("invalid remembered policy snapshot declaration"))?;
        if snapshot.schema_version != 1
            || !hex_digest(&snapshot.store_id, 32)
            || snapshot.store_id != checkpoint.store_id
            || snapshot.revision != checkpoint.revision
            || !hex_digest(&checkpoint.snapshot_sha256, 64)
            || hex::encode(Sha256::digest(bytes)) != checkpoint.snapshot_sha256
            || snapshot.observed_unix_secs > checkpoint.observed_unix_secs
            || snapshot.policies.len() > MAX_POLICIES
        {
            return Err(invalid(
                "remembered policy snapshot does not match latest checkpoint",
            ));
        }
        let mut policies = BTreeMap::new();
        for policy in snapshot.policies {
            validate_policy(&policy, snapshot.observed_unix_secs)?;
            if policies.insert(policy.id.clone(), policy).is_some() {
                return Err(invalid("duplicate remembered policy identity"));
            }
        }
        let now = clock()?;
        if now < checkpoint.observed_unix_secs {
            return Err(failed("remembered policy clock moved backwards"));
        }
        Ok(Self {
            key,
            state: Mutex::new(State {
                store_id: snapshot.store_id,
                revision: snapshot.revision,
                observed_unix_secs: now,
                policies,
            }),
            clock,
        })
    }

    pub fn seal(&self) -> Result<SealedApprovalPolicies, WasiDiagnostic> {
        let mut state = self.lock()?;
        // Snapshot publication is not authorization. Preserve the high-water
        // mark so revocations can still be persisted with a broken clock.
        if let Ok(now) = (self.clock)() {
            state.observed_unix_secs = state.observed_unix_secs.max(now);
        }
        let payload = serde_json::to_vec(&Snapshot {
            schema_version: 1,
            store_id: state.store_id.clone(),
            revision: state.revision,
            observed_unix_secs: state.observed_unix_secs,
            policies: state.policies.values().cloned().collect(),
        })
        .map_err(|_| failed("cannot serialize remembered policies"))?;
        if payload.len() > MAX_SNAPSHOT_BYTES - HEADER_BYTES - TAG_BYTES {
            return Err(invalid("remembered policy snapshot exceeds budget"));
        }
        let mut bytes = Vec::with_capacity(HEADER_BYTES + payload.len() + TAG_BYTES);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&payload);
        let mut mac = self.key.mac()?;
        mac.update(&bytes);
        bytes.extend_from_slice(&mac.finalize().into_bytes());
        let checkpoint = ApprovalPolicyCheckpoint {
            store_id: state.store_id.clone(),
            revision: state.revision,
            snapshot_sha256: hex::encode(Sha256::digest(&bytes)),
            observed_unix_secs: state.observed_unix_secs,
        };
        Ok(SealedApprovalPolicies { bytes, checkpoint })
    }

    /// Expired entries remain visible for revocation but never match an action.
    pub fn policies(&self) -> Result<Vec<RememberedApprovalPolicy>, WasiDiagnostic> {
        let state = self.lock()?;
        Ok(state.policies.values().cloned().collect())
    }

    /// A matching description is not authority. The integrating host still needs
    /// a bounded current execution grant; no pending work is run by this method.
    pub fn matching_policy(
        &self,
        request: &ApprovalRequest,
    ) -> Result<Option<RememberedApprovalPolicy>, WasiDiagnostic> {
        let mut state = self.lock()?;
        let now = self.observe(&mut state)?;
        Ok(state
            .policies
            .values()
            .find(|policy| {
                request.remembered_binding_sha256.as_deref() == Some(policy.binding_sha256.as_str())
                    && now < policy.expires_at_unix_secs
                    && match policy.scope {
                        ApprovalScope::Exact => policy.action == request.action,
                        ApprovalScope::SameTarget { max_payload_bytes } => {
                            same_target(&policy.action, &request.action)
                                && payload_bytes(&request.action) <= max_payload_bytes
                        }
                    }
            })
            .cloned())
    }

    pub fn revoke(&self, policy_id: &str) -> Result<bool, WasiDiagnostic> {
        let mut state = self.lock()?;
        // Reducing consent must remain possible even if the wall clock is wrong.
        if !state.policies.contains_key(policy_id) {
            return Ok(false);
        }
        let revision = next_revision(&state)?;
        state.policies.remove(policy_id);
        state.revision = revision;
        Ok(true)
    }

    pub fn clear(&self) -> Result<(), WasiDiagnostic> {
        let mut state = self.lock()?;
        if !state.policies.is_empty() {
            let revision = next_revision(&state)?;
            state.policies.clear();
            state.revision = revision;
        }
        Ok(())
    }

    pub(crate) fn remember_request(
        &self,
        request: &ApprovalRequest,
        scope: ApprovalScope,
        valid_for: Duration,
    ) -> Result<String, WasiDiagnostic> {
        let seconds = valid_for.as_secs();
        if seconds == 0 || seconds > MAX_POLICY_SECONDS || valid_for.subsec_nanos() != 0 {
            return Err(invalid(
                "remembered consent requires whole seconds within 30 days",
            ));
        }
        let binding = request
            .remembered_binding_sha256
            .as_ref()
            .ok_or_else(|| invalid("remembered consent requires a trusted stable host binding"))?;
        let mut state = self.lock()?;
        let now = self.observe(&mut state)?;
        let policy = RememberedApprovalPolicy {
            id: policy_id(binding, &scope, &request.action)?,
            binding_sha256: binding.clone(),
            scope,
            action: request.action.clone(),
            approved_at_unix_secs: now,
            expires_at_unix_secs: now
                .checked_add(seconds)
                .ok_or_else(|| invalid("remembered policy deadline overflow"))?,
        };
        validate_policy(&policy, now)?;
        if state
            .policies
            .values()
            .filter(|entry| entry.id != policy.id && now < entry.expires_at_unix_secs)
            .count()
            >= MAX_POLICIES
        {
            return Err(invalid("remembered policy capacity reached"));
        }
        let revision = next_revision(&state)?;
        state
            .policies
            .retain(|_, entry| now < entry.expires_at_unix_secs);
        let id = policy.id.clone();
        state.policies.insert(id.clone(), policy);
        state.revision = revision;
        Ok(id)
    }

    fn observe(&self, state: &mut State) -> Result<u64, WasiDiagnostic> {
        let now = (self.clock)()?;
        if now < state.observed_unix_secs {
            return Err(failed("remembered policy clock moved backwards"));
        }
        state.observed_unix_secs = now;
        Ok(now)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, WasiDiagnostic> {
        self.state
            .lock()
            .map_err(|_| failed("remembered policy lock poisoned"))
    }
}

fn validate_policy(policy: &RememberedApprovalPolicy, observed: u64) -> Result<(), WasiDiagnostic> {
    if !hex_digest(&policy.binding_sha256, 64)
        || policy.approved_at_unix_secs > observed
        || policy.expires_at_unix_secs <= policy.approved_at_unix_secs
        || policy.expires_at_unix_secs - policy.approved_at_unix_secs > MAX_POLICY_SECONDS
        || payload_bytes(&policy.action) > MAX_PAYLOAD_BYTES
        || serde_json::to_vec(&policy.action)
            .map_err(|_| invalid("invalid policy action"))?
            .len()
            > MAX_ACTION_BYTES
        || policy.id != policy_id(&policy.binding_sha256, &policy.scope, &policy.action)?
    {
        return Err(invalid("invalid remembered policy scope or identity"));
    }
    if let ApprovalScope::SameTarget { max_payload_bytes } = policy.scope {
        if max_payload_bytes < payload_bytes(&policy.action)
            || max_payload_bytes > MAX_PAYLOAD_BYTES
        {
            return Err(invalid("invalid remembered policy payload ceiling"));
        }
    }
    let (digest, preimage) = match &policy.action {
        SensitiveAction::FileWrite {
            payload_sha256,
            target_sha256,
            ..
        } => (payload_sha256, target_sha256.as_deref()),
        SensitiveAction::NetworkSend { payload_sha256, .. } => (payload_sha256, None),
    };
    if !hex_digest(digest, 64) || preimage.is_some_and(|digest| !hex_digest(digest, 64)) {
        return Err(invalid("invalid remembered policy content identity"));
    }
    Ok(())
}

fn policy_id(
    binding: &str,
    scope: &ApprovalScope,
    action: &SensitiveAction,
) -> Result<String, WasiDiagnostic> {
    let mut identity = action.clone();
    let kind = match scope {
        ApprovalScope::Exact => "exact",
        ApprovalScope::SameTarget { .. } => {
            match &mut identity {
                SensitiveAction::FileWrite {
                    target_sha256,
                    payload_sha256,
                    payload_bytes,
                    ..
                } => {
                    *target_sha256 = None;
                    *payload_sha256 = String::new();
                    *payload_bytes = 0;
                }
                SensitiveAction::NetworkSend {
                    payload_sha256,
                    payload_bytes,
                    ..
                } => {
                    *payload_sha256 = String::new();
                    *payload_bytes = 0;
                }
            }
            "sameTarget"
        }
    };
    let bytes = serde_json::to_vec(&("core-wasi-remembered-policy-v1", binding, kind, identity))
        .map_err(|_| invalid("cannot compute remembered policy identity"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn hex_digest(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn next_revision(state: &State) -> Result<u64, WasiDiagnostic> {
    state
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("remembered policy revision overflow"))
}
fn system_time() -> Result<u64, WasiDiagnostic> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .map_err(|_| failed("remembered policy wall clock unavailable"))
}
fn invalid(message: &str) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiResourcePolicy, message)
}
fn failed(message: &str) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiHostFailure, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Public synthetic fixture key, never a production credential.
    fn key() -> ApprovalPolicyKey {
        ApprovalPolicyKey::from_secret_bytes([42; 32])
    }
    fn fixture() -> (RememberedApprovalPolicies, Arc<AtomicU64>, Clock) {
        let time = Arc::new(AtomicU64::new(1000));
        let observed = Arc::clone(&time);
        let clock: Clock = Arc::new(move || Ok(observed.load(Ordering::SeqCst)));
        let policies = RememberedApprovalPolicies::new_with_clock(key(), clock.clone()).unwrap();
        (policies, time, clock)
    }
    fn request() -> ApprovalRequest {
        ApprovalRequest {
            schema_version: 1,
            id: "a".repeat(64),
            plugin_id: "fixture.component".into(),
            plugin_version: "1.0.0".into(),
            component_sha256: "b".repeat(64),
            host_context_sha256: "c".repeat(64),
            remembered_binding_sha256: Some("d".repeat(64)),
            action: SensitiveAction::FileWrite {
                grant_id: "path-0".into(),
                root: "fixture-root".into(),
                relative_path: "output.txt".into(),
                target_sha256: None,
                payload_sha256: "e".repeat(64),
                payload_bytes: 1,
            },
        }
    }
    fn remember(store: &RememberedApprovalPolicies) -> String {
        store
            .remember_request(
                &request(),
                ApprovalScope::SameTarget {
                    max_payload_bytes: 4,
                },
                Duration::from_secs(100),
            )
            .unwrap()
    }
    fn restore(
        sealed: &SealedApprovalPolicies,
        clock: Clock,
    ) -> Result<RememberedApprovalPolicies, WasiDiagnostic> {
        RememberedApprovalPolicies::restore_with_clock(
            key(),
            &sealed.bytes,
            &sealed.checkpoint,
            clock,
        )
    }

    #[test]
    fn mac_domain_matches_an_independent_standard_library_fixture() {
        // Reference computed with Python's standard-library hmac/sha256 using
        // the public synthetic key, domain separator and exact fixture bytes.
        let mut mac = key().mac().unwrap();
        mac.update(b"fixture snapshot");
        assert_eq!(
            hex::encode(mac.finalize().into_bytes()),
            "6928f5cf6b3007e3cc31bcb0319630eb6cad6ab09033155f3c2226bd30cef2f4"
        );
    }

    #[test]
    fn roundtrip_is_authenticated_and_does_not_bind_session_nonce() {
        let (store, _, clock) = fixture();
        let id = remember(&store);
        let sealed = store.seal().unwrap();
        let restored = restore(&sealed, clock).unwrap();
        let mut fresh = request();
        fresh.id = "f".repeat(64);
        fresh.host_context_sha256 = "0".repeat(64);
        assert_eq!(restored.matching_policy(&fresh).unwrap().unwrap().id, id);
        assert_eq!(restored.policies().unwrap(), store.policies().unwrap());
        assert_eq!(restored.seal().unwrap().bytes, sealed.bytes);
        assert_eq!(format!("{:?}", key()), "ApprovalPolicyKey([REDACTED])");
        let generated = ApprovalPolicyKey::generate().unwrap();
        assert_eq!(generated.secret_bytes().len(), 32);
    }

    #[test]
    fn altered_bytes_wrong_key_truncation_and_extra_bytes_are_rejected() {
        let (store, _, clock) = fixture();
        remember(&store);
        let sealed = store.seal().unwrap();
        for index in [0, 4, 5, HEADER_BYTES, sealed.bytes.len() - 1] {
            let mut bytes = sealed.bytes.clone();
            bytes[index] ^= 1;
            assert!(RememberedApprovalPolicies::restore_with_clock(
                key(),
                &bytes,
                &sealed.checkpoint,
                clock.clone()
            )
            .is_err());
        }
        assert!(RememberedApprovalPolicies::restore_with_clock(
            ApprovalPolicyKey::from_secret_bytes([43; 32]),
            &sealed.bytes,
            &sealed.checkpoint,
            clock.clone()
        )
        .is_err());
        for length in [0, 4, HEADER_BYTES, sealed.bytes.len() - 1] {
            assert!(RememberedApprovalPolicies::restore_with_clock(
                key(),
                &sealed.bytes[..length],
                &sealed.checkpoint,
                clock.clone()
            )
            .is_err());
        }
        let mut extra = sealed.bytes.clone();
        extra.push(0);
        assert!(RememberedApprovalPolicies::restore_with_clock(
            key(),
            &extra,
            &sealed.checkpoint,
            clock
        )
        .is_err());
    }

    #[test]
    fn revoked_policy_old_valid_snapshot_and_clock_rollback_fail_closed() {
        let (store, time, clock) = fixture();
        let id = remember(&store);
        let old = store.seal().unwrap();
        time.store(1010, Ordering::SeqCst);
        assert!(store.revoke(&id).unwrap());
        assert!(!store.revoke(&id).unwrap());
        let latest = store.seal().unwrap();
        assert!(RememberedApprovalPolicies::restore_with_clock(
            key(),
            &old.bytes,
            &latest.checkpoint,
            clock.clone()
        )
        .is_err());
        assert!(RememberedApprovalPolicies::restore_with_clock(
            key(),
            &latest.bytes,
            &old.checkpoint,
            clock.clone()
        )
        .is_err());
        let restored = restore(&latest, clock.clone()).unwrap();
        assert!(restored.matching_policy(&request()).unwrap().is_none());
        time.store(1009, Ordering::SeqCst);
        assert!(restored.matching_policy(&request()).is_err());
        assert_eq!(restored.seal().unwrap().checkpoint.observed_unix_secs, 1010);
        assert!(restore(&latest, clock).is_err());
        // Revocation itself must not be blocked by a clock rollback.
        assert!(store.clear().is_ok());
    }

    #[test]
    fn separate_store_checkpoint_cannot_substitute_for_latest_state() {
        let (store, _, clock) = fixture();
        remember(&store);
        let sealed = store.seal().unwrap();
        let (other, _, _) = fixture();
        remember(&other);
        let unrelated = other.seal().unwrap();
        assert!(RememberedApprovalPolicies::restore_with_clock(
            key(),
            &sealed.bytes,
            &unrelated.checkpoint,
            clock.clone()
        )
        .is_err());
        let mut changed = sealed.checkpoint.clone();
        changed.snapshot_sha256 = "0".repeat(64);
        assert!(RememberedApprovalPolicies::restore_with_clock(
            key(),
            &sealed.bytes,
            &changed,
            clock
        )
        .is_err());
    }

    #[test]
    fn exact_and_same_target_keep_target_binding_payload_and_expiry_boundaries() {
        let (store, time, _) = fixture();
        let original = request();
        let id = store
            .remember_request(&original, ApprovalScope::Exact, Duration::from_secs(100))
            .unwrap();
        let mut changed = original.clone();
        if let SensitiveAction::FileWrite {
            payload_sha256,
            payload_bytes,
            target_sha256,
            ..
        } = &mut changed.action
        {
            *payload_sha256 = "f".repeat(64);
            *payload_bytes = 2;
            *target_sha256 = Some("a".repeat(64));
        }
        assert!(store.matching_policy(&changed).unwrap().is_none());
        store.revoke(&id).unwrap();
        let id = remember(&store);
        assert_eq!(store.matching_policy(&changed).unwrap().unwrap().id, id);
        for alter in 0..5 {
            let mut outside = changed.clone();
            if let SensitiveAction::FileWrite {
                root,
                relative_path,
                grant_id,
                payload_bytes,
                ..
            } = &mut outside.action
            {
                match alter {
                    0 => outside.remembered_binding_sha256 = Some("f".repeat(64)),
                    1 => *root = "other-root".into(),
                    2 => *relative_path = "different.txt".into(),
                    3 => *grant_id = "path-1".into(),
                    _ => *payload_bytes = 5,
                }
            }
            assert!(store.matching_policy(&outside).unwrap().is_none());
        }
        time.store(1100, Ordering::SeqCst);
        assert!(store.matching_policy(&changed).unwrap().is_none());
        assert_eq!(store.policies().unwrap().len(), 1);
    }

    #[test]
    fn same_target_changes_replace_policy_without_duplicate_or_implicit_widening() {
        let (store, _, _) = fixture();
        let id = remember(&store);
        let mut changed = request();
        if let SensitiveAction::FileWrite {
            payload_bytes,
            payload_sha256,
            ..
        } = &mut changed.action
        {
            *payload_bytes = 2;
            *payload_sha256 = "f".repeat(64);
        }
        let replacement = store
            .remember_request(
                &changed,
                ApprovalScope::SameTarget {
                    max_payload_bytes: 2,
                },
                Duration::from_secs(100),
            )
            .unwrap();
        assert_eq!(replacement, id);
        assert_eq!(store.policies().unwrap().len(), 1);
        if let SensitiveAction::FileWrite { payload_bytes, .. } = &mut changed.action {
            *payload_bytes = 3;
        }
        assert!(store.matching_policy(&changed).unwrap().is_none());
        assert!(store.revoke(&id).unwrap());
        assert!(store.matching_policy(&request()).unwrap().is_none());
    }

    #[test]
    fn network_method_query_port_scheme_and_body_ceiling_remain_exact_targets() {
        let (store, _, _) = fixture();
        let mut original = request();
        original.action = SensitiveAction::NetworkSend {
            scheme: "https".into(),
            host: "fixture.example.invalid".into(),
            port: 443,
            method: "POST".into(),
            path_and_query: "/submit?scope=one".into(),
            payload_sha256: "e".repeat(64),
            payload_bytes: 1,
        };
        store
            .remember_request(
                &original,
                ApprovalScope::SameTarget {
                    max_payload_bytes: 4,
                },
                Duration::from_secs(100),
            )
            .unwrap();
        for alter in 0..6 {
            let mut outside = original.clone();
            if let SensitiveAction::NetworkSend {
                scheme,
                host,
                port,
                method,
                path_and_query,
                payload_bytes,
                ..
            } = &mut outside.action
            {
                match alter {
                    0 => *scheme = "http".into(),
                    1 => *host = "other.example.invalid".into(),
                    2 => *port = 8443,
                    3 => *method = "GET".into(),
                    4 => *path_and_query = "/submit?scope=two".into(),
                    _ => *payload_bytes = 5,
                }
            }
            assert!(store.matching_policy(&outside).unwrap().is_none());
        }
        assert!(store.matching_policy(&original).unwrap().is_some());
    }

    #[test]
    fn invalid_options_capacity_revision_and_expired_entry_replacement_are_bounded() {
        let (store, time, _) = fixture();
        for ttl in [
            Duration::ZERO,
            Duration::from_millis(1),
            Duration::from_secs(MAX_POLICY_SECONDS + 1),
        ] {
            assert!(store
                .remember_request(&request(), ApprovalScope::Exact, ttl)
                .is_err());
        }
        for ceiling in [0, MAX_PAYLOAD_BYTES + 1] {
            assert!(store
                .remember_request(
                    &request(),
                    ApprovalScope::SameTarget {
                        max_payload_bytes: ceiling
                    },
                    Duration::from_secs(1)
                )
                .is_err());
        }
        let mut unbound = request();
        unbound.remembered_binding_sha256 = None;
        assert!(store
            .remember_request(&unbound, ApprovalScope::Exact, Duration::from_secs(1))
            .is_err());
        for index in 0..MAX_POLICIES {
            let mut next = request();
            if let SensitiveAction::FileWrite { relative_path, .. } = &mut next.action {
                *relative_path = format!("{index}.txt");
            }
            store
                .remember_request(&next, ApprovalScope::Exact, Duration::from_secs(1))
                .unwrap();
        }
        assert!(store
            .remember_request(&request(), ApprovalScope::Exact, Duration::from_secs(1))
            .is_err());
        assert_eq!(store.policies().unwrap().len(), MAX_POLICIES);
        time.store(1001, Ordering::SeqCst);
        remember(&store);
        assert_eq!(store.policies().unwrap().len(), 1);
        store.lock().unwrap().revision = u64::MAX;
        assert!(store.clear().is_err());
        assert_eq!(store.policies().unwrap().len(), 1);
    }

    fn reseal(store: &RememberedApprovalPolicies, payload: Vec<u8>) -> SealedApprovalPolicies {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&payload);
        let mut mac = store.key.mac().unwrap();
        mac.update(&bytes);
        bytes.extend_from_slice(&mac.finalize().into_bytes());
        let state = store.lock().unwrap();
        let checkpoint = ApprovalPolicyCheckpoint {
            store_id: state.store_id.clone(),
            revision: state.revision,
            snapshot_sha256: hex::encode(Sha256::digest(&bytes)),
            observed_unix_secs: state.observed_unix_secs,
        };
        SealedApprovalPolicies { bytes, checkpoint }
    }

    #[test]
    fn authenticated_but_malformed_unknown_duplicate_and_oversized_declarations_refuse() {
        let (store, _, clock) = fixture();
        remember(&store);
        let sealed = store.seal().unwrap();
        let source: serde_json::Value =
            serde_json::from_slice(&sealed.bytes[HEADER_BYTES..sealed.bytes.len() - TAG_BYTES])
                .unwrap();
        for variant in 0..7 {
            let mut value = source.clone();
            match variant {
                0 => value["unexpected"] = true.into(),
                1 => value["schemaVersion"] = 2.into(),
                2 => value["policies"][0]["bindingSha256"] = "not-a-digest".into(),
                3 => value["policies"][0]["expiresAtUnixSecs"] = 999.into(),
                4 => {
                    let copy = value["policies"][0].clone();
                    value["policies"].as_array_mut().unwrap().push(copy);
                }
                5 => value["policies"][0]["scope"]["unexpected"] = true.into(),
                _ => value["policies"][0]["action"]["unexpected"] = true.into(),
            }
            assert!(restore(
                &reseal(&store, serde_json::to_vec(&value).unwrap()),
                clock.clone()
            )
            .is_err());
        }
        assert!(restore(&reseal(&store, b"not json".to_vec()), clock.clone()).is_err());
        let oversized = vec![0; MAX_SNAPSHOT_BYTES + 1];
        assert!(RememberedApprovalPolicies::restore_with_clock(
            key(),
            &oversized,
            &sealed.checkpoint,
            clock
        )
        .is_err());
    }

    #[test]
    fn revoke_and_clear_still_reduce_consent_when_clock_rolls_back() {
        let (store, time, _) = fixture();
        let id = remember(&store);
        time.store(999, Ordering::SeqCst);
        assert!(store.revoke(&id).unwrap());
        assert!(store.clear().is_ok());
        assert!(store.policies().unwrap().is_empty());
        assert_eq!(store.seal().unwrap().checkpoint.observed_unix_secs, 1000);
        time.store(1000, Ordering::SeqCst);
        assert!(store.policies().unwrap().is_empty());
    }
}
