//! Bounded native preference owners. Handles are not permission credentials.
use std::collections::BTreeMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use latexsnipper_conversion::formula_completion::preferences::persistence::*;
use latexsnipper_conversion::formula_completion::{fragments::*, preferences::*, scoped::*};
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::{json, Value};

use crate::application_c::{ffi_response, parse_request, AdapterError};

const MAX_STORES: usize = 64;
const MAX_CAPTURES: usize = 128;
const MAX_METADATA_BYTES: usize = 4096;
const MAX_JSON_BYTES: usize = 1024 * 1024;
type Result<T> = std::result::Result<T, AdapterError>;
type Owner = Arc<Mutex<Option<State>>>;
static OWNERS: Lazy<Mutex<Owners>> = Lazy::new(|| Mutex::new(Owners::default()));
// Routing identities, not credentials. Global tokens prevent cross-owner aliasing.
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Owners {
    next: u64,
    entries: BTreeMap<u64, Owner>,
}
struct State {
    core: FormulaPreferenceStore,
    captures: BTreeMap<u64, FormulaPreferenceCapture>,
    receipts: BTreeMap<u64, FormulaPreferenceReceipt>,
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

struct Decimal(u64);
impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty()
            || value.len() > 20
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|byte| byte.is_ascii_digit())
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
        capture_id: Decimal,
    },
    RankFragments {
        request: Box<FormulaFragmentCompletionRequest>,
    },
    RecordAcceptance {
        capture_id: Decimal,
        event_id: Decimal,
        request: Box<FormulaFragmentCompletionApplyRequest>,
    },
    Undo {
        receipt_id: Decimal,
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

fn error(code: &str, message: &str) -> AdapterError {
    AdapterError::new(code, message, code == "PREFERENCE_BUSY")
}
fn core_error(value: FormulaPreferenceError) -> AdapterError {
    let code = match value {
        FormulaPreferenceError::InvalidIdentity => "INVALID_ARGUMENT",
        FormulaPreferenceError::ScopeMismatch => "SCOPE_MISMATCH",
        FormulaPreferenceError::StaleFeedback => "STALE_FEEDBACK",
        FormulaPreferenceError::LimitExceeded => "LIMIT_EXCEEDED",
        FormulaPreferenceError::Completion(value) => match value {
            ScopedFormulaCompletionError::InvalidCatalog => "INVALID_CATALOG",
            ScopedFormulaCompletionError::InvalidCursor => "INVALID_CURSOR",
            ScopedFormulaCompletionError::LimitExceeded => "LIMIT_EXCEEDED",
            ScopedFormulaCompletionError::StaleContext => "STALE_CONTEXT",
            ScopedFormulaCompletionError::UnknownCandidate => "UNKNOWN_CANDIDATE",
        },
    };
    error(code, &value.to_string())
}
fn storage_error(value: FormulaPreferenceStorageError) -> AdapterError {
    let code = match value {
        FormulaPreferenceStorageError::Disabled => "PREFERENCE_STORAGE_DISABLED",
        FormulaPreferenceStorageError::InvalidKey => "INVALID_ARGUMENT",
        FormulaPreferenceStorageError::InvalidSnapshot => "INVALID_SNAPSHOT",
        FormulaPreferenceStorageError::AuthenticationFailed => "AUTHENTICATION_FAILED",
        FormulaPreferenceStorageError::ScopeMismatch => "SCOPE_MISMATCH",
        FormulaPreferenceStorageError::StaleCheckpoint => "STALE_CHECKPOINT",
        FormulaPreferenceStorageError::SensitiveStorageDenied => "SENSITIVE_STORAGE_DENIED",
        FormulaPreferenceStorageError::LimitExceeded => "LIMIT_EXCEEDED",
    };
    error(code, &value.to_string())
}
fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    value.try_lock().map_err(|cause| match cause {
        TryLockError::WouldBlock => error(
            "PREFERENCE_BUSY",
            "Preference owner is busy; no operation was replayed.",
        ),
        TryLockError::Poisoned(_) => error("INTERNAL", "Preference owner is unavailable."),
    })
}
fn owner(handle: u64) -> Result<Owner> {
    lock(&OWNERS)?.entries.get(&handle).cloned().ok_or_else(|| {
        error(
            "PREFERENCE_NOT_FOUND",
            "Preference handle is unknown or closed.",
        )
    })
}
fn key(encoded: &str) -> Result<[u8; 32]> {
    let digit = |byte| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    };
    if encoded.len() != 64 {
        return Err(error(
            "INVALID_ARGUMENT",
            "Preference keyHex must encode 32 bytes as lowercase hexadecimal.",
        ));
    }
    let mut key = [0; 32];
    for (index, pair) in encoded.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        key[index] = digit(pair[0])
            .zip(digit(pair[1]))
            .map(|(high, low)| high * 16 + low)
            .ok_or_else(|| error("INVALID_ARGUMENT", "Invalid preference key encoding."))?;
    }
    Ok(key)
}
// Redact serde diagnostics: even unknown field names may be private metadata.
unsafe fn decode<T: DeserializeOwned>(pointer: *const u8, length: usize, max: usize) -> Result<T> {
    if pointer.is_null() || length == 0 || length > max {
        return Err(error(
            "INVALID_ARGUMENT",
            "Preference JSON buffer is null, empty or over budget.",
        ));
    }
    unsafe { parse_request(pointer, length) }.map_err(|_| {
        error(
            "INVALID_JSON",
            "Invalid closed preference JSON or revision encoding.",
        )
    })
}
fn bounded(value: Value) -> Result<Value> {
    if serde_json::to_vec(&value)
        .map_err(|_| error("INTERNAL", "Preference serialization failed."))?
        .len()
        > MAX_JSON_BYTES
    {
        return Err(error("OUTPUT_TOO_LARGE", "Preference data exceeds 1 MiB."));
    }
    Ok(value)
}
fn stale() -> AdapterError {
    error(
        "STALE_FEEDBACK",
        "Feedback token is unknown, consumed or invalidated.",
    )
}

fn next_token(counter: &AtomicU64) -> Result<u64> {
    // Keep the 1.88 MSRV without using fetch_update (deprecated in 1.99) or
    // its newer replacement. Relaxed ordering only allocates routing identities.
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .ok_or_else(|| error("LIMIT_EXCEEDED", "Preference token space exhausted."))?;
        match counter.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Ok(next),
            Err(actual) => current = actual,
        }
    }
}

impl State {
    fn info(&self) -> Value {
        json!({"schemaVersion":1, "scope":self.core.scope(), "policy":self.core.policy(),
            "eventCount":self.core.event_count(), "captureCount":self.captures.len(), "receiptCount":self.receipts.len(),
            "inMemory":true, "persistence":false, "authenticatedCheckpoints":true,
            "automaticStorage":false, "automaticCapture":false})
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
                    return Err(error(
                        "LIMIT_EXCEEDED",
                        "Pending preference capture budget exhausted.",
                    ));
                }
                let token = next_token(&NEXT_TOKEN)?;
                self.captures.insert(token, self.core.capture_context());
                Ok(json!({"captureId":token.to_string()}))
            }
            Command::DiscardCapture { capture_id } => {
                self.captures.remove(&capture_id.0).ok_or_else(stale)?;
                Ok(json!({"discarded":true}))
            }
            Command::RankFragments { request } => {
                let result = self.core.rank_fragments(&request).map_err(core_error)?;
                bounded(
                    json!({"completion":result.completion, "order":result.order, "features":result.features}),
                )
            }
            Command::RecordAcceptance {
                capture_id,
                event_id,
                request,
            } => {
                let capture = self.captures.get(&capture_id.0).ok_or_else(stale)?;
                // Reserve token space BEFORE Core records any feedback.
                let token = next_token(&NEXT_TOKEN)?;
                let receipt = self
                    .core
                    .record_acceptance_at(capture, &request, event_id.0)
                    .map_err(core_error)?;
                self.captures.remove(&capture_id.0);
                let id = receipt.map(|receipt| {
                    self.receipts.insert(token, receipt);
                    token.to_string()
                });
                Ok(json!({"receiptId":id}))
            }
            Command::Undo { receipt_id } => {
                let receipt = self.receipts.get(&receipt_id.0).ok_or_else(stale)?;
                self.core.record_undo(receipt).map_err(core_error)?;
                self.receipts.remove(&receipt_id.0);
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
                bounded(json!({"snapshotJson":snapshot,"checkpoint":checkpoint.0.to_string()}))
            }
        }
    }
}

fn register(core: FormulaPreferenceStore) -> Result<Value> {
    let state = State {
        core,
        captures: BTreeMap::new(),
        receipts: BTreeMap::new(),
    };
    let mut owners = lock(&OWNERS)?;
    if owners.entries.len() >= MAX_STORES {
        return Err(error(
            "LIMIT_EXCEEDED",
            "Preference owner budget exhausted.",
        ));
    }
    let handle = owners
        .next
        .checked_add(1)
        .ok_or_else(|| error("LIMIT_EXCEEDED", "Preference handle space exhausted."))?;
    let info = state.info();
    owners
        .entries
        .insert(handle, Arc::new(Mutex::new(Some(state))));
    owners.next = handle;
    Ok(json!({"handle":handle.to_string(), "info":info}))
}

/// Discover owner and explicit checkpoint support. No automatic disk storage.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_preferences_capabilities() -> *mut c_char {
    ffi_response(|| {
        Ok::<_, AdapterError>(
            json!({"schemaVersion":1, "inMemory":true, "persistence":false,
        "authenticatedCheckpoints":true,"automaticStorage":false,
        "maxSnapshotBytes":FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES,"storagePermissionRequired":true,
        "automaticCapture":false, "maxStores":MAX_STORES, "maxCaptures":MAX_CAPTURES,
        "maxEvents":FORMULA_PREFERENCE_MAX_EVENTS, "maxMetadataBytes":MAX_METADATA_BYTES,
        "maxRequestBytes":MAX_JSON_BYTES, "maxResultDataBytes":MAX_JSON_BYTES,
        "idEncoding":"canonical-decimal-u64-string", "defaultEnabled":false}),
        )
    })
}

/// Create a default-off owner. keyHex is private host key material; never log it.
/// # Safety
/// The buffer must contain `length` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_preferences_create(
    pointer: *const u8,
    length: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: Create = unsafe { decode(pointer, length, MAX_METADATA_BYTES) }?;
        if request.schema_version != 1 {
            return Err(error("VERSION_MISMATCH", "Unsupported preference schema."));
        }
        let core = FormulaPreferenceStore::new(request.scope, key(&request.key_hex)?)
            .map_err(core_error)?;
        register(core)
    })
}

/// Restore a NEW default-off owner. Scope/checkpoint must come from trusted host
/// freshness state, not from the snapshot. Never restores captures or receipts.
/// # Safety
/// The buffer must contain `length` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_preferences_restore(
    pointer: *const u8,
    length: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: Restore = unsafe { decode(pointer, length, MAX_JSON_BYTES) }?;
        if request.schema_version != 1 {
            return Err(error("VERSION_MISMATCH", "Unsupported preference schema."));
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
        register(core)
    })
}

/// Explicit commands return data only. Capture before dispatch; record only after
/// actual host acceptance. Failed feedback must not roll back a successful edit.
/// # Safety
/// The buffer must contain `length` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_preferences_command(
    handle: u64,
    pointer: *const u8,
    length: usize,
) -> *mut c_char {
    ffi_response(|| {
        let command: Command = unsafe { decode(pointer, length, MAX_JSON_BYTES) }?;
        if !matches!(
            &command,
            Command::RankFragments { .. } | Command::RecordAcceptance { .. }
        ) && length > MAX_METADATA_BYTES
        {
            return Err(error(
                "INVALID_ARGUMENT",
                "Preference metadata exceeds 4 KiB.",
            ));
        }
        let entry = owner(handle)?;
        let mut state = lock(&entry)?;
        state
            .as_mut()
            .ok_or_else(|| error("PREFERENCE_CLOSED", "Preference owner is closed."))?
            .execute(command)
    })
}

/// Close once; stale handles are never reused in this loaded library instance.
/// Busy returns without removing the owner, allowing explicit later close.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_preferences_close(handle: u64) -> *mut c_char {
    ffi_response(|| {
        let mut owners = lock(&OWNERS)?;
        let entry = owners.entries.get(&handle).cloned().ok_or_else(|| {
            error(
                "PREFERENCE_NOT_FOUND",
                "Preference handle is unknown or closed.",
            )
        })?;
        let mut state = lock(&entry)?;
        state.take();
        owners.entries.remove(&handle);
        Ok(json!({"handle":handle.to_string(), "closed":true}))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_counter_is_unique_and_refuses_wrap() {
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(next_token(&counter).unwrap(), u64::MAX);
        assert!(next_token(&counter).is_err());
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }
    #[test]
    fn token_counter_stays_unique_under_concurrent_allocation_and_exhaustion() {
        let counter = AtomicU64::new(0);
        let mut allocated = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        (0..128)
                            .map(|_| next_token(&counter).unwrap())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|worker| worker.join().unwrap())
                .collect::<Vec<_>>()
        });
        allocated.sort_unstable();
        assert_eq!(allocated, (1..=1024u64).collect::<Vec<_>>());
        let counter = AtomicU64::new(u64::MAX - 10);
        let mut final_tokens = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..32)
                .map(|_| scope.spawn(|| next_token(&counter).ok()))
                .collect();
            workers
                .into_iter()
                .filter_map(|worker| worker.join().unwrap())
                .collect::<Vec<_>>()
        });
        final_tokens.sort_unstable();
        assert_eq!(
            final_tokens,
            ((u64::MAX - 9)..=u64::MAX).collect::<Vec<_>>()
        );
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }
    #[test]
    fn lock_contention_never_waits_and_poison_does_not_reset_state() {
        let value = Mutex::new(42);
        let guard = lock(&value).unwrap();
        assert!(format!("{:?}", lock(&value).err().unwrap()).contains("PREFERENCE_BUSY"));
        drop(guard);
        let _ = std::panic::catch_unwind(|| {
            let _guard = value.lock().unwrap();
            panic!("fixture");
        });
        assert!(format!("{:?}", lock(&value).err().unwrap()).contains("INTERNAL"));
    }
    #[test]
    fn decimal_wire_accepts_all_u64_bits_but_not_json_numbers_or_alternate_forms() {
        assert_eq!(
            serde_json::from_str::<Decimal>(r#""18446744073709551615""#)
                .unwrap()
                .0,
            u64::MAX
        );
        for source in [
            "1",
            "true",
            r#""01""#,
            r#""+1""#,
            r#""1e3""#,
            r#""18446744073709551616""#,
        ] {
            assert!(serde_json::from_str::<Decimal>(source).is_err());
        }
    }

    #[test]
    fn busy_close_keeps_owner_and_successful_close_invalidates_retained_entry() {
        use crate::application_c::latexsnipper_string_free;
        use std::ffi::CStr;
        fn take(pointer: *mut c_char) -> Value {
            assert!(!pointer.is_null());
            let value = unsafe { serde_json::from_slice(CStr::from_ptr(pointer).to_bytes()) };
            unsafe { latexsnipper_string_free(pointer) };
            value.unwrap()
        }
        let request = json!({"schemaVersion":1, "scope":{"userSha256":"a".repeat(64),
            "projectSha256":null,"documentSha256":null,"generation":"0"},"keyHex":"ab".repeat(32)})
        .to_string();
        let created = take(unsafe {
            latexsnipper_formula_preferences_create(request.as_ptr(), request.len())
        });
        let handle = created["data"]["handle"].as_str().unwrap().parse().unwrap();
        let retained = owner(handle).unwrap();
        let guard = lock(&retained).unwrap();
        let busy = take(latexsnipper_formula_preferences_close(handle));
        assert_eq!(busy["error"]["code"], "PREFERENCE_BUSY");
        assert_eq!(busy["error"]["recoverable"], true);
        drop(guard);
        assert!(owner(handle).is_ok());
        assert_eq!(
            take(latexsnipper_formula_preferences_close(handle))["ok"],
            true
        );
        assert!(lock(&retained).unwrap().is_none());
        assert!(owner(handle).is_err());
    }
}
