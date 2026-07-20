use std::path::PathBuf;

use clap::Parser;
use latexsnipper_benchmark::{run_case, run_incremental_golden_case, BenchmarkCase};

#[derive(Debug, Parser)]
#[command(name = "latexsnipper-benchmark")]
struct Args {
    /// Benchmark case JSON file.
    #[arg(long, conflicts_with = "golden_case")]
    case: Option<PathBuf>,
    /// Directory containing an incremental golden case.
    #[arg(long, conflicts_with = "case")]
    golden_case: Option<PathBuf>,
    /// Report JSON destination. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let output = match (args.case, args.golden_case) {
        (Some(case_path), None) => {
            let case: BenchmarkCase = serde_json::from_slice(&std::fs::read(case_path)?)?;
            serde_json::to_vec_pretty(&run_case(&case)?)?
        }
        (None, Some(case_dir)) => {
            let result = run_incremental_golden_case(case_dir)?;
            serde_json::to_vec_pretty(&serde_json::json!({
                "id": result.id,
                "passed": true,
                "revision": result.revision,
                "changedStableIds": result.changed_stable_ids,
            }))?
        }
        (None, None) => return Err("either --case or --golden-case is required".into()),
        (Some(_), Some(_)) => unreachable!("clap enforces conflicts"),
    };
    if let Some(path) = args.output {
        std::fs::write(path, output)?;
    } else {
        println!("{}", String::from_utf8(output)?);
    }
    Ok(())
}
