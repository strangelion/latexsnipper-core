//! Additive, stateless C transport for the shared Core editing API.
use std::ffi::c_char;

use latexsnipper_conversion::formula_completion::{
    scoped::{
        ScopedFormulaCompletionApplyRequest, ScopedFormulaCompletionError,
        ScopedFormulaCompletionRequest, FORMULA_COMPLETION_MAX_DEFINITIONS,
        FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS, SCOPED_FORMULA_COMPLETION_VERSION,
    },
    FormulaCompletionApplyRequest, FormulaCompletionError, FormulaCompletionRequest,
    FORMULA_COMPLETION_MAX_BYTES, FORMULA_COMPLETION_MAX_CANDIDATES, FORMULA_COMPLETION_MAX_DEPTH,
    FORMULA_COMPLETION_MAX_NODES, FORMULA_COMPLETION_VERSION,
};
use serde::Serialize;
use serde_json::{json, Value};

use crate::application_c::{ffi_response, parse_request, AdapterError};

const MAX_COMPLETION_DATA_BYTES: usize = 1024 * 1024;

fn completion_error(error: FormulaCompletionError) -> AdapterError {
    let code = match error {
        FormulaCompletionError::InvalidCursor => "INVALID_CURSOR",
        FormulaCompletionError::LimitExceeded => "LIMIT_EXCEEDED",
        FormulaCompletionError::StaleContext => "STALE_CONTEXT",
        FormulaCompletionError::UnknownCandidate => "UNKNOWN_CANDIDATE",
    };
    AdapterError::new(code, error.to_string(), false)
}

fn scoped_completion_error(error: ScopedFormulaCompletionError) -> AdapterError {
    let code = match error {
        ScopedFormulaCompletionError::InvalidCatalog => "INVALID_CATALOG",
        ScopedFormulaCompletionError::InvalidCursor => "INVALID_CURSOR",
        ScopedFormulaCompletionError::LimitExceeded => "LIMIT_EXCEEDED",
        ScopedFormulaCompletionError::StaleContext => "STALE_CONTEXT",
        ScopedFormulaCompletionError::UnknownCandidate => "UNKNOWN_CANDIDATE",
    };
    AdapterError::new(code, error.to_string(), false)
}

fn bound_data(data: impl Serialize) -> Result<Value, AdapterError> {
    let value = serde_json::to_value(data)
        .map_err(|_| AdapterError::new("INTERNAL", "Completion serialization failed.", false))?;
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| AdapterError::new("INTERNAL", "Completion serialization failed.", false))?;
    if bytes.len() > MAX_COMPLETION_DATA_BYTES {
        return Err(AdapterError::new(
            "OUTPUT_TOO_LARGE",
            "Serialized completion data exceeds the 1 MiB limit.",
            false,
        ));
    }
    Ok(value)
}

/// Discover the completion grammar and limits, without creating a session.
/// Free the v3 JSON envelope once with `latexsnipper_string_free`.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_completion_capabilities() -> *mut c_char {
    ffi_response(|| {
        Ok::<_, AdapterError>(json!({
            "schemaVersion": FORMULA_COMPLETION_VERSION,
            "inputFormats": ["latex"],
            "offsetEncoding": "utf-8-bytes",
            "revisionEncoding": "canonical-decimal-u64-string",
            "maxInputBytes": FORMULA_COMPLETION_MAX_BYTES,
            "maxNodes": FORMULA_COMPLETION_MAX_NODES,
            "maxDepthExclusive": FORMULA_COMPLETION_MAX_DEPTH,
            "maxCandidates": FORMULA_COMPLETION_MAX_CANDIDATES,
            "maxResultDataBytes": MAX_COMPLETION_DATA_BYTES
        }))
    })
}

/// Query Core candidate edits. No files, sessions or host documents are changed.
/// Free the v3 JSON envelope once with `latexsnipper_string_free`.
///
/// # Safety
/// `request_json` must reference `request_json_len` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_complete(
    request_json: *const u8,
    request_json_len: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: FormulaCompletionRequest =
            unsafe { parse_request(request_json, request_json_len) }?;
        bound_data(request.execute().map_err(completion_error)?)
    })
}

/// Recheck a concise snapshot and return one canonical edit by candidate ID.
/// The host owns explicit acceptance, revision updates, undo and document writes.
/// Free the v3 JSON envelope once with `latexsnipper_string_free`.
///
/// # Safety
/// `request_json` must reference `request_json_len` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_completion_apply(
    request_json: *const u8,
    request_json_len: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: FormulaCompletionApplyRequest =
            unsafe { parse_request(request_json, request_json_len) }?;
        bound_data(request.execute().map_err(completion_error)?)
    })
}

/// Discover additive scoped completion; the v1 capability response is unchanged.
/// Free the v3 JSON envelope once with `latexsnipper_string_free`.
#[no_mangle]
pub extern "C" fn latexsnipper_formula_scoped_completion_capabilities() -> *mut c_char {
    ffi_response(|| {
        Ok::<_, AdapterError>(json!({
            "schemaVersion": SCOPED_FORMULA_COMPLETION_VERSION,
            "inputFormats": ["latex"],
            "offsetEncoding": "utf-8-bytes",
            "revisionEncoding": "canonical-decimal-u64-string",
            "scopeIdentityEncoding": "opaque-lowercase-sha256",
            "definitionRevisionEncoding": "canonical-decimal-u64-string",
            "generationEncoding": "canonical-decimal-u64-string",
            "maxInputBytes": FORMULA_COMPLETION_MAX_BYTES,
            "maxNodes": FORMULA_COMPLETION_MAX_NODES,
            "maxDepthExclusive": FORMULA_COMPLETION_MAX_DEPTH,
            "maxCandidates": FORMULA_COMPLETION_MAX_CANDIDATES,
            "maxResultDataBytes": MAX_COMPLETION_DATA_BYTES,
            "maxDefinitions": FORMULA_COMPLETION_MAX_DEFINITIONS,
            "maxDefinitionArguments": FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS,
            "macroExpansion": false,
            "persistence": false
        }))
    })
}

/// Query schema-v2 edits with host-authorized signatures, never macro bodies.
/// Free the v3 JSON envelope once with `latexsnipper_string_free`.
///
/// # Safety
/// `request_json` must reference `request_json_len` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_complete_scoped(
    request_json: *const u8,
    request_json_len: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: ScopedFormulaCompletionRequest =
            unsafe { parse_request(request_json, request_json_len) }?;
        bound_data(request.execute().map_err(scoped_completion_error)?)
    })
}

/// Recheck current scope, definitions and editor state before returning an edit.
/// The host owns authorization, reset generations, acceptance and document writes.
/// Free the v3 JSON envelope once with `latexsnipper_string_free`.
///
/// # Safety
/// `request_json` must reference `request_json_len` readable bytes for this call.
#[no_mangle]
pub unsafe extern "C" fn latexsnipper_formula_completion_apply_scoped(
    request_json: *const u8,
    request_json_len: usize,
) -> *mut c_char {
    ffi_response(|| {
        let request: ScopedFormulaCompletionApplyRequest =
            unsafe { parse_request(request_json, request_json_len) }?;
        bound_data(request.execute().map_err(scoped_completion_error)?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application_c::latexsnipper_string_free;

    unsafe fn take_json(pointer: *mut c_char) -> Value {
        assert!(!pointer.is_null());
        let result =
            serde_json::from_slice(unsafe { std::ffi::CStr::from_ptr(pointer) }.to_bytes());
        unsafe { latexsnipper_string_free(pointer) };
        result.unwrap()
    }

    fn call(
        function: unsafe extern "C" fn(*const u8, usize) -> *mut c_char,
        params: Value,
    ) -> Value {
        let bytes = serde_json::to_vec(&params).unwrap();
        unsafe { take_json(function(bytes.as_ptr(), bytes.len())) }
    }

    #[test]
    fn c_completion_roundtrips_precise_revisions_and_canonical_edits() {
        let capabilities = unsafe { take_json(latexsnipper_formula_completion_capabilities()) };
        assert_eq!(capabilities["ok"], true);
        assert_eq!(capabilities["data"]["inputFormats"], json!(["latex"]));
        assert_eq!(
            capabilities["data"]["maxResultDataBytes"],
            MAX_COMPLETION_DATA_BYTES
        );
        for _ in 0..32 {
            let params = json!({"source":"中\\fr", "cursor":6, "inputFormat":"latex", "revision":"18446744073709551615"});
            let queried = call(latexsnipper_formula_complete, params.clone());
            assert_eq!(queried["ok"], true, "{queried}");
            assert_eq!(
                queried["data"]["completion"]["context"]["revision"],
                "18446744073709551615"
            );
            let mut apply = params;
            apply["snapshot"] = queried["data"]["snapshot"].clone();
            apply["candidateId"] = json!("builtin:frac");
            let applied = call(latexsnipper_formula_completion_apply, apply.clone());
            assert_eq!(applied["ok"], true, "{applied}");
            assert_eq!(applied["data"]["source"], "中\\frac{}{}");
            assert_eq!(applied["data"]["cursor"], 9);
            apply["revision"] = json!("0");
            assert_eq!(
                call(latexsnipper_formula_completion_apply, apply)["error"]["code"],
                "STALE_CONTEXT"
            );
        }
    }

    #[test]
    fn c_completion_refuses_bad_buffers_params_limits_and_unknown_candidates() {
        for function in [
            latexsnipper_formula_complete,
            latexsnipper_formula_completion_apply,
            latexsnipper_formula_complete_scoped,
            latexsnipper_formula_completion_apply_scoped,
        ] {
            for length in [0, 1, 1024 * 1024 + 1] {
                let response = unsafe { take_json(function(std::ptr::null(), length)) };
                assert_eq!(response["error"]["code"], "INVALID_ARGUMENT");
            }
            for bytes in [b"\xff".as_slice(), b"{}", b"{\"source\":\"x\"}"] {
                let response = unsafe { take_json(function(bytes.as_ptr(), bytes.len())) };
                assert_eq!(response["error"]["code"], "INVALID_JSON");
            }
        }
        let params = json!({"source":"中\\fr", "cursor":6, "inputFormat":"latex", "revision":"0"});
        for (field, value, code) in [
            ("revision", json!(0), "INVALID_JSON"),
            ("revision", json!("01"), "INVALID_JSON"),
            ("inputFormat", json!("typst"), "INVALID_JSON"),
            ("insert", json!("untrusted"), "INVALID_JSON"),
            ("cursor", json!(1), "INVALID_CURSOR"),
            ("source", json!("x".repeat(65_537)), "LIMIT_EXCEEDED"),
        ] {
            let mut invalid = params.clone();
            invalid[field] = value;
            assert_eq!(
                call(latexsnipper_formula_complete, invalid)["error"]["code"],
                code
            );
        }
        let queried = call(latexsnipper_formula_complete, params.clone());
        let mut invalid = params;
        invalid["snapshot"] = queried["data"]["snapshot"].clone();
        invalid["candidateId"] = json!("external:untrusted");
        assert_eq!(
            call(latexsnipper_formula_completion_apply, invalid)["error"]["code"],
            "UNKNOWN_CANDIDATE"
        );
    }

    #[test]
    fn scoped_c_completion_binds_definition_content_scope_and_reset_epoch() {
        let caps = unsafe { take_json(latexsnipper_formula_scoped_completion_capabilities()) };
        assert_eq!(caps["data"]["schemaVersion"], 2);
        assert_eq!(caps["data"]["maxDefinitions"], 64);
        assert_eq!(
            unsafe { take_json(latexsnipper_formula_completion_capabilities()) }["data"]
                ["schemaVersion"],
            1
        );
        let params = json!({"source": "中\\cust", "cursor": 8, "revision": "18446744073709551615", "inputFormat": "latex",
            "catalog": {"scope": {"userSha256": "a".repeat(64), "projectSha256": "b".repeat(64),
                "documentSha256": "c".repeat(64), "generation": "18446744073709551615"},
                "definitions": [{"id": "pair", "revision": "18446744073709551615", "userSha256": "a".repeat(64),
                    "projectSha256": "b".repeat(64), "command": "custompair", "arguments": ["math", "text"]}]}});
        for _ in 0..32 {
            let reply = call(latexsnipper_formula_complete_scoped, params.clone());
            assert_eq!(reply["ok"], true, "{reply}");
            assert_eq!(
                reply["data"]["candidates"][0]["definition"]["revision"],
                "18446744073709551615"
            );
            let mut apply = params.clone();
            apply["snapshot"] = reply["data"]["snapshot"].clone();
            apply["candidateId"] = json!("definition:pair");
            let accepted = call(latexsnipper_formula_completion_apply_scoped, apply.clone());
            assert_eq!(accepted["data"]["source"], "中\\custompair{}{}");
            assert_eq!(accepted["data"]["tabStops"].as_array().unwrap().len(), 2);
            for field in ["generation", "project", "signature", "candidate"] {
                let mut invalid = apply.clone();
                match field {
                    "generation" => invalid["catalog"]["scope"]["generation"] = json!("0"),
                    "project" => {
                        invalid["catalog"]["scope"]["projectSha256"] = json!("d".repeat(64))
                    }
                    "signature" => invalid["catalog"]["definitions"][0]["arguments"] = json!([]),
                    _ => invalid["candidateId"] = json!("unknown"),
                }
                let expected = if field == "candidate" {
                    "UNKNOWN_CANDIDATE"
                } else {
                    "STALE_CONTEXT"
                };
                assert_eq!(
                    call(latexsnipper_formula_completion_apply_scoped, invalid)["error"]["code"],
                    expected
                );
            }
        }
        for (field, value, code) in [
            ("command", json!("mysymbol2"), "INVALID_CATALOG"),
            ("command", json!("frac"), "INVALID_CATALOG"),
            (
                "arguments",
                json!(["math", "math", "math", "math", "math"]),
                "LIMIT_EXCEEDED",
            ),
            ("body", json!(r"\input{private}"), "INVALID_JSON"),
            ("revision", json!(42), "INVALID_JSON"),
        ] {
            let mut invalid = params.clone();
            invalid["catalog"]["definitions"][0][field] = value;
            assert_eq!(
                call(latexsnipper_formula_complete_scoped, invalid)["error"]["code"],
                code
            );
        }
        let mut foreign = params;
        foreign["catalog"]["scope"]["userSha256"] = json!("d".repeat(64));
        let reply = call(latexsnipper_formula_complete_scoped, foreign);
        assert_eq!(reply["data"]["candidates"], json!([]));
        assert!(!reply.to_string().contains("custompair"));
    }

    #[test]
    fn c_completion_budget_counts_serialized_data_not_plain_string_length() {
        let overhead = serde_json::to_vec(&json!({"source":""})).unwrap().len();
        assert!(
            bound_data(json!({"source":"x".repeat(MAX_COMPLETION_DATA_BYTES - overhead)})).is_ok()
        );
        assert!(
            bound_data(json!({"source":"x".repeat(MAX_COMPLETION_DATA_BYTES - overhead + 1)}))
                .is_err()
        );
        assert!(bound_data(json!({"source":"\"".repeat(MAX_COMPLETION_DATA_BYTES / 2)})).is_err());
    }
}
