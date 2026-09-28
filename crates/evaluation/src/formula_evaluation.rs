use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use latexsnipper_ast::{Diagnostic, DiagnosticLevel};
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use latexsnipper_syntax::latex::LatexParser;
use latexsnipper_syntax::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::formula_corpus::{
    validate_formula_corpus, ExpectedFormulaOutcome, FormulaCategory, FormulaCorpus,
    FormulaCorpusError, FormulaCorpusPlan, FormulaCorpusRecord, FormulaCorpusTier,
    FormulaOutputTarget,
};

pub const FORMULA_EVALUATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaStageStatus {
    Success,
    Error,
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedFormulaOutcome {
    Accepted,
    AcceptedWithDiagnostics,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaDiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaDiagnosticEvidence {
    pub level: FormulaDiagnosticLevel,
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    pub span_start: Option<usize>,
    pub span_end: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaStageEvidence {
    pub status: FormulaStageStatus,
    pub elapsed_ns: u64,
    pub output_bytes: Option<usize>,
    pub output_sha256: Option<String>,
    pub diagnostic_count: usize,
    pub diagnostics: Vec<FormulaDiagnosticEvidence>,
    pub error: Option<String>,
    pub deferred_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaConversionEvidence {
    pub target: FormulaOutputTarget,
    pub conversion: FormulaStageEvidence,
    pub round_trip_to_latex: Option<FormulaStageEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaRecordEvidence {
    pub record_id: String,
    pub category: FormulaCategory,
    pub expected_outcome: ExpectedFormulaOutcome,
    pub observed_outcome: ObservedFormulaOutcome,
    pub expected_outcome_matched: bool,
    pub parse: FormulaStageEvidence,
    pub conversions: Vec<FormulaConversionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaLatencySummary {
    pub sample_count: usize,
    pub min_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaEvaluationSummary {
    pub record_count: usize,
    pub valid_record_count: usize,
    pub malformed_record_count: usize,
    pub parse_success_count: usize,
    pub expected_outcome_match_count: usize,
    pub conversion_attempt_count: usize,
    pub conversion_success_count: usize,
    pub round_trip_attempt_count: usize,
    pub round_trip_success_count: usize,
    pub deferred_target_count: usize,
    pub total_elapsed_ns: u64,
    pub parse_latency: FormulaLatencySummary,
    pub conversion_latency: FormulaLatencySummary,
    pub round_trip_latency: FormulaLatencySummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaEvaluationReport {
    pub schema_version: u32,
    pub plan_id: String,
    pub corpus_sha256: String,
    pub tier: FormulaCorpusTier,
    pub source_commit: String,
    pub generated_at_utc: String,
    pub summary: FormulaEvaluationSummary,
    pub records: Vec<FormulaRecordEvidence>,
}

#[derive(Debug, Error)]
pub enum FormulaEvaluationError {
    #[error(transparent)]
    Corpus(#[from] FormulaCorpusError),
    #[error("invalid formula evaluation report: {0}")]
    Invalid(String),
}

pub fn evaluate_formula_corpus(
    plan: &FormulaCorpusPlan,
    corpus: &FormulaCorpus,
    source_commit: String,
    generated_at_utc: String,
) -> Result<FormulaEvaluationReport, FormulaEvaluationError> {
    validate_formula_corpus(plan, corpus)?;
    validate_source_commit(&source_commit)?;
    validate_generated_at(&generated_at_utc)?;

    let started = Instant::now();
    let records = corpus
        .records
        .iter()
        .map(evaluate_record)
        .collect::<Vec<_>>();
    let summary = summarize(&records, started.elapsed());
    let report = FormulaEvaluationReport {
        schema_version: FORMULA_EVALUATION_SCHEMA_VERSION,
        plan_id: plan.id.clone(),
        corpus_sha256: corpus.content_sha256.clone(),
        tier: corpus.tier,
        source_commit,
        generated_at_utc,
        summary,
        records,
    };
    validate_formula_evaluation_report(plan, corpus, &report)?;
    Ok(report)
}

pub fn validate_formula_evaluation_report(
    plan: &FormulaCorpusPlan,
    corpus: &FormulaCorpus,
    report: &FormulaEvaluationReport,
) -> Result<(), FormulaEvaluationError> {
    validate_formula_corpus(plan, corpus)?;
    if report.schema_version != FORMULA_EVALUATION_SCHEMA_VERSION {
        return invalid(format!(
            "schemaVersion must be {FORMULA_EVALUATION_SCHEMA_VERSION}, got {}",
            report.schema_version
        ));
    }
    if report.plan_id != plan.id {
        return invalid("report planId does not match the formula corpus plan");
    }
    if report.corpus_sha256 != corpus.content_sha256 {
        return invalid("report corpusSha256 does not match the evaluated corpus");
    }
    if report.tier != corpus.tier {
        return invalid("report tier does not match the evaluated corpus");
    }
    validate_source_commit(&report.source_commit)?;
    validate_generated_at(&report.generated_at_utc)?;
    if report.records.len() != corpus.records.len()
        || report.summary.record_count != corpus.records.len()
    {
        return invalid("report must contain exactly one result for every corpus record");
    }
    let expected_ids: BTreeSet<_> = corpus
        .records
        .iter()
        .map(|record| record.id.as_str())
        .collect();
    let actual_ids: BTreeSet<_> = report
        .records
        .iter()
        .map(|record| record.record_id.as_str())
        .collect();
    if actual_ids.len() != report.records.len() || actual_ids != expected_ids {
        return invalid("report record IDs must exactly match the evaluated corpus");
    }
    for record in &report.records {
        validate_stage_diagnostics(&record.parse, &record.record_id, "parse")?;
        for conversion in &record.conversions {
            validate_stage_diagnostics(&conversion.conversion, &record.record_id, "conversion")?;
            if let Some(round_trip) = &conversion.round_trip_to_latex {
                validate_stage_diagnostics(round_trip, &record.record_id, "round-trip")?;
            }
        }
    }
    if report.summary.valid_record_count + report.summary.malformed_record_count
        != report.summary.record_count
    {
        return invalid("valid and malformed record counts must sum to recordCount");
    }
    if report.summary.parse_success_count > report.summary.record_count
        || report.summary.expected_outcome_match_count > report.summary.record_count
        || report.summary.conversion_success_count > report.summary.conversion_attempt_count
        || report.summary.round_trip_success_count > report.summary.round_trip_attempt_count
    {
        return invalid("report summary contains impossible success counts");
    }
    if report.summary.parse_latency.sample_count != report.summary.record_count
        || report.summary.conversion_latency.sample_count != report.summary.conversion_attempt_count
        || report.summary.round_trip_latency.sample_count != report.summary.round_trip_attempt_count
    {
        return invalid("report latency sample counts do not match stage attempt counts");
    }
    Ok(())
}

fn evaluate_record(record: &FormulaCorpusRecord) -> FormulaRecordEvidence {
    let parse_started = Instant::now();
    let parsed = LatexParser.parse(&record.source);
    let (observed_outcome, parse) = match parsed {
        Ok(document) => {
            let diagnostic_count = document.diagnostics.len();
            let diagnostics = document
                .diagnostics
                .iter()
                .map(formula_diagnostic_evidence)
                .collect::<Vec<_>>();
            let has_fatal_diagnostic = document.diagnostics.iter().any(|diagnostic| {
                diagnostic.level == DiagnosticLevel::Error && !diagnostic.recoverable
            });
            let observed = if has_fatal_diagnostic {
                ObservedFormulaOutcome::Rejected
            } else if diagnostic_count == 0 {
                ObservedFormulaOutcome::Accepted
            } else {
                ObservedFormulaOutcome::AcceptedWithDiagnostics
            };
            (
                observed,
                FormulaStageEvidence {
                    status: FormulaStageStatus::Success,
                    elapsed_ns: elapsed_ns(parse_started.elapsed()),
                    output_bytes: None,
                    output_sha256: None,
                    diagnostic_count,
                    diagnostics,
                    error: None,
                    deferred_reason: None,
                },
            )
        }
        Err(error) => (
            ObservedFormulaOutcome::Rejected,
            FormulaStageEvidence {
                status: FormulaStageStatus::Error,
                elapsed_ns: elapsed_ns(parse_started.elapsed()),
                output_bytes: None,
                output_sha256: None,
                diagnostic_count: 0,
                diagnostics: Vec::new(),
                error: Some(error.to_string()),
                deferred_reason: None,
            },
        ),
    };
    let expected_outcome_matched = match record.expected_outcome {
        ExpectedFormulaOutcome::Valid => observed_outcome == ObservedFormulaOutcome::Accepted,
        ExpectedFormulaOutcome::RecoverableError => {
            observed_outcome == ObservedFormulaOutcome::AcceptedWithDiagnostics
        }
        ExpectedFormulaOutcome::Reject => observed_outcome == ObservedFormulaOutcome::Rejected,
    };

    let conversions = record
        .output_targets
        .iter()
        .copied()
        .map(|target| evaluate_conversion(record, target))
        .collect();
    FormulaRecordEvidence {
        record_id: record.id.clone(),
        category: record.category,
        expected_outcome: record.expected_outcome,
        observed_outcome,
        expected_outcome_matched,
        parse,
        conversions,
    }
}

fn evaluate_conversion(
    record: &FormulaCorpusRecord,
    target: FormulaOutputTarget,
) -> FormulaConversionEvidence {
    let Some(format) = semantic_output_format(target) else {
        return FormulaConversionEvidence {
            target,
            conversion: deferred_stage("requires visual or Office export evidence layer"),
            round_trip_to_latex: None,
        };
    };
    let Some(source) = record.normalized_source.as_deref() else {
        return FormulaConversionEvidence {
            target,
            conversion: deferred_stage("malformed input is evaluated by parser diagnostics only"),
            round_trip_to_latex: None,
        };
    };

    let started = Instant::now();
    match DocumentConverter::convert_latex_string(source, format) {
        Ok(output) if !output.trim().is_empty() => {
            let conversion = successful_stage(started.elapsed(), &output);
            let round_trip_to_latex = round_trip(target, &output);
            FormulaConversionEvidence {
                target,
                conversion,
                round_trip_to_latex,
            }
        }
        Ok(_) => FormulaConversionEvidence {
            target,
            conversion: error_stage(started.elapsed(), "conversion produced empty output"),
            round_trip_to_latex: None,
        },
        Err(error) => FormulaConversionEvidence {
            target,
            conversion: error_stage(started.elapsed(), error.to_string()),
            round_trip_to_latex: None,
        },
    }
}

fn round_trip(target: FormulaOutputTarget, output: &str) -> Option<FormulaStageEvidence> {
    let started = Instant::now();
    let result = match target {
        FormulaOutputTarget::Mathml => {
            DocumentConverter::convert_mathml_string(output, OutputFormat::Latex)
        }
        FormulaOutputTarget::Omml => {
            DocumentConverter::convert_omml_string(output, OutputFormat::Latex)
        }
        FormulaOutputTarget::Typst => {
            DocumentConverter::convert_typst_string(output, OutputFormat::Latex)
        }
        _ => return None,
    };
    Some(match result {
        Ok(round_trip) if !round_trip.trim().is_empty() => {
            successful_stage(started.elapsed(), &round_trip)
        }
        Ok(_) => error_stage(started.elapsed(), "round-trip produced empty LaTeX"),
        Err(error) => error_stage(started.elapsed(), error.to_string()),
    })
}

fn semantic_output_format(target: FormulaOutputTarget) -> Option<OutputFormat> {
    match target {
        FormulaOutputTarget::Latex => Some(OutputFormat::Latex),
        FormulaOutputTarget::Mathml => Some(OutputFormat::MathML),
        FormulaOutputTarget::Omml => Some(OutputFormat::OMML),
        FormulaOutputTarget::Typst => Some(OutputFormat::Typst),
        FormulaOutputTarget::Svg
        | FormulaOutputTarget::Png
        | FormulaOutputTarget::Docx
        | FormulaOutputTarget::Pptx
        | FormulaOutputTarget::Xlsx => None,
    }
}

fn summarize(records: &[FormulaRecordEvidence], elapsed: Duration) -> FormulaEvaluationSummary {
    let parse_latencies = records
        .iter()
        .map(|record| record.parse.elapsed_ns)
        .collect::<Vec<_>>();
    let conversion_latencies = records
        .iter()
        .flat_map(|record| &record.conversions)
        .filter(|conversion| conversion.conversion.status != FormulaStageStatus::Deferred)
        .map(|conversion| conversion.conversion.elapsed_ns)
        .collect::<Vec<_>>();
    let round_trip_latencies = records
        .iter()
        .flat_map(|record| &record.conversions)
        .filter_map(|conversion| conversion.round_trip_to_latex.as_ref())
        .map(|round_trip| round_trip.elapsed_ns)
        .collect::<Vec<_>>();
    let mut summary = FormulaEvaluationSummary {
        record_count: records.len(),
        valid_record_count: 0,
        malformed_record_count: 0,
        parse_success_count: 0,
        expected_outcome_match_count: 0,
        conversion_attempt_count: 0,
        conversion_success_count: 0,
        round_trip_attempt_count: 0,
        round_trip_success_count: 0,
        deferred_target_count: 0,
        total_elapsed_ns: elapsed_ns(elapsed),
        parse_latency: summarize_latencies(parse_latencies),
        conversion_latency: summarize_latencies(conversion_latencies),
        round_trip_latency: summarize_latencies(round_trip_latencies),
    };
    for record in records {
        if record.category == FormulaCategory::Malformed {
            summary.malformed_record_count += 1;
        } else {
            summary.valid_record_count += 1;
        }
        if record.parse.status == FormulaStageStatus::Success {
            summary.parse_success_count += 1;
        }
        if record.expected_outcome_matched {
            summary.expected_outcome_match_count += 1;
        }
        for conversion in &record.conversions {
            match conversion.conversion.status {
                FormulaStageStatus::Success => {
                    summary.conversion_attempt_count += 1;
                    summary.conversion_success_count += 1;
                }
                FormulaStageStatus::Error => summary.conversion_attempt_count += 1,
                FormulaStageStatus::Deferred => summary.deferred_target_count += 1,
            }
            if let Some(round_trip) = &conversion.round_trip_to_latex {
                summary.round_trip_attempt_count += 1;
                if round_trip.status == FormulaStageStatus::Success {
                    summary.round_trip_success_count += 1;
                }
            }
        }
    }
    summary
}

fn summarize_latencies(mut samples: Vec<u64>) -> FormulaLatencySummary {
    if samples.is_empty() {
        return FormulaLatencySummary {
            sample_count: 0,
            min_ns: 0,
            p50_ns: 0,
            p95_ns: 0,
            p99_ns: 0,
            max_ns: 0,
        };
    }
    samples.sort_unstable();
    FormulaLatencySummary {
        sample_count: samples.len(),
        min_ns: samples[0],
        p50_ns: nearest_rank(&samples, 50),
        p95_ns: nearest_rank(&samples, 95),
        p99_ns: nearest_rank(&samples, 99),
        max_ns: samples[samples.len() - 1],
    }
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    let rank = (sorted.len() * percentile).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn successful_stage(elapsed: Duration, output: &str) -> FormulaStageEvidence {
    FormulaStageEvidence {
        status: FormulaStageStatus::Success,
        elapsed_ns: elapsed_ns(elapsed),
        output_bytes: Some(output.len()),
        output_sha256: Some(format!("{:x}", Sha256::digest(output.as_bytes()))),
        diagnostic_count: 0,
        diagnostics: Vec::new(),
        error: None,
        deferred_reason: None,
    }
}

fn error_stage(elapsed: Duration, error: impl Into<String>) -> FormulaStageEvidence {
    FormulaStageEvidence {
        status: FormulaStageStatus::Error,
        elapsed_ns: elapsed_ns(elapsed),
        output_bytes: None,
        output_sha256: None,
        diagnostic_count: 0,
        diagnostics: Vec::new(),
        error: Some(error.into()),
        deferred_reason: None,
    }
}

fn deferred_stage(reason: impl Into<String>) -> FormulaStageEvidence {
    FormulaStageEvidence {
        status: FormulaStageStatus::Deferred,
        elapsed_ns: 0,
        output_bytes: None,
        output_sha256: None,
        diagnostic_count: 0,
        diagnostics: Vec::new(),
        error: None,
        deferred_reason: Some(reason.into()),
    }
}

fn formula_diagnostic_evidence(diagnostic: &Diagnostic) -> FormulaDiagnosticEvidence {
    let span = diagnostic.source.as_ref().and_then(|source| source.span);
    FormulaDiagnosticEvidence {
        level: match diagnostic.level {
            DiagnosticLevel::Info => FormulaDiagnosticLevel::Info,
            DiagnosticLevel::Warning => FormulaDiagnosticLevel::Warning,
            DiagnosticLevel::Error => FormulaDiagnosticLevel::Error,
        },
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        recoverable: diagnostic.recoverable,
        span_start: span.map(|span| span.start),
        span_end: span.map(|span| span.end),
    }
}

fn validate_stage_diagnostics(
    stage: &FormulaStageEvidence,
    record_id: &str,
    stage_name: &str,
) -> Result<(), FormulaEvaluationError> {
    if stage.diagnostic_count != stage.diagnostics.len() {
        return invalid(format!(
            "record '{record_id}' {stage_name} diagnosticCount does not match diagnostics"
        ));
    }
    for diagnostic in &stage.diagnostics {
        if diagnostic.code.trim().is_empty() || diagnostic.message.trim().is_empty() {
            return invalid(format!(
                "record '{record_id}' {stage_name} diagnostics require code and message"
            ));
        }
        if diagnostic
            .span_start
            .zip(diagnostic.span_end)
            .is_some_and(|(start, end)| start >= end)
        {
            return invalid(format!(
                "record '{record_id}' {stage_name} diagnostic span must be non-empty"
            ));
        }
    }
    Ok(())
}

fn elapsed_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

fn validate_source_commit(value: &str) -> Result<(), FormulaEvaluationError> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        invalid("sourceCommit must be a lowercase 40- or 64-character object ID")
    } else {
        Ok(())
    }
}

fn validate_generated_at(value: &str) -> Result<(), FormulaEvaluationError> {
    if value.len() < 20
        || !value.ends_with('Z')
        || value.as_bytes().get(4) != Some(&b'-')
        || value.as_bytes().get(7) != Some(&b'-')
        || value.as_bytes().get(10) != Some(&b'T')
        || value.as_bytes().get(13) != Some(&b':')
        || value.as_bytes().get(16) != Some(&b':')
    {
        invalid("generatedAtUtc must be an RFC 3339 UTC timestamp ending in Z")
    } else {
        Ok(())
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, FormulaEvaluationError> {
    Err(FormulaEvaluationError::Invalid(message.into()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::formula_corpus::{generate_formula_pilot, read_formula_plan};

    use super::*;

    fn plan_and_pilot() -> (FormulaCorpusPlan, FormulaCorpus) {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../evaluation/formula-corpus/plan.json");
        let plan = read_formula_plan(&path).expect("checked-in plan must parse");
        let pilot = generate_formula_pilot(&plan).expect("pilot generation must succeed");
        (plan, pilot)
    }

    #[test]
    fn pilot_evaluation_records_every_stage_without_panicking() {
        let (plan, pilot) = plan_and_pilot();
        let report = evaluate_formula_corpus(
            &plan,
            &pilot,
            "a".repeat(40),
            "2026-09-28T00:00:00Z".to_string(),
        )
        .expect("pilot evaluation should succeed");
        assert_eq!(report.summary.record_count, pilot.records.len());
        assert_eq!(report.summary.valid_record_count, 10);
        assert_eq!(report.summary.malformed_record_count, 2);
        assert!(report.summary.conversion_attempt_count > 0);
        assert!(report.summary.deferred_target_count > 0);
        assert_eq!(
            report.summary.parse_latency.sample_count,
            pilot.records.len()
        );
        assert_eq!(
            report.summary.conversion_latency.sample_count,
            report.summary.conversion_attempt_count
        );
        assert!(report.summary.parse_latency.p99_ns >= report.summary.parse_latency.p50_ns);
        assert_eq!(
            report.summary.expected_outcome_match_count,
            report.summary.record_count
        );
        let recoverable = report
            .records
            .iter()
            .find(|record| record.record_id.ends_with("malformed-recoverable"))
            .expect("recoverable malformed record should be present");
        assert_eq!(recoverable.parse.diagnostics.len(), 1);
        assert_eq!(
            recoverable.parse.diagnostics[0].code,
            "W_LATEX_UNCLOSED_GROUP"
        );
        assert!(recoverable.parse.diagnostics[0].recoverable);
        assert!(recoverable.parse.diagnostics[0].span_start.is_some());
        let rejected = report
            .records
            .iter()
            .find(|record| record.record_id.ends_with("malformed-reject"))
            .expect("rejected malformed record should be present");
        assert_eq!(rejected.observed_outcome, ObservedFormulaOutcome::Rejected);
        assert_eq!(
            rejected.parse.diagnostics[0].code,
            "E_LATEX_UNCLOSED_ENVIRONMENT"
        );
        assert!(!rejected.parse.diagnostics[0].recoverable);
        validate_formula_evaluation_report(&plan, &pilot, &report)
            .expect("generated report should validate");
    }

    #[test]
    fn report_validation_rejects_missing_records() {
        let (plan, pilot) = plan_and_pilot();
        let mut report = evaluate_formula_corpus(
            &plan,
            &pilot,
            "b".repeat(40),
            "2026-09-28T00:00:00Z".to_string(),
        )
        .expect("pilot evaluation should succeed");
        report.records.pop();
        let error = validate_formula_evaluation_report(&plan, &pilot, &report)
            .expect_err("incomplete reports must be rejected");
        assert!(error.to_string().contains("exactly one result"));
    }
}
