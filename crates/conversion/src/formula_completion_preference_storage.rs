//! Explicit authenticated checkpoints. The host owns storage, keys and freshness.
use super::{
    FeedbackEvent, FormulaDefinitionScope, FormulaPreferenceStore, FORMULA_PREFERENCE_MAX_EVENTS,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES: usize = 256 * 1024;
type Result<T> = std::result::Result<T, FormulaPreferenceStorageError>;
type HmacSha256 = Hmac<Sha256>;

/// Neither feedback consent nor a snapshot can enable durable storage implicitly.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaPreferenceStoragePermission {
    pub storage_enabled: bool,
    pub allow_sensitive_storage: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaPreferenceStorageError {
    Disabled,
    InvalidKey,
    InvalidSnapshot,
    AuthenticationFailed,
    ScopeMismatch,
    StaleCheckpoint,
    SensitiveStorageDenied,
    LimitExceeded,
}
impl std::fmt::Display for FormulaPreferenceStorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Disabled => "Preference storage is disabled by the host.",
            Self::InvalidKey => {
                "Preference storage requires separate nonzero host keys and a valid scope."
            }
            Self::InvalidSnapshot => "Invalid closed preference checkpoint.",
            Self::AuthenticationFailed => {
                "Preference checkpoint authentication or key binding failed."
            }
            Self::ScopeMismatch => "Preference checkpoint does not match the current host scope.",
            Self::StaleCheckpoint => {
                "Preference checkpoint is not the expected current checkpoint."
            }
            Self::SensitiveStorageDenied => {
                "Sensitive preference storage requires explicit host permission."
            }
            Self::LimitExceeded => "Preference checkpoint exceeds its resource budget.",
        })
    }
}
impl std::error::Error for FormulaPreferenceStorageError {}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    body: Body,
    authentication: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Body {
    schema_version: u16,
    scope: FormulaDefinitionScope,
    #[serde(with = "super::super::wire::revision_decimal")]
    checkpoint: u64,
    key_binding: String,
    events: Vec<Event>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Event {
    #[serde(with = "super::super::wire::revision_decimal")]
    event_id: u64,
    digest: String,
    undone: bool,
    sensitive: bool,
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn digest(raw: &str) -> Result<[u8; 32]> {
    if raw.len() != 64 {
        return Err(FormulaPreferenceStorageError::InvalidSnapshot);
    }
    let digit = |byte| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    };
    let mut bytes = [0; 32];
    for (index, pair) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        bytes[index] = digit(pair[0])
            .zip(digit(pair[1]))
            .map(|(a, b)| a * 16 + b)
            .ok_or(FormulaPreferenceStorageError::InvalidSnapshot)?;
    }
    Ok(bytes)
}
fn mac(key: &[u8; 32], domain: &[u8], content: &[u8]) -> Result<HmacSha256> {
    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|_| FormulaPreferenceStorageError::InvalidKey)?;
    mac.update(domain);
    mac.update(content);
    Ok(mac)
}
const SNAPSHOT_DOMAIN: &[u8] = b"latexsnipper:preference-checkpoint:v1\0";
const KEY_DOMAIN: &[u8] = b"latexsnipper:preference-key-binding:v1\0";
fn permission(permission: FormulaPreferenceStoragePermission) -> Result<()> {
    if permission.storage_enabled {
        Ok(())
    } else {
        Err(FormulaPreferenceStorageError::Disabled)
    }
}
fn keys(feedback_key: &[u8; 32], storage_key: &[u8; 32]) -> Result<()> {
    if feedback_key == storage_key || storage_key.iter().all(|b| *b == 0) {
        Err(FormulaPreferenceStorageError::InvalidKey)
    } else {
        Ok(())
    }
}
fn sensitive(events: &[Event], permission: FormulaPreferenceStoragePermission) -> Result<()> {
    if !permission.allow_sensitive_storage && events.iter().any(|event| event.sensitive) {
        Err(FormulaPreferenceStorageError::SensitiveStorageDenied)
    } else {
        Ok(())
    }
}

impl FormulaPreferenceStore {
    /// No IO: persist this bounded JSON in host-protected storage, atomically with
    /// an independently protected freshness checkpoint. This is not encryption.
    pub fn export_checkpoint(
        &mut self,
        storage_key: &[u8; 32],
        checkpoint: u64,
        storage_permission: FormulaPreferenceStoragePermission,
    ) -> Result<String> {
        permission(storage_permission)?;
        keys(&self.key, storage_key)?;
        if checkpoint == 0
            || self
                .last_checkpoint
                .is_some_and(|previous| checkpoint <= previous)
        {
            return Err(FormulaPreferenceStorageError::StaleCheckpoint);
        }
        let body = Body {
            schema_version: 1,
            scope: self.scope.clone(),
            checkpoint,
            key_binding: hex(&mac(storage_key, KEY_DOMAIN, &self.key)?
                .finalize()
                .into_bytes()),
            events: self
                .events
                .iter()
                .map(|event| Event {
                    event_id: event.id,
                    digest: hex(&event.key),
                    undone: event.undone,
                    sensitive: event.sensitive,
                })
                .collect(),
        };
        sensitive(&body.events, storage_permission)?;
        let encoded = serde_json::to_vec(&body)
            .map_err(|_| FormulaPreferenceStorageError::InvalidSnapshot)?;
        let authentication = hex(&mac(storage_key, SNAPSHOT_DOMAIN, &encoded)?
            .finalize()
            .into_bytes());
        let encoded = serde_json::to_string(&Snapshot {
            body,
            authentication,
        })
        .map_err(|_| FormulaPreferenceStorageError::InvalidSnapshot)?;
        if encoded.len() > FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES {
            return Err(FormulaPreferenceStorageError::LimitExceeded);
        }
        // A failed host write may retry the SAME returned bytes, not manufacture
        // a new export with an old sequence. Keep retries explicit and bounded.
        self.last_checkpoint = Some(checkpoint);
        Ok(encoded)
    }

    /// Restore creates a NEW owner, always with feedback/ranking disabled. No
    /// captures or receipts survive. Expected scope/checkpoint must come from
    /// current trusted host state, never from the untrusted snapshot itself.
    pub fn restore_checkpoint(
        scope: FormulaDefinitionScope,
        feedback_key: [u8; 32],
        storage_key: &[u8; 32],
        expected_checkpoint: u64,
        snapshot_json: &str,
        storage_permission: FormulaPreferenceStoragePermission,
    ) -> Result<Self> {
        permission(storage_permission)?;
        keys(&feedback_key, storage_key)?;
        if snapshot_json.len() > FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES {
            return Err(FormulaPreferenceStorageError::LimitExceeded);
        }
        let mut store = Self::new(scope, feedback_key)
            .map_err(|_| FormulaPreferenceStorageError::InvalidKey)?;
        let snapshot: Snapshot = serde_json::from_str(snapshot_json)
            .map_err(|_| FormulaPreferenceStorageError::InvalidSnapshot)?;
        let body = snapshot.body;
        if body.schema_version != 1 || body.checkpoint == 0 {
            return Err(FormulaPreferenceStorageError::InvalidSnapshot);
        }
        if body.events.len() > FORMULA_PREFERENCE_MAX_EVENTS {
            return Err(FormulaPreferenceStorageError::LimitExceeded);
        }
        let encoded = serde_json::to_vec(&body)
            .map_err(|_| FormulaPreferenceStorageError::InvalidSnapshot)?;
        mac(storage_key, SNAPSHOT_DOMAIN, &encoded)?
            .verify_slice(&digest(&snapshot.authentication)?)
            .map_err(|_| FormulaPreferenceStorageError::AuthenticationFailed)?;
        if body.scope != store.scope {
            return Err(FormulaPreferenceStorageError::ScopeMismatch);
        }
        if expected_checkpoint == 0 || body.checkpoint != expected_checkpoint {
            return Err(FormulaPreferenceStorageError::StaleCheckpoint);
        }
        mac(storage_key, KEY_DOMAIN, &store.key)?
            .verify_slice(&digest(&body.key_binding)?)
            .map_err(|_| FormulaPreferenceStorageError::AuthenticationFailed)?;
        sensitive(&body.events, storage_permission)?;
        let mut previous = None;
        for event in body.events {
            if event.event_id == 0 || previous.is_some_and(|id| event.event_id <= id) {
                return Err(FormulaPreferenceStorageError::InvalidSnapshot);
            }
            store.events.push(FeedbackEvent {
                id: event.event_id,
                key: digest(&event.digest)?,
                undone: event.undone,
                sensitive: event.sensitive,
            });
            previous = Some(event.event_id);
        }
        store.last_event = previous;
        store.last_checkpoint = Some(body.checkpoint);
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> FormulaDefinitionScope {
        FormulaDefinitionScope {
            user_sha256: "a".repeat(64),
            project_sha256: None,
            document_sha256: None,
            generation: 0,
        }
    }
    fn permission() -> FormulaPreferenceStoragePermission {
        FormulaPreferenceStoragePermission {
            storage_enabled: true,
            allow_sensitive_storage: true,
        }
    }
    #[test]
    fn even_authenticated_records_must_satisfy_finite_ledger_invariants() {
        let mut store = FormulaPreferenceStore::new(scope(), [19; 32]).unwrap();
        let encoded = store.export_checkpoint(&[27; 32], 1, permission()).unwrap();
        for kind in ["duplicate", "zero", "reverse", "digest", "count"] {
            let mut snapshot: Snapshot = serde_json::from_str(&encoded).unwrap();
            snapshot.body.events = match kind {
                "duplicate" => vec![(1, "a".repeat(64)), (1, "a".repeat(64))],
                "zero" => vec![(0, "a".repeat(64))],
                "reverse" => vec![(2, "a".repeat(64)), (1, "a".repeat(64))],
                "digest" => vec![(1, "PRIVATE_INVALID_DIGEST".into())],
                _ => (1..=1025).map(|id| (id, "a".repeat(64))).collect(),
            }
            .into_iter()
            .map(|(event_id, digest)| Event {
                event_id,
                digest,
                undone: false,
                sensitive: false,
            })
            .collect();
            let body = serde_json::to_vec(&snapshot.body).unwrap();
            snapshot.authentication = hex(&mac(&[27; 32], SNAPSHOT_DOMAIN, &body)
                .unwrap()
                .finalize()
                .into_bytes());
            let raw = serde_json::to_string(&snapshot).unwrap();
            let result = FormulaPreferenceStore::restore_checkpoint(
                scope(),
                [19; 32],
                &[27; 32],
                1,
                &raw,
                permission(),
            );
            assert_eq!(
                result.err().unwrap(),
                if kind == "count" {
                    FormulaPreferenceStorageError::LimitExceeded
                } else {
                    FormulaPreferenceStorageError::InvalidSnapshot
                }
            );
        }
    }
}
