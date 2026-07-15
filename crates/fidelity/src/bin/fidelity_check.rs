use std::path::PathBuf;

use latexsnipper_fidelity::{load_and_validate_index, run, RunOptions};

fn main() {
    if let Err(error) = execute() {
        eprintln!("fidelity-check: {error}");
        std::process::exit(2);
    }
}

fn execute() -> latexsnipper_fidelity::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    if command != "run" && command != "validate" {
        return Err(latexsnipper_fidelity::FidelityError::InvalidCorpus(
            "usage: fidelity-check <validate|run> --index PATH [--repository-root PATH --output PATH --source-commit SHA --generated-at-utc TIME]".to_string(),
        ));
    }
    let mut index = None;
    let mut repository_root = PathBuf::from(".");
    let mut output = None;
    let mut source_commit = "local".to_string();
    let mut generated_at_utc = "unspecified".to_string();
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| {
            latexsnipper_fidelity::FidelityError::InvalidCorpus(format!("missing value for {flag}"))
        })?;
        match flag.as_str() {
            "--index" => index = Some(PathBuf::from(value)),
            "--repository-root" => repository_root = PathBuf::from(value),
            "--output" => output = Some(PathBuf::from(value)),
            "--source-commit" => source_commit = value,
            "--generated-at-utc" => generated_at_utc = value,
            _ => {
                return Err(latexsnipper_fidelity::FidelityError::InvalidCorpus(
                    format!("unknown option {flag}"),
                ));
            }
        }
    }
    let index_path = index.ok_or_else(|| {
        latexsnipper_fidelity::FidelityError::InvalidCorpus("--index is required".to_string())
    })?;
    let index = load_and_validate_index(&index_path, &repository_root)?;
    if command == "validate" {
        println!("validated {} fidelity corpus cases", index.cases.len());
        return Ok(());
    }
    let output = output.ok_or_else(|| {
        latexsnipper_fidelity::FidelityError::InvalidCorpus("--output is required".to_string())
    })?;
    let report = run(
        &index,
        &RunOptions::ci(repository_root, source_commit, generated_at_utc),
    )?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "wrote {} fidelity case reports to {}",
        report.cases.len(),
        output.display()
    );
    if !report.passed {
        std::process::exit(1);
    }
    Ok(())
}
