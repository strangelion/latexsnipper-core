use std::path::PathBuf;

use clap::Parser;
use latexsnipper_benchmark::formula::{
    evaluate_formula_benchmark, validate_formula_manifest_files, write_formula_csv,
    FormulaBenchmarkManifest, FormulaPredictionBundle, NormalizationRules,
};
use latexsnipper_benchmark::{run_case, run_incremental_golden_case, BenchmarkCase};

#[derive(Debug, Parser)]
#[command(name = "latexsnipper-benchmark")]
struct Args {
    /// Benchmark case JSON file.
    #[arg(
        long,
        conflicts_with_all = ["golden_case", "formula_manifest", "predictions"]
    )]
    case: Option<PathBuf>,
    /// Directory containing an incremental golden case.
    #[arg(
        long,
        conflicts_with_all = ["case", "formula_manifest", "predictions"]
    )]
    golden_case: Option<PathBuf>,
    /// Versioned formula benchmark manifest.
    #[arg(long, requires = "predictions", conflicts_with_all = ["case", "golden_case"])]
    formula_manifest: Option<PathBuf>,
    /// Formula predictions and reproducibility metadata.
    #[arg(long, requires = "formula_manifest", conflicts_with_all = ["case", "golden_case"])]
    predictions: Option<PathBuf>,
    /// Versioned normalization rules. Required for formula benchmarks.
    #[arg(long, requires = "formula_manifest")]
    normalization: Option<PathBuf>,
    /// Report JSON destination. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Optional CSV report destination for formula benchmarks.
    #[arg(long, requires = "formula_manifest")]
    csv_output: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let output = match (
        args.case,
        args.golden_case,
        args.formula_manifest,
        args.predictions,
    ) {
        (Some(case_path), None, None, None) => {
            let case: BenchmarkCase = serde_json::from_slice(&std::fs::read(case_path)?)?;
            serde_json::to_vec_pretty(&run_case(&case)?)?
        }
        (None, Some(case_dir), None, None) => {
            let result = run_incremental_golden_case(case_dir)?;
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": result.id,
                "passed": true,
                "revision": result.revision,
                "changedStableIds": result.changed_stable_ids,
            }))?
        }
        (None, None, Some(manifest_path), Some(predictions_path)) => {
            let normalization_path = args
                .normalization
                .ok_or("--normalization is required for formula benchmarks")?;
            let manifest: FormulaBenchmarkManifest =
                serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
            validate_formula_manifest_files(&manifest_path, &manifest)?;
            let predictions: FormulaPredictionBundle =
                serde_json::from_slice(&std::fs::read(predictions_path)?)?;
            let normalization: NormalizationRules =
                serde_json::from_slice(&std::fs::read(normalization_path)?)?;
            let report = evaluate_formula_benchmark(&manifest, &predictions, &normalization)?;
            if let Some(csv_path) = args.csv_output.as_deref() {
                write_formula_csv(csv_path, &report)?;
            }
            serde_json::to_vec_pretty(&report)?
        }
        (None, None, None, None) => {
            return Err("one of --case, --golden-case, or --formula-manifest is required".into())
        }
        _ => unreachable!("clap enforces input pairing and conflicts"),
    };
    if let Some(path) = args.output {
        std::fs::write(path, output)?;
    } else {
        println!("{}", String::from_utf8(output)?);
    }
    Ok(())
}
