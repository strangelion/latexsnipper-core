//! Benchmark summary for the deterministic formula/Office corpus.
//!
//! The evaluation crate owns record-level semantic evidence. This module
//! derives a compact performance and quality summary from the same run so the
//! 10,000-record tier is not evaluated twice.

use latexsnipper_evaluation::formula_corpus::{
    FormulaCorpus, FormulaCorpusPlan, FormulaCorpusTier,
};
use latexsnipper_evaluation::formula_evaluation::{
    evaluate_formula_corpus, FormulaEvaluationError, FormulaEvaluationReport, FormulaLatencySummary,
};
use serde::{Deserialize, Serialize};

pub const FORMULA_CORPUS_BENCHMARK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCorpusBenchmarkSummary {
    pub record_count: usize,
    pub expected_outcome_match_count: usize,
    pub expected_outcome_match_rate: f64,
    pub parse_success_rate: f64,
    pub conversion_success_rate: f64,
    pub round_trip_success_rate: f64,
    pub deferred_target_count: usize,
    pub total_elapsed_ns: u64,
    pub records_per_second: Option<f64>,
    pub parse_latency: FormulaLatencySummary,
    pub conversion_latency: FormulaLatencySummary,
    pub round_trip_latency: FormulaLatencySummary,
    pub peak_memory_bytes: Option<u64>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaCorpusBenchmarkReport {
    pub schema_version: u32,
    pub plan_id: String,
    pub corpus_sha256: String,
    pub tier: FormulaCorpusTier,
    pub source_commit: String,
    pub generated_at_utc: String,
    pub summary: FormulaCorpusBenchmarkSummary,
}

/// Evaluate the corpus once and return both the record-level evidence and a
/// compact benchmark summary derived from that exact run.
pub fn run_formula_corpus_benchmark(
    plan: &FormulaCorpusPlan,
    corpus: &FormulaCorpus,
    source_commit: String,
    generated_at_utc: String,
) -> Result<(FormulaCorpusBenchmarkReport, FormulaEvaluationReport), FormulaEvaluationError> {
    let evaluation = evaluate_formula_corpus(
        plan,
        corpus,
        source_commit.clone(),
        generated_at_utc.clone(),
    )?;
    let summary = &evaluation.summary;
    let benchmark = FormulaCorpusBenchmarkReport {
        schema_version: FORMULA_CORPUS_BENCHMARK_SCHEMA_VERSION,
        plan_id: evaluation.plan_id.clone(),
        corpus_sha256: evaluation.corpus_sha256.clone(),
        tier: evaluation.tier,
        source_commit,
        generated_at_utc,
        summary: FormulaCorpusBenchmarkSummary {
            record_count: summary.record_count,
            expected_outcome_match_count: summary.expected_outcome_match_count,
            expected_outcome_match_rate: rate(
                summary.expected_outcome_match_count,
                summary.record_count,
            ),
            parse_success_rate: rate(summary.parse_success_count, summary.record_count),
            conversion_success_rate: rate(
                summary.conversion_success_count,
                summary.conversion_attempt_count,
            ),
            round_trip_success_rate: rate(
                summary.round_trip_success_count,
                summary.round_trip_attempt_count,
            ),
            deferred_target_count: summary.deferred_target_count,
            total_elapsed_ns: summary.total_elapsed_ns,
            records_per_second: (summary.total_elapsed_ns > 0).then(|| {
                summary.record_count as f64 * 1_000_000_000.0 / summary.total_elapsed_ns as f64
            }),
            parse_latency: summary.parse_latency.clone(),
            conversion_latency: summary.conversion_latency.clone(),
            round_trip_latency: summary.round_trip_latency.clone(),
            peak_memory_bytes: None,
            limitations: vec![
                "peak process memory is not measured by the portable corpus runner".to_string(),
                "the deterministic full tier is synthetic contract-scale evidence, not accuracy on 10,000 independent real-world formulas".to_string(),
                "Microsoft Office application fidelity requires the external Office harness".to_string(),
            ],
        },
    };
    Ok((benchmark, evaluation))
}

fn rate(success: usize, attempts: usize) -> f64 {
    if attempts == 0 {
        1.0
    } else {
        success as f64 / attempts as f64
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use latexsnipper_evaluation::formula_corpus::{generate_formula_pilot, read_formula_plan};

    use super::*;

    #[test]
    fn pilot_runner_emits_truthful_rates_percentiles_and_limitations() {
        let repository_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let plan = read_formula_plan(&repository_root.join("evaluation/formula-corpus/plan.json"))
            .unwrap();
        let corpus = generate_formula_pilot(&plan).unwrap();
        let (benchmark, evaluation) = run_formula_corpus_benchmark(
            &plan,
            &corpus,
            "0000000000000000000000000000000000000000".to_string(),
            "2026-09-28T00:00:00Z".to_string(),
        )
        .unwrap();

        assert_eq!(benchmark.schema_version, 1);
        assert_eq!(benchmark.summary.record_count, corpus.records.len());
        assert_eq!(
            benchmark.summary.expected_outcome_match_count,
            evaluation.summary.expected_outcome_match_count
        );
        assert!((0.0..=1.0).contains(&benchmark.summary.expected_outcome_match_rate));
        assert!(benchmark.summary.parse_latency.p50_ns <= benchmark.summary.parse_latency.p95_ns);
        assert!(benchmark.summary.parse_latency.p95_ns <= benchmark.summary.parse_latency.p99_ns);
        assert!(benchmark.summary.records_per_second.is_some());
        assert_eq!(benchmark.summary.peak_memory_bytes, None);
        assert!(benchmark
            .summary
            .limitations
            .iter()
            .any(|limitation| limitation.contains("not measured")));
    }
}
