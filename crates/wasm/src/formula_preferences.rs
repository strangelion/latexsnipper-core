//! Owned, volatile preference feedback shared by both WASM build profiles.
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use latexsnipper_api_types::{ApiEnvelopeV3, ApiErrorV3};
use latexsnipper_conversion::formula_completion::preferences::persistence::*;
use latexsnipper_conversion::formula_completion::{fragments::*, preferences::*, scoped::*};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

type Result<T> = std::result::Result<T, ApiErrorV3>;
const MAX_OWNERS: u32 = 64;
const MAX_CAPTURES: usize = 128;
const MAX_METADATA: usize = 4096;
const MAX_DATA: usize = 1024 * 1024;
thread_local! { static OWNERS: Cell<u32> = const { Cell::new(0) }; }

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
fn error(code: &str, message: &str) -> ApiErrorV3 {
    ApiErrorV3 {
        code: code.into(),
        message: message.into(),
        recoverable: code == "PREFERENCE_BUSY",
        details: None,
    }
}
fn invalid() -> ApiErrorV3 {
    error(
        "INVALID_JSON",
        "Invalid closed preference JSON or revision encoding.",
    )
}
fn stale() -> ApiErrorV3 {
    error(
        "STALE_FEEDBACK",
        "Feedback token is unknown, consumed or invalidated.",
    )
}
fn limit() -> ApiErrorV3 {
    error(
        "LIMIT_EXCEEDED",
        "Preference owner, capture or identity budget exhausted.",
    )
}
fn key(raw: &str) -> Result<[u8; 32]> {
    if raw.len() != 64 {
        return Err(invalid());
    }
    let digit = |b| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    };
    let mut key = [0; 32];
    for (i, pair) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        key[i] = digit(pair[0])
            .zip(digit(pair[1]))
            .map(|(a, b)| a * 16 + b)
            .ok_or_else(invalid)?;
    }
    Ok(key)
}
fn core_error(value: FormulaPreferenceError) -> ApiErrorV3 {
    match value {
        FormulaPreferenceError::Completion(value) => {
            crate::formula_completion::scoped_completion_error(value)
        }
        value => error(
            match value {
                FormulaPreferenceError::InvalidIdentity => "INVALID_ARGUMENT",
                FormulaPreferenceError::ScopeMismatch => "SCOPE_MISMATCH",
                FormulaPreferenceError::StaleFeedback => "STALE_FEEDBACK",
                FormulaPreferenceError::LimitExceeded => "LIMIT_EXCEEDED",
                _ => "INTERNAL",
            },
            &value.to_string(),
        ),
    }
}
fn storage_error(value: FormulaPreferenceStorageError) -> ApiErrorV3 {
    error(
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
        &value.to_string(),
    )
}
fn decode<T: serde::de::DeserializeOwned>(raw: &str, maximum: usize) -> Result<T> {
    if raw.len() > maximum {
        return Err(error(
            "INPUT_TOO_LARGE",
            "Preference JSON exceeds its transfer budget.",
        ));
    }
    serde_json::from_str(raw).map_err(|_| invalid())
}
fn response(result: Result<Value>) -> JsValue {
    let envelope = match result {
        Ok(data) => ApiEnvelopeV3::success(data, Vec::new()),
        Err(error) => ApiEnvelopeV3::failure(error, Vec::new()),
    };
    envelope
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or_else(|_| JsValue::from_str("Preference serialization failed."))
}
fn bound(value: Value) -> Result<Value> {
    if serde_json::to_vec(&value)
        .map_err(|_| error("INTERNAL", "Preference serialization failed."))?
        .len()
        > MAX_DATA
    {
        Err(error("OUTPUT_TOO_LARGE", "Preference data exceeds 1 MiB."))
    } else {
        Ok(value)
    }
}
fn namespace() -> Result<String> {
    // No user callback runs while a preference state or owner counter is borrowed.
    let unavailable = || {
        error(
            "PREFERENCE_UNAVAILABLE",
            "Web Crypto randomness is required for preference identity.",
        )
    };
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto"))
        .map_err(|_| unavailable())?;
    let function = js_sys::Reflect::get(&crypto, &JsValue::from_str("getRandomValues"))
        .map_err(|_| unavailable())?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| unavailable())?;
    let bytes = js_sys::Uint8Array::new_with_length(16);
    function.call1(&crypto, &bytes).map_err(|_| unavailable())?;
    Ok(bytes.to_vec().iter().map(|b| format!("{b:02x}")).collect())
}
struct State {
    core: FormulaPreferenceStore,
    namespace: String,
    next: u64,
    captures: BTreeMap<String, FormulaPreferenceCapture>,
    receipts: BTreeMap<String, FormulaPreferenceReceipt>,
}
impl State {
    fn next_id(&mut self) -> Result<String> {
        self.next = self.next.checked_add(1).ok_or_else(limit)?;
        Ok(format!("pref:{}:{}", self.namespace, self.next))
    }
    fn info(&self) -> Value {
        json!({"schemaVersion":1,"scope":self.core.scope(),"policy":self.core.policy(),
            "eventCount":self.core.event_count(),"captureCount":self.captures.len(),"receiptCount":self.receipts.len(),
            "inMemory":true,"persistence":false,"automaticCapture":false,
            "authenticatedCheckpoints":true,"automaticStorage":false})
    }
    fn execute(&mut self, command: Command) -> Result<Value> {
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
                let id = self.next_id()?;
                self.captures
                    .insert(id.clone(), self.core.capture_context());
                Ok(json!({"captureId":id}))
            }
            Command::DiscardCapture { capture_id } => {
                self.captures.remove(&capture_id).ok_or_else(stale)?;
                Ok(json!({"discarded":true}))
            }
            Command::RankFragments { request } => {
                let result = self.core.rank_fragments(&request).map_err(core_error)?;
                bound(
                    json!({"completion":result.completion,"order":result.order,"features":result.features}),
                )
            }
            Command::RecordAcceptance {
                capture_id,
                event_id,
                request,
            } => {
                let id = self.next_id()?;
                let capture = self.captures.get(&capture_id).ok_or_else(stale)?;
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
                bound(json!({"snapshotJson":snapshot,"checkpoint":checkpoint.0.to_string()}))
            }
        }
    }
}

/// A trusted host owns this object, consent, key lifecycle and explicit feedback.
#[wasm_bindgen]
pub struct FormulaPreferencesWasm {
    state: RefCell<Option<State>>,
    // The instance count and JS randomness belong to the creating JS agent.
    agent: std::marker::PhantomData<std::rc::Rc<()>>,
}
#[wasm_bindgen]
impl FormulaPreferencesWasm {
    pub fn command(&self, command_json: &str) -> JsValue {
        response((|| {
            let command: Command = decode(command_json, MAX_DATA)?;
            if !matches!(
                &command,
                Command::RankFragments { .. } | Command::RecordAcceptance { .. }
            ) && command_json.len() > MAX_METADATA
            {
                return Err(error(
                    "INPUT_TOO_LARGE",
                    "Preference metadata exceeds 4 KiB.",
                ));
            }
            let mut state = self
                .state
                .try_borrow_mut()
                .map_err(|_| error("PREFERENCE_BUSY", "Preference owner is busy."))?;
            state
                .as_mut()
                .ok_or_else(|| error("PREFERENCE_CLOSED", "Preference owner is closed."))?
                .execute(command)
        })())
    }
    pub fn close(&self) -> JsValue {
        response((|| {
            self.release()?;
            Ok(json!(true))
        })())
    }
}
impl FormulaPreferencesWasm {
    fn release(&self) -> Result<()> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| error("PREFERENCE_BUSY", "Preference owner is busy."))?;
        if state.take().is_some() {
            OWNERS.with(|count| count.set(count.get().saturating_sub(1)));
        }
        Ok(())
    }
}
impl Drop for FormulaPreferencesWasm {
    fn drop(&mut self) {
        if self.state.get_mut().take().is_some() {
            OWNERS.with(|count| count.set(count.get().saturating_sub(1)));
        }
    }
}
#[wasm_bindgen]
pub fn create_formula_preferences_v1(
    configuration_json: &str,
) -> std::result::Result<FormulaPreferencesWasm, JsValue> {
    let create = || -> Result<FormulaPreferencesWasm> {
        let request: Create = decode(configuration_json, MAX_METADATA)?;
        if request.schema_version != 1 {
            return Err(error("VERSION_MISMATCH", "Unsupported preference schema."));
        }
        if OWNERS.with(|count| count.get() >= MAX_OWNERS) {
            return Err(limit());
        }
        let core = FormulaPreferenceStore::new(request.scope, key(&request.key_hex)?)
            .map_err(core_error)?;
        own(core)
    };
    create().map_err(|value| response(Err(value)))
}

fn own(core: FormulaPreferenceStore) -> Result<FormulaPreferencesWasm> {
    if OWNERS.with(|count| count.get() >= MAX_OWNERS) {
        return Err(limit());
    }
    let namespace = namespace()?;
    // Web Crypto can invoke host code; recheck before reserving the owner slot.
    OWNERS.with(|count| {
        if count.get() >= MAX_OWNERS {
            return Err(limit());
        }
        count.set(count.get() + 1);
        Ok(())
    })?;
    Ok(FormulaPreferencesWasm {
        agent: std::marker::PhantomData,
        state: RefCell::new(Some(State {
            core,
            namespace,
            next: 0,
            captures: BTreeMap::new(),
            receipts: BTreeMap::new(),
        })),
    })
}

/// Explicitly restore a NEW default-off owner. The host must supply current
/// trusted scope/freshness and separate storage consent; no file IO or replay.
#[wasm_bindgen]
pub fn restore_formula_preferences_v1(
    configuration_json: &str,
) -> std::result::Result<FormulaPreferencesWasm, JsValue> {
    let restore = || -> Result<FormulaPreferencesWasm> {
        let request: Restore = decode(configuration_json, MAX_DATA)?;
        if request.schema_version != 1 {
            return Err(error("VERSION_MISMATCH", "Unsupported preference schema."));
        }
        if OWNERS.with(|count| count.get() >= MAX_OWNERS) {
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
        own(core)
    };
    restore().map_err(|value| response(Err(value)))
}
#[wasm_bindgen]
pub fn formula_preference_capabilities_v3() -> JsValue {
    response(Ok(
        json!({"schemaVersion":1,"defaultEnabled":false,"automaticCapture":false,"persistence":false,
        "authenticatedCheckpoints":true,"automaticStorage":false,"storagePermissionRequired":true,
        "maxSnapshotBytes":FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES,
        "maxOwners":MAX_OWNERS,"maxCaptures":MAX_CAPTURES,"maxEvents":FORMULA_PREFERENCE_MAX_EVENTS,
        "maxMetadataBytes":MAX_METADATA,"maxRequestBytes":MAX_DATA,"maxResultDataBytes":MAX_DATA,
        "revisionEncoding":"canonical-decimal-u64-string","identityEncoding":"opaque-instance-token"}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_json_refuses_duplicates_unknown_fields_and_lossy_numbers() {
        for input in [
            r#"{"kind":"info","ignored":true}"#,
            r#"{"kind":"capture","ignored":true}"#,
            r#"{"kind":"info","kind":"capture"}"#,
            r#"{"kind":"clear","generation":1,"keyHex":"ab"}"#,
            r#"{"kind":"clear","generation":"01","keyHex":"ab"}"#,
            r#"{"kind":"clear","generation":"18446744073709551616","keyHex":"ab"}"#,
        ] {
            assert!(decode::<Command>(input, MAX_DATA).is_err());
        }
        assert!(decode::<Command>(
            r#"{"kind":"clear","generation":"18446744073709551615","keyHex":"ab"}"#,
            MAX_DATA
        )
        .is_ok());
        assert!(key(&"AB".repeat(32)).is_err());
    }
}
