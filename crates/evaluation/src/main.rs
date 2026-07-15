use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use latexsnipper_evaluation::schema::{
    CorpusEvidence, EvidenceReport, ExecutionIdentity, GateConfig, GateResult, ModelIdentity,
    PredictionBundle, PredictionSet, SamplePrediction, ThresholdDirection, ValidationTier,
    EVIDENCE_SCHEMA_VERSION, PREDICTION_SCHEMA_VERSION,
};
use latexsnipper_evaluation::validation::{
    compute_manifest_digest, expected_metrics, read_json, selected_corpora, validate_gate_config,
    validate_prediction_bundle,
};
use latexsnipper_evaluation::{
    evaluate_corpus, load_and_validate_index, validate_evidence, ValidationError,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(
    name = "ocr-eval",
    about = "Validate OCR corpora and produce reproducible evidence"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    ContractPredictions {
        #[arg(long)]
        index: PathBuf,
        #[arg(long, value_enum)]
        tier: TierArg,
        #[arg(long)]
        model_spec: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Digest {
        #[arg(long)]
        manifest: PathBuf,
    },
    Validate {
        #[arg(long)]
        index: PathBuf,
    },
    Evaluate {
        #[arg(long)]
        index: PathBuf,
        #[arg(long)]
        predictions: PathBuf,
        #[arg(long)]
        gates: PathBuf,
        #[arg(long, value_enum)]
        tier: TierArg,
        #[arg(long)]
        source_commit: String,
        #[arg(long)]
        generated_at_utc: String,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TierArg {
    PullRequest,
    Scheduled,
    Release,
}

impl From<TierArg> for ValidationTier {
    fn from(value: TierArg) -> Self {
        match value {
            TierArg::PullRequest => Self::PullRequest,
            TierArg::Scheduled => Self::Scheduled,
            TierArg::Release => Self::Release,
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(passed) => {
            if passed {
                ExitCode::SUCCESS
            } else {
                eprintln!("evaluation gates failed");
                ExitCode::from(2)
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<bool, Box<dyn std::error::Error>> {
    match cli.command {
        Command::ContractPredictions {
            index,
            tier,
            model_spec,
            output,
        } => write_contract_predictions(index, tier.into(), model_spec, output),
        Command::Digest { manifest } => {
            let corpus = read_json(&manifest)?;
            println!("{}", compute_manifest_digest(&manifest, &corpus)?);
            Ok(true)
        }
        Command::Validate { index } => {
            let loaded = load_and_validate_index(&index)?;
            println!("validated {} OCR corpora", loaded.corpora.len());
            Ok(true)
        }
        Command::Evaluate {
            index,
            predictions,
            gates,
            tier,
            source_commit,
            generated_at_utc,
            output,
        } => evaluate(EvaluateArgs {
            index,
            predictions,
            gates,
            tier: tier.into(),
            source_commit,
            generated_at_utc,
            output,
        }),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContractOracleSpec {
    schema_version: u32,
    id: String,
    purpose: String,
    behavior: String,
    production_model: bool,
}

fn write_contract_predictions(
    index: PathBuf,
    tier: ValidationTier,
    model_spec: PathBuf,
    output: PathBuf,
) -> Result<bool, Box<dyn std::error::Error>> {
    let loaded = load_and_validate_index(&index)?;
    let spec_bytes = fs::read(&model_spec)?;
    let spec: ContractOracleSpec = serde_json::from_slice(&spec_bytes)?;
    if spec.schema_version != 1
        || spec.id.trim().is_empty()
        || spec.purpose.trim().is_empty()
        || spec.behavior.trim().is_empty()
        || spec.production_model
    {
        return Err(Box::new(ValidationError::Invalid(
            "contract oracle spec must be schema v1, documented, and explicitly non-production"
                .to_string(),
        )));
    }
    let model = ModelIdentity {
        id: spec.id,
        sha256: format!("{:x}", Sha256::digest(&spec_bytes)),
    };
    let execution = ExecutionIdentity {
        runtime: "evaluation-contract-oracle-v1".to_string(),
        provider: "deterministic-fixture".to_string(),
        platform: "portable".to_string(),
        preprocessing_version: "corpus-reference-v1".to_string(),
        postprocessing_version: "none".to_string(),
    };
    let runs = selected_corpora(&loaded, tier)
        .into_iter()
        .map(|corpus| PredictionSet {
            schema_version: PREDICTION_SCHEMA_VERSION,
            corpus_id: corpus.manifest.id.clone(),
            corpus_sha256: corpus.manifest.content_sha256.clone(),
            model: model.clone(),
            execution: execution.clone(),
            predictions: corpus
                .manifest
                .samples
                .iter()
                .map(|sample| SamplePrediction {
                    sample_id: sample.id.clone(),
                    prediction: sample.annotation.clone(),
                })
                .collect(),
        })
        .collect();
    let bundle = PredictionBundle {
        schema_version: PREDICTION_SCHEMA_VERSION,
        tier,
        runs,
    };
    validate_prediction_bundle(&loaded, &bundle, tier)?;
    let json = serde_json::to_vec_pretty(&bundle)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, json)?;
    println!(
        "wrote non-production contract predictions to {}",
        output.display()
    );
    Ok(true)
}

struct EvaluateArgs {
    index: PathBuf,
    predictions: PathBuf,
    gates: PathBuf,
    tier: ValidationTier,
    source_commit: String,
    generated_at_utc: String,
    output: PathBuf,
}

fn evaluate(args: EvaluateArgs) -> Result<bool, Box<dyn std::error::Error>> {
    let loaded = load_and_validate_index(&args.index)?;
    let predictions: PredictionBundle = read_json(&args.predictions)?;
    let gates: GateConfig = read_json(&args.gates)?;
    validate_prediction_bundle(&loaded, &predictions, args.tier)?;
    validate_gate_config(&loaded, &gates, args.tier)?;

    let expected = expected_metrics();
    let mut corpora = Vec::new();
    for corpus in selected_corpora(&loaded, args.tier) {
        let run = predictions
            .runs
            .iter()
            .find(|run| run.corpus_id == corpus.manifest.id)
            .expect("prediction completeness was validated");
        let metrics = evaluate_corpus(&corpus.manifest, run)?;
        let metric_names: BTreeSet<_> = metrics.keys().map(String::as_str).collect();
        let expected_names = expected
            .get(&corpus.manifest.task)
            .expect("every corpus task has a metric contract");
        if &metric_names != expected_names {
            return Err(Box::new(ValidationError::Invalid(format!(
                "metric contract mismatch for task {:?}",
                corpus.manifest.task
            ))));
        }

        let thresholds = gates
            .thresholds
            .get(&corpus.manifest.task)
            .expect("gate completeness was validated");
        let mut gate_results = Vec::new();
        for (name, threshold) in thresholds {
            let actual = metrics.get(name).ok_or_else(|| {
                ValidationError::Invalid(format!(
                    "gate references unknown metric '{name}' for task {:?}",
                    corpus.manifest.task
                ))
            })?;
            let passed = match threshold.direction {
                ThresholdDirection::Minimum => actual.value >= threshold.value,
                ThresholdDirection::Maximum => actual.value <= threshold.value,
            };
            gate_results.push(GateResult {
                metric: name.clone(),
                actual: actual.value,
                threshold: threshold.value,
                direction: threshold.direction,
                passed,
                rationale: threshold.rationale.clone(),
            });
        }
        let passed = gate_results.iter().all(|gate| gate.passed);
        corpora.push(CorpusEvidence {
            corpus_id: corpus.manifest.id.clone(),
            corpus_sha256: corpus.manifest.content_sha256.clone(),
            task: corpus.manifest.task,
            model: run.model.clone(),
            execution: run.execution.clone(),
            metrics,
            gates: gate_results,
            passed,
        });
    }
    let passed = corpora.iter().all(|corpus| corpus.passed);
    let report = EvidenceReport {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        metric_schema_version: 1,
        tier: args.tier,
        source_commit: args.source_commit,
        generated_at_utc: args.generated_at_utc,
        corpora,
        passed,
    };
    validate_evidence(&report)?;
    let json = serde_json::to_vec_pretty(&report)?;
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, json)?;
    println!(
        "wrote {} evidence for {} corpora to {}",
        if passed { "passing" } else { "failing" },
        report.corpora.len(),
        args.output.display()
    );
    Ok(passed)
}
