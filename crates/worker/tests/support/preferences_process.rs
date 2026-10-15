use super::Process;
use serde_json::{json, Value};

fn create_params() -> Value {
    json!({"schemaVersion":1,"keyHex":"ab".repeat(32),"scope":{"userSha256":"a".repeat(64),
        "projectSha256":null,"documentSha256":"c".repeat(64),"generation":"0"}})
}
fn new(worker: &mut Process) -> Value {
    let created = worker.call(
        json!("pref-create"),
        "formula.preferences.create",
        create_params(),
    );
    assert_eq!(created["ok"], true, "{created}");
    created["data"]["ownerId"].clone()
}
fn command(worker: &mut Process, owner: &Value, body: Value) -> Value {
    worker.call(
        json!("pref-command"),
        "formula.preferences.command",
        json!({"ownerId":owner,"command":body}),
    )
}
fn enabled(sensitive: bool) -> Value {
    json!({"kind":"set-policy","policy":{"rankingEnabled":true,"feedbackEnabled":true,"allowSensitiveFeedback":sensitive}})
}
fn capture(worker: &mut Process, owner: &Value) -> Value {
    let result = command(worker, owner, json!({"kind":"capture"}));
    assert_eq!(result["ok"], true, "{result}");
    result["data"]["captureId"].clone()
}
fn input() -> Value {
    json!({"source":"中😀\\loc","cursor":11,"revision":"18446744073709551615","inputFormat":"latex",
        "catalog":{"commands":{"scope":create_params()["scope"],"definitions":[]},
        "policy":{"retrievalEnabled":true,"sensitiveDocument":false,"allowInSensitiveDocument":false},
        "fragments":[{"id":"first","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localfirst","label":"First","source":"x"},
        {"id":"second","revision":"18446744073709551615","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localsecond","label":"Second","source":r"\frac{α}{}"}]}})
}
fn accepted(worker: &mut Process, input: &Value) -> Value {
    let reply = worker.call(
        json!("pref-query"),
        "formula.complete.fragments",
        input.clone(),
    );
    assert_eq!(reply["ok"], true, "{reply}");
    let mut accepted = input.clone();
    accepted["snapshot"] = reply["data"]["snapshot"].clone();
    accepted["candidateId"] = json!("fragment:second");
    accepted
}
fn record(
    worker: &mut Process,
    owner: &Value,
    token: Value,
    request: &Value,
    event: &str,
) -> Value {
    command(
        worker,
        owner,
        json!({"kind":"record-acceptance","captureId":token,"eventId":event,"request":request}),
    )
}
fn stop(worker: &mut Process) {
    assert_eq!(
        worker.call(json!("stop"), "worker.shutdown", json!({}))["ok"],
        true
    );
    worker.successful_exit();
}

#[test]
fn preference_owner_reuses_actual_process_and_preserves_core_edit_and_old_routes() {
    let mut worker = Process::start();
    let caps = worker.call(json!("caps"), "formula.capabilities", json!({}));
    assert_eq!(caps["data"]["preferences"]["maxOwners"], 64);
    assert_eq!(caps["data"]["preferences"]["defaultEnabled"], false);
    assert_eq!(caps["data"]["completion"]["schemaVersion"], 1);
    assert_eq!(caps["data"]["scopedCompletion"]["schemaVersion"], 2);
    let owner = new(&mut worker);
    let input = input();
    let accepted = accepted(&mut worker, &input);
    let token = capture(&mut worker, &owner);
    assert!(
        record(&mut worker, &owner, token.clone(), &accepted, "1")["data"]["receiptId"].is_null()
    );
    assert_eq!(
        record(&mut worker, &owner, token, &accepted, "1")["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(command(&mut worker, &owner, enabled(false))["ok"], true);
    let token = capture(&mut worker, &owner);
    let recorded = record(
        &mut worker,
        &owner,
        token,
        &accepted,
        "18446744073709551615",
    );
    assert_eq!(recorded["ok"], true, "{recorded}");
    let receipt = recorded["data"]["receiptId"].clone();
    let rank = command(
        &mut worker,
        &owner,
        json!({"kind":"rank-fragments","request":input}),
    );
    assert_eq!(rank["data"]["order"], json!([1, 0]));
    assert_eq!(
        rank["data"]["features"][1]["lastAcceptedEvent"],
        "18446744073709551615"
    );
    assert_eq!(rank["data"]["completion"]["snapshot"], accepted["snapshot"]);
    let applied = worker.call(
        json!("canonical"),
        "formula.completion.apply.fragments",
        accepted,
    );
    assert_eq!(applied["data"]["source"], "中😀\\frac{α}{}");
    assert_eq!(applied["data"]["cursor"], 17);
    assert_eq!(
        command(
            &mut worker,
            &owner,
            json!({"kind":"undo","receiptId":receipt})
        )["ok"],
        true
    );
    assert_eq!(
        command(
            &mut worker,
            &owner,
            json!({"kind":"undo","receiptId":receipt})
        )["error"]["code"],
        "STALE_FEEDBACK"
    );
    let info = command(&mut worker, &owner, json!({"kind":"info"}));
    assert!(info["data"].get("keyHex").is_none());
    assert_eq!(info["data"]["eventCount"], 1);
    let status = worker.call(json!("status"), "worker.status", json!({}));
    assert_eq!(status["data"]["liveSessions"], 0);
    assert_eq!(status["data"]["livePreferenceOwners"], 1);
    let old = worker.call(
        json!("old-convert"),
        "formula.convert",
        json!({"content":"x^2","inputFormat":"latex","outputFormat":"omml"}),
    );
    assert_eq!(old["ok"], true);
    stop(&mut worker);
}

#[test]
fn preference_policy_aba_clear_and_scope_refuse_late_feedback() {
    let mut worker = Process::start();
    let first = new(&mut worker);
    let second = new(&mut worker);
    command(&mut worker, &first, enabled(false));
    command(&mut worker, &second, enabled(false));
    let input = input();
    let accepted = accepted(&mut worker, &input);
    let a = capture(&mut worker, &first);
    let b = capture(&mut worker, &second);
    assert_ne!(a, b);
    assert_eq!(
        record(&mut worker, &second, a.clone(), &accepted, "1")["error"]["code"],
        "STALE_FEEDBACK"
    );
    command(&mut worker, &first, enabled(true));
    command(&mut worker, &first, enabled(false));
    assert_eq!(
        record(&mut worker, &first, a, &accepted, "1")["error"]["code"],
        "STALE_FEEDBACK"
    );
    let pending = capture(&mut worker, &first);
    assert_eq!(
        command(
            &mut worker,
            &first,
            json!({"kind":"clear","generation":"1","keyHex":"bc".repeat(32)})
        )["ok"],
        true
    );
    assert_eq!(
        record(&mut worker, &first, pending, &accepted, "1")["error"]["code"],
        "STALE_FEEDBACK"
    );
    assert_eq!(
        command(
            &mut worker,
            &first,
            json!({"kind":"rank-fragments","request":input})
        )["error"]["code"],
        "SCOPE_MISMATCH"
    );
    assert_eq!(
        worker.call(
            json!("close"),
            "formula.preferences.close",
            json!({"ownerId":first})
        )["ok"],
        true
    );
    assert_eq!(
        command(&mut worker, &first, json!({"kind":"info"}))["error"]["code"],
        "PREFERENCE_NOT_FOUND"
    );
    assert_ne!(new(&mut worker), first);
    stop(&mut worker);
}

#[test]
fn preference_raw_transport_rejects_duplicate_keys_unknown_fields_and_leaking_diagnostics() {
    let mut worker = Process::start();
    let owner = new(&mut worker);
    let id = owner.as_str().unwrap();
    for body in [
        r#"{"kind":"info","PRIVATE_SECRET":true}"#,
        r#"{"kind":"capture","PRIVATE_SECRET":true}"#,
        r#"{"kind":"capture","kind":"info"}"#,
        r#"{"kind":"clear","generation":"01","keyHex":"bad"}"#,
        r#"{"kind":"undo","receiptId":1}"#,
        r#"{"kind":"set-policy","policy":{"rankingEnabled":true,"feedbackEnabled":false,"feedbackEnabled":true,"allowSensitiveFeedback":false}}"#,
    ] {
        let raw = format!(
            r#"{{"version":1,"id":"raw","action":"formula.preferences.command","params":{{"ownerId":"{id}","command":{body}}}}}"#
        );
        let reply = worker.raw(&raw);
        assert_eq!(reply["error"]["code"], "INVALID_PARAMS", "{reply}");
        assert!(reply["error"]["details"].is_null());
        assert!(!reply.to_string().contains("PRIVATE_SECRET"));
    }
    let params = create_params().to_string();
    let duplicate = params.replace(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"schemaVersion\":1",
    );
    let raw = format!(
        r#"{{"version":1,"id":"duplicate","action":"formula.preferences.create","params":{duplicate}}}"#
    );
    assert_eq!(worker.raw(&raw)["error"]["code"], "INVALID_PARAMS");
    let malformed = r#"{"version":1,"id":"private","action":"formula.preferences.create","params":{},"PRIVATE_SECRET":true}"#;
    let reply = worker.raw(malformed);
    assert_eq!(reply["error"]["code"], "INVALID_REQUEST");
    assert!(reply["error"]["details"].is_null());
    assert!(!reply.to_string().contains("PRIVATE_SECRET"));
    let oversized = format!(
        r#"{{"version":1,"id":"meta","action":"formula.preferences.command","params":{{"ownerId":"{id}",{}"command":{{"kind":"capture"}}}}}}"#,
        " ".repeat(4096)
    );
    assert_eq!(worker.raw(&oversized)["error"]["code"], "INPUT_TOO_LARGE");
    assert_eq!(
        command(&mut worker, &owner, json!({"kind":"info"}))["data"]["captureCount"],
        0
    );
    stop(&mut worker);
}

#[test]
fn preference_restart_eof_and_other_live_worker_never_reuse_opaque_identity() {
    let mut first = Process::start();
    let owner = new(&mut first);
    command(&mut first, &owner, enabled(false));
    let old_capture = capture(&mut first, &owner);
    let input = input();
    let accepted = accepted(&mut first, &input);
    let mut second = Process::start();
    let new_owner = new(&mut second);
    assert_ne!(owner, new_owner);
    command(&mut second, &new_owner, enabled(false));
    let new_capture = capture(&mut second, &new_owner);
    assert_ne!(old_capture, new_capture);
    assert_eq!(
        command(&mut second, &owner, json!({"kind":"info"}))["error"]["code"],
        "PREFERENCE_NOT_FOUND"
    );
    assert_eq!(
        record(&mut second, &new_owner, old_capture.clone(), &accepted, "1")["error"]["code"],
        "STALE_FEEDBACK"
    );
    first.input.take();
    first.successful_exit();
    let mut restarted = Process::start();
    let restart_owner = new(&mut restarted);
    assert_ne!(restart_owner, owner);
    assert_ne!(restart_owner, new_owner);
    assert_eq!(
        command(&mut restarted, &owner, json!({"kind":"info"}))["error"]["code"],
        "PREFERENCE_NOT_FOUND"
    );
    assert_eq!(
        record(&mut restarted, &restart_owner, old_capture, &accepted, "1")["error"]["code"],
        "STALE_FEEDBACK"
    );
    stop(&mut second);
    stop(&mut restarted);
}

#[test]
fn preference_limits_sensitive_allow_output_refusal_and_recovery_are_explicit() {
    let mut worker = Process::start();
    let owners: Vec<_> = (0..64).map(|_| new(&mut worker)).collect();
    assert_eq!(
        worker.call(json!("full"), "formula.preferences.create", create_params())["error"]["code"],
        "LIMIT_EXCEEDED"
    );
    let owner = &owners[0];
    let tokens: Vec<_> = (0..128).map(|_| capture(&mut worker, owner)).collect();
    assert_eq!(
        command(&mut worker, owner, json!({"kind":"capture"}))["error"]["code"],
        "LIMIT_EXCEEDED"
    );
    assert_eq!(
        command(
            &mut worker,
            owner,
            json!({"kind":"discard-capture","captureId":tokens[0]})
        )["ok"],
        true
    );
    assert!(!tokens.contains(&capture(&mut worker, owner)));
    command(&mut worker, owner, enabled(false));
    let mut input = input();
    input["catalog"]["policy"]["sensitiveDocument"] = json!(true);
    input["catalog"]["policy"]["allowInSensitiveDocument"] = json!(true);
    let accepted = accepted(&mut worker, &input);
    let token = capture(&mut worker, owner);
    assert!(record(&mut worker, owner, token, &accepted, "1")["data"]["receiptId"].is_null());
    command(&mut worker, owner, enabled(true));
    let token = capture(&mut worker, owner);
    assert_eq!(
        record(&mut worker, owner, token, &accepted, "1")["ok"],
        true
    );
    let mut huge = input.clone();
    let mut entry = huge["catalog"]["fragments"][0].clone();
    entry["source"] = json!(format!("x %{}\n", "\"".repeat(8188)));
    huge["catalog"]["fragments"] = json!((0..60)
        .map(|index| {
            let mut item = entry.clone();
            item["id"] = json!(format!("entry{index}"));
            item
        })
        .collect::<Vec<_>>());
    huge["source"] = json!(format!("{}\\loc", "x".repeat(3900)));
    huge["cursor"] = json!(3904);
    assert_eq!(
        command(
            &mut worker,
            owner,
            json!({"kind":"rank-fragments","request":huge})
        )["error"]["code"],
        "OUTPUT_TOO_LARGE"
    );
    assert_eq!(
        command(
            &mut worker,
            owner,
            json!({"kind":"rank-fragments","request":input})
        )["data"]["order"],
        json!([1, 0])
    );
    for event in 2..=1024 {
        let token = capture(&mut worker, owner);
        assert_eq!(
            record(&mut worker, owner, token, &accepted, &event.to_string())["ok"],
            true
        );
    }
    let token = capture(&mut worker, owner);
    assert_eq!(
        record(&mut worker, owner, token, &accepted, "1025")["error"]["code"],
        "LIMIT_EXCEEDED"
    );
    assert_eq!(
        command(&mut worker, owner, json!({"kind":"info"}))["data"]["eventCount"],
        1024
    );
    for owner in owners {
        assert_eq!(
            worker.call(
                json!("release"),
                "formula.preferences.close",
                json!({"ownerId":owner})
            )["ok"],
            true
        );
    }
    assert_eq!(
        worker.call(json!("empty"), "worker.status", json!({}))["data"]["livePreferenceOwners"],
        0
    );
    assert!(new(&mut worker).is_string());
    stop(&mut worker);
}
