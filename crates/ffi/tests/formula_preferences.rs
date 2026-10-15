use latexsnipper_ffi::{
    application_c::latexsnipper_string_free,
    formula_completion_c::latexsnipper_formula_complete_fragments, formula_preferences_c::*,
};
use serde_json::{json, Value};
use std::ffi::{c_char, CStr};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());
fn take(pointer: *mut c_char) -> Value {
    assert!(!pointer.is_null());
    let parsed = unsafe { serde_json::from_slice(CStr::from_ptr(pointer).to_bytes()) };
    unsafe { latexsnipper_string_free(pointer) };
    parsed.unwrap()
}
fn create_json() -> Value {
    json!({"schemaVersion":1,"scope":{"userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":"c".repeat(64),"generation":"0"},"keyHex":"ab".repeat(32)})
}
fn create(value: Value) -> Value {
    let bytes = value.to_string();
    take(unsafe { latexsnipper_formula_preferences_create(bytes.as_ptr(), bytes.len()) })
}
struct Owner(u64);
impl Owner {
    fn new() -> Self {
        let reply = create(create_json());
        assert_eq!(reply["ok"], true, "{reply}");
        Self(reply["data"]["handle"].as_str().unwrap().parse().unwrap())
    }
    fn call(&self, value: Value) -> Value {
        let bytes = value.to_string();
        take(unsafe {
            latexsnipper_formula_preferences_command(self.0, bytes.as_ptr(), bytes.len())
        })
    }
    fn capture(&self) -> Value {
        let reply = self.call(json!({"kind":"capture"}));
        assert_eq!(reply["ok"], true, "{reply}");
        reply["data"]["captureId"].clone()
    }
    fn enable(&self, sensitive: bool) {
        assert_eq!(self.call(json!({"kind":"set-policy","policy":{"rankingEnabled":true,"feedbackEnabled":true,"allowSensitiveFeedback":sensitive}}))["ok"], true);
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        take(latexsnipper_formula_preferences_close(self.0));
    }
}
fn request() -> Value {
    json!({"source":"中😀\\loc","cursor":11,"revision":"18446744073709551615","inputFormat":"latex",
        "catalog":{"commands":{"scope":create_json()["scope"],"definitions":[]},
        "policy":{"retrievalEnabled":true,"sensitiveDocument":false,"allowInSensitiveDocument":false},
        "fragments":[{"id":"first","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localfirst","label":"First","source":"x"},
        {"id":"second","revision":"18446744073709551615","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localsecond","label":"Second","source":r"\frac{α}{}"}]}})
}
fn accepted(input: &Value) -> Value {
    let bytes = input.to_string();
    let reply =
        take(unsafe { latexsnipper_formula_complete_fragments(bytes.as_ptr(), bytes.len()) });
    assert_eq!(reply["ok"], true, "{reply}");
    let mut accepted = input.clone();
    accepted["snapshot"] = reply["data"]["snapshot"].clone();
    accepted["candidateId"] = json!("fragment:second");
    accepted
}
fn rank(owner: &Owner, request: &Value) -> Value {
    owner.call(json!({"kind":"rank-fragments","request":request}))
}
fn record(owner: &Owner, capture: Value, request: &Value, event: &str) -> Value {
    owner.call(
        json!({"kind":"record-acceptance","captureId":capture,"eventId":event,"request":request}),
    )
}

fn export(owner: &Owner, checkpoint: &str, sensitive: bool) -> Value {
    owner.call(json!({"kind":"export-checkpoint", "storageKeyHex":"cd".repeat(32),
        "checkpoint":checkpoint,"permission":{"storageEnabled":true,"allowSensitiveStorage":sensitive}}))
}
fn restore_request(snapshot: &str, checkpoint: &str) -> Value {
    let mut value = create_json();
    value["storageKeyHex"] = json!("cd".repeat(32));
    value["expectedCheckpoint"] = json!(checkpoint);
    value["snapshotJson"] = json!(snapshot);
    value["permission"] = json!({"storageEnabled":true,"allowSensitiveStorage":false});
    value
}
fn restore(value: Value) -> Value {
    let bytes = value.to_string();
    take(unsafe { latexsnipper_formula_preferences_restore(bytes.as_ptr(), bytes.len()) })
}

#[test]
fn c_checkpoint_roundtrip_is_default_off_precise_and_never_recovers_feedback_tokens() {
    let _serial = SERIAL.lock().unwrap();
    let original = Owner::new();
    original.enable(false);
    let input = request();
    let accept = accepted(&input);
    let old_capture = original.capture();
    let old_receipt =
        record(&original, original.capture(), &accept, "9")["data"]["receiptId"].clone();
    let receipt = record(&original, original.capture(), &accept, "10")["data"]["receiptId"].clone();
    assert_eq!(owner_undo(&original, receipt)["ok"], true);
    let exported = export(&original, "18446744073709551615", false);
    assert_eq!(exported["ok"], true, "{exported}");
    assert_eq!(exported["data"]["checkpoint"], "18446744073709551615");
    let snapshot = exported["data"]["snapshotJson"].as_str().unwrap();
    for private in [
        "fragment:second",
        "Second",
        "keyHex",
        "captureId",
        "receiptId",
        r"\frac",
    ] {
        assert!(!snapshot.contains(private));
    }
    let reply = restore(restore_request(snapshot, "18446744073709551615"));
    assert_eq!(reply["ok"], true, "{reply}");
    let restored = Owner(reply["data"]["handle"].as_str().unwrap().parse().unwrap());
    assert_ne!(restored.0, original.0);
    let info = &reply["data"]["info"];
    assert_eq!(info["eventCount"], 2);
    assert_eq!(info["captureCount"], 0);
    assert_eq!(info["receiptCount"], 0);
    assert_eq!(info["authenticatedCheckpoints"], true);
    assert_eq!(
        info["policy"],
        json!({"rankingEnabled":false,"feedbackEnabled":false,"allowSensitiveFeedback":false})
    );
    assert_eq!(rank(&restored, &input)["data"]["order"], json!([0, 1]));
    restored.enable(false);
    assert_eq!(
        rank(&restored, &input)["data"]["features"][1],
        json!({"accepted":2,"undone":1,"lastAcceptedEvent":"9"})
    );
    assert_eq!(
        owner_undo(&restored, old_receipt)["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(
        record(&restored, old_capture, &accept, "11")["error"]["code"],
        "STALE_FEEDBACK"
    );
    let capture = restored.capture();
    assert_eq!(
        record(&restored, capture.clone(), &accept, "10")["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(record(&restored, capture, &accept, "11")["ok"], true);
    assert_eq!(
        export(&restored, "18446744073709551615", false)["error"]["code"],
        "STALE_CHECKPOINT"
    );
    assert_eq!(
        export(&original, "18446744073709551615", false)["error"]["code"],
        "STALE_CHECKPOINT"
    );
}

#[test]
fn c_checkpoint_requires_separate_storage_consent_and_trusted_scope_freshness() {
    let _serial = SERIAL.lock().unwrap();
    let owner = Owner::new();
    owner.enable(true);
    let mut input = request();
    input["catalog"]["policy"]["sensitiveDocument"] = json!(true);
    input["catalog"]["policy"]["allowInSensitiveDocument"] = json!(true);
    assert_eq!(
        record(&owner, owner.capture(), &accepted(&input), "1")["ok"],
        true
    );
    // Disabling learning does not remove the sensitive-storage consent gate.
    owner.call(json!({"kind":"set-policy","policy":{"rankingEnabled":false,"feedbackEnabled":false,"allowSensitiveFeedback":false}}));
    assert_eq!(
        export(&owner, "1", false)["error"]["code"],
        "SENSITIVE_STORAGE_DENIED"
    );
    let exported = export(&owner, "1", true);
    assert_eq!(exported["ok"], true);
    let snapshot = exported["data"]["snapshotJson"].as_str().unwrap();
    let base = restore_request(snapshot, "1");
    assert_eq!(
        restore(base.clone())["error"]["code"],
        "SENSITIVE_STORAGE_DENIED"
    );
    let mut allowed = base;
    allowed["permission"]["allowSensitiveStorage"] = json!(true);
    for (field, value, code) in [
        (
            "permission",
            json!({"storageEnabled":false,"allowSensitiveStorage":true}),
            "PREFERENCE_STORAGE_DISABLED",
        ),
        ("storageKeyHex", json!("ab".repeat(32)), "INVALID_ARGUMENT"),
        (
            "storageKeyHex",
            json!("ef".repeat(32)),
            "AUTHENTICATION_FAILED",
        ),
        ("keyHex", json!("ef".repeat(32)), "AUTHENTICATION_FAILED"),
        ("expectedCheckpoint", json!("2"), "STALE_CHECKPOINT"),
    ] {
        let mut invalid = allowed.clone();
        invalid[field] = value;
        assert_eq!(restore(invalid)["error"]["code"], code);
    }
    let mut changed = allowed.clone();
    changed["scope"]["generation"] = json!("1");
    assert_eq!(restore(changed)["error"]["code"], "SCOPE_MISMATCH");
    let mut tampered: Value = serde_json::from_str(snapshot).unwrap();
    tampered["body"]["events"][0]["undone"] = json!(true);
    let mut changed = allowed.clone();
    changed["snapshotJson"] = json!(tampered.to_string());
    assert_eq!(restore(changed)["error"]["code"], "AUTHENTICATION_FAILED");
    let valid = restore(allowed);
    assert_eq!(valid["ok"], true);
    let restored = Owner(valid["data"]["handle"].as_str().unwrap().parse().unwrap());
    assert_eq!(
        restored.call(json!({"kind":"info"}))["data"]["eventCount"],
        1
    );
}

#[test]
fn c_checkpoint_wire_rejects_coercion_duplicate_fields_and_oversize_without_leaking_data() {
    let _serial = SERIAL.lock().unwrap();
    let owner = Owner::new();
    let caps = take(latexsnipper_formula_preferences_capabilities());
    assert_eq!(caps["data"]["authenticatedCheckpoints"], true);
    assert_eq!(caps["data"]["maxSnapshotBytes"], 256 * 1024);
    assert_eq!(caps["data"]["persistence"], false);
    let exported = export(&owner, "1", false);
    let snapshot = exported["data"]["snapshotJson"].as_str().unwrap();
    let base = restore_request(snapshot, "1");
    for (field, value, code) in [
        ("expectedCheckpoint", json!(1), "INVALID_JSON"),
        ("expectedCheckpoint", json!("01"), "INVALID_JSON"),
        ("permission", json!({"storageEnabled":true}), "INVALID_JSON"),
        (
            "permission",
            json!({"storageEnabled":1,"allowSensitiveStorage":false}),
            "INVALID_JSON",
        ),
        (
            "permission",
            json!({"storageEnabled":true,"allowSensitiveStorage":false,"PRIVATE_SECRET":true}),
            "INVALID_JSON",
        ),
        (
            "snapshotJson",
            json!(" ".repeat(256 * 1024 + 1)),
            "LIMIT_EXCEEDED",
        ),
        (
            "snapshotJson",
            json!("{\"PRIVATE_SECRET\":true}"),
            "INVALID_SNAPSHOT",
        ),
        ("schemaVersion", json!(2), "VERSION_MISMATCH"),
    ] {
        let mut invalid = base.clone();
        invalid[field] = value;
        let reply = restore(invalid);
        assert_eq!(reply["error"]["code"], code);
        assert!(!reply.to_string().contains("PRIVATE_SECRET"));
    }
    let raw = base.to_string().replacen(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"schemaVersion\":1",
        1,
    );
    assert_eq!(
        take(unsafe { latexsnipper_formula_preferences_restore(raw.as_ptr(), raw.len()) })["error"]
            ["code"],
        "INVALID_JSON"
    );
    assert_eq!(
        take(unsafe { latexsnipper_formula_preferences_restore(std::ptr::null(), 0) })["error"]
            ["code"],
        "INVALID_ARGUMENT"
    );
    let over = vec![b' '; 1024 * 1024 + 1];
    assert_eq!(
        take(unsafe { latexsnipper_formula_preferences_restore(over.as_ptr(), over.len()) })
            ["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(owner.call(json!({"kind":"export-checkpoint","storageKeyHex":"cd".repeat(32),"checkpoint":"2", "permission":{"storageEnabled":false,"allowSensitiveStorage":false}}))["error"]["code"], "PREFERENCE_STORAGE_DISABLED");
    assert_eq!(export(&owner, "2", false)["ok"], true);
    let owners: Vec<_> = (0..63).map(|_| Owner::new()).collect();
    assert_eq!(restore(base)["error"]["code"], "LIMIT_EXCEEDED");
    drop(owners);
}

#[test]
fn c_preferences_default_off_guarded_feedback_and_undo_preserve_core_candidates() {
    let _serial = SERIAL.lock().unwrap();
    let owner = Owner::new();
    let input = request();
    let accept = accepted(&input);
    let caps = take(latexsnipper_formula_preferences_capabilities());
    assert_eq!(caps["data"]["maxStores"], 64);
    assert_eq!(caps["data"]["defaultEnabled"], false);
    let disabled_capture = owner.capture();
    assert!(record(&owner, disabled_capture.clone(), &accept, "1")["data"]["receiptId"].is_null());
    assert_eq!(
        record(&owner, disabled_capture, &accept, "1")["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(rank(&owner, &input)["data"]["order"], json!([0, 1]));
    owner.enable(false);
    let capture = owner.capture();
    let recorded = record(&owner, capture.clone(), &accept, "18446744073709551615");
    assert_eq!(recorded["ok"], true, "{recorded}");
    let receipt = recorded["data"]["receiptId"].clone();
    let ranked = rank(&owner, &input);
    assert_eq!(ranked["data"]["order"], json!([1, 0]));
    assert_eq!(
        ranked["data"]["features"][1]["lastAcceptedEvent"],
        "18446744073709551615"
    );
    assert_eq!(ranked["data"]["completion"]["snapshot"], accept["snapshot"]);
    assert_eq!(
        owner.call(json!({"kind":"undo","receiptId":receipt}))["ok"],
        true
    );
    assert_eq!(
        owner.call(json!({"kind":"undo","receiptId":receipt}))["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(
        record(&owner, capture, &accept, "2")["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(rank(&owner, &input)["data"]["order"], json!([0, 1]));
    let info = owner.call(json!({"kind":"info"}));
    assert!(info["data"].get("keyHex").is_none());
    assert_eq!(info["data"]["captureCount"], 0);
    assert_eq!(info["data"]["receiptCount"], 0);
}

#[test]
fn c_tokens_never_alias_another_owner_or_return_after_policy_aba_clear_close() {
    let _serial = SERIAL.lock().unwrap();
    let first = Owner::new();
    let second = Owner::new();
    first.enable(false);
    second.enable(false);
    let input = request();
    let accept = accepted(&input);
    let a = first.capture();
    let b = second.capture();
    assert_ne!(a, b);
    assert_eq!(
        record(&second, a.clone(), &accept, "1")["error"]["code"],
        "STALE_FEEDBACK"
    );
    let receipt = record(&first, a, &accept, "1")["data"]["receiptId"].clone();
    assert_eq!(
        owner_undo(&second, receipt.clone())["error"]["code"],
        "STALE_FEEDBACK"
    );
    let pending = first.capture();
    first.enable(true);
    first.enable(false);
    assert_eq!(
        record(&first, pending, &accept, "2")["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(
        owner_undo(&first, receipt)["error"]["code"],
        "STALE_FEEDBACK"
    );
    let pending = first.capture();
    assert_eq!(
        first.call(json!({"kind":"clear","generation":"0","keyHex":"bc".repeat(32)}))["error"]
            ["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        first.call(json!({"kind":"clear","generation":"1","keyHex":"bc".repeat(32)}))["ok"],
        true
    );
    assert_eq!(
        record(&first, pending, &accept, "2")["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(rank(&first, &input)["error"]["code"], "SCOPE_MISMATCH");
    let mut current = input;
    current["catalog"]["commands"]["scope"]["generation"] = json!("1");
    assert_eq!(rank(&first, &current)["data"]["features"][1]["accepted"], 0);
    let handle = first.0;
    assert_eq!(
        take(latexsnipper_formula_preferences_close(handle))["ok"],
        true
    );
    assert_eq!(
        first.call(json!({"kind":"info"}))["error"]["code"],
        "PREFERENCE_NOT_FOUND"
    );
    let replacement = Owner::new();
    assert_ne!(replacement.0, handle);
    assert_eq!(
        take(latexsnipper_formula_preferences_close(handle))["error"]["code"],
        "PREFERENCE_NOT_FOUND"
    );
}
fn owner_undo(owner: &Owner, receipt: Value) -> Value {
    owner.call(json!({"kind":"undo","receiptId":receipt}))
}

#[test]
fn c_preference_input_is_closed_bounded_exact_and_diagnostic_details_are_redacted() {
    let _serial = SERIAL.lock().unwrap();
    let owner = Owner::new();
    for bytes in [
        b"\xff".as_slice(),
        b"{",
        br#"{"kind":"info","PRIVATE_SECRET":true}"#,
        br#"{"kind":"capture","PRIVATE_SECRET":true}"#,
        br#"{"kind":"info","kind":"capture"}"#,
        br#"{"kind":"undo","receiptId":1}"#,
        br#"{"kind":"clear","generation":"01","keyHex":"not-a-key"}"#,
    ] {
        let reply = take(unsafe {
            latexsnipper_formula_preferences_command(owner.0, bytes.as_ptr(), bytes.len())
        });
        assert_eq!(reply["error"]["code"], "INVALID_JSON");
        assert!(reply["error"]["details"].is_null());
        assert!(!reply.to_string().contains("PRIVATE_SECRET"));
    }
    assert_eq!(
        take(unsafe { latexsnipper_formula_preferences_create(std::ptr::null(), 0) })["error"]
            ["code"],
        "INVALID_ARGUMENT"
    );
    let over = vec![b' '; 1024 * 1024 + 1];
    assert_eq!(
        take(unsafe {
            latexsnipper_formula_preferences_command(owner.0, over.as_ptr(), over.len())
        })["error"]["code"],
        "INVALID_ARGUMENT"
    );
    let metadata = format!("{}{{\"kind\":\"capture\"}}", " ".repeat(4096));
    assert_eq!(
        take(unsafe {
            latexsnipper_formula_preferences_command(owner.0, metadata.as_ptr(), metadata.len())
        })["error"]["code"],
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        owner.call(json!({"kind":"info"}))["data"]["captureCount"],
        0
    );
    let mut invalid = create_json();
    invalid["keyHex"] = json!("00".repeat(32));
    assert_eq!(create(invalid)["error"]["code"], "INVALID_ARGUMENT");
    let mut invalid = create_json();
    invalid["schemaVersion"] = json!(2);
    assert_eq!(create(invalid)["error"]["code"], "VERSION_MISMATCH");
}

#[test]
fn c_capacity_is_bounded_and_explicit_discard_and_close_release_resources() {
    let _serial = SERIAL.lock().unwrap();
    let owners: Vec<_> = (0..64).map(|_| Owner::new()).collect();
    assert_eq!(create(create_json())["error"]["code"], "LIMIT_EXCEEDED");
    let owner = &owners[0];
    let captures: Vec<_> = (0..128).map(|_| owner.capture()).collect();
    assert_eq!(
        owner.call(json!({"kind":"capture"}))["error"]["code"],
        "LIMIT_EXCEEDED"
    );
    assert_eq!(
        owner.call(json!({"kind":"discard-capture","captureId":captures[0]}))["ok"],
        true
    );
    let replacement = owner.capture();
    assert!(!captures.contains(&replacement));
    owner.enable(false);
    let input = request();
    let accept = accepted(&input);
    for id in 1..=1024 {
        let capture = owner.capture();
        let result = record(owner, capture, &accept, &id.to_string());
        assert_eq!(result["ok"], true, "{result}");
    }
    let result = record(owner, owner.capture(), &accept, "1025");
    assert_eq!(result["error"]["code"], "LIMIT_EXCEEDED");
    assert_eq!(
        owner.call(json!({"kind":"info"}))["data"]["eventCount"],
        1024
    );
    let exported = export(owner, "1", false);
    assert_eq!(exported["ok"], true);
    let snapshot = exported["data"]["snapshotJson"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(snapshot.len() > 4096 && snapshot.len() <= 256 * 1024);
    drop(owners);
    let reopened = Owner::new();
    assert_eq!(
        reopened.call(json!({"kind":"info"}))["data"]["eventCount"],
        0
    );
    let reply = restore(restore_request(&snapshot, "1"));
    assert_eq!(reply["ok"], true, "{reply}");
    let restored = Owner(reply["data"]["handle"].as_str().unwrap().parse().unwrap());
    assert_eq!(
        restored.call(json!({"kind":"info"}))["data"]["eventCount"],
        1024
    );
}

#[test]
fn c_sensitive_feedback_is_separate_from_retrieval_and_rank_output_budget_recovers() {
    let _serial = SERIAL.lock().unwrap();
    let owner = Owner::new();
    owner.enable(false);
    let mut input = request();
    input["catalog"]["policy"]["sensitiveDocument"] = json!(true);
    input["catalog"]["policy"]["allowInSensitiveDocument"] = json!(true);
    let accept = accepted(&input);
    assert!(record(&owner, owner.capture(), &accept, "1")["data"]["receiptId"].is_null());
    owner.enable(true);
    assert_eq!(record(&owner, owner.capture(), &accept, "1")["ok"], true);
    assert_eq!(rank(&owner, &input)["data"]["order"], json!([1, 0]));
    let mut huge = input.clone();
    let mut fragment = huge["catalog"]["fragments"][0].clone();
    fragment["source"] = json!(format!("x %{}\n", "\"".repeat(8188)));
    huge["catalog"]["fragments"] = json!((0..60)
        .map(|i| {
            let mut entry = fragment.clone();
            entry["id"] = json!(format!("entry{i}"));
            entry
        })
        .collect::<Vec<_>>());
    huge["source"] = json!(format!("{}\\loc", "x".repeat(3900)));
    huge["cursor"] = json!(3904);
    assert_eq!(rank(&owner, &huge)["error"]["code"], "OUTPUT_TOO_LARGE");
    assert_eq!(rank(&owner, &input)["data"]["order"], json!([1, 0]));
}
