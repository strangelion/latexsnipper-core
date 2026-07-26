use std::path::PathBuf;

use clap::Parser;
use latexsnipper_benchmark::table::{
    evaluate_table_benchmark, validate_table_manifest_files, TableBenchmarkManifest,
    TablePredictionBundle,
};

#[derive(Debug, Parser)]
#[command(name = "latexsnipper-table-benchmark")]
struct Args {
    #[arg(long)]
    manifest: PathBuf,
    #[arg(long)]
    predictions: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let manifest: TableBenchmarkManifest = serde_json::from_slice(&std::fs::read(&args.manifest)?)?;
    validate_table_manifest_files(&args.manifest, &manifest)?;
    let predictions: TablePredictionBundle =
        serde_json::from_slice(&std::fs::read(args.predictions)?)?;
    let report = evaluate_table_benchmark(&manifest, &predictions)?;
    std::fs::write(args.output, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
