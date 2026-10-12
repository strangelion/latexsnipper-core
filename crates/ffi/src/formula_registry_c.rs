//! Instance-local formula routing for trusted native application hosts.
//! Numeric handles are not credentials. No guest API grants human consent.
use std::collections::BTreeMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

use latexsnipper_conversion::conversion_registry::{
    ConversionFallbackPolicy, ConversionRegistryError, RegisteredConversionRequest,
    SemanticConversionRegistry, CONVERSION_REGISTRY_VERSION,
};
use latexsnipper_conversion::FormulaConversionMode;
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::application_c::{ffi_response, parse_request, AdapterError};

const MAX_HANDLES: usize = 64;
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CREATE_BYTES: usize = 32 * 1024;
const MAX_APPROVAL_BYTES: usize = 16 * 1024;
type Result<T> = std::result::Result<T, AdapterError>;

#[cfg(feature = "wasi-formula")]
type ProfileDeclaration = latexsnipper_plugin_wasi::WasiFormulaProfile;
#[cfg(not(feature = "wasi-formula"))]
type ProfileDeclaration = Value;

static HANDLES: Lazy<Mutex<Handles>> = Lazy::new(|| Mutex::new(Handles::default()));

#[derive(Default)]
struct Handles {
    next: u64,
    preparing: usize,
    entries: BTreeMap<u64, Arc<Entry>>,
}

struct Reservation(bool);

impl Reservation {
    fn acquire() -> Result<Self> {
        let mut handles = handles()?;
        if handles.entries.len() + handles.preparing >= MAX_HANDLES {
            return Err(error(
                "FORMULA_REGISTRY_LIMIT",
                "At most 64 formula registries are allowed.",
            ));
        }
        handles.preparing += 1;
        Ok(Self(true))
    }

    fn publish(mut self, entry: Entry) -> Result<u64> {
        let mut handles = handles()?;
        handles.next = handles
            .next
            .checked_add(1)
            .ok_or_else(|| error("FORMULA_REGISTRY_LIMIT", "Formula handle space exhausted."))?;
        let handle = handles.next;
        handles.entries.insert(handle, Arc::new(entry));
        handles.preparing -= 1;
        self.0 = false;
        Ok(handle)
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.0 {
            if let Ok(mut handles) = HANDLES.lock() {
                handles.preparing -= 1;
            }
        }
    }
}

struct Entry {
    registry: Mutex<SemanticConversionRegistry>,
    closed: AtomicBool,
    cancelled: AtomicBool,
    profile_sha256: Option<String>,
    profile_backend: Option<String>,
    #[cfg(feature = "wasi-formula")]
    cancellation: latexsnipper_plugin::CancellationToken,
    #[cfg(feature = "wasi-formula")]
    approvals: Option<Arc<latexsnipper_plugin_wasi::SensitiveActionController>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateRequest {
    schema_version: u16,
    #[serde(default)]
    plugin_store: Option<String>,
    #[serde(default)]
    profile: Option<ProfileDeclaration>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConvertRequest {
    schema_version: u16,
    input: String,
    output: String,
    mode: FormulaConversionMode,
    backend: Option<String>,
    content: String,
    context_sha256: String,
    #[serde(default)]
    policy: ConversionFallbackPolicy,
}

fn error(code: &str, message: &str) -> AdapterError {
    AdapterError::new(code, message, false)
}

fn registry_error(value: ConversionRegistryError) -> AdapterError {
    let code = serde_json::to_value(value.code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "INTERNAL".into());
    AdapterError::new(code, value.message, false)
}

fn handles() -> Result<std::sync::MutexGuard<'static, Handles>> {
    HANDLES
        .lock()
        .map_err(|_| error("INTERNAL", "Formula handle registry is unavailable."))
}

fn entry(handle: u64) -> Result<Arc<Entry>> {
    handles()?.entries.get(&handle).cloned().ok_or_else(|| {
        error(
            "FORMULA_REGISTRY_NOT_FOUND",
            "Formula registry handle is unknown or closed.",
        )
    })
}

fn bounded(value: Value) -> Result<Value> {
    let length = serde_json::to_vec(&value)
        .map_err(|_| error("INTERNAL", "Formula response cannot be serialized."))?
        .len();
    if length > MAX_RESPONSE_BYTES {
        return Err(error(
            "OUTPUT_TOO_LARGE",
            "Serialized formula data exceeds 2 MiB.",
        ));
    }
    Ok(value)
}

impl CreateRequest {
    fn build(self) -> Result<Entry> {
        if self.schema_version != CONVERSION_REGISTRY_VERSION {
            return Err(error(
                "VERSION_MISMATCH",
                "Unsupported formula registry schema.",
            ));
        }
        match (self.profile, self.plugin_store) {
            (None, None) => Ok(Entry::builtin()),
            (Some(profile), Some(root)) => {
                if root.is_empty() || root.len() > 4096 || root.chars().any(char::is_control) {
                    return Err(error(
                        "INVALID_ARGUMENT",
                        "Invalid existing plugin store path.",
                    ));
                }
                #[cfg(not(feature = "wasi-formula"))]
                {
                    let _ = (profile, root);
                    Err(error(
                        "FEATURE_NOT_COMPILED",
                        "WASI profiles require wasi-formula.",
                    ))
                }
                #[cfg(feature = "wasi-formula")]
                {
                    let bytes = serde_json::to_vec(&profile)
                        .map_err(|_| error("INVALID_JSON", "Profile cannot be serialized."))?;
                    let profile = latexsnipper_plugin_wasi::WasiFormulaProfile::from_json(&bytes)
                        .map_err(registry_error)?;
                    let store = latexsnipper_plugin::RemotePluginStore::new(root.into());
                    let cancellation = latexsnipper_plugin::CancellationToken::default();
                    let prepared = profile
                        .prepare_with_cancellation(&store, cancellation.clone())
                        .map_err(registry_error)?;
                    Ok(Entry {
                        registry: Mutex::new(prepared.registry),
                        profile_sha256: Some(prepared.profile_sha256),
                        profile_backend: Some(prepared.backend_id),
                        cancellation,
                        approvals: Some(prepared.approval_controller),
                        closed: AtomicBool::new(false),
                        cancelled: AtomicBool::new(false),
                    })
                }
            }
            _ => Err(error(
                "INVALID_ARGUMENT",
                "Profile and pluginStore must be supplied together.",
            )),
        }
    }
}

impl Entry {
    fn builtin() -> Self {
        Self {
            registry: Mutex::new(SemanticConversionRegistry::default()),
            closed: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            profile_sha256: None,
            profile_backend: None,
            #[cfg(feature = "wasi-formula")]
            cancellation: latexsnipper_plugin::CancellationToken::default(),
            #[cfg(feature = "wasi-formula")]
            approvals: None,
        }
    }
    fn check(&self) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(error(
                "FORMULA_REGISTRY_NOT_FOUND",
                "Formula registry is closed.",
            ));
        }
        if self.cancelled.load(Ordering::Acquire) {
            return Err(error(
                "FORMULA_REGISTRY_CANCELLED",
                "Create a new registry after cancellation.",
            ));
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, SemanticConversionRegistry>> {
        self.check()?;
        let registry = self.registry.try_lock().map_err(|value| match value {
            TryLockError::WouldBlock => {
                error("FORMULA_REGISTRY_BUSY", "Another operation is running.")
            }
            TryLockError::Poisoned(_) => error(
                "FORMULA_REGISTRY_POISONED",
                "Registry cannot continue after panic.",
            ),
        })?;
        self.check()?;
        Ok(registry)
    }

    fn stop(&self, close: bool) -> Result<()> {
        if close {
            self.closed.store(true, Ordering::Release);
        }
        self.cancelled.store(true, Ordering::Release);
        #[cfg(feature = "wasi-formula")]
        {
            self.cancellation.cancel();
            if let Some(approvals) = &self.approvals {
                approvals.clear().map_err(wasi_error)?;
            }
        }
        Ok(())
    }
}

// Fixed native compiler stack, not an increase to guest execution budgets.
fn worker<T: Send + 'static>(operation: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    std::thread::Builder::new()
        .name("snipper-formula-ffi".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(operation)
        .map_err(|_| error("INTERNAL", "Cannot start formula host worker."))?
        .join()
        .map_err(|_| error("INTERNAL", "Formula host panicked; no retry was made."))?
}

/// Create an instance-local registry, not an OCR session or package installer.
/// # Safety
/// The pointer must reference `length` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_registry_create(
    bytes: *const u8,
    length: usize,
) -> *mut c_char {
    ffi_response(|| {
        if length > MAX_CREATE_BYTES {
            return Err(error("INVALID_ARGUMENT", "Creation JSON exceeds 32 KiB."));
        }
        let request: CreateRequest = unsafe { parse_request(bytes, length) }?;
        // Reserve capacity without holding the global lock during compilation;
        // other instances remain cancellable and closable during activation.
        let reservation = Reservation::acquire()?;
        let prepared = if request.profile.is_some() {
            worker(move || request.build())?
        } else {
            request.build()?
        };
        let mut result = json!({ "abiVersion": 1, "schemaVersion": CONVERSION_REGISTRY_VERSION,
            "wasiFormulaCompiled": cfg!(feature = "wasi-formula"),
            "profileSha256": prepared.profile_sha256, "profileBackend": prepared.profile_backend });
        result["handle"] = json!(reservation.publish(prepared)?);
        Ok(result)
    })
}

/// Query formats/routes. The registered profile backend is initially disabled.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_registry_capabilities(handle: u64) -> *mut c_char {
    ffi_response(|| {
        let entry = entry(handle)?;
        let registry = entry.lock()?;
        bounded(json!({ "schemaVersion": CONVERSION_REGISTRY_VERSION,
            "coreVersion": env!("CARGO_PKG_VERSION"), "profileSha256": entry.profile_sha256,
            "formats": registry.extension_formats(), "routes": registry.capabilities() }))
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EnableRequest {
    backend: String,
    enabled: bool,
}

/// Explicit route enablement; does not enable a package or approve side effects.
/// # Safety
/// The pointer must reference `length` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_registry_set_enabled(
    handle: u64,
    bytes: *const u8,
    length: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: EnableRequest = unsafe { parse_request(bytes, length) }?;
        entry(handle)?
            .lock()?
            .set_backend_enabled(&request.backend, request.enabled)
            .map_err(registry_error)?;
        Ok(json!({ "backend": request.backend, "enabled": request.enabled }))
    })
}

/// Audited converted/rejected report. A candidate is never an Office write permit.
/// # Safety
/// The pointer must reference `length` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_registry_convert(
    handle: u64,
    bytes: *const u8,
    length: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: ConvertRequest = unsafe { parse_request(bytes, length) }?;
        request
            .policy
            .validate_for_backend(request.backend.as_deref())
            .map_err(registry_error)?;
        let entry = entry(handle)?;
        let needs_worker = entry.profile_backend.is_some();
        let operation = move || {
            let registry = entry.lock()?;
            let report = registry.convert_with_fallback(
                &RegisteredConversionRequest {
                    schema_version: request.schema_version,
                    input: request.input,
                    output: request.output,
                    mode: request.mode,
                    backend: request.backend,
                    content: request.content,
                    context_sha256: request.context_sha256,
                },
                &request.policy,
            );
            // Closing/cancelling a native call cannot undo work already entered,
            // but a completed candidate must not escape after observed closure.
            entry.check()?;
            bounded(
                serde_json::to_value(report)
                    .map_err(|_| error("INTERNAL", "Cannot serialize formula report."))?,
            )
        };
        if needs_worker {
            worker(operation)
        } else {
            operation()
        }
    })
}

/// Terminal cancellation, without acquiring the conversion mutex or retrying.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_registry_cancel(handle: u64) -> *mut c_char {
    ffi_response(|| {
        entry(handle)?.stop(false)?;
        Ok(json!({ "handle": handle, "cancelled": true }))
    })
}

/// Removes the handle and signals in-flight WASI cancellation/revocation.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_registry_close(handle: u64) -> *mut c_char {
    ffi_response(|| {
        let entry = handles()?.entries.remove(&handle).ok_or_else(|| {
            error(
                "FORMULA_REGISTRY_NOT_FOUND",
                "Formula registry handle is unknown or closed.",
            )
        })?;
        entry.stop(true)?;
        Ok(json!({ "handle": handle, "closed": true }))
    })
}

#[cfg(feature = "wasi-formula")]
fn wasi_error(value: latexsnipper_plugin_wasi::WasiDiagnostic) -> AdapterError {
    AdapterError::new(value.code.as_str(), value.message, false)
}

/// Sensitive request/grant snapshots, not approval. May contain sensitive paths.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_registry_approvals(handle: u64) -> *mut c_char {
    ffi_response(|| {
        let entry = entry(handle)?;
        entry.check()?;
        #[cfg(feature = "wasi-formula")]
        if let Some(approvals) = &entry.approvals {
            return bounded(json!({ "pending": approvals.pending().map_err(wasi_error)?,
                "grants": approvals.grants().map_err(wasi_error)? }));
        }
        Ok(json!({ "pending": [], "grants": [] }))
    })
}

#[cfg(feature = "wasi-formula")]
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum ApprovalCommand {
    Once {
        request_id: String,
    },
    Approve {
        request_id: String,
        scope: ApprovalScope,
        uses: u32,
        valid_for_ms: u64,
    },
    Batch {
        request_ids: Vec<String>,
        valid_for_ms: u64,
    },
    Revoke {
        request_id: String,
    },
    Clear,
}

#[cfg(feature = "wasi-formula")]
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum ApprovalScope {
    Exact,
    SameTarget { max_payload_bytes: usize },
}

/// Only the trusted application host may call after explicit user consent.
/// This is not a guest, MCP, plugin-controlled or automatic approval interface.
/// # Safety
/// The pointer must reference `length` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_registry_approval(
    handle: u64,
    bytes: *const u8,
    length: usize,
) -> *mut c_char {
    ffi_response::<Value>(|| {
        if length > MAX_APPROVAL_BYTES {
            return Err(error("INVALID_ARGUMENT", "Approval JSON exceeds 16 KiB."));
        }
        let entry = entry(handle)?;
        entry.check()?;
        #[cfg(not(feature = "wasi-formula"))]
        {
            let _ = (bytes, length);
            Err(error(
                "FEATURE_NOT_COMPILED",
                "Approvals require wasi-formula.",
            ))
        }
        #[cfg(feature = "wasi-formula")]
        {
            let command: ApprovalCommand = unsafe { parse_request(bytes, length) }?;
            let approvals = entry.approvals.as_ref().ok_or_else(|| {
                error(
                    "INVALID_ARGUMENT",
                    "Registry has no WASI approval controller.",
                )
            })?;
            match command {
                ApprovalCommand::Once { request_id } => approvals.approve_once(&request_id),
                ApprovalCommand::Approve {
                    request_id,
                    scope,
                    uses,
                    valid_for_ms,
                } => approvals.approve(
                    &request_id,
                    match scope {
                        ApprovalScope::Exact => latexsnipper_plugin_wasi::ApprovalScope::Exact,
                        ApprovalScope::SameTarget { max_payload_bytes } => {
                            latexsnipper_plugin_wasi::ApprovalScope::SameTarget {
                                max_payload_bytes,
                            }
                        }
                    },
                    uses,
                    std::time::Duration::from_millis(valid_for_ms),
                ),
                ApprovalCommand::Batch {
                    request_ids,
                    valid_for_ms,
                } => approvals
                    .approve_batch(&request_ids, std::time::Duration::from_millis(valid_for_ms)),
                ApprovalCommand::Revoke { request_id } => approvals.revoke(&request_id).map(|_| ()),
                ApprovalCommand::Clear => approvals.clear(),
            }
            .map_err(wasi_error)?;
            entry.check()?;
            Ok(json!({ "updated": true }))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparing_capacity_is_reserved_and_failed_creation_releases_it() {
        let mut reserved = Vec::new();
        for _ in 0..MAX_HANDLES {
            reserved.push(Reservation::acquire().unwrap());
        }
        assert!(Reservation::acquire().is_err());
        drop(reserved.pop());
        let replacement = Reservation::acquire().unwrap();
        drop(replacement);
        drop(reserved);
        assert_eq!(handles().unwrap().preparing, 0);
    }

    #[test]
    fn busy_registry_does_not_block_cancellation_or_close_checks() {
        let entry = Entry::builtin();
        let guard = entry.lock().unwrap();
        assert!(entry.lock().is_err());
        entry.stop(true).unwrap();
        assert!(entry.check().is_err());
        drop(guard);
        assert!(entry.lock().is_err());
    }
}
