use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use latexsnipper_evaluation::formula_corpus::{
    generate_formula_full, generate_formula_pilot, generate_formula_pull_request,
    read_formula_corpus, read_formula_plan, validate_formula_corpus, validate_formula_plan,
    FormulaCorpusError,
};
use latexsnipper_evaluation::formula_evaluation::evaluate_formula_corpus;

#[derive(Debug, Parser)]
#[command(
    name = "formula-corpus",
    about = "Validate and generate the deterministic formula/Office corpus contract"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    ValidatePlan {
        #[arg(long)]
        plan: PathBuf,
    },
    GeneratePilot {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    GeneratePullRequest {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    GenerateFull {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    ValidateCorpus {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        corpus: PathBuf,
    },
    Evaluate {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long)]
        source_commit: String,
        #[arg(long)]
        generated_at_utc: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        require_expected_outcomes: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::ValidatePlan { plan } => {
            let plan = read_formula_plan(&plan)?;
            validate_formula_plan(&plan)?;
            println!(
                "validated formula corpus plan '{}' for {} formulas and {} compound documents",
                plan.id, plan.target_formula_count, plan.compound_document_count
            );
        }
        Command::GeneratePilot { plan, output } => {
            let plan = read_formula_plan(&plan)?;
            validate_formula_plan(&plan)?;
            let corpus = generate_formula_pilot(&plan)?;
            let json = serde_json::to_vec_pretty(&corpus)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&output, json)?;
            println!(
                "generated {} deterministic pilot records at {} (contentSha256={})",
                corpus.records.len(),
                output.display(),
                corpus.content_sha256
            );
            if corpus.content_sha256 != plan.pilot_content_sha256 {
                return Err(Box::new(FormulaCorpusError::Invalid(format!(
                    "pilot digest differs from the frozen plan: expected {}, got {}",
                    plan.pilot_content_sha256, corpus.content_sha256
                ))));
            }
            validate_formula_corpus(&plan, &corpus)?;
        }
        Command::GeneratePullRequest { plan, output } => {
            let plan = read_formula_plan(&plan)?;
            let corpus = generate_formula_pull_request(&plan)?;
            let json = serde_json::to_vec_pretty(&corpus)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&output, json)?;
            println!(
                "generated {} deterministic pull-request records at {} (contentSha256={})",
                corpus.records.len(),
                output.display(),
                corpus.content_sha256
            );
        }
        Command::GenerateFull { plan, output } => {
            let plan = read_formula_plan(&plan)?;
            let corpus = generate_formula_full(&plan)?;
            let json = serde_json::to_vec_pretty(&corpus)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&output, json)?;
            println!(
                "generated {} deterministic full records at {} (contentSha256={})",
                corpus.records.len(),
                output.display(),
                corpus.content_sha256
            );
        }
        Command::ValidateCorpus { plan, corpus } => {
            let plan = read_formula_plan(&plan)?;
            let corpus = read_formula_corpus(&corpus)?;
            validate_formula_corpus(&plan, &corpus)?;
            println!(
                "validated {:?} formula corpus '{}' with {} records",
                corpus.tier,
                corpus.plan_id,
                corpus.records.len()
            );
        }
        Command::Evaluate {
            plan,
            corpus,
            source_commit,
            generated_at_utc,
            output,
            require_expected_outcomes,
        } => {
            let plan = read_formula_plan(&plan)?;
            let corpus = read_formula_corpus(&corpus)?;
            let report = evaluate_formula_corpus(&plan, &corpus, source_commit, generated_at_utc)?;
            let json = serde_json::to_vec_pretty(&report)?;
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&output, json)?;
            println!(
                "evaluated {} records: parse {}/{}, expected outcomes {}/{}, conversions {}/{}, round trips {}/{}; wrote {}",
                report.summary.record_count,
                report.summary.parse_success_count,
                report.summary.record_count,
                report.summary.expected_outcome_match_count,
                report.summary.record_count,
                report.summary.conversion_success_count,
                report.summary.conversion_attempt_count,
                report.summary.round_trip_success_count,
                report.summary.round_trip_attempt_count,
                output.display()
            );
            if require_expected_outcomes
                && report.summary.expected_outcome_match_count != report.summary.record_count
            {
                return Err(Box::new(FormulaCorpusError::Invalid(format!(
                    "expected-outcome gate failed: {}/{} records matched",
                    report.summary.expected_outcome_match_count, report.summary.record_count
                ))));
            }
        }
    }
    Ok(())
}
