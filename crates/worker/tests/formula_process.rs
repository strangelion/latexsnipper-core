use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

#[path = "support/preferences_process.rs"]
mod preferences;

struct Process {
    child: Child,
    input: Option<ChildStdin>,
    responses: Receiver<String>,
}

impl Process {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_latexsnipper-worker"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (sender, responses) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            responses,
        }
    }

    fn call(&mut self, id: Value, action: &str, params: Value) -> Value {
        let response = self.raw(
            &json!({ "version": 1, "id": id, "action": action, "params": params }).to_string(),
        );
        assert_eq!(response["id"], id);
        response
    }

    fn raw(&mut self, request: &str) -> Value {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{request}").unwrap();
        input.flush().unwrap();
        let line = self
            .responses
            .recv_timeout(Duration::from_secs(15))
            .expect("worker must flush a response while stdin remains open");
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["protocolVersion"], 1);
        response
    }

    fn successful_exit(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "worker did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(self.responses.recv_timeout(Duration::from_secs(1)).is_err());
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn scoped_completion_rpc_filters_definitions_and_rejects_cross_scope_edits() {
    let mut worker = Process::start();
    let caps = worker.call(json!("caps-scoped"), "formula.capabilities", json!({}));
    assert_eq!(caps["data"]["scopedCompletion"]["schemaVersion"], 2);
    assert_eq!(
        caps["data"]["scopedCompletion"]["argumentKinds"],
        json!(["math", "text", "optional-math", "optional-text"])
    );
    assert_eq!(
        caps["data"]["scopedCompletion"]["maxLeadingOptionalArguments"],
        1
    );
    let catalog = json!({
        "scope": {"userSha256": "a".repeat(64), "projectSha256": "b".repeat(64),
            "documentSha256": "c".repeat(64), "generation": "18446744073709551615"},
        "definitions": [{"id": "pair", "revision": "18446744073709551615",
            "userSha256": "a".repeat(64), "projectSha256": "b".repeat(64),
            "command": "custompair", "arguments": ["math", "text"]}]
    });
    let params = json!({"source": "中\\cust", "cursor": 8,
        "revision": "18446744073709551615", "inputFormat": "latex", "catalog": catalog});
    for kind in ["optional-math", "optional-text"] {
        let mut optional = params.clone();
        optional["catalog"]["definitions"][0]["arguments"] = json!([kind, "math", "text"]);
        let result = worker.call(json!(kind), "formula.complete.scoped", optional.clone());
        assert_eq!(result["ok"], true, "{result}");
        optional["snapshot"] = result["data"]["snapshot"].clone();
        optional["candidateId"] = json!("definition:pair:optional");
        let applied = worker.call(
            json!(format!("apply-{kind}")),
            "formula.completion.apply.scoped",
            optional,
        );
        assert_eq!(applied["data"]["source"], "中\\custompair[]{}{}");
        assert_eq!(applied["data"]["tabStops"].as_array().unwrap().len(), 3);
    }
    let completed = worker.call(
        json!("scoped-query"),
        "formula.complete.scoped",
        params.clone(),
    );
    assert_eq!(completed["ok"], true, "{completed}");
    let data = &completed["data"];
    assert_eq!(data["context"]["schemaVersion"], 2);
    assert_eq!(
        data["candidates"][0]["definition"]["revision"],
        "18446744073709551615"
    );
    let mut apply = params.clone();
    apply["snapshot"] = data["snapshot"].clone();
    apply["candidateId"] = json!("definition:pair");
    for change in [
        "project",
        "generation",
        "signature",
        "revision",
        "source",
        "snapshot",
    ] {
        let mut stale = apply.clone();
        match change {
            "project" => stale["catalog"]["scope"]["projectSha256"] = json!("d".repeat(64)),
            "generation" => stale["catalog"]["scope"]["generation"] = json!("0"),
            "signature" => stale["catalog"]["definitions"][0]["arguments"] = json!([]),
            "revision" => stale["revision"] = json!("0"),
            "source" => stale["source"] = json!("中\\custx"),
            _ => stale["snapshot"]["definitionsSha256"] = json!("0".repeat(64)),
        }
        let failed = worker.call(json!(change), "formula.completion.apply.scoped", stale);
        assert_eq!(failed["error"]["code"], "STALE_CONTEXT", "{failed}");
    }
    let mut foreign = params.clone();
    foreign["catalog"]["scope"]["userSha256"] = json!("d".repeat(64));
    let foreign = worker.call(json!("foreign"), "formula.complete.scoped", foreign);
    assert_eq!(foreign["data"]["candidates"], json!([]));
    assert!(!foreign.to_string().contains("custompair"));
    let mut invalid = apply.clone();
    invalid["catalog"]["definitions"][0]["body"] = json!(r"\input{private}");
    let failed = worker.call(json!("body"), "formula.completion.apply.scoped", invalid);
    assert_eq!(failed["error"]["code"], "INVALID_PARAMS");
    let applied = worker.call(
        json!("scoped-apply"),
        "formula.completion.apply.scoped",
        apply,
    );
    assert_eq!(applied["data"]["source"], "中\\custompair{}{}");
    assert_eq!(applied["data"]["tabStops"].as_array().unwrap().len(), 2);
    let ordinary = worker.call(
        json!("v1-after-scoped"),
        "formula.complete",
        json!({"source": r"\fr", "cursor": 3, "revision": "0", "inputFormat": "latex"}),
    );
    assert_eq!(ordinary["data"]["snapshot"]["schemaVersion"], 1);
    let status = worker.call(json!("scoped-status"), "worker.status", json!({}));
    assert_eq!(status["data"]["liveSessions"], 0);
    let stopped = worker.call(json!("scoped-stop"), "worker.shutdown", json!({}));
    assert_eq!(stopped["ok"], true);
    worker.successful_exit();
}

#[test]
fn delimiter_ownership_and_closure_survive_the_real_jsonl_transport() {
    let mut worker = Process::start();
    let sized_source = "中\\Bigl(x";
    let sized_input = json!({"source": sized_source, "cursor": sized_source.len(), "revision": "42", "inputFormat": "latex"});
    let sized = worker.call(
        json!("sized-query"),
        "formula.complete",
        sized_input.clone(),
    );
    let mut sized_apply = sized_input;
    sized_apply["snapshot"] = sized["data"]["snapshot"].clone();
    sized_apply["candidateId"] = json!("structure:close-delimiter");
    let sized = worker.call(
        json!("sized-apply"),
        "formula.completion.apply",
        sized_apply,
    );
    assert_eq!(sized["data"]["source"], "中\\Bigl(x\\Bigr)");
    let source = "中\\left(x";
    let params = json!({"source": source, "cursor": source.len(), "revision": "18446744073709551615", "inputFormat": "latex"});
    let reply = worker.call(json!("fence-query"), "formula.complete", params.clone());
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["data"]["completion"]["context"]["slot"], "group");
    let mut apply = params;
    apply["snapshot"] = reply["data"]["snapshot"].clone();
    apply["candidateId"] = json!("structure:close-delimiter");
    let accepted = worker.call(json!("fence-apply"), "formula.completion.apply", apply);
    assert_eq!(accepted["data"]["source"], "中\\left(x\\right)");
    let template = worker.call(
        json!("fence-template"),
        "formula.complete",
        json!({"source": "\\lang", "cursor": 5, "revision": "0", "inputFormat": "latex"}),
    );
    let applied = worker.call(json!("fence-template-apply"), "formula.completion.apply",
        json!({"source": "\\lang", "cursor": 5, "revision": "0", "inputFormat": "latex", "snapshot": template["data"]["snapshot"], "candidateId": "builtin:langle"}));
    assert_eq!(applied["data"]["source"], r"\langle\rangle");
    assert_eq!(applied["data"]["cursor"], 7);
    worker.call(json!("fence-stop"), "worker.shutdown", json!({}));
    worker.successful_exit();
}

#[test]
fn local_fragment_rpc_keeps_policy_scope_and_editor_guards_in_the_real_process() {
    let mut worker = Process::start();
    let caps = worker.call(json!("fragment-caps"), "formula.capabilities", json!({}));
    assert_eq!(caps["data"]["fragmentCompletion"]["schemaVersion"], 3);
    assert_eq!(caps["data"]["fragmentCompletion"]["maxFragments"], 64);
    assert_eq!(caps["data"]["fragmentCompletion"]["learning"], false);
    let catalog = json!({"commands": {"scope": {"userSha256": "a".repeat(64), "projectSha256": "b".repeat(64),
        "documentSha256": "c".repeat(64), "generation": "18446744073709551615"}, "definitions": []},
        "policy": {"retrievalEnabled": true, "sensitiveDocument": false, "allowInSensitiveDocument": false},
        "fragments": [{"id":"saved", "revision":"18446744073709551615", "userSha256":"a".repeat(64),
            "projectSha256": null, "documentSha256": "c".repeat(64), "trigger":"bayes", "label":"Saved formula", "source":r"\frac{}{}"}]});
    let params = json!({"source":"中\\bay", "cursor":7, "revision":"18446744073709551615", "inputFormat":"latex", "catalog":catalog});
    let result = worker.call(
        json!("fragment-query"),
        "formula.complete.fragments",
        params.clone(),
    );
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["data"]["context"]["schemaVersion"], 3);
    assert_eq!(
        result["data"]["candidates"][0]["fragment"]["revision"],
        "18446744073709551615"
    );
    assert!(result["data"]["candidates"][0]["definition"].is_null());
    let mut apply = params.clone();
    apply["snapshot"] = result["data"]["snapshot"].clone();
    apply["candidateId"] = json!("fragment:saved");
    let accepted = worker.call(
        json!("fragment-apply"),
        "formula.completion.apply.fragments",
        apply.clone(),
    );
    assert_eq!(accepted["data"]["source"], "中\\frac{}{}");
    assert_eq!(
        accepted["data"]["tabStops"],
        json!([{"start":9,"end":9},{"start":11,"end":11}])
    );
    for field in ["disabled", "sensitive", "generation", "body", "clear"] {
        let mut changed = apply.clone();
        match field {
            "disabled" => changed["catalog"]["policy"]["retrievalEnabled"] = json!(false),
            "sensitive" => changed["catalog"]["policy"]["sensitiveDocument"] = json!(true),
            "generation" => changed["catalog"]["commands"]["scope"]["generation"] = json!("0"),
            "clear" => changed["catalog"]["fragments"] = json!([]),
            _ => changed["catalog"]["fragments"][0]["source"] = json!("x+1"),
        }
        let failed = worker.call(json!(field), "formula.completion.apply.fragments", changed);
        assert_eq!(failed["error"]["code"], "STALE_CONTEXT");
    }
    for field in ["disabled", "foreign", "sensitive"] {
        let mut changed = params.clone();
        changed["catalog"]["fragments"][0]["source"] = json!(r"PRIVATE_SECRET\frac{");
        match field {
            "disabled" => changed["catalog"]["policy"]["retrievalEnabled"] = json!(false),
            "foreign" => {
                changed["catalog"]["fragments"][0]["documentSha256"] = json!("d".repeat(64))
            }
            _ => changed["catalog"]["policy"]["sensitiveDocument"] = json!(true),
        }
        let hidden = worker.call(
            json!(format!("hidden-{field}")),
            "formula.complete.fragments",
            changed,
        );
        assert_eq!(hidden["data"]["candidates"], json!([]));
        assert!(!hidden.to_string().contains("PRIVATE_SECRET"));
    }
    let mut invalid = apply;
    invalid["insert"] = json!("untrusted");
    let failed = worker.call(
        json!("fragment-payload"),
        "formula.completion.apply.fragments",
        invalid,
    );
    assert_eq!(failed["error"]["code"], "INVALID_PARAMS");
    let status = worker.call(json!("fragment-status"), "worker.status", json!({}));
    assert_eq!(status["data"]["liveSessions"], 0);
    let stopped = worker.call(json!("fragment-stop"), "worker.shutdown", json!({}));
    assert_eq!(stopped["ok"], true);
    worker.successful_exit();
}

#[test]
fn formula_rpc_reuses_real_process_after_error_and_flushes_before_shutdown() {
    let mut worker = Process::start();
    let capabilities = worker.call(json!(1), "formula.capabilities", json!({}));
    assert_eq!(capabilities["ok"], true);
    assert!(
        capabilities["data"]["conversions"]
            .as_array()
            .unwrap()
            .len()
            > 100
    );
    for source in ["x^2", "y^4"] {
        let response = worker.call(json!(source), "formula.convert", json!({
            "content": source, "inputFormat": "latex", "outputFormat": "latex-fragment", "mode": "best-effort"
        }));
        assert_eq!(response["ok"], true);
        assert_eq!(response["data"]["content"], source);
        assert_eq!(response["data"]["contentKind"], "latex-fragment");
    }
    let failed = worker.call(
        json!(4),
        "formula.convert",
        json!({
            "content": "x", "inputFormat": "mtef", "outputFormat": "omml"
        }),
    );
    assert_eq!(failed["error"]["code"], "UNSUPPORTED_FORMAT");
    let oversized = worker.call(
        json!("output-budget"),
        "formula.convert",
        json!({
            "content": format!("{}x", "x+".repeat(6000)),
            "inputFormat": "latex", "outputFormat": "omml"
        }),
    );
    assert_eq!(oversized["error"]["code"], "OUTPUT_TOO_LARGE");
    let healthy = worker.call(json!(5), "worker.status", json!({}));
    assert_eq!(healthy["data"]["liveSessions"], 0);
    let converted = worker.call(json!(6), "formula.convert", json!({
        "content": "frac(a,b)", "inputFormat": "typst", "outputFormat": "markdown_inline", "mode": "best-effort"
    }));
    assert_eq!(converted["data"]["content"], r"$\frac{a}{b}$");
    let linear = worker.call(json!("linear"), "formula.convert", json!({
        "content": "(a+b)/c", "inputFormat": "ascii-math", "outputFormat": "unicode-math", "mode": "best-effort"
    }));
    assert_eq!(linear["ok"], true);
    assert_eq!(linear["data"]["content"], "(a + b)/(c)");
    let mixed = worker.call(json!("mixed-linear"), "formula.convert", json!({
        "content": "body $x$", "inputFormat": "markdown", "outputFormat": "ascii-math", "mode": "best-effort"
    }));
    assert_eq!(mixed["error"]["code"], "CONVERSION_FAILED");
    let recovered = worker.call(json!("recovered-linear"), "formula.convert", json!({
        "content": "a/b", "inputFormat": "unicode-math", "outputFormat": "ascii-math", "mode": "best-effort"
    }));
    assert_eq!(recovered["ok"], true);
    assert_eq!(recovered["data"]["content"], "frac(a)(b)");
    let shutdown = worker.call(json!(7), "worker.shutdown", json!({}));
    assert_eq!(shutdown["data"]["closed"], true);
    worker.successful_exit();
}

#[test]
fn formula_rpc_real_process_exits_successfully_on_eof() {
    let mut worker = Process::start();
    let converted = worker.call(
        json!("before-eof"),
        "formula.convert",
        json!({
            "content": "x^2", "inputFormat": "latex", "outputFormat": "omml"
        }),
    );
    assert_eq!(converted["ok"], true);
    worker.input.take();
    worker.successful_exit();
}

#[test]
fn completion_rpc_uses_lossless_revisions_and_survives_invalid_or_stale_edits() {
    let mut worker = Process::start();
    let capabilities = worker.call(json!("caps"), "formula.capabilities", json!({}));
    assert_eq!(
        capabilities["data"]["completion"]["inputFormats"],
        json!(["latex"])
    );
    assert_eq!(
        capabilities["data"]["completion"]["offsetEncoding"],
        "utf-8-bytes"
    );
    let source = "中\\fr";
    let revision = "18446744073709551615";
    let params = json!({"source": source, "cursor": source.len(), "revision": revision, "inputFormat": "latex"});
    let response = worker.call(json!("complete"), "formula.complete", params.clone());
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["data"]["snapshot"]["revision"], revision);
    assert_eq!(
        response["data"]["completion"]["context"]["revision"],
        revision
    );
    assert!(response["data"]["completion"]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| candidate["id"] == "builtin:frac"));
    let mut apply = params.clone();
    apply["snapshot"] = response["data"]["snapshot"].clone();
    apply["candidateId"] = json!("builtin:frac");
    for (field, value, code) in [
        ("revision", json!("0"), "STALE_CONTEXT"),
        ("source", json!("中\\sr"), "STALE_CONTEXT"),
        ("cursor", json!(source.len() - 1), "STALE_CONTEXT"),
        ("cursor", json!(1), "INVALID_CURSOR"),
        (
            "candidateId",
            json!("external:arbitrary"),
            "UNKNOWN_CANDIDATE",
        ),
        ("insert", json!("untrusted"), "INVALID_PARAMS"),
    ] {
        let mut invalid = apply.clone();
        invalid[field] = value;
        let failed = worker.call(json!(field), "formula.completion.apply", invalid);
        assert_eq!(failed["error"]["code"], code, "{failed}");
    }
    for (field, value, code) in [
        ("revision", json!(18446744073709551615u64), "INVALID_PARAMS"),
        ("revision", json!("01"), "INVALID_PARAMS"),
        ("inputFormat", json!("typst"), "INVALID_PARAMS"),
        ("source", json!("x".repeat(65_537)), "LIMIT_EXCEEDED"),
        ("source", json!("x".repeat(4096)), "LIMIT_EXCEEDED"),
        (
            "source",
            json!(format!("{}x", "{".repeat(64))),
            "LIMIT_EXCEEDED",
        ),
    ] {
        let mut invalid = params.clone();
        invalid[field] = value;
        let failed = worker.call(json!(field), "formula.complete", invalid);
        assert_eq!(failed["error"]["code"], code, "{failed}");
    }
    let applied = worker.call(json!("apply"), "formula.completion.apply", apply);
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(applied["data"]["source"], "中\\frac{}{}");
    assert_eq!(applied["data"]["cursor"], 9);
    assert_eq!(
        applied["data"]["tabStops"],
        json!([{"start":9,"end":9},{"start":11,"end":11}])
    );
    let healthy = worker.call(json!("status"), "worker.status", json!({}));
    assert_eq!(healthy["data"]["liveSessions"], 0);
    let shutdown = worker.call(json!("stop"), "worker.shutdown", json!({}));
    assert_eq!(shutdown["ok"], true);
    worker.successful_exit();
}
