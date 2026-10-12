#![cfg(feature = "wasi-formula")]
#[path = "../../plugin-wasi/tests/support/formula_profile_store.rs"]
mod fixture;
use std::io::Write;
use std::process::{Command, Stdio};

fn run(
    profile: &std::path::Path,
    store: &std::path::Path,
    exporter: bool,
    source: &[u8],
    extra: &[&str],
) -> std::process::Output {
    run_route(
        profile,
        store,
        if exporter { "latex" } else { "fixture:power" },
        if exporter { "fixture:power" } else { "omml" },
        source,
        extra,
    )
}

fn run_route(
    profile: &std::path::Path,
    store: &std::path::Path,
    input: &str,
    output: &str,
    source: &[u8],
    extra: &[&str],
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_snipper"));
    command
        .args([
            "formula",
            "convert",
            "--from",
            input,
            "--to",
            output,
            "--context-sha256",
            &"a".repeat(64),
            "--backend",
            "fixture:profile",
        ])
        .arg("--profile")
        .arg(profile)
        .arg("--plugin-store")
        .arg(store)
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(source).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn real_cli_wasi_profile_has_explicit_enable_validated_output_and_no_package_enable() {
    let (temporary, store) = fixture::store();
    let root = temporary.path().join("store");
    let path = temporary.path().join("profile.json");
    std::fs::write(&path, serde_json::to_vec(&fixture::profile(false)).unwrap()).unwrap();
    let disabled = run(&path, &root, false, b"x:2", &[]);
    assert_eq!(
        disabled.status.code(),
        Some(6),
        "{}",
        String::from_utf8_lossy(&disabled.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&disabled.stdout).unwrap();
    assert_eq!(value["outcome"]["error"]["code"], "BACKEND_DISABLED");
    for source in [b"x:2".as_slice(), b"y:4"] {
        let result = run(&path, &root, false, source, &["--enable-profile-backend"]);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["outcome"]["result"]["backend"], "fixture:profile");
        assert!(value["outcome"]["result"]["candidate"]["content"]
            .as_str()
            .unwrap()
            .contains("m:sSup"));
    }
    let invalid = run(
        &path,
        &root,
        false,
        b"x:2+y",
        &["--enable-profile-backend", "--fallback", "core:builtin"],
    );
    assert_eq!(invalid.status.code(), Some(3));
    let value: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert!(value["attempts"].as_array().unwrap().is_empty());
    std::fs::write(&path, serde_json::to_vec(&fixture::profile(true)).unwrap()).unwrap();
    let result = run(
        &path,
        &root,
        true,
        b"$$z^7$$",
        &["--enable-profile-backend"],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["outcome"]["result"]["candidate"]["content"], "z:7");
    store.set_enabled("fixture.component", false).unwrap();
    let result = run(&path, &root, true, b"x^2", &["--enable-profile-backend"]);
    assert_eq!(result.status.code(), Some(9));
    assert!(result.stdout.is_empty());
    assert!(!store.get("fixture.component").unwrap().unwrap().enabled);
}

#[test]
fn real_cli_linear_profile_preserves_finite_projection_and_default_deny() {
    let (temporary, store) = fixture::store();
    let root = temporary.path().join("store");
    let path = temporary.path().join("profile.json");
    let mut profile = fixture::profile(false);
    profile["operation"] = serde_json::json!({"kind":"importer","output":"unicode-math"});
    std::fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
    let disabled = run_route(&path, &root, "fixture:power", "unicode-math", b"x:2", &[]);
    assert_eq!(disabled.status.code(), Some(6));
    let imported = run_route(
        &path,
        &root,
        "fixture:power",
        "unicode-math",
        b"x:2",
        &["--enable-profile-backend"],
    );
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&imported.stdout).unwrap();
    assert_eq!(
        value["outcome"]["result"]["candidate"]["content"],
        "〖x〗^(2)"
    );
    profile["operation"] = serde_json::json!({"kind":"exporter","input":"ascii-math"});
    std::fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
    let exported = run_route(
        &path,
        &root,
        "ascii-math",
        "fixture:power",
        b"{:y:}^(4)",
        &["--enable-profile-backend"],
    );
    assert!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&exported.stdout).unwrap();
    assert_eq!(value["outcome"]["result"]["candidate"]["content"], "y:4");
    assert!(store.get("fixture.component").unwrap().unwrap().enabled);
}
