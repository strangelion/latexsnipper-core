//! Trusted, stream-owned volatile feedback. No session, global store or disk IO.
use super::{bound_completion_result, WorkerError};
use latexsnipper_conversion::formula_completion::preferences::persistence::*;
use latexsnipper_conversion::formula_completion::{fragments::*, preferences::*, scoped::*};
use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::{json, value::RawValue, Value};
use std::collections::BTreeMap;

pub(super) const MAX_OWNERS: usize = 64;
const MAX_CAPTURES: usize = 128;
const MAX_METADATA: usize = 4096;
type Result<T> = std::result::Result<T, WorkerError>;

#[derive(Default)]
pub(super) struct Registry {
    namespace: Option<String>,
    next: u64,
    entries: BTreeMap<String, State>,
}
struct State {
    core: FormulaPreferenceStore,
    captures: BTreeMap<String, FormulaPreferenceCapture>,
    receipts: BTreeMap<String, FormulaPreferenceReceipt>,
}

#[derive(Deserialize)]
struct RawParams<'a> {
    #[serde(borrow)]
    params: &'a RawValue,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Create {
    schema_version: u16,
    scope: FormulaDefinitionScope,
    key_hex: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Restore {
    schema_version: u16,
    scope: FormulaDefinitionScope,
    key_hex: String,
    storage_key_hex: String,
    expected_checkpoint: Decimal,
    snapshot_json: String,
    permission: FormulaPreferenceStoragePermission,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Target {
    owner_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Call {
    owner_id: String,
    command: Command,
}
struct Decimal(u64);
impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty()
            || value.len() > 20
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(serde::de::Error::custom("Invalid canonical u64 string."));
        }
        value
            .parse()
            .map(Self)
            .map_err(|_| serde::de::Error::custom("Invalid canonical u64 string."))
    }
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Command {
    Info {},
    SetPolicy {
        policy: FormulaPreferencePolicy,
    },
    Capture {},
    DiscardCapture {
        capture_id: String,
    },
    RankFragments {
        request: Box<FormulaFragmentCompletionRequest>,
    },
    RecordAcceptance {
        capture_id: String,
        event_id: Decimal,
        request: Box<FormulaFragmentCompletionApplyRequest>,
    },
    Undo {
        receipt_id: String,
    },
    Clear {
        generation: Decimal,
        key_hex: String,
    },
    ExportCheckpoint {
        storage_key_hex: String,
        checkpoint: Decimal,
        permission: FormulaPreferenceStoragePermission,
    },
}
fn invalid() -> WorkerError {
    WorkerError::protocol(
        "INVALID_PARAMS",
        "Invalid closed preference parameters or revision encoding.",
    )
}
fn stale() -> WorkerError {
    WorkerError::protocol(
        "STALE_FEEDBACK",
        "Feedback token is unknown, consumed or invalidated.",
    )
}
fn not_found() -> WorkerError {
    WorkerError::protocol(
        "PREFERENCE_NOT_FOUND",
        "Preference owner is unknown or closed.",
    )
}
fn limit() -> WorkerError {
    WorkerError::protocol(
        "LIMIT_EXCEEDED",
        "Preference owner, capture or identity budget exhausted.",
    )
}
fn decode<T: DeserializeOwned>(raw: &str) -> Result<T> {
    serde_json::from_str(raw).map_err(|_| invalid())
}
fn metadata(raw: &str) -> Result<()> {
    if raw.len() > MAX_METADATA {
        Err(WorkerError::protocol(
            "INPUT_TOO_LARGE",
            "Preference metadata exceeds 4 KiB.",
        ))
    } else {
        Ok(())
    }
}
fn key(raw: &str) -> Result<[u8; 32]> {
    let digit = |b| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    if raw.len() != 64 {
        return Err(invalid());
    }
    let mut key = [0; 32];
    for (i, pair) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        key[i] = digit(pair[0])
            .zip(digit(pair[1]))
            .map(|(a, b)| a * 16 + b)
            .ok_or_else(invalid)?;
    }
    Ok(key)
}
fn core_error(value: FormulaPreferenceError) -> WorkerError {
    match value {
        FormulaPreferenceError::Completion(value) => super::scoped_completion_error(value),
        value => WorkerError::protocol(
            match value {
                FormulaPreferenceError::InvalidIdentity => "INVALID_ARGUMENT",
                FormulaPreferenceError::ScopeMismatch => "SCOPE_MISMATCH",
                FormulaPreferenceError::StaleFeedback => "STALE_FEEDBACK",
                FormulaPreferenceError::LimitExceeded => "LIMIT_EXCEEDED",
                _ => "INTERNAL",
            },
            value.to_string(),
        ),
    }
}
fn storage_error(value: FormulaPreferenceStorageError) -> WorkerError {
    WorkerError::protocol(
        match value {
            FormulaPreferenceStorageError::Disabled => "PREFERENCE_STORAGE_DISABLED",
            FormulaPreferenceStorageError::InvalidKey => "INVALID_ARGUMENT",
            FormulaPreferenceStorageError::InvalidSnapshot => "INVALID_SNAPSHOT",
            FormulaPreferenceStorageError::AuthenticationFailed => "AUTHENTICATION_FAILED",
            FormulaPreferenceStorageError::ScopeMismatch => "SCOPE_MISMATCH",
            FormulaPreferenceStorageError::StaleCheckpoint => "STALE_CHECKPOINT",
            FormulaPreferenceStorageError::SensitiveStorageDenied => "SENSITIVE_STORAGE_DENIED",
            FormulaPreferenceStorageError::LimitExceeded => "LIMIT_EXCEEDED",
        },
        value.to_string(),
    )
}
fn valid_token(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 96 {
        Err(invalid())
    } else {
        Ok(())
    }
}

pub(super) fn capabilities() -> Value {
    json!({"schemaVersion":1,"actions":["formula.preferences.create","formula.preferences.restore","formula.preferences.command","formula.preferences.close"],
        "authenticatedCheckpoints":true,"automaticStorage":false,"storagePermissionRequired":true,
        "maxSnapshotBytes":FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES,
        "defaultEnabled":false,"automaticCapture":false,"persistence":false,"maxOwners":MAX_OWNERS,"maxCaptures":MAX_CAPTURES,
        "maxEvents":FORMULA_PREFERENCE_MAX_EVENTS,"maxMetadataBytes":MAX_METADATA,"maxResultDataBytes":super::MAX_COMPLETION_RESULT_BYTES,
        "identityEncoding":"opaque-worker-instance-token","revisionEncoding":"canonical-decimal-u64-string"})
}

impl Registry {
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(super) fn close_all(&mut self) {
        self.entries.clear();
    }
    fn next_id(&mut self) -> Result<String> {
        if self.namespace.is_none() {
            let mut bytes = [0; 16];
            getrandom::fill(&mut bytes).map_err(|_| {
                WorkerError::protocol(
                    "PREFERENCE_UNAVAILABLE",
                    "Native randomness is unavailable for preference identity.",
                )
            })?;
            self.namespace = Some(bytes.iter().map(|b| format!("{b:02x}")).collect());
        }
        let next = self.next.checked_add(1).ok_or_else(limit)?;
        self.next = next;
        Ok(format!(
            "pref:{}:{next}",
            self.namespace.as_ref().ok_or_else(not_found)?
        ))
    }
    fn register(&mut self, core: FormulaPreferenceStore) -> Result<Value> {
        if self.entries.len() >= MAX_OWNERS {
            return Err(limit());
        }
        let id = self.next_id()?;
        let state = State {
            core,
            captures: BTreeMap::new(),
            receipts: BTreeMap::new(),
        };
        let info = state.info();
        self.entries.insert(id.clone(), state);
        Ok(json!({"ownerId":id,"info":info}))
    }
    pub(super) fn dispatch(&mut self, action: &str, line: &str) -> Result<Value> {
        // The legacy envelope validates routing; parse params AGAIN from raw JSON
        // so duplicate private keys are never erased through serde_json::Value.
        let raw: RawParams<'_> = serde_json::from_str(line).map_err(|_| invalid())?;
        let raw = raw.params.get();
        match action {
            "formula.preferences.create" => {
                metadata(raw)?;
                let request: Create = decode(raw)?;
                if request.schema_version != 1 {
                    return Err(WorkerError::protocol(
                        "VERSION_MISMATCH",
                        "Unsupported preference schema.",
                    ));
                }
                if self.entries.len() >= MAX_OWNERS {
                    return Err(limit());
                }
                let core = FormulaPreferenceStore::new(request.scope, key(&request.key_hex)?)
                    .map_err(core_error)?;
                self.register(core)
            }
            "formula.preferences.restore" => {
                let request: Restore = decode(raw)?;
                if request.schema_version != 1 {
                    return Err(WorkerError::protocol(
                        "VERSION_MISMATCH",
                        "Unsupported preference schema.",
                    ));
                }
                if self.entries.len() >= MAX_OWNERS {
                    return Err(limit());
                }
                let core = FormulaPreferenceStore::restore_checkpoint(
                    request.scope,
                    key(&request.key_hex)?,
                    &key(&request.storage_key_hex)?,
                    request.expected_checkpoint.0,
                    &request.snapshot_json,
                    request.permission,
                )
                .map_err(storage_error)?;
                self.register(core)
            }
            "formula.preferences.close" => {
                metadata(raw)?;
                let target: Target = decode(raw)?;
                valid_token(&target.owner_id)?;
                self.entries
                    .remove(&target.owner_id)
                    .ok_or_else(not_found)?;
                Ok(json!({"ownerId":target.owner_id,"closed":true}))
            }
            "formula.preferences.command" => {
                let call: Call = decode(raw)?;
                valid_token(&call.owner_id)?;
                if !matches!(
                    &call.command,
                    Command::RankFragments { .. } | Command::RecordAcceptance { .. }
                ) {
                    metadata(raw)?;
                }
                if !self.entries.contains_key(&call.owner_id) {
                    return Err(not_found());
                }
                let reserved = if matches!(
                    &call.command,
                    Command::Capture { .. } | Command::RecordAcceptance { .. }
                ) {
                    Some(self.next_id()?)
                } else {
                    None
                };
                self.entries
                    .get_mut(&call.owner_id)
                    .ok_or_else(not_found)?
                    .execute(call.command, reserved)
            }
            _ => Err(WorkerError::protocol(
                "UNKNOWN_ACTION",
                "Unknown preference action.",
            )),
        }
    }
}
impl State {
    fn info(&self) -> Value {
        json!({"schemaVersion":1,"scope":self.core.scope(),"policy":self.core.policy(),
        "eventCount":self.core.event_count(),"captureCount":self.captures.len(),"receiptCount":self.receipts.len(),
        "inMemory":true,"persistence":false,"automaticCapture":false,
        "authenticatedCheckpoints":true,"automaticStorage":false})
    }
    fn execute(&mut self, command: Command, reserved: Option<String>) -> Result<Value> {
        match command {
            Command::Info {} => Ok(self.info()),
            Command::SetPolicy { policy } => {
                let changed = self.core.policy() != policy;
                self.core.set_policy(policy).map_err(core_error)?;
                if changed {
                    self.captures.clear();
                    self.receipts.clear();
                }
                Ok(json!({"policy":self.core.policy()}))
            }
            Command::Capture {} => {
                if self.captures.len() >= MAX_CAPTURES {
                    return Err(limit());
                }
                let id = reserved.ok_or_else(limit)?;
                self.captures
                    .insert(id.clone(), self.core.capture_context());
                Ok(json!({"captureId":id}))
            }
            Command::DiscardCapture { capture_id } => {
                valid_token(&capture_id)?;
                self.captures.remove(&capture_id).ok_or_else(stale)?;
                Ok(json!({"discarded":true}))
            }
            Command::RankFragments { request } => {
                let result = self.core.rank_fragments(&request).map_err(core_error)?;
                bound_completion_result(
                    json!({"completion":result.completion,"order":result.order,"features":result.features}),
                )
            }
            Command::RecordAcceptance {
                capture_id,
                event_id,
                request,
            } => {
                valid_token(&capture_id)?;
                let capture = self.captures.get(&capture_id).ok_or_else(stale)?;
                let id = reserved.ok_or_else(limit)?;
                let receipt = self
                    .core
                    .record_acceptance_at(capture, &request, event_id.0)
                    .map_err(core_error)?;
                self.captures.remove(&capture_id);
                let receipt_id = receipt.map(|receipt| {
                    self.receipts.insert(id.clone(), receipt);
                    id
                });
                Ok(json!({"receiptId":receipt_id}))
            }
            Command::Undo { receipt_id } => {
                valid_token(&receipt_id)?;
                let receipt = self.receipts.get(&receipt_id).ok_or_else(stale)?;
                self.core.record_undo(receipt).map_err(core_error)?;
                self.receipts.remove(&receipt_id);
                Ok(json!({"undone":true}))
            }
            Command::Clear {
                generation,
                key_hex,
            } => {
                self.core
                    .clear(generation.0, key(&key_hex)?)
                    .map_err(core_error)?;
                self.captures.clear();
                self.receipts.clear();
                Ok(self.info())
            }
            Command::ExportCheckpoint {
                storage_key_hex,
                checkpoint,
                permission,
            } => {
                let snapshot = self
                    .core
                    .export_checkpoint(&key(&storage_key_hex)?, checkpoint.0, permission)
                    .map_err(storage_error)?;
                bound_completion_result(
                    json!({"snapshotJson":snapshot,"checkpoint":checkpoint.0.to_string()}),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn create(registry: &mut Registry) -> String {
        let line = json!({"params":{"schemaVersion":1,"scope":{"userSha256":"a".repeat(64),
            "projectSha256":null,"documentSha256":null,"generation":"0"},"keyHex":"ab".repeat(32)}})
        .to_string();
        registry
            .dispatch("formula.preferences.create", &line)
            .unwrap()["ownerId"]
            .as_str()
            .unwrap()
            .into()
    }
    #[test]
    fn close_all_erases_owners_without_resetting_identity_sequence() {
        let mut registry = Registry::default();
        let old = create(&mut registry);
        assert_eq!(registry.len(), 1);
        registry.close_all();
        assert_eq!(registry.len(), 0);
        let new = create(&mut registry);
        assert_ne!(old, new);
        let line = json!({"params":{"ownerId":old,"command":{"kind":"info"}}}).to_string();
        assert_eq!(
            registry
                .dispatch("formula.preferences.command", &line)
                .unwrap_err()
                .code,
            "PREFERENCE_NOT_FOUND"
        );
    }
    #[test]
    fn exhaustion_cannot_wrap_or_allocate_a_capture() {
        let mut registry = Registry::default();
        let owner = create(&mut registry);
        registry.next = u64::MAX;
        let line = json!({"params":{"ownerId":owner,"command":{"kind":"capture"}}}).to_string();
        assert_eq!(
            registry
                .dispatch("formula.preferences.command", &line)
                .unwrap_err()
                .code,
            "LIMIT_EXCEEDED"
        );
        assert_eq!(registry.entries[&owner].captures.len(), 0);
        assert_eq!(registry.next, u64::MAX);
    }
}
