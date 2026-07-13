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
