use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use latexsnipper_benchmark::formula_corpus::{
    render_formula_corpus_benchmark_markdown, run_formula_corpus_benchmark,
};
use latexsnipper_evaluation::formula_corpus::{read_formula_corpus, read_formula_plan};

#[derive(Debug, Parser)]
#[command(
    name = "latexsnipper-formula-corpus-benchmark",
    about = "Run the deterministic formula corpus once and emit evaluation plus benchmark evidence"
)]
struct Args {
    #[arg(long)]
    plan: PathBuf,
    #[arg(long)]
    corpus: PathBuf,
    #[arg(long)]
    source_commit: String,
    #[arg(long)]
    generated_at_utc: String,
    /// Compact performance and quality summary destination.
    #[arg(long)]
    output: PathBuf,
    /// Optional record-level evaluation evidence destination.
    #[arg(long)]
    evaluation_output: Option<PathBuf>,
    /// Optional user-readable Markdown evidence destination.
    #[arg(long)]
    markdown_output: Option<PathBuf>,
    /// Human-readable runner identity written to Markdown evidence.
    #[arg(long)]
    environment_label: Option<String>,
    /// Fail when any record does not match its frozen expected outcome.
    #[arg(long)]
    require_expected_outcomes: bool,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args
        .evaluation_output
        .as_ref()
        .is_some_and(|path| path == &args.output)
    {
        return Err("--output and --evaluation-output must be different files".into());
    }
    if args
        .markdown_output
        .as_ref()
        .is_some_and(|path| path == &args.output)
        || args.markdown_output.as_ref().is_some_and(|path| {
            args.evaluation_output
                .as_ref()
                .is_some_and(|evaluation| evaluation == path)
        })
    {
        return Err("--markdown-output must differ from JSON output paths".into());
    }
    let plan = read_formula_plan(&args.plan)?;
    let corpus = read_formula_corpus(&args.corpus)?;
    let (benchmark, evaluation) =
        run_formula_corpus_benchmark(&plan, &corpus, args.source_commit, args.generated_at_utc)?;

    write_json(&args.output, &benchmark)?;
    if let Some(path) = args.evaluation_output.as_deref() {
        write_json(path, &evaluation)?;
    }
    if let Some(path) = args.markdown_output.as_deref() {
        let default_environment_label =
            format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
        let environment_label = args
            .environment_label
            .as_deref()
            .unwrap_or(&default_environment_label);
        let markdown =
            render_formula_corpus_benchmark_markdown(&plan, &benchmark, environment_label);
        write_text(path, &markdown)?;
    }
    println!(
        "benchmarked {} records: expected outcomes {}/{}, {:.2} records/s; wrote {}",
        benchmark.summary.record_count,
        benchmark.summary.expected_outcome_match_count,
        benchmark.summary.record_count,
        benchmark.summary.records_per_second.unwrap_or_default(),
        args.output.display()
    );
    if args.require_expected_outcomes
        && benchmark.summary.expected_outcome_match_count != benchmark.summary.record_count
    {
        return Err(format!(
            "expected-outcome gate failed: {}/{} records matched",
            benchmark.summary.expected_outcome_match_count, benchmark.summary.record_count
        )
        .into());
    }
    Ok(())
}

fn write_json(
    path: &Path,
    value: &impl serde::Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn write_text(path: &Path, value: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, value)?;
    Ok(())
}
