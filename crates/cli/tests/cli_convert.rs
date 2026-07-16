use std::io::Write;
use std::process::{Command, Stdio};

fn workspace() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "latexsnipper-cli-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn snipper() -> Command {
    Command::new(env!("CARGO_BIN_EXE_snipper"))
}

#[test]
fn convert_supports_file_and_stdin_to_stdout() {
    let directory = workspace();
    let input = directory.join("formula.tex");
    std::fs::write(&input, r"\frac{a}{b}").unwrap();

    let output = snipper()
        .args([
            "convert",
            input.to_str().unwrap(),
            "--to",
            "json",
            "-o",
            "-",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("schema_version"));
    let json_input = output.stdout;

    let mut child = snipper()
        .args(["convert", "-", "--to", "json", "-o", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&json_input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("schema_version"));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn advanced_page_range_and_strict_preservation_are_typed_and_reported() {
    let directory = workspace();
    let input = directory.join("formula.tex");
    std::fs::write(&input, "a+b").unwrap();
    let applied = snipper()
        .args([
            "convert",
            input.to_str().unwrap(),
            "--to",
            "json",
            "-o",
            "-",
            "--page-range",
            "1-1",
            "--strict-preservation",
            "--diagnostics",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    assert!(String::from_utf8_lossy(&applied.stderr).contains("CLI_OPTIONS_APPLIED"));

    let invalid = snipper()
        .args([
            "convert",
            input.to_str().unwrap(),
            "--to",
            "json",
            "--page-range",
            "2-1",
        ])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("END >= START"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn binary_stdout_requires_explicit_opt_in() {
    let directory = workspace();
    let input = directory.join("formula.tex");
    std::fs::write(&input, "E=mc^2").unwrap();

    let output = snipper()
        .args([
            "convert",
            input.to_str().unwrap(),
            "--to",
            "docx",
            "-o",
            "-",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("binary stdout is disabled"));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn atomic_output_refuses_clobber_and_leaves_no_temporary_file() {
    let directory = workspace();
    let input = directory.join("formula.tex");
    let output_path = directory.join("result.json");
    std::fs::write(&input, "a+b").unwrap();

    let first = snipper()
        .args([
            "convert",
            input.to_str().unwrap(),
            "--to",
            "json",
            "-o",
            output_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = snipper()
        .args([
            "convert",
            input.to_str().unwrap(),
            "--to",
            "json",
            "-o",
            output_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(second.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&second.stderr).contains("output already exists"));
    let original = std::fs::read(&output_path).unwrap();

    std::fs::write(&input, "c+d").unwrap();
    let replacement = snipper()
        .args([
            "convert",
            input.to_str().unwrap(),
            "--to",
            "json",
            "-o",
            output_path.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        replacement.status.success(),
        "{}",
        String::from_utf8_lossy(&replacement.stderr)
    );
    assert_ne!(std::fs::read(&output_path).unwrap(), original);
    assert!(std::fs::read_dir(&directory).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains(".snipper-")));

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn convert_help_is_generated_from_the_format_registry() {
    let output = snipper().args(["convert", "--help"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for expected in ["latex_display", "docx", "pdf", "xlsx"] {
        assert!(stdout.contains(expected), "missing {expected} in {stdout}");
    }
}

#[test]
fn batch_conversion_preserves_relative_paths_and_writes_a_report() {
    let directory = workspace();
    let input = directory.join("input");
    let nested = input.join("nested");
    let output_dir = directory.join("output");
    let report = directory.join("report.json");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(input.join("first.tex"), "a+b").unwrap();
    std::fs::write(nested.join("second.tex"), r"\frac{c}{d}").unwrap();

    let result = snipper()
        .args([
            "convert",
            input.to_str().unwrap(),
            "--recursive",
            "--to",
            "json",
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--jobs",
            "2",
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(output_dir.join("first.json").is_file());
    assert!(output_dir.join("nested/second.json").is_file());
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["total"], 2);
    assert_eq!(report["successful"], 2);
    assert_eq!(report["failed"], 0);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn batch_continue_on_error_returns_partial_failure_and_complete_report() {
    let directory = workspace();
    let valid = directory.join("valid.tex");
    let missing = directory.join("missing.tex");
    let output_dir = directory.join("output");
    let report = directory.join("report.json");
    std::fs::write(&valid, "x+y").unwrap();

    let result = snipper()
        .args([
            "convert",
            valid.to_str().unwrap(),
            missing.to_str().unwrap(),
            "--to",
            "json",
            "--output-dir",
            output_dir.to_str().unwrap(),
            "--jobs",
            "2",
            "--continue-on-error",
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(10));
    assert!(output_dir.join("valid.json").is_file());
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(report["total"], 2);
    assert_eq!(report["successful"], 1);
    assert_eq!(report["failed"], 1);
    assert_eq!(report["files"].as_array().unwrap().len(), 2);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn completions_and_manpages_are_generated_from_the_cli_schema() {
    for shell in ["bash", "powershell"] {
        let result = snipper().args(["completions", shell]).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(stdout.contains("snipper"));
        assert!(stdout.contains("convert"));
    }

    let directory = workspace();
    let result = snipper()
        .args(["manpages", "--output-dir", directory.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let manpage = std::fs::read_to_string(directory.join("snipper.1")).unwrap();
    assert!(manpage.contains("snipper"));
    assert!(manpage.contains("convert"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn model_purge_requires_confirmation_stays_scoped_and_preserves_manifest() {
    let directory = workspace();
    let models = directory.join("models");
    let first = models.join("formula-det/default");
    let second = models.join("text-rec/base");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(first.join("model.onnx"), b"fixture").unwrap();
    std::fs::write(second.join("model.onnx"), b"fixture").unwrap();
    std::fs::write(models.join("model-manifest.json"), b"{}").unwrap();

    let refused = snipper()
        .current_dir(&directory)
        .args(["models", "purge"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert!(first.is_dir());

    let traversal = snipper()
        .current_dir(&directory)
        .args(["models", "purge", "--category", "../outside", "--yes"])
        .output()
        .unwrap();
    assert_eq!(traversal.status.code(), Some(2));
    assert!(second.is_dir());

    let removed = snipper()
        .current_dir(&directory)
        .args(["models", "purge", "--yes"])
        .output()
        .unwrap();
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(!first.exists());
    assert!(!second.exists());
    assert!(models.join("model-manifest.json").is_file());

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn plugin_management_verifies_installs_disabled_and_reports_tampering() {
    let directory = workspace();
    let package = directory.join("package");
    let store = directory.join("store");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("plugin.wasm"), b"fixture component").unwrap();
    let manifest = serde_json::json!({
        "id": "example.plugin",
        "name": "Example Plugin",
        "version": "1.0.0",
        "pluginApiVersion": 1,
        "abiVersion": 1,
        "coreVersionRequirement": "^2.0.0",
        "capabilities": [],
        "hooks": [],
        "priority": 0,
        "dependencies": [],
        "before": [],
        "after": [],
        "permissions": {},
        "platforms": [],
        "architectures": [],
        "license": "MIT",
        "entrypoint": "plugin.wasm",
        "checksumSha256": "f07a9bfe5aa29b53c1b093fd13fd43d4de7f1af45da70524dc5afb899683b2a3",
        "signature": null,
        "configurationSchema": null,
        "class": "wasi_component"
    });
    std::fs::write(
        package.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let verified = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "verify",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    assert!(String::from_utf8_lossy(&verified.stdout).contains("\"verified\": true"));

    let installed = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "install",
            package.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );
    assert!(String::from_utf8_lossy(&installed.stdout).contains("\"enabled\": false"));

    let enabled = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "enable",
            "example.plugin",
        ])
        .output()
        .unwrap();
    assert!(enabled.status.success());
    let doctor = snipper()
        .args(["plugin", "--store-dir", store.to_str().unwrap(), "doctor"])
        .output()
        .unwrap();
    assert!(doctor.status.success());
    assert!(
        String::from_utf8_lossy(&doctor.stdout).contains("\"wasiComponentHostAvailable\": false")
    );
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("\"nativeProcessOsSandboxed\": false"));
    assert!(String::from_utf8_lossy(&doctor.stdout)
        .contains("\"enforcementScope\": \"brokered-host-operations\""));

    std::fs::write(
        store.join("packages/example.plugin/plugin.wasm"),
        b"tampered",
    )
    .unwrap();
    let tampered = snipper()
        .args(["plugin", "--store-dir", store.to_str().unwrap(), "doctor"])
        .output()
        .unwrap();
    assert_eq!(tampered.status.code(), Some(9));
    assert!(String::from_utf8_lossy(&tampered.stdout).contains("\"verified\": false"));

    let uninstalled = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "uninstall",
            "example.plugin",
        ])
        .output()
        .unwrap();
    assert!(uninstalled.status.success());
    assert!(!store.join("packages/example.plugin").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn signed_registry_cli_requires_https_ids_and_explicit_trust_confirmation() {
    let directory = workspace();
    let store = directory.join("store");

    let insecure = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "registry",
            "add",
            "insecure",
            "http://registry.example.invalid/",
        ])
        .output()
        .unwrap();
    assert_eq!(insecure.status.code(), Some(9));
    assert!(String::from_utf8_lossy(&insecure.stderr).contains("HTTPS"));

    let added = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "registry",
            "add",
            "official",
            "https://registry.example.invalid/",
        ])
        .output()
        .unwrap();
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    assert!(String::from_utf8_lossy(&added.stdout).contains("unverified"));

    let unconfirmed = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "registry",
            "trust",
            "official",
            "missing-root.json",
        ])
        .output()
        .unwrap();
    assert_eq!(unconfirmed.status.code(), Some(9));
    assert!(String::from_utf8_lossy(&unconfirmed.stderr).contains("explicit --yes"));

    let arbitrary_url = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "install",
            "https://attacker.invalid/plugin.zip",
        ])
        .output()
        .unwrap();
    assert_eq!(arbitrary_url.status.code(), Some(9));
    assert!(String::from_utf8_lossy(&arbitrary_url.stderr).contains("arbitrary URL"));

    let update_without_scope = snipper()
        .args(["plugin", "--store-dir", store.to_str().unwrap(), "update"])
        .output()
        .unwrap();
    assert_eq!(update_without_scope.status.code(), Some(9));

    let removed = snipper()
        .args([
            "plugin",
            "--store-dir",
            store.to_str().unwrap(),
            "registry",
            "remove",
            "official",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(removed.status.success());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn migration_writes_a_new_v3_plugin_manifest_and_preserves_source() {
    let directory = workspace();
    let source = directory.join("plugin.json");
    let manifest = latexsnipper_plugin::PluginManifest::built_in("fixture.plugin", "1.0.0");
    let original = serde_json::to_vec_pretty(&manifest).unwrap();
    std::fs::write(&source, &original).unwrap();

    let output = snipper()
        .args([
            "migrate",
            "plugin-manifest",
            source.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let migrated_path = directory.join("plugin.v3.json");
    let migrated: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&migrated_path).unwrap()).unwrap();
    assert_eq!(migrated["schemaVersion"], 3);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["wroteOutput"], true);
    assert_eq!(report["report"]["status"], "migrated");

    std::fs::write(&migrated_path, b"stale").unwrap();
    let replacement = snipper()
        .args([
            "migrate",
            "plugin-manifest",
            source.to_str().unwrap(),
            "--force",
        ])
        .output()
        .unwrap();
    assert!(
        replacement.status.success(),
        "{}",
        String::from_utf8_lossy(&replacement.stderr)
    );
    let replaced: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&migrated_path).unwrap()).unwrap();
    assert_eq!(replaced["schemaVersion"], 3);
    assert_eq!(std::fs::read(&source).unwrap(), original);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn unsafe_model_migration_requires_manual_action_without_writing_output() {
    let directory = workspace();
    let source = directory.join("model-manifest.json");
    std::fs::write(
        &source,
        br#"{
          "sourceId":"fixture",
          "sourceLabel":"Fixture",
          "version":"2.0.0",
          "baseUrl":"https://example.invalid",
          "categories":{"formula":{"required":true,"default":"fixture","variants":[{"id":"fixture","files":["model.onnx"]}]}}
        }"#,
    )
    .unwrap();

    let output = snipper()
        .args([
            "migrate",
            "model-manifest",
            source.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(11));
    assert!(!directory.join("model-manifest.v3.json").exists());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["wroteOutput"], false);
    assert_eq!(report["report"]["status"], "requires_manual_action");

    let inspected = snipper()
        .args(["migrate", "inspect", source.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(inspected.status.code(), Some(11));
    let inspected_report: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(
        inspected_report["report"]["status"],
        "requires_manual_action"
    );

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn capability_json_defaults_to_v3_envelope_and_keeps_explicit_v2_adapter() {
    let v3 = snipper()
        .args(["capabilities", "--format", "json"])
        .output()
        .unwrap();
    assert!(v3.status.success());
    let v3: serde_json::Value = serde_json::from_slice(&v3.stdout).unwrap();
    assert_eq!(v3["ok"], true);
    assert_eq!(v3["versions"]["apiEnvelopeVersion"], 3);
    assert_eq!(v3["versions"]["capabilitySchemaVersion"], 3);

    let v2 = snipper()
        .args(["capabilities", "--format", "json", "--api-version", "2"])
        .output()
        .unwrap();
    assert!(v2.status.success());
    let v2: serde_json::Value = serde_json::from_slice(&v2.stdout).unwrap();
    assert!(v2.get("versions").is_none());
    assert!(v2.get("capabilities").is_some());
}

#[test]
fn cli_rejects_options_that_would_otherwise_be_ignored() {
    let unknown_capability_format = snipper()
        .args(["capabilities", "--format", "yaml"])
        .output()
        .unwrap();
    assert_eq!(unknown_capability_format.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unknown_capability_format.stderr)
        .contains("unsupported capability output format"));

    let ignored_capability_api = snipper()
        .args(["capabilities", "--format", "table", "--api-version", "3"])
        .output()
        .unwrap();
    assert_eq!(ignored_capability_api.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&ignored_capability_api.stderr)
        .contains("only meaningful with --format json"));

    let conflicting_model_scope = snipper()
        .args(["models", "download", "--category", "formula-det", "--all"])
        .output()
        .unwrap();
    assert_eq!(conflicting_model_scope.status.code(), Some(2));
}
