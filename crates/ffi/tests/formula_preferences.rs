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
    drop(owners);
    let reopened = Owner::new();
    assert_eq!(
        reopened.call(json!({"kind":"info"}))["data"]["eventCount"],
        0
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
