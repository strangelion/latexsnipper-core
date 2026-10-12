//! Host-owned, nonblocking consent for sensitive WASI broker actions.
//! Requests are descriptions, not capabilities. No guest interface grants consent.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{VerifiedComponentPackage, WasiDiagnostic, WasiDiagnosticCode};

const MAX_ENTRIES: usize = 128;
const MAX_USES: u32 = 10_000;
const MAX_TTL: Duration = Duration::from_secs(3600);
const MAX_ACTION_BYTES: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SensitiveAction {
    FileWrite {
        grant_id: String,
        root: String,
        relative_path: String,
        /// Observed old content; None means the target did not exist.
        target_sha256: Option<String>,
        payload_sha256: String,
        payload_bytes: usize,
    },
    NetworkSend {
        scheme: String,
        host: String,
        port: u16,
        method: String,
        path_and_query: String,
        payload_sha256: String,
        payload_bytes: usize,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub schema_version: u16,
    pub id: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub component_sha256: String,
    pub host_context_sha256: String,
    /// Stable identity for a future trusted-host remembered policy, not a grant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remembered_binding_sha256: Option<String>,
    pub action: SensitiveAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ApprovalScope {
    /// The complete operation, including the payload digest and byte count.
    Exact,
    /// Explicit consent to varied payloads/old content for the same file or network route.
    /// No directory prefix, origin-wide wildcard or method expansion is allowed.
    SameTarget { max_payload_bytes: usize },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalGrantSnapshot {
    pub request_id: String,
    pub scope: ApprovalScope,
    pub remaining_uses: u32,
    pub in_flight_uses: usize,
    pub expires_in_millis: u64,
}

#[derive(Default)]
struct State {
    pending: BTreeMap<String, ApprovalRequest>,
    grants: BTreeMap<String, Grant>,
}
struct Grant {
    request: ApprovalRequest,
    scope: ApprovalScope,
    remaining: u32,
    lease: Arc<ApprovalLease>,
}

impl Grant {
    fn covers(&self, request: &ApprovalRequest) -> bool {
        self.request.host_context_sha256 == request.host_context_sha256
            && self.request.remembered_binding_sha256 == request.remembered_binding_sha256
            && self.request.plugin_id == request.plugin_id
            && self.request.plugin_version == request.plugin_version
            && self.request.component_sha256 == request.component_sha256
            && match self.scope {
                ApprovalScope::Exact => self.request.action == request.action,
                ApprovalScope::SameTarget { max_payload_bytes } => {
                    same_target(&self.request.action, &request.action)
                        && payload_bytes(&request.action) <= max_payload_bytes
                }
            }
    }
}

#[derive(Debug)]
struct ApprovalLease {
    revoked: AtomicBool,
    expires: Instant,
}

/// Private per-use authority; exhaustion does not invalidate a started use.
#[derive(Debug)]
pub(crate) struct ActionPermit {
    lease: Arc<ApprovalLease>,
}

impl ActionPermit {
    pub(crate) fn check(&self) -> Result<(), WasiDiagnostic> {
        self.check_at(Instant::now())
    }

    fn check_at(&self, now: Instant) -> Result<(), WasiDiagnostic> {
        if self.lease.revoked.load(Ordering::Acquire) {
            return Err(WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiApprovalRevoked,
                "sensitive action consent was revoked before publication or dispatch",
            ));
        }
        if now >= self.lease.expires {
            return Err(WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiApprovalExpired,
                "sensitive action consent expired before publication or dispatch",
            ));
        }
        Ok(())
    }
}

fn lease(expires: Instant) -> Arc<ApprovalLease> {
    Arc::new(ApprovalLease {
        revoked: AtomicBool::new(false),
        expires,
    })
}

/// In-memory authority, usable only by trusted host code after user consent.
/// Never auto-approves, calls a UI callback, waits for a dialog or persists grants.
#[derive(Default)]
pub struct SensitiveActionController {
    state: Mutex<State>,
}

impl SensitiveActionController {
    /// Only trusted host code may call this after a distinct remember-choice confirmation.
    /// Stores a policy, not an execution grant; pending work is not replayed or approved.
    pub fn remember(
        &self,
        request_id: &str,
        policies: &crate::RememberedApprovalPolicies,
        scope: ApprovalScope,
        valid_for: Duration,
    ) -> Result<String, WasiDiagnostic> {
        let state = self.lock()?;
        let request = state.pending.get(request_id).ok_or_else(|| {
            invalid("remembered consent requires a current owned pending request")
        })?;
        policies.remember_request(request, scope, valid_for)
    }

    pub fn pending(&self) -> Result<Vec<ApprovalRequest>, WasiDiagnostic> {
        Ok(self.lock()?.pending.values().cloned().collect())
    }

    pub fn grants(&self) -> Result<Vec<ApprovalGrantSnapshot>, WasiDiagnostic> {
        let now = Instant::now();
        let mut state = self.lock()?;
        prune(&mut state, now);
        Ok(state
            .grants
            .iter()
            .map(|(id, grant)| ApprovalGrantSnapshot {
                request_id: id.clone(),
                scope: grant.scope.clone(),
                remaining_uses: grant.remaining,
                in_flight_uses: Arc::strong_count(&grant.lease).saturating_sub(1),
                expires_in_millis: grant
                    .lease
                    .expires
                    .saturating_duration_since(now)
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            })
            .collect())
    }

    pub fn approve_once(&self, request_id: &str) -> Result<(), WasiDiagnostic> {
        self.approve(
            request_id,
            ApprovalScope::Exact,
            1,
            Duration::from_secs(300),
        )
    }

    /// User-approved finite batch/session, with no automatic scope widening.
    pub fn approve(
        &self,
        request_id: &str,
        scope: ApprovalScope,
        uses: u32,
        valid_for: Duration,
    ) -> Result<(), WasiDiagnostic> {
        validate_options(uses, valid_for)?;
        let now = Instant::now();
        let expires = now
            .checked_add(valid_for)
            .ok_or_else(|| invalid("approval deadline overflow"))?;
        let mut state = self.lock()?;
        prune(&mut state, now);
        let request = state
            .pending
            .get(request_id)
            .cloned()
            .or_else(|| {
                state
                    .grants
                    .get(request_id)
                    .map(|grant| grant.request.clone())
            })
            .ok_or_else(|| invalid("approval request is not owned by this controller"))?;
        if let ApprovalScope::SameTarget { max_payload_bytes } = scope {
            if max_payload_bytes < payload_bytes(&request.action)
                || max_payload_bytes > 16 * 1024 * 1024
            {
                return Err(invalid("same-target approval byte limit is invalid"));
            }
        }
        if !state.grants.contains_key(request_id) && state.grants.len() >= MAX_ENTRIES {
            return Err(invalid("approval grant capacity reached"));
        }
        let grant = Grant {
            request,
            scope,
            remaining: uses,
            lease: lease(expires),
        };
        // Covered requests are descriptions, not an execution queue.
        state.pending.retain(|_, pending| !grant.covers(pending));
        if let Some(old) = state.grants.insert(request_id.into(), grant) {
            old.lease.revoked.store(true, Ordering::Release);
        }
        Ok(())
    }

    /// One confirmation for a displayed list of exact actions, each usable once.
    /// Validation and insertion are atomic; an unknown or duplicate ID changes nothing.
    pub fn approve_batch(
        &self,
        request_ids: &[String],
        valid_for: Duration,
    ) -> Result<(), WasiDiagnostic> {
        validate_options(1, valid_for)?;
        if request_ids.is_empty()
            || request_ids.len() > MAX_ENTRIES
            || request_ids.iter().collect::<BTreeSet<_>>().len() != request_ids.len()
        {
            return Err(invalid("invalid exact approval batch"));
        }
        let now = Instant::now();
        let expires = now
            .checked_add(valid_for)
            .ok_or_else(|| invalid("approval deadline overflow"))?;
        let mut state = self.lock()?;
        prune(&mut state, now);
        let requests =
            request_ids
                .iter()
                .map(|id| {
                    state.pending.get(id).cloned().ok_or_else(|| {
                        invalid("approval batch contains an unknown pending request")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
        let additional = request_ids
            .iter()
            .filter(|id| !state.grants.contains_key(*id))
            .count();
        if state.grants.len() + additional > MAX_ENTRIES {
            return Err(invalid("approval grant capacity reached"));
        }
        for request in requests {
            state.pending.remove(&request.id);
            if let Some(old) = state.grants.insert(
                request.id.clone(),
                Grant {
                    request,
                    scope: ApprovalScope::Exact,
                    remaining: 1,
                    lease: lease(expires),
                },
            ) {
                old.lease.revoked.store(true, Ordering::Release);
            }
        }
        Ok(())
    }

    /// Revokes future uses and interrupts staged IO at its next checkpoint.
    /// Already-published files or dispatched native network IO cannot be undone.
    pub fn revoke(&self, request_id: &str) -> Result<bool, WasiDiagnostic> {
        let mut state = self.lock()?;
        let pending = state.pending.remove(request_id).is_some();
        let grant = state.grants.remove(request_id);
        if let Some(grant) = &grant {
            grant.lease.revoked.store(true, Ordering::Release);
        }
        Ok(grant.is_some() || pending)
    }

    pub fn clear(&self) -> Result<(), WasiDiagnostic> {
        let mut state = self.lock()?;
        for grant in state.grants.values() {
            grant.lease.revoked.store(true, Ordering::Release);
        }
        *state = State::default();
        Ok(())
    }

    pub(crate) fn invalidate_context(&self, context: &ApprovalContext) {
        // A poisoned controller already rejects every future authorization.
        if let Ok(mut state) = self.lock() {
            state
                .pending
                .retain(|_, request| request.host_context_sha256 != context.context_sha256);
            state.grants.retain(|_, grant| {
                let keep = grant.request.host_context_sha256 != context.context_sha256;
                if !keep {
                    grant.lease.revoked.store(true, Ordering::Release);
                }
                keep
            });
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, WasiDiagnostic> {
        self.state.lock().map_err(|poisoned| {
            for grant in poisoned.get_ref().grants.values() {
                grant.lease.revoked.store(true, Ordering::Release);
            }
            WasiDiagnostic::new(
                WasiDiagnosticCode::PluginWasiHostFailure,
                "approval controller lock poisoned",
            )
        })
    }

    pub(crate) fn authorize(
        &self,
        context: &ApprovalContext,
        action: SensitiveAction,
    ) -> Result<ActionPermit, WasiDiagnostic> {
        let action_bytes =
            serde_json::to_vec(&action).map_err(|_| invalid("action serialization failed"))?;
        if action_bytes.len() > MAX_ACTION_BYTES {
            return Err(invalid("sensitive action metadata exceeds budget"));
        }
        let identity = serde_json::to_vec(&(
            1_u16,
            &context.context_sha256,
            &context.plugin_id,
            &context.plugin_version,
            &context.component_sha256,
            &context.remembered_binding_sha256,
            &action,
        ))
        .map_err(|_| invalid("approval identity serialization failed"))?;
        let id = format!("{:x}", Sha256::digest(identity));
        let request = ApprovalRequest {
            schema_version: 1,
            id: id.clone(),
            plugin_id: context.plugin_id.clone(),
            plugin_version: context.plugin_version.clone(),
            component_sha256: context.component_sha256.clone(),
            host_context_sha256: context.context_sha256.clone(),
            remembered_binding_sha256: context.remembered_binding_sha256.clone(),
            action,
        };
        let mut state = self.lock()?;
        prune(&mut state, Instant::now());
        let matching = |grant: &Grant| grant.remaining > 0 && grant.covers(&request);
        let selected = state
            .grants
            .get(&id)
            .filter(|grant| matching(grant))
            .map(|_| id.clone())
            .or_else(|| {
                state
                    .grants
                    .iter()
                    .find(|(_, grant)| matching(grant))
                    .map(|(id, _)| id.clone())
            });
        if let Some(grant) = selected.and_then(|id| state.grants.get_mut(&id)) {
            grant.remaining -= 1;
            let permit = ActionPermit {
                lease: Arc::clone(&grant.lease),
            };
            state.pending.remove(&id);
            return Ok(permit);
        }
        if !state.pending.contains_key(&id) && state.pending.len() >= MAX_ENTRIES {
            return Err(invalid("pending approval capacity reached"));
        }
        state.pending.entry(id.clone()).or_insert(request);
        Err(WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiApprovalRequired,
            format!("host user approval required: {id}"),
        ))
    }
}

#[derive(Clone)]
pub(crate) struct ApprovalContext {
    plugin_id: String,
    plugin_version: String,
    component_sha256: String,
    context_sha256: String,
    pub(crate) remembered_binding_sha256: Option<String>,
}
impl ApprovalContext {
    pub(crate) fn new(package: &VerifiedComponentPackage) -> Result<Self, WasiDiagnostic> {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| invalid("cannot create host approval identity"))?;
        let manifest = serde_json::to_vec(&package.manifest)
            .map_err(|_| invalid("cannot bind verified manifest"))?;
        let identity = serde_json::to_vec(&(
            env!("CARGO_PKG_VERSION"),
            Sha256::digest(manifest).to_vec(),
            nonce,
        ))
        .map_err(|_| invalid("cannot bind host approval identity"))?;
        Ok(Self {
            plugin_id: package.manifest.id.clone(),
            plugin_version: package.manifest.version.clone(),
            component_sha256: package.component_sha256.clone(),
            context_sha256: format!("{:x}", Sha256::digest(identity)),
            remembered_binding_sha256: None,
        })
    }
    pub(crate) fn invalidate(&mut self) {
        self.remembered_binding_sha256 = None;
        self.context_sha256 = format!(
            "{:x}",
            Sha256::digest(format!("approval-context-change:{}", self.context_sha256))
        );
    }
}

pub(crate) fn payload_bytes(action: &SensitiveAction) -> usize {
    match action {
        SensitiveAction::FileWrite { payload_bytes, .. }
        | SensitiveAction::NetworkSend { payload_bytes, .. } => *payload_bytes,
    }
}
pub(crate) fn same_target(first: &SensitiveAction, second: &SensitiveAction) -> bool {
    match (first, second) {
        (
            SensitiveAction::FileWrite {
                grant_id: a,
                root: b,
                relative_path: c,
                ..
            },
            SensitiveAction::FileWrite {
                grant_id: x,
                root: y,
                relative_path: z,
                ..
            },
        ) => (a, b, c) == (x, y, z),
        (
            SensitiveAction::NetworkSend {
                scheme: a,
                host: b,
                port: c,
                method: d,
                path_and_query: e,
                ..
            },
            SensitiveAction::NetworkSend {
                scheme: v,
                host: w,
                port: x,
                method: y,
                path_and_query: z,
                ..
            },
        ) => (a, b, c, d, e) == (v, w, x, y, z),
        _ => false,
    }
}
fn validate_options(uses: u32, ttl: Duration) -> Result<(), WasiDiagnostic> {
    if uses == 0 || uses > MAX_USES || ttl.is_zero() || ttl > MAX_TTL {
        return Err(invalid(
            "approval requires 1..10000 uses and a positive lifetime of at most one hour",
        ));
    }
    Ok(())
}
fn prune(state: &mut State, now: Instant) {
    state.grants.retain(|_, grant| {
        (grant.remaining > 0 || Arc::strong_count(&grant.lease) > 1) && grant.lease.expires > now
    });
}
fn invalid(message: &str) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiResourcePolicy, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> ApprovalContext {
        ApprovalContext {
            plugin_id: "fixture.component".into(),
            plugin_version: "1.0.0".into(),
            component_sha256: "a".repeat(64),
            context_sha256: "b".repeat(64),
            remembered_binding_sha256: None,
        }
    }
    fn action(payload: &str) -> SensitiveAction {
        SensitiveAction::FileWrite {
            grant_id: "path-0".into(),
            root: "fixture-root".into(),
            relative_path: "output.txt".into(),
            target_sha256: None,
            payload_sha256: format!("{:x}", Sha256::digest(payload)),
            payload_bytes: payload.len(),
        }
    }

    #[test]
    fn remember_requires_owned_pending_request_and_does_not_issue_a_grant() {
        let controller = SensitiveActionController::default();
        let policies =
            crate::RememberedApprovalPolicies::new(crate::ApprovalPolicyKey::generate().unwrap())
                .unwrap();
        let mut context = context();
        assert!(controller
            .remember(
                "unknown",
                &policies,
                ApprovalScope::Exact,
                Duration::from_secs(60)
            )
            .is_err());
        controller.authorize(&context, action("x")).unwrap_err();
        let request = controller.pending().unwrap().remove(0);
        assert!(controller
            .remember(
                &request.id,
                &policies,
                ApprovalScope::Exact,
                Duration::from_secs(60)
            )
            .is_err());
        controller.clear().unwrap();
        context.remembered_binding_sha256 = Some("c".repeat(64));
        controller.authorize(&context, action("x")).unwrap_err();
        let request = controller.pending().unwrap().remove(0);
        let id = controller
            .remember(
                &request.id,
                &policies,
                ApprovalScope::SameTarget {
                    max_payload_bytes: 4,
                },
                Duration::from_secs(60),
            )
            .unwrap();
        assert_eq!(policies.matching_policy(&request).unwrap().unwrap().id, id);
        assert!(controller.grants().unwrap().is_empty());
        assert_eq!(controller.pending().unwrap().len(), 1);
        assert!(controller.authorize(&context, action("x")).is_err());
        controller.approve_once(&request.id).unwrap();
        assert!(controller.authorize(&context, action("x")).is_ok());
    }

    #[test]
    fn remembered_binding_metadata_is_not_authority_and_cannot_reuse_a_grant() {
        let controller = SensitiveActionController::default();
        let mut context = context();
        context.remembered_binding_sha256 = Some("c".repeat(64));
        controller.authorize(&context, action("x")).unwrap_err();
        let request = controller.pending().unwrap().remove(0);
        controller.approve_once(&request.id).unwrap();
        let mut changed = context.clone();
        changed.remembered_binding_sha256 = Some("d".repeat(64));
        assert!(controller.authorize(&changed, action("x")).is_err());
        let changed_request = controller.pending().unwrap().remove(0);
        assert_ne!(changed_request.id, request.id);
        assert_eq!(controller.grants().unwrap()[0].remaining_uses, 1);
        assert!(controller.authorize(&context, action("x")).is_ok());
    }

    #[test]
    fn exhausted_started_use_remains_revocable_and_reapproval_is_not_a_blacklist() {
        let controller = SensitiveActionController::default();
        let context = context();
        controller.authorize(&context, action("x")).unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller.approve_once(&id).unwrap();
        let permit = controller.authorize(&context, action("x")).unwrap();
        assert!(permit.check().is_ok());
        let grants = controller.grants().unwrap();
        assert_eq!(grants[0].remaining_uses, 0);
        assert_eq!(grants[0].in_flight_uses, 1);
        assert!(controller.authorize(&context, action("x")).is_err());
        assert!(controller.revoke(&id).unwrap());
        assert_eq!(
            permit.check().unwrap_err().code,
            WasiDiagnosticCode::PluginWasiApprovalRevoked
        );
        controller.authorize(&context, action("x")).unwrap_err();
        controller.approve_once(&id).unwrap();
        let fresh = controller.authorize(&context, action("x")).unwrap();
        assert!(fresh.check().is_ok());
        assert!(permit.check().is_err());
        drop(fresh);
        assert!(controller.grants().unwrap().is_empty());
    }

    #[test]
    fn updating_consent_revokes_old_permits_without_revoking_the_new_grant() {
        let controller = SensitiveActionController::default();
        let context = context();
        controller.authorize(&context, action("x")).unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller
            .approve(&id, ApprovalScope::Exact, 2, Duration::from_secs(60))
            .unwrap();
        let old = controller.authorize(&context, action("x")).unwrap();
        controller.approve_once(&id).unwrap();
        assert_eq!(
            old.check().unwrap_err().code,
            WasiDiagnosticCode::PluginWasiApprovalRevoked
        );
        let fresh = controller.authorize(&context, action("x")).unwrap();
        assert!(fresh.check().is_ok());
        controller.clear().unwrap();
        assert_eq!(
            fresh.check().unwrap_err().code,
            WasiDiagnosticCode::PluginWasiApprovalRevoked
        );
    }

    #[test]
    fn started_use_expiry_is_checked_independently_of_remaining_quota() {
        let controller = SensitiveActionController::default();
        let context = context();
        controller.authorize(&context, action("x")).unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller.approve_once(&id).unwrap();
        let permit = controller.authorize(&context, action("x")).unwrap();
        assert!(permit.check().is_ok());
        assert_eq!(
            permit.check_at(permit.lease.expires).unwrap_err().code,
            WasiDiagnosticCode::PluginWasiApprovalExpired
        );
        assert_eq!(controller.grants().unwrap()[0].remaining_uses, 0);
    }

    #[test]
    fn observed_controller_poisoning_invalidates_started_permits() {
        let controller = SensitiveActionController::default();
        let context = context();
        controller.authorize(&context, action("x")).unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller.approve_once(&id).unwrap();
        let permit = controller.authorize(&context, action("x")).unwrap();
        std::thread::scope(|scope| {
            assert!(scope
                .spawn(|| {
                    let _guard = controller.state.lock().unwrap();
                    panic!("controlled mutex poisoning");
                })
                .join()
                .is_err());
        });
        assert_eq!(
            controller.revoke(&id).unwrap_err().code,
            WasiDiagnosticCode::PluginWasiHostFailure
        );
        assert_eq!(
            permit.check().unwrap_err().code,
            WasiDiagnosticCode::PluginWasiApprovalRevoked
        );
        assert!(controller.authorize(&context, action("x")).is_err());
    }

    #[test]
    fn invalidated_context_removes_stale_requests_and_revokes_only_its_permits() {
        let controller = SensitiveActionController::default();
        let first = context();
        let mut second = first.clone();
        second.invalidate();
        let mut permits = Vec::new();
        for context in [&first, &second] {
            controller.authorize(context, action("x")).unwrap_err();
            let id = controller
                .pending()
                .unwrap()
                .into_iter()
                .find(|request| request.host_context_sha256 == context.context_sha256)
                .unwrap()
                .id;
            controller.approve_once(&id).unwrap();
            permits.push(controller.authorize(context, action("x")).unwrap());
            controller
                .authorize(context, action("pending"))
                .unwrap_err();
        }
        controller.invalidate_context(&first);
        assert_eq!(
            permits[0].check().unwrap_err().code,
            WasiDiagnosticCode::PluginWasiApprovalRevoked
        );
        assert!(permits[1].check().is_ok());
        assert_eq!(controller.pending().unwrap().len(), 1);
        assert_eq!(
            controller.pending().unwrap()[0].host_context_sha256,
            second.context_sha256
        );
        controller.clear().unwrap();
        assert!(permits[1].check().is_err());
    }

    #[cfg(windows)]
    #[test]
    fn live_consent_revocation_interrupts_partial_named_stream_copy_without_reprompting() {
        use crate::permissions::CheckedWriteError;
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        let target = root.join("output.txt");
        let mut stream = target.as_os_str().to_os_string();
        stream.push(":data");
        std::fs::write(&target, b"original").unwrap();
        let metadata = vec![b'x'; 128 * 1024];
        std::fs::write(&stream, &metadata).unwrap();
        let value: latexsnipper_plugin::PluginPermissionsV3 = serde_json::from_value(serde_json::json!({
            "paths": [{"path": "write", "access": "write"}], "temporaryDirectory": false, "clocks": false, "randomness": false,
            "registrations": {"capabilities": false, "importers": false, "exporters": false, "runtimes": false},
            "limits": {"maxConcurrentExecutions": 1}
        })).unwrap();
        let permissions =
            crate::ComponentPermissions::from_manifest(&value, temporary.path()).unwrap();
        let controller = SensitiveActionController::default();
        let context = context();
        let expected = hex::encode(Sha256::digest(b"original"));
        let operation = SensitiveAction::FileWrite {
            grant_id: "path-0".into(),
            root: root.to_str().unwrap().into(),
            relative_path: "output.txt".into(),
            target_sha256: Some(expected.clone()),
            payload_sha256: hex::encode(Sha256::digest(b"new")),
            payload_bytes: 3,
        };
        controller
            .authorize(&context, operation.clone())
            .unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller.approve_once(&id).unwrap();
        let permit = controller.authorize(&context, operation).unwrap();
        let mut revoked_during_copy = false;
        let result = permissions.write_file_checked(
            "path-0",
            "output.txt",
            b"new",
            metadata.len(),
            Some(&expected),
            || {
                for entry in std::fs::read_dir(&root).unwrap() {
                    let path = entry.unwrap().path();
                    if path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(".snipper-write-")
                    {
                        let mut named = path.as_os_str().to_os_string();
                        named.push(":data");
                        if std::fs::metadata(&named).is_ok_and(|metadata| metadata.len() > 0)
                            && !revoked_during_copy
                        {
                            assert!(controller.revoke(&id).unwrap());
                            revoked_during_copy = true;
                        }
                    }
                }
                permit.check()
            },
        );
        assert!(revoked_during_copy);
        assert!(matches!(
            result,
            Err(CheckedWriteError::Interrupted(WasiDiagnostic {
                code: WasiDiagnosticCode::PluginWasiApprovalRevoked,
                ..
            }))
        ));
        assert!(controller.pending().unwrap().is_empty());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert_eq!(std::fs::read(&stream).unwrap(), metadata);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn revoking_clearing_or_expiring_staged_consent_preserves_original_and_cleans_up() {
        use crate::permissions::CheckedWriteError;
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir(temporary.path().join("write")).unwrap();
        let root = temporary.path().join("write");
        std::fs::write(root.join("output.txt"), b"original").unwrap();
        let manifest_permissions: latexsnipper_plugin::PluginPermissionsV3 = serde_json::from_value(serde_json::json!({
            "paths": [{"path": "write", "access": "write"}],
            "temporaryDirectory": false, "clocks": false, "randomness": false,
            "registrations": {"capabilities": false, "importers": false, "exporters": false, "runtimes": false},
            "limits": {"maxConcurrentExecutions": 1}
        })).unwrap();
        let permissions =
            crate::ComponentPermissions::from_manifest(&manifest_permissions, temporary.path())
                .unwrap();
        let payload = vec![b'x'; 128 * 1024];
        let expected = format!("{:x}", Sha256::digest(b"original"));
        let operation = SensitiveAction::FileWrite {
            grant_id: "path-0".into(),
            root: root.to_str().unwrap().into(),
            relative_path: "output.txt".into(),
            target_sha256: Some(expected.clone()),
            payload_sha256: format!("{:x}", Sha256::digest(&payload)),
            payload_bytes: payload.len(),
        };
        for stop in 0..3 {
            let controller = SensitiveActionController::default();
            let context = context();
            controller
                .authorize(&context, operation.clone())
                .unwrap_err();
            let id = controller.pending().unwrap()[0].id.clone();
            controller.approve_once(&id).unwrap();
            let permit = controller.authorize(&context, operation.clone()).unwrap();
            let mut checkpoints = 0;
            let result = permissions.write_file_checked(
                "path-0",
                "output.txt",
                &payload,
                payload.len(),
                Some(&expected),
                || {
                    checkpoints += 1;
                    if checkpoints == 3 && stop < 2 {
                        if stop == 1 {
                            controller.clear().unwrap();
                        } else {
                            assert!(controller.revoke(&id).unwrap());
                        }
                    }
                    // Deterministic expiry exactly at the pre-publication checkpoint.
                    if checkpoints == 5 && stop == 2 {
                        return permit.check_at(permit.lease.expires);
                    }
                    permit.check()
                },
            );
            let Err(CheckedWriteError::Interrupted(error)) = result else {
                panic!("invalid consent must prevent publication");
            };
            assert_eq!(
                error.code,
                if stop == 2 {
                    WasiDiagnosticCode::PluginWasiApprovalExpired
                } else {
                    WasiDiagnosticCode::PluginWasiApprovalRevoked
                }
            );
            assert_eq!(std::fs::read(root.join("output.txt")).unwrap(), b"original");
            assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
            assert!(controller.authorize(&context, operation.clone()).is_err());
        }
        drop(permissions);
        temporary.close().unwrap();
    }
    #[test]
    fn consent_is_default_deny_once_atomic_and_not_a_permanent_refusal() {
        let controller = SensitiveActionController::default();
        let context = context();
        assert_eq!(
            controller
                .authorize(&context, action("x"))
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiApprovalRequired
        );
        let id = controller.pending().unwrap()[0].id.clone();
        controller.approve_once(&id).unwrap();
        assert!(controller.authorize(&context, action("x")).is_ok());
        assert!(controller.authorize(&context, action("x")).is_err());
        controller.revoke(&id).unwrap();
        assert!(controller.authorize(&context, action("x")).is_err());
        controller.approve_once(&id).unwrap();
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| controller.authorize(&context, action("x")).is_ok()))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|result| *result)
                .count()
        });
        assert_eq!(results, 1);
    }
    #[test]
    fn explicit_target_session_allows_varied_payloads_without_scope_widening() {
        let controller = SensitiveActionController::default();
        let mut context = context();
        controller.authorize(&context, action("x")).unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller
            .approve(
                &id,
                ApprovalScope::SameTarget {
                    max_payload_bytes: 4,
                },
                3,
                Duration::from_secs(60),
            )
            .unwrap();
        for payload in ["x", "yz"] {
            assert!(controller.authorize(&context, action(payload)).is_ok());
        }
        assert!(controller.authorize(&context, action("large")).is_err());
        let mut other = action("x");
        if let SensitiveAction::FileWrite { relative_path, .. } = &mut other {
            *relative_path = "other.txt".into();
        }
        assert!(controller.authorize(&context, other).is_err());
        assert_eq!(controller.grants().unwrap()[0].remaining_uses, 1);
        context.invalidate();
        assert!(controller.authorize(&context, action("x")).is_err());
        controller.revoke(&id).unwrap();
        assert!(controller.grants().unwrap().is_empty());
    }

    #[test]
    fn same_target_consent_dismisses_only_covered_pending_descriptions() {
        let network = SensitiveAction::NetworkSend {
            scheme: "https".into(),
            host: "fixture.example.invalid".into(),
            port: 443,
            method: "POST".into(),
            path_and_query: "/formula?purpose=test".into(),
            payload_sha256: "a".repeat(64),
            payload_bytes: 1,
        };
        for initial in [action("x"), network] {
            let controller = SensitiveActionController::default();
            let context = context();
            controller.authorize(&context, initial.clone()).unwrap_err();
            let id = controller.pending().unwrap()[0].id.clone();
            let mut changed = initial.clone();
            let mut oversized = initial.clone();
            let mut other_target = initial.clone();
            match &mut changed {
                SensitiveAction::FileWrite {
                    payload_sha256,
                    payload_bytes,
                    target_sha256,
                    ..
                } => {
                    *payload_sha256 = "c".repeat(64);
                    *payload_bytes = 2;
                    *target_sha256 = Some("d".repeat(64));
                }
                SensitiveAction::NetworkSend {
                    payload_sha256,
                    payload_bytes,
                    ..
                } => {
                    *payload_sha256 = "c".repeat(64);
                    *payload_bytes = 2;
                }
            }
            match &mut oversized {
                SensitiveAction::FileWrite { payload_bytes, .. }
                | SensitiveAction::NetworkSend { payload_bytes, .. } => *payload_bytes = 5,
            }
            match &mut other_target {
                SensitiveAction::FileWrite { relative_path, .. } => {
                    *relative_path = "other.txt".into();
                }
                SensitiveAction::NetworkSend { path_and_query, .. } => {
                    *path_and_query = "/formula?purpose=other".into();
                }
            }
            for candidate in [&changed, &oversized, &other_target] {
                controller
                    .authorize(&context, candidate.clone())
                    .unwrap_err();
            }
            for field in 0..4 {
                let mut other = context.clone();
                match field {
                    0 => other.plugin_id = "other.component".into(),
                    1 => other.plugin_version = "2.0.0".into(),
                    2 => other.component_sha256 = "e".repeat(64),
                    _ => other.context_sha256 = "f".repeat(64),
                }
                controller.authorize(&other, initial.clone()).unwrap_err();
            }
            assert_eq!(controller.pending().unwrap().len(), 8);
            controller
                .approve(
                    &id,
                    ApprovalScope::SameTarget {
                        max_payload_bytes: 4,
                    },
                    2,
                    Duration::from_secs(60),
                )
                .unwrap();
            assert_eq!(controller.pending().unwrap().len(), 6);
            assert_eq!(controller.grants().unwrap()[0].remaining_uses, 2);
            assert!(controller.authorize(&context, changed).is_ok());
            assert!(controller.authorize(&context, initial.clone()).is_ok());
            assert_eq!(controller.pending().unwrap().len(), 6);
            assert_eq!(
                controller.authorize(&context, initial).unwrap_err().code,
                WasiDiagnosticCode::PluginWasiApprovalRequired
            );
            assert_eq!(controller.pending().unwrap().len(), 7);
        }
    }

    #[test]
    fn exact_consent_does_not_dismiss_pending_payload_variants() {
        let controller = SensitiveActionController::default();
        let context = context();
        controller.authorize(&context, action("x")).unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller.authorize(&context, action("y")).unwrap_err();
        controller.approve_once(&id).unwrap();
        let pending = controller.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].action, action("y"));
        assert!(controller.authorize(&context, action("x")).is_ok());
        assert!(controller.authorize(&context, action("y")).is_err());
    }

    #[test]
    fn batch_confirmation_is_closed_atomic_and_expiry_is_enforced() {
        let controller = SensitiveActionController::default();
        let context = context();
        for payload in ["x", "y"] {
            controller.authorize(&context, action(payload)).unwrap_err();
        }
        let ids: Vec<_> = controller
            .pending()
            .unwrap()
            .into_iter()
            .map(|request| request.id)
            .collect();
        assert!(controller
            .approve_batch(&[ids[0].clone(), "unknown".into()], Duration::from_secs(60))
            .is_err());
        assert_eq!(controller.pending().unwrap().len(), 2);
        assert!(controller.grants().unwrap().is_empty());
        controller
            .approve_batch(&ids, Duration::from_secs(60))
            .unwrap();
        for payload in ["x", "y"] {
            assert!(controller.authorize(&context, action(payload)).is_ok());
        }
        controller.authorize(&context, action("x")).unwrap_err();
        let current_id = controller.pending().unwrap()[0].id.clone();
        controller.approve_once(&current_id).unwrap();
        for grant in controller.state.lock().unwrap().grants.values_mut() {
            Arc::get_mut(&mut grant.lease).unwrap().expires = Instant::now();
        }
        assert!(controller.authorize(&context, action("x")).is_err());
        assert!(controller
            .approve(&current_id, ApprovalScope::Exact, 0, Duration::from_secs(1))
            .is_err());
        assert!(controller
            .approve(
                &current_id,
                ApprovalScope::Exact,
                1,
                MAX_TTL + Duration::from_secs(1)
            )
            .is_err());
    }

    #[test]
    fn exact_consent_binds_old_file_content_and_verified_component_identity() {
        let controller = SensitiveActionController::default();
        let context = context();
        controller.authorize(&context, action("x")).unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller.approve_once(&id).unwrap();
        let mut changed = action("x");
        if let SensitiveAction::FileWrite { target_sha256, .. } = &mut changed {
            *target_sha256 = Some("c".repeat(64));
        }
        assert!(controller.authorize(&context, changed).is_err());
        let mut updated = context.clone();
        updated.plugin_version = "2.0.0".into();
        assert!(controller.authorize(&updated, action("x")).is_err());
        updated = context.clone();
        updated.component_sha256 = "c".repeat(64);
        assert!(controller.authorize(&updated, action("x")).is_err());
        assert_eq!(controller.grants().unwrap()[0].remaining_uses, 1);
    }

    #[test]
    fn network_session_keeps_destination_method_query_and_byte_ceiling() {
        let controller = SensitiveActionController::default();
        let context = context();
        let action = SensitiveAction::NetworkSend {
            scheme: "https".into(),
            host: "fixture.example.invalid".into(),
            port: 443,
            method: "POST".into(),
            path_and_query: "/formula?purpose=test".into(),
            payload_sha256: "a".repeat(64),
            payload_bytes: 1,
        };
        controller.authorize(&context, action.clone()).unwrap_err();
        let id = controller.pending().unwrap()[0].id.clone();
        controller
            .approve(
                &id,
                ApprovalScope::SameTarget {
                    max_payload_bytes: 4,
                },
                2,
                Duration::from_secs(60),
            )
            .unwrap();
        for field in 0..6 {
            let mut altered = action.clone();
            if let SensitiveAction::NetworkSend {
                scheme,
                host,
                port,
                method,
                path_and_query,
                payload_bytes,
                ..
            } = &mut altered
            {
                match field {
                    0 => *scheme = "http".into(),
                    1 => *host = "other.example.invalid".into(),
                    2 => *port = 8443,
                    3 => *method = "DELETE".into(),
                    4 => *path_and_query = "/other".into(),
                    _ => *payload_bytes = 5,
                }
            }
            assert!(controller.authorize(&context, altered).is_err());
        }
        assert!(controller.authorize(&context, action).is_ok());
        assert_eq!(controller.grants().unwrap()[0].remaining_uses, 1);
    }

    #[test]
    fn request_capacity_unknown_ids_and_duplicate_batches_fail_closed() {
        let controller = SensitiveActionController::default();
        let context = context();
        assert!(controller.approve_once("unowned").is_err());
        for value in 0..MAX_ENTRIES {
            controller
                .authorize(&context, action(&value.to_string()))
                .unwrap_err();
        }
        assert_eq!(controller.pending().unwrap().len(), MAX_ENTRIES);
        assert_eq!(
            controller
                .authorize(&context, action("overflow"))
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiResourcePolicy
        );
        let id = controller.pending().unwrap()[0].id.clone();
        assert!(controller
            .approve_batch(&[id.clone(), id], Duration::from_secs(60))
            .is_err());
        assert!(controller.grants().unwrap().is_empty());
        controller.clear().unwrap();
        assert!(controller.pending().unwrap().is_empty());
    }
}
