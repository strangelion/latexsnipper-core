use std::io::Write;
use std::process::{Command, Output, Stdio};

fn convert(extra: &[&str], source: &[u8]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_snipper"));
    command.args(["formula", "convert", "--from", "latex", "--to", "omml"]);
    if !extra.contains(&"--context-sha256") {
        command.args(["--context-sha256", &"a".repeat(64)]);
    }
    command
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(source).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn formula_cli_uses_registered_builtin_report_and_preserves_raw_source_identity() {
    use sha2::{Digest, Sha256};
    let source = b" $$x^2$$ ";
    let output = convert(&[], source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["outcome"]["kind"], "converted");
    assert_eq!(value["outcome"]["result"]["backend"], "core:builtin");
    assert_eq!(
        value["outcome"]["result"]["inputSha256"],
        format!("{:x}", Sha256::digest(source))
    );
    assert!(value["outcome"]["result"]["candidate"]["content"]
        .as_str()
        .unwrap()
        .contains("m:sSup"));
    assert_ne!(value["outcome"]["result"]["contextSha256"], "a".repeat(64));
    assert_eq!(value["attempts"].as_array().unwrap().len(), 1);
}

#[test]
fn explicit_availability_fallback_and_terminal_input_errors_are_distinct() {
    let output = convert(
        &["--backend", "missing:backend", "--fallback", "core:builtin"],
        b"x^2",
    );
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["attempts"].as_array().unwrap().len(), 2);
    assert_eq!(value["attempts"][0]["status"]["kind"], "unavailable");
    let output = convert(
        &["--mode", "strict", "--fallback", "missing:backend"],
        br"\unknown{x}",
    );
    assert_eq!(output.status.code(), Some(3));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["outcome"]["kind"], "rejected");
    assert_eq!(value["attempts"].as_array().unwrap().len(), 1);
}

#[test]
fn invalid_policy_is_refused_before_any_profile_io_or_activation() {
    let output = convert(
        &[
            "--profile",
            "nonexistent.json",
            "--backend",
            "missing:backend",
            "--fallback",
            "missing:backend",
        ],
        b"x",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("fallback"));
}

#[test]
fn malformed_source_context_and_oversize_are_bounded() {
    let output = convert(&[], &[0xff]);
    assert_eq!(output.status.code(), Some(3));
    let output = convert(&[], &vec![b'x'; 64 * 1024 + 1]);
    assert_eq!(output.status.code(), Some(3));
    let output = convert(&["--context-sha256", "invalid"], b"x");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn capabilities_are_json_and_do_not_claim_an_enabled_profile() {
    let output = Command::new(env!("CARGO_BIN_EXE_snipper"))
        .args(["formula", "capabilities"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["formats"].as_array().unwrap().is_empty());
    assert!(value["profileSha256"].is_null());
    assert!(value["routes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|route| route["backend"] == "core:builtin"));
}

#[test]
#[cfg(not(feature = "wasi-formula"))]
fn default_build_has_no_implicit_wasi_loader() {
    let output = convert(&["--profile", "nonexistent.json"], b"x");
    assert_eq!(output.status.code(), Some(9));
    assert!(String::from_utf8_lossy(&output.stderr).contains("wasi-formula"));
}
