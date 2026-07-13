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
