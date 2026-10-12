use latexsnipper_ffi::application_c::latexsnipper_string_free;
use latexsnipper_ffi::formula_registry_c::*;
use serde_json::{json, Value};
use std::ffi::{c_char, CStr};

#[cfg(feature = "wasi-formula")]
#[path = "../../plugin-wasi/tests/support/formula_profile_store.rs"]
mod fixture;

fn take(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let value = unsafe { serde_json::from_slice(CStr::from_ptr(pointer).to_bytes()).unwrap() };
    unsafe { latexsnipper_string_free(pointer) };
    value
}

fn create(value: Value) -> Value {
    let bytes = serde_json::to_vec(&value).unwrap();
    take(unsafe { latexsnipper_formula_registry_create(bytes.as_ptr(), bytes.len()) })
}

fn request(source: &str) -> Value {
    json!({ "schemaVersion": 1, "input": "latex", "output": "omml",
        "mode": "strict", "backend": null, "content": source, "contextSha256": "a".repeat(64) })
}

fn convert(handle: u64, request: Value) -> Value {
    let bytes = serde_json::to_vec(&request).unwrap();
    take(unsafe { latexsnipper_formula_registry_convert(handle, bytes.as_ptr(), bytes.len()) })
}

fn enable(handle: u64, backend: &str, enabled: bool) -> Value {
    let bytes = serde_json::to_vec(&json!({"backend": backend, "enabled": enabled})).unwrap();
    take(unsafe { latexsnipper_formula_registry_set_enabled(handle, bytes.as_ptr(), bytes.len()) })
}

#[test]
fn actual_c_calls_have_audited_fallback_identity_and_lifecycle() {
    let result = create(json!({"schemaVersion": 1}));
    assert_eq!(result["ok"], true);
    let handle = result["data"]["handle"].as_u64().unwrap();
    let capabilities = take(latexsnipper_formula_registry_capabilities(handle));
    assert_eq!(capabilities["ok"], true);
    assert!(capabilities["data"]["routes"].as_array().unwrap().len() >= 144);
    assert_eq!(
        take(latexsnipper_formula_registry_approvals(handle))["data"]["pending"],
        json!([])
    );
    let direct = convert(handle, request(r"\frac{1}{2}"));
    assert_eq!(direct["data"]["outcome"]["kind"], "converted");
    assert!(direct["data"]["outcome"]["result"]["candidate"]["content"]
        .as_str()
        .unwrap()
        .contains("<m:f>"));
    let mut fallback = request("x^2");
    fallback["backend"] = json!("missing:backend");
    fallback["policy"] = json!({"schemaVersion":1,"backends":["core:builtin"]});
    let result = convert(handle, fallback.clone());
    assert_eq!(result["data"]["outcome"]["kind"], "converted");
    assert_eq!(result["data"]["attempts"].as_array().unwrap().len(), 2);
    fallback["content"] = json!("y^3");
    let altered = convert(handle, fallback);
    assert_ne!(
        result["data"]["requestSha256"],
        altered["data"]["requestSha256"]
    );
    assert_ne!(
        result["data"]["decisionSha256"],
        altered["data"]["decisionSha256"]
    );
    assert_eq!(enable(handle, "missing:backend", true)["ok"], false);
    assert_eq!(
        take(latexsnipper_formula_registry_cancel(handle))["ok"],
        true
    );
    assert_eq!(
        convert(handle, request("x"))["error"]["code"],
        "FORMULA_REGISTRY_CANCELLED"
    );
    assert_eq!(
        take(latexsnipper_formula_registry_close(handle))["ok"],
        true
    );
    assert_eq!(
        take(latexsnipper_formula_registry_close(handle))["error"]["code"],
        "FORMULA_REGISTRY_NOT_FOUND"
    );
    assert_eq!(
        convert(handle, request("x"))["error"]["code"],
        "FORMULA_REGISTRY_NOT_FOUND"
    );
}

#[test]
fn c_json_rejects_unknown_versions_fields_shapes_and_null_before_reading() {
    for value in [
        json!({"schemaVersion":2}),
        json!({"schemaVersion":1,"allowAll":true}),
        json!({"schemaVersion":1,"pluginStore":"not-opened"}),
        json!({"schemaVersion":1,"profile":{}}),
    ] {
        assert_eq!(create(value)["ok"], false);
    }
    for length in [0, 1, 32 * 1024 + 1] {
        assert_eq!(
            take(unsafe { latexsnipper_formula_registry_create(std::ptr::null(), length) })["ok"],
            false
        );
    }
    let handle = create(json!({"schemaVersion":1}))["data"]["handle"]
        .as_u64()
        .unwrap();
    for (source, context, expected) in [
        ("x", "bad".to_string(), "INVALID_DECLARATION"),
        ("x", "a".repeat(64), "LIMIT_EXCEEDED"),
    ] {
        let mut value = request(source);
        value["contextSha256"] = json!(context);
        if expected == "LIMIT_EXCEEDED" {
            value["content"] = json!("x".repeat(64 * 1024 + 1));
        }
        let result = convert(handle, value);
        assert_eq!(result["data"]["outcome"]["kind"], "rejected");
        assert!(result["data"]["attempts"].as_array().unwrap().is_empty());
        assert_eq!(result["data"]["outcome"]["error"]["code"], expected);
    }
    let mut value = request("x");
    value["ignored"] = json!(true);
    assert_eq!(convert(handle, value)["error"]["code"], "INVALID_JSON");
    assert_eq!(
        take(latexsnipper_formula_registry_close(handle))["ok"],
        true
    );
}

#[cfg(not(feature = "wasi-formula"))]
#[test]
fn default_build_refuses_profile_without_store_io() {
    let result = create(json!({"schemaVersion":1,"pluginStore":"absent-store","profile":{}}));
    assert_eq!(result["error"]["code"], "FEATURE_NOT_COMPILED");
}

#[cfg(feature = "wasi-formula")]
#[test]
fn nested_profile_duplicate_fields_are_not_erased_before_validation() {
    let bytes = br#"{"schemaVersion":1,"pluginStore":"absent-store","profile":{"schemaVersion":1,"schemaVersion":2}}"#;
    let result = take(unsafe { latexsnipper_formula_registry_create(bytes.as_ptr(), bytes.len()) });
    assert_eq!(result["error"]["code"], "INVALID_JSON");
}

#[cfg(feature = "wasi-formula")]
#[test]
fn real_c_profile_reuses_guest_checks_default_deny_revocation_and_export_shape() {
    let (temporary, store) = fixture::store();
    let root = temporary.path().join("store");
    for exporter in [false, true] {
        let created = create(
            json!({"schemaVersion":1,"pluginStore":root,"profile":fixture::profile(exporter)}),
        );
        assert_eq!(created["ok"], true, "{created}");
        let handle = created["data"]["handle"].as_u64().unwrap();
        let mut value = request(if exporter { "$$z^7$$" } else { "x:2" });
        value["input"] = json!(if exporter { "latex" } else { "fixture:power" });
        value["output"] = json!(if exporter { "fixture:power" } else { "omml" });
        value["mode"] = json!("best-effort");
        value["backend"] = json!("fixture:profile");
        assert_eq!(
            convert(handle, value.clone())["data"]["outcome"]["error"]["code"],
            "BACKEND_DISABLED"
        );
        assert_eq!(enable(handle, "fixture:profile", true)["ok"], true);
        for source in if exporter {
            ["$$z^7$$", "x^2"]
        } else {
            ["x:2", "y:4"]
        } {
            value["content"] = json!(source);
            let result = convert(handle, value.clone());
            assert_eq!(result["data"]["outcome"]["kind"], "converted", "{result}");
            let candidate = result["data"]["outcome"]["result"]["candidate"]["content"]
                .as_str()
                .unwrap();
            if exporter {
                assert_eq!(candidate, if source == "x^2" { "x:2" } else { "z:7" });
            } else {
                assert!(candidate.contains("m:sSup"));
            }
        }
        let command = serde_json::to_vec(&json!({"kind":"once","requestId":"unknown"})).unwrap();
        assert_eq!(
            take(unsafe {
                latexsnipper_formula_registry_approval(handle, command.as_ptr(), command.len())
            })["ok"],
            false
        );
        assert_eq!(
            take(latexsnipper_formula_registry_approvals(handle))["data"]["grants"],
            json!([])
        );
        value["policy"] = json!({"schemaVersion":1,"backends":["core:builtin"]});
        store.set_enabled("fixture.component", false).unwrap();
        let revoked = convert(handle, value.clone());
        assert_eq!(revoked["data"]["outcome"]["kind"], "rejected");
        assert_eq!(revoked["data"]["attempts"].as_array().unwrap().len(), 1);
        assert_eq!(
            take(latexsnipper_formula_registry_close(handle))["ok"],
            true
        );
        store.set_enabled("fixture.component", true).unwrap();
    }
    drop(store);
    temporary.close().unwrap();
}

#[cfg(feature = "wasi-formula")]
#[test]
fn real_c_linear_profiles_use_the_same_canonical_projection_and_ownership() {
    let (temporary, store) = fixture::store();
    let root = temporary.path().join("store");
    for (exporter, format, source, expected) in [
        (false, "ascii-math", "y:4", "y^(4)"),
        (true, "unicode-math", "〖x〗^(2)", "x:2"),
    ] {
        let mut profile = fixture::profile(exporter);
        profile["operation"] = if exporter {
            json!({"kind":"exporter","input":format})
        } else {
            json!({"kind":"importer","output":format})
        };
        let created = create(json!({"schemaVersion":1,"pluginStore":root,"profile":profile}));
        assert_eq!(created["ok"], true, "{created}");
        let handle = created["data"]["handle"].as_u64().unwrap();
        let mut value = request(source);
        value["input"] = json!(if exporter { format } else { "fixture:power" });
        value["output"] = json!(if exporter { "fixture:power" } else { format });
        value["mode"] = json!("best-effort");
        value["backend"] = json!("fixture:profile");
        assert_eq!(
            convert(handle, value.clone())["data"]["outcome"]["error"]["code"],
            "BACKEND_DISABLED"
        );
        assert_eq!(enable(handle, "fixture:profile", true)["ok"], true);
        let result = convert(handle, value);
        assert_eq!(result["data"]["outcome"]["kind"], "converted", "{result}");
        assert_eq!(
            result["data"]["outcome"]["result"]["candidate"]["content"],
            expected
        );
        assert_eq!(
            take(latexsnipper_formula_registry_approvals(handle))["data"]["grants"],
            json!([])
        );
        assert_eq!(
            take(latexsnipper_formula_registry_close(handle))["ok"],
            true
        );
    }
    drop(store);
    temporary.close().unwrap();
}
