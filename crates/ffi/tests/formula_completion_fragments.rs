use latexsnipper_ffi::application_c::latexsnipper_string_free;
use latexsnipper_ffi::formula_completion_c::*;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr};

fn take(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let value = unsafe { serde_json::from_slice(CStr::from_ptr(pointer).to_bytes()) };
    unsafe { latexsnipper_string_free(pointer) };
    value.unwrap()
}

fn call(function: unsafe extern "C" fn(*const u8, usize) -> *mut c_char, request: Value) -> Value {
    let bytes = serde_json::to_vec(&request).unwrap();
    take(unsafe { function(bytes.as_ptr(), bytes.len()) })
}

fn request() -> Value {
    json!({"source":"中😀\\bay", "cursor":11, "revision":"18446744073709551615", "inputFormat":"latex",
        "catalog": {"commands": {"scope": {"userSha256":"a".repeat(64), "projectSha256":"b".repeat(64),
            "documentSha256":"c".repeat(64), "generation":"18446744073709551615"},
            "definitions":[{"id":"private-pair", "revision":"0", "userSha256":"a".repeat(64), "projectSha256":null,
                "command":"privatepair", "arguments":["math","text"]}]},
            "policy":{"retrievalEnabled":true,"sensitiveDocument":false,"allowInSensitiveDocument":false},
            "fragments":[{"id":"saved", "revision":"18446744073709551615", "userSha256":"a".repeat(64),
                "projectSha256":"b".repeat(64), "documentSha256":"c".repeat(64), "trigger":"bayes", "label":"Saved formula", "source":r"\frac{α}{}"}]}})
}

#[test]
fn c_fragments_reuse_string_ownership_and_keep_old_capabilities_unchanged() {
    let caps = take(latexsnipper_formula_fragment_completion_capabilities());
    assert_eq!(caps["data"]["schemaVersion"], 3);
    assert_eq!(caps["data"]["maxRequestBytes"], 1024 * 1024);
    assert_eq!(caps["data"]["maxResultDataBytes"], 1024 * 1024);
    assert_eq!(caps["data"]["maxFragments"], 64);
    assert_eq!(caps["data"]["maxFragmentBytes"], 8192);
    assert_eq!(caps["data"]["learning"], false);
    assert_eq!(caps["data"]["persistence"], false);
    assert_eq!(caps["data"]["macroExpansion"], false);
    assert_eq!(
        take(latexsnipper_formula_completion_capabilities())["data"]["schemaVersion"],
        1
    );
    assert_eq!(
        take(latexsnipper_formula_scoped_completion_capabilities())["data"]["schemaVersion"],
        2
    );
    for _ in 0..32 {
        let mut params = request();
        let result = call(latexsnipper_formula_complete_fragments, params.clone());
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(
            result["data"]["snapshot"]["revision"],
            "18446744073709551615"
        );
        assert_eq!(
            result["data"]["candidates"][0]["fragment"]["revision"],
            "18446744073709551615"
        );
        assert!(result["data"]["candidates"][0]["definition"].is_null());
        params["snapshot"] = result["data"]["snapshot"].clone();
        params["candidateId"] = json!("fragment:saved");
        let applied = call(latexsnipper_formula_completion_apply_fragments, params);
        assert_eq!(applied["data"]["source"], "中😀\\frac{α}{}");
        assert_eq!(applied["data"]["cursor"], "中😀\\frac{α}{".len());
        assert_eq!(applied["data"]["tabStops"], json!([{"start":17,"end":17}]));
    }
}

#[test]
fn c_fragments_filter_inactive_private_values_before_validation() {
    for field in ["user", "project", "document", "disabled", "sensitive"] {
        let mut params = request();
        params["catalog"]["fragments"][0]["label"] = json!("PRIVATE_SECRET");
        params["catalog"]["fragments"][0]["source"] = json!("x".repeat(8193));
        params["catalog"]["fragments"][0]["trigger"] = json!("invalid123");
        match field {
            "user" => params["catalog"]["fragments"][0]["userSha256"] = json!("d".repeat(64)),
            "project" => params["catalog"]["fragments"][0]["projectSha256"] = json!("d".repeat(64)),
            "document" => {
                params["catalog"]["fragments"][0]["documentSha256"] = json!("d".repeat(64))
            }
            "disabled" => params["catalog"]["policy"]["retrievalEnabled"] = json!(false),
            _ => params["catalog"]["policy"]["sensitiveDocument"] = json!(true),
        }
        let result = call(latexsnipper_formula_complete_fragments, params.clone());
        assert_eq!(result["data"]["candidates"], json!([]), "{field}: {result}");
        assert!(!result.to_string().contains("PRIVATE_SECRET"));
        params["source"] = json!(r"\fr");
        params["cursor"] = json!(3);
        let base = call(latexsnipper_formula_complete_fragments, params);
        assert!(base["data"]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["edit"]["id"] == "builtin:frac"));
    }
}

#[test]
fn c_fragment_acceptance_rechecks_every_current_scope_policy_and_content_guard() {
    let original = request();
    let result = call(latexsnipper_formula_complete_fragments, original.clone());
    let mut apply = original;
    apply["snapshot"] = result["data"]["snapshot"].clone();
    apply["candidateId"] = json!("fragment:saved");
    for field in [
        "revision",
        "source",
        "cursor",
        "generation",
        "user",
        "project",
        "document",
        "body",
        "fragment-revision",
        "definition",
        "disabled",
        "sensitive",
        "allow-sensitive",
        "clear",
        "unknown",
    ] {
        let mut changed = apply.clone();
        match field {
            "revision" => changed["revision"] = json!("0"),
            "source" => changed["source"] = json!("中😀\\bax"),
            "cursor" => changed["cursor"] = json!(10),
            "generation" => changed["catalog"]["commands"]["scope"]["generation"] = json!("0"),
            "user" => changed["catalog"]["commands"]["scope"]["userSha256"] = json!("d".repeat(64)),
            "project" => {
                changed["catalog"]["commands"]["scope"]["projectSha256"] = json!("d".repeat(64))
            }
            "document" => {
                changed["catalog"]["commands"]["scope"]["documentSha256"] = json!("d".repeat(64))
            }
            "body" => changed["catalog"]["fragments"][0]["source"] = json!("x+1"),
            "fragment-revision" => changed["catalog"]["fragments"][0]["revision"] = json!("0"),
            "definition" => {
                changed["catalog"]["commands"]["definitions"][0]["arguments"] = json!([])
            }
            "disabled" => changed["catalog"]["policy"]["retrievalEnabled"] = json!(false),
            "sensitive" => changed["catalog"]["policy"]["sensitiveDocument"] = json!(true),
            "allow-sensitive" => {
                changed["catalog"]["policy"]["allowInSensitiveDocument"] = json!(true)
            }
            "clear" => changed["catalog"]["fragments"] = json!([]),
            _ => changed["candidateId"] = json!("fragment:unknown"),
        }
        let failed = call(latexsnipper_formula_completion_apply_fragments, changed);
        assert_eq!(
            failed["error"]["code"],
            if field == "unknown" {
                "UNKNOWN_CANDIDATE"
            } else {
                "STALE_CONTEXT"
            },
            "{field}: {failed}"
        );
    }
    let mut invalid = apply.clone();
    invalid["cursor"] = json!(1);
    assert_eq!(
        call(latexsnipper_formula_completion_apply_fragments, invalid)["error"]["code"],
        "INVALID_CURSOR"
    );
    let mut v1 = apply.clone();
    v1.as_object_mut().unwrap().remove("catalog");
    v1["snapshot"]
        .as_object_mut()
        .unwrap()
        .remove("catalogSha256");
    assert_eq!(
        call(latexsnipper_formula_completion_apply, v1)["error"]["code"],
        "STALE_CONTEXT"
    );
    apply["catalog"] = apply["catalog"]["commands"].clone();
    apply["snapshot"]["definitionsSha256"] = apply["snapshot"]["catalogSha256"].clone();
    apply["snapshot"]
        .as_object_mut()
        .unwrap()
        .remove("catalogSha256");
    assert_eq!(
        call(latexsnipper_formula_completion_apply_scoped, apply)["error"]["code"],
        "STALE_CONTEXT"
    );
}

#[test]
fn c_fragment_transport_refuses_bad_buffers_schema_and_active_templates() {
    for function in [
        latexsnipper_formula_complete_fragments,
        latexsnipper_formula_completion_apply_fragments,
    ] {
        for length in [0, 1, 1024 * 1024 + 1] {
            let failed = take(unsafe { function(std::ptr::null(), length) });
            assert_eq!(failed["error"]["code"], "INVALID_ARGUMENT");
        }
        for bytes in [
            b"\xff".as_slice(),
            b"{}",
            b"{\"revision\":\"0\",\"revision\":\"1\"}",
        ] {
            let failed = take(unsafe { function(bytes.as_ptr(), bytes.len()) });
            assert_eq!(failed["error"]["code"], "INVALID_JSON");
        }
        let oversized = vec![b' '; 1024 * 1024 + 1];
        assert_eq!(
            take(unsafe { function(oversized.as_ptr(), oversized.len()) })["error"]["code"],
            "INVALID_ARGUMENT"
        );
    }
    for (field, value, code) in [
        ("revision", json!(42), "INVALID_JSON"),
        ("revision", json!("01"), "INVALID_JSON"),
        ("inputFormat", json!("typst"), "INVALID_JSON"),
        ("insert", json!("untrusted"), "INVALID_JSON"),
        ("source", json!("x".repeat(65_537)), "LIMIT_EXCEEDED"),
    ] {
        let mut invalid = request();
        invalid[field] = value;
        assert_eq!(
            call(latexsnipper_formula_complete_fragments, invalid)["error"]["code"],
            code
        );
    }
    for (source, code) in [
        (r"\frac{", "INVALID_CATALOG"),
        (r"$$x$$", "INVALID_CATALOG"),
        ("x % trailing", "INVALID_CATALOG"),
        (&"x".repeat(8193), "LIMIT_EXCEEDED"),
    ] {
        let mut invalid = request();
        invalid["catalog"]["fragments"][0]["source"] = json!(source);
        assert_eq!(
            call(latexsnipper_formula_complete_fragments, invalid)["error"]["code"],
            code
        );
    }
}

#[test]
fn c_fragment_output_limit_is_not_a_partial_response_and_next_call_still_works() {
    let mut params = request();
    let mut entry = params["catalog"]["fragments"][0].clone();
    entry["source"] = json!(format!("x %{}\n", "\"".repeat(8188)));
    params["catalog"]["fragments"] = json!((0..60)
        .map(|index| {
            let mut entry = entry.clone();
            entry["id"] = json!(format!("entry-{index}"));
            entry
        })
        .collect::<Vec<_>>());
    let source = format!("{}\\bay", "x".repeat(3900));
    params["source"] = json!(source);
    params["cursor"] = json!(source.len());
    assert!(serde_json::to_vec(&params).unwrap().len() <= 1024 * 1024);
    let limited = call(latexsnipper_formula_complete_fragments, params);
    assert_eq!(limited["error"]["code"], "OUTPUT_TOO_LARGE", "{limited}");
    assert!(limited.get("data").is_none());
    assert_eq!(
        call(latexsnipper_formula_complete_fragments, request())["ok"],
        true
    );
}
