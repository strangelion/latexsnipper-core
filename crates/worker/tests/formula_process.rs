use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

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
        let input = self.input.as_mut().unwrap();
        writeln!(
            input,
            "{}",
            json!({ "version": 1, "id": id, "action": action, "params": params })
        )
        .unwrap();
        input.flush().unwrap();
        let line = self
            .responses
            .recv_timeout(Duration::from_secs(15))
            .expect("worker must flush a response while stdin remains open");
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response["id"], id);
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
