use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use latexsnipper_evaluation::{compare_int8, DeterministicResultBundle, Int8Thresholds};

#[derive(Debug, Parser)]
#[command(
    name = "int8-contract",
    about = "Compare signed-off FP32 and actual INT8 evidence bundles"
)]
struct Cli {
    #[arg(long)]
    baseline: PathBuf,
    #[arg(long)]
    candidate: PathBuf,
    #[arg(long)]
    thresholds: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(validated) if validated => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(2),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<bool, Box<dyn std::error::Error>> {
    let baseline: DeterministicResultBundle = serde_json::from_slice(&fs::read(cli.baseline)?)?;
    let candidate: DeterministicResultBundle = serde_json::from_slice(&fs::read(cli.candidate)?)?;
    let thresholds: Int8Thresholds = serde_json::from_slice(&fs::read(cli.thresholds)?)?;
    let report = compare_int8(&baseline, &candidate, &thresholds);
    if let Some(parent) = cli.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&cli.output, serde_json::to_vec_pretty(&report)?)?;
    println!(
        "INT8 comparison {:?}; validated={}; evidence={}",
        report.classification, report.validated, report.evidence_sha256
    );
    Ok(report.validated)
}
