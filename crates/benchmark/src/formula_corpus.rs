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
use std::fmt::Write as _;

use crate::process_memory::peak_resident_set_bytes;

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

/// Render a stable, user-readable view of the benchmark evidence. The report
/// deliberately labels package/application-only capabilities as deferred
/// instead of promoting an unmeasured path to "supported".
pub fn render_formula_corpus_benchmark_markdown(
    plan: &FormulaCorpusPlan,
    report: &FormulaCorpusBenchmarkReport,
    environment_label: &str,
) -> String {
    let summary = &report.summary;
    let mut output = String::new();
    writeln!(output, "# Formula and Office corpus benchmark").unwrap();
    writeln!(output).unwrap();
    writeln!(
        output,
        "> Generated evidence. Contract pass rates are not model accuracy on independent real-world data."
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(output, "## Evidence identity").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "| Field | Value |").unwrap();
    writeln!(output, "|---|---|").unwrap();
    writeln!(output, "| Plan | `{}` |", markdown_cell(&report.plan_id)).unwrap();
    writeln!(output, "| Tier | `{:?}` |", report.tier).unwrap();
    writeln!(output, "| Corpus SHA-256 | `{}` |", report.corpus_sha256).unwrap();
    writeln!(output, "| Source commit | `{}` |", report.source_commit).unwrap();
    writeln!(
        output,
        "| Generated at (UTC) | `{}` |",
        report.generated_at_utc
    )
    .unwrap();
    writeln!(
        output,
        "| Environment | {} |",
        markdown_cell(environment_label)
    )
    .unwrap();
    writeln!(output, "| Seed | `{}` |", plan.seed).unwrap();
    writeln!(output, "| Formula records | `{}` |", summary.record_count).unwrap();
    writeln!(
        output,
        "| Planned compound documents | `{}` |",
        plan.compound_document_count
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(output, "## Contract results").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "| Measurement | Result | Status |").unwrap();
    writeln!(output, "|---|---:|---|").unwrap();
    writeln!(
        output,
        "| Expected outcomes | {}/{} ({}) | **Verified** |",
        summary.expected_outcome_match_count,
        summary.record_count,
        percentage(summary.expected_outcome_match_rate)
    )
    .unwrap();
    writeln!(
        output,
        "| Parse success | {} | **Verified** |",
        percentage(summary.parse_success_rate)
    )
    .unwrap();
    writeln!(
        output,
        "| Attempted semantic conversions | {} | **Verified** |",
        percentage(summary.conversion_success_rate)
    )
    .unwrap();
    writeln!(
        output,
        "| Attempted semantic round trips | {} | **Verified** |",
        percentage(summary.round_trip_success_rate)
    )
    .unwrap();
    writeln!(
        output,
        "| Visual/Office targets | {} deferred observations | **Not measured here** |",
        summary.deferred_target_count
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(output, "## Performance").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "| Stage | P50 | P95 | P99 |").unwrap();
    writeln!(output, "|---|---:|---:|---:|").unwrap();
    write_latency_row(&mut output, "Parse", &summary.parse_latency);
    write_latency_row(&mut output, "Conversion", &summary.conversion_latency);
    write_latency_row(&mut output, "Round trip", &summary.round_trip_latency);
    writeln!(output).unwrap();
    writeln!(
        output,
        "- Total measured time: **{}**",
        duration(summary.total_elapsed_ns)
    )
    .unwrap();
    match summary.records_per_second {
        Some(rate) => writeln!(output, "- Throughput: **{rate:.2} records/s**").unwrap(),
        None => writeln!(output, "- Throughput: **Not measured**").unwrap(),
    }
    match summary.peak_memory_bytes {
        Some(bytes) => writeln!(
            output,
            "- Peak process resident memory (lifetime high-water mark): **{bytes} bytes ({:.2} MiB)**",
            bytes as f64 / (1024.0 * 1024.0)
        )
        .unwrap(),
        None => writeln!(output, "- Peak process memory: **Not measured**").unwrap(),
    }
    writeln!(output).unwrap();
    writeln!(output, "## Capability boundary").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "| Capability | Status | Evidence boundary |").unwrap();
    writeln!(output, "|---|---|---|").unwrap();
    writeln!(output, "| Formula parsing and declared error outcomes | **Verified** | Deterministic compositional corpus |").unwrap();
    writeln!(output, "| LaTeX, MathML, OMML and Typst semantic conversion | **Verified** | Attempted Core conversions only |").unwrap();
    writeln!(
        output,
        "| Semantic conversion back to LaTeX | **Verified** | Attempted Core round trips only |"
    )
    .unwrap();
    writeln!(output, "| SVG, PNG and Office package visual fidelity | **Deferred** | Requires visual/package evidence layers |").unwrap();
    writeln!(output, "| Word, Excel and PowerPoint application behavior | **Not measured** | Requires installed-Office automation |").unwrap();
    writeln!(output, "| OLE activation, clipboard paste and field recalculation | **Not measured** | Requires the external Office harness |").unwrap();
    writeln!(output, "| Real-world formula/model accuracy | **Not claimed** | Requires a licensed representative corpus |").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "## Known limitations").unwrap();
    writeln!(output).unwrap();
    for limitation in &summary.limitations {
        writeln!(output, "- {}", markdown_cell(limitation)).unwrap();
    }
    writeln!(output).unwrap();
    writeln!(output, "## Reproduce").unwrap();
    writeln!(output).unwrap();
    writeln!(output, "See [`docs/benchmark.md`](../benchmark.md) for the digest-frozen generation and benchmark commands.").unwrap();
    output
}

fn write_latency_row(output: &mut String, label: &str, latency: &FormulaLatencySummary) {
    writeln!(
        output,
        "| {label} | {} | {} | {} |",
        duration(latency.p50_ns),
        duration(latency.p95_ns),
        duration(latency.p99_ns)
    )
    .unwrap();
}

fn duration(nanoseconds: u64) -> String {
    if nanoseconds >= 1_000_000_000 {
        format!("{:.3} s", nanoseconds as f64 / 1_000_000_000.0)
    } else if nanoseconds >= 1_000_000 {
        format!("{:.3} ms", nanoseconds as f64 / 1_000_000.0)
    } else if nanoseconds >= 1_000 {
        format!("{:.3} us", nanoseconds as f64 / 1_000.0)
    } else {
        format!("{nanoseconds} ns")
    }
}

fn percentage(rate: f64) -> String {
    format!("{:.2}%", rate * 100.0)
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string()
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
    let peak_memory_bytes = peak_resident_set_bytes();
    let mut limitations = vec![
        "peak resident memory is the process lifetime high-water mark and includes startup, corpus loading, evaluation, and report construction".to_string(),
        "the deterministic full tier is synthetic contract-scale evidence, not accuracy on 10,000 independent real-world formulas".to_string(),
        "Microsoft Office application fidelity requires the external Office harness".to_string(),
    ];
    if peak_memory_bytes.is_none() {
        limitations.insert(
            0,
            "peak process memory is unavailable on this runner platform".to_string(),
        );
    }
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
            peak_memory_bytes,
            limitations,
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
        #[cfg(any(windows, unix))]
        assert!(benchmark
            .summary
            .peak_memory_bytes
            .is_some_and(|bytes| bytes > 0));
        assert!(benchmark
            .summary
            .limitations
            .iter()
            .any(|limitation| limitation.contains("lifetime high-water mark")));

        let markdown = render_formula_corpus_benchmark_markdown(
            &plan,
            &benchmark,
            "test-os-x86_64 | rustc test",
        );
        assert!(markdown.contains("# Formula and Office corpus benchmark"));
        assert!(markdown.contains("test-os-x86_64 \\| rustc test"));
        assert!(markdown.contains("**Verified**"));
        assert!(markdown.contains("**Not measured**"));
        assert!(markdown.contains("**Not claimed**"));
        #[cfg(any(windows, unix))]
        assert!(markdown.contains("lifetime high-water mark"));
        assert!(markdown.contains("Contract pass rates are not model accuracy"));
        assert!(markdown.ends_with('\n'));
    }

    #[test]
    fn markdown_helpers_format_stable_human_readable_values() {
        assert_eq!(duration(999), "999 ns");
        assert_eq!(duration(1_500), "1.500 us");
        assert_eq!(duration(1_500_000), "1.500 ms");
        assert_eq!(duration(1_500_000_000), "1.500 s");
        assert_eq!(percentage(0.9375), "93.75%");
        assert_eq!(markdown_cell("a|b\nc"), "a\\|b c");
    }
}
