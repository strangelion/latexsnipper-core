//! Bounded native preference owners. Handles are not permission credentials.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::c_char;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use latexsnipper_conversion::formula_completion::preferences::persistence::*;
use latexsnipper_conversion::formula_completion::{fragments::*, preferences::*, scoped::*};
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::{json, Value};

use crate::application_c::{ffi_response, parse_request, AdapterError};

#[cfg(all(windows, feature = "protected-preferences"))]
mod windows_storage;

const MAX_STORES: usize = 64;
const MAX_CAPTURES: usize = 128;
const MAX_METADATA_BYTES: usize = 4096;
const MAX_JSON_BYTES: usize = 1024 * 1024;
const MAX_STORAGE_OPEN_BYTES: usize = 16 * 1024;
const MAX_PREPARED_STAGES: usize = 4;
type Result<T> = std::result::Result<T, AdapterError>;
type Owner = Arc<Mutex<Option<State>>>;
static OWNERS: Lazy<Mutex<Owners>> = Lazy::new(|| Mutex::new(Owners::default()));
// Routing identities, not credentials. Global tokens prevent cross-owner aliasing.
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Owners {
    next: u64,
    entries: BTreeMap<u64, Owner>,
    reservations: BTreeSet<u64>,
}
struct State {
    core: OwnerCore,
    captures: BTreeMap<u64, FormulaPreferenceCapture>,
    receipts: BTreeMap<u64, FormulaPreferenceReceipt>,
}

enum OwnerCore {
    Memory(Box<FormulaPreferenceStore>),
    #[cfg(all(windows, feature = "protected-preferences"))]
    Protected(Box<windows_storage::ProtectedOwner>),
}
impl Deref for OwnerCore {
    type Target = FormulaPreferenceStore;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Memory(core) => core,
            #[cfg(all(windows, feature = "protected-preferences"))]
            Self::Protected(owner) => owner.session.preferences(),
        }
    }
}
impl DerefMut for OwnerCore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Memory(core) => core,
            #[cfg(all(windows, feature = "protected-preferences"))]
            Self::Protected(owner) => owner.session.preferences_mut(),
        }
    }
}
impl OwnerCore {
    fn is_protected(&self) -> bool {
        match self {
            Self::Memory(_) => false,
            #[cfg(all(windows, feature = "protected-preferences"))]
            Self::Protected(_) => true,
        }
    }
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
    // Keep closed decoding on unsupported builds, then return explicit
    // unavailable rather than pretending to process these platform fields.
    #[cfg_attr(not(all(windows, feature = "protected-preferences")), allow(dead_code))]
    StoragePrepare {
        permission: FormulaPreferenceStoragePermission,
    },
    #[cfg_attr(not(all(windows, feature = "protected-preferences")), allow(dead_code))]
    StoragePrepareClear {
        generation: Decimal,
    },
    #[cfg_attr(not(all(windows, feature = "protected-preferences")), allow(dead_code))]
    StorageCommit {
        stage_id: Decimal,
        permission: FormulaPreferenceStoragePermission,
    },
    #[cfg_attr(not(all(windows, feature = "protected-preferences")), allow(dead_code))]
    StorageReconcile {
        stage_id: Decimal,
        permission: FormulaPreferenceStoragePermission,
    },
    #[cfg_attr(not(all(windows, feature = "protected-preferences")), allow(dead_code))]
    StorageDiscardStage {
        stage_id: Decimal,
    },
    #[cfg_attr(not(all(windows, feature = "protected-preferences")), allow(dead_code))]
    StorageCleanup {
        maximum: u16,
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
        let value = json!({"schemaVersion":1, "scope":self.core.scope(), "policy":self.core.policy(),
            "eventCount":self.core.event_count(), "captureCount":self.captures.len(), "receiptCount":self.receipts.len(),
            "inMemory":true, "persistence":false, "authenticatedCheckpoints":true,
            "automaticStorage":false, "automaticCapture":false});
        #[cfg(all(windows, feature = "protected-preferences"))]
        if let OwnerCore::Protected(owner) = &self.core {
            let mut value = value;
            value["persistence"] = json!(true);
            value["protectedStorage"] = owner.info();
            return value;
        }
        value
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
                if self.core.is_protected() {
                    return Err(error(
                        "PREFERENCE_STORAGE_MANAGED",
                        "Use an explicit protected clear transaction.",
                    ));
                }
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
                if self.core.is_protected() {
                    return Err(error(
                        "PREFERENCE_STORAGE_MANAGED",
                        "Protected owners do not expose keys or raw checkpoint export.",
                    ));
                }
                let snapshot = self
                    .core
                    .export_checkpoint(&key(&storage_key_hex)?, checkpoint.0, permission)
                    .map_err(storage_error)?;
                bounded(json!({"snapshotJson":snapshot,"checkpoint":checkpoint.0.to_string()}))
            }
            command @ (Command::StoragePrepare { .. }
            | Command::StoragePrepareClear { .. }
            | Command::StorageCommit { .. }
            | Command::StorageReconcile { .. }
            | Command::StorageDiscardStage { .. }
            | Command::StorageCleanup { .. }) => {
                #[cfg(all(windows, feature = "protected-preferences"))]
                if let OwnerCore::Protected(owner) = &mut self.core {
                    let outcome = owner.execute(command)?;
                    if outcome.cleared {
                        self.captures.clear();
                        self.receipts.clear();
                    }
                    let mut value = outcome.value;
                    value["info"] = self.info();
                    return bounded(value);
                }
                let _ = command;
                Err(error(
                    "PREFERENCE_STORAGE_UNAVAILABLE",
                    "This owner has no enabled protected storage provider.",
                ))
            }
        }
    }
}

impl Owners {
    fn reserve(&mut self) -> Result<u64> {
        if self.entries.len() + self.reservations.len() >= MAX_STORES {
            return Err(error(
                "LIMIT_EXCEEDED",
                "Preference owner budget exhausted.",
            ));
        }
        let handle = self
            .next
            .checked_add(1)
            .ok_or_else(|| error("LIMIT_EXCEEDED", "Preference handle space exhausted."))?;
        self.next = handle;
        self.reservations.insert(handle);
        Ok(handle)
    }
}
#[cfg(any(test, all(windows, feature = "protected-preferences")))]
struct Reservation {
    handle: u64,
    live: bool,
}
#[cfg(any(test, all(windows, feature = "protected-preferences")))]
impl Drop for Reservation {
    fn drop(&mut self) {
        if self.live {
            // Registry operations contain no external IO/callbacks. Cleanup must
            // release a reservation even during short-lived registry contention.
            if let Ok(mut owners) = OWNERS.lock() {
                owners.reservations.remove(&self.handle);
            }
        }
    }
}
#[cfg(any(test, all(windows, feature = "protected-preferences")))]
fn register_with(create: impl FnOnce() -> Result<OwnerCore>) -> Result<Value> {
    let mut reservation = Reservation {
        handle: lock(&OWNERS)?.reserve()?,
        live: true,
    };
    // Reserve quota before provider IO, without holding the registry lock during
    // native calls. A failed factory never exposes a guessed/reserved handle.
    let state = State {
        core: create()?,
        captures: BTreeMap::new(),
        receipts: BTreeMap::new(),
    };
    let info = state.info();
    let mut owners = OWNERS
        .lock()
        .map_err(|_| error("INTERNAL", "Preference registry is unavailable."))?;
    owners.reservations.remove(&reservation.handle);
    owners
        .entries
        .insert(reservation.handle, Arc::new(Mutex::new(Some(state))));
    reservation.live = false;
    Ok(json!({"handle":reservation.handle.to_string(), "info":info}))
}
fn register(core: FormulaPreferenceStore) -> Result<Value> {
    // Volatile create/restore retain the original prompt BUSY contract. Only a
    // factory that has already performed native provisioning needs the short
    // blocking publication section in register_with.
    let state = State {
        core: OwnerCore::Memory(Box::new(core)),
        captures: BTreeMap::new(),
        receipts: BTreeMap::new(),
    };
    let info = state.info();
    let mut owners = lock(&OWNERS)?;
    let handle = owners.reserve()?;
    owners.reservations.remove(&handle);
    owners
        .entries
        .insert(handle, Arc::new(Mutex::new(Some(state))));
    Ok(json!({"handle":handle.to_string(), "info":info}))
}

/// Discover owner and explicit checkpoint support. No automatic disk storage.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_preferences_capabilities() -> *mut c_char {
    ffi_response(|| {
        Ok::<_, AdapterError>(
            json!({"schemaVersion":1, "inMemory":true, "persistence":cfg!(all(windows, feature="protected-preferences")),
        "authenticatedCheckpoints":true,"automaticStorage":false,
        "maxSnapshotBytes":FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES,"storagePermissionRequired":true,
        "automaticCapture":false, "maxStores":MAX_STORES, "maxCaptures":MAX_CAPTURES,
        "maxEvents":FORMULA_PREFERENCE_MAX_EVENTS, "maxMetadataBytes":MAX_METADATA_BYTES,
        "maxRequestBytes":MAX_JSON_BYTES, "maxResultDataBytes":MAX_JSON_BYTES,
        "idEncoding":"canonical-decimal-u64-string", "defaultEnabled":false,
        "protectedStorage":{"available":cfg!(all(windows, feature="protected-preferences")),
            "provider":"windows-dpapi","maxOpenRequestBytes":MAX_STORAGE_OPEN_BYTES,
            "maxPreparedStages":MAX_PREPARED_STAGES,"keysReturned":false}}),
        )
    })
}

/// Explicit trusted-host Windows storage factory. Never use as an untrusted
/// guest RPC or substitute its request flags for current user consent.
/// # Safety
/// When supported, pointer must reference length readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_preferences_open_windows(
    pointer: *const u8,
    length: usize,
) -> *mut c_char {
    ffi_response(|| {
        #[cfg(all(windows, feature = "protected-preferences"))]
        {
            let request: windows_storage::Open =
                unsafe { decode(pointer, length, MAX_STORAGE_OPEN_BYTES) }?;
            register_with(|| {
                request
                    .open()
                    .map(|owner| OwnerCore::Protected(Box::new(owner)))
            })
        }
        #[cfg(not(all(windows, feature = "protected-preferences")))]
        {
            let _ = (pointer, length);
            Err::<Value, _>(error(
                "PREFERENCE_STORAGE_UNAVAILABLE",
                "Windows protected preference storage is not enabled in this build/platform.",
            ))
        }
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

/// Volatile owners perform no storage IO. Explicit storage commands on a managed
/// owner may perform synchronous native IO. Capture before dispatch; record only
/// after actual host acceptance. Failed feedback must not roll back an edit.
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
    static REGISTRY_TESTS: Mutex<()> = Mutex::new(());
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
        let _serial = REGISTRY_TESTS.lock().unwrap();
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

    #[test]
    fn factory_reservation_is_private_and_does_not_hold_global_lock_during_io() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        let _serial = REGISTRY_TESTS.lock().unwrap();
        let make_core = || {
            FormulaPreferenceStore::new(
                FormulaDefinitionScope {
                    user_sha256: "a".repeat(64),
                    project_sha256: None,
                    document_sha256: None,
                    generation: 0,
                },
                [0xab; 32],
            )
            .unwrap()
        };
        let registered = register(make_core()).unwrap();
        let existing = registered["handle"].as_str().unwrap().parse().unwrap();
        let (ready_tx, ready_rx) = channel();
        let (release_tx, release_rx) = channel();
        let worker = std::thread::spawn(move || {
            register_with(|| {
                ready_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| error("INTERNAL", "Fixture release was not received."))?;
                Ok(OwnerCore::Memory(Box::new(make_core())))
            })
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Another live owner remains queryable while the factory is waiting.
        let retained = owner(existing).unwrap();
        assert!(lock(&retained)
            .unwrap()
            .as_ref()
            .unwrap()
            .info()
            .is_object());
        let reserved = {
            let owners = lock(&OWNERS).unwrap();
            assert_eq!(owners.reservations.len(), 1);
            *owners.reservations.first().unwrap()
        };
        assert!(owner(reserved).is_err());
        release_tx.send(()).unwrap();
        let created = worker.join().unwrap().unwrap();
        assert_eq!(created["handle"], reserved.to_string());
        assert!(lock(&OWNERS).unwrap().reservations.is_empty());
        // Test-only exact handles; free every returned C allocation.
        for handle in [existing, reserved] {
            let reply = latexsnipper_formula_preferences_close(handle);
            assert!(!reply.is_null());
            unsafe { crate::application_c::latexsnipper_string_free(reply) };
        }
    }
}
