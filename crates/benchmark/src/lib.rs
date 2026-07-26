//! Executable benchmark contracts for OCR and document workflows.
//!
//! This crate complements `latexsnipper-evaluation`: evaluation measures
//! prediction quality, while benchmark cases measure execution behavior and
//! incremental equivalence under a reproducible case contract.

pub mod formula;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use latexsnipper_ast::{Block, Span};
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use latexsnipper_evaluation::CorpusTask;
use latexsnipper_incremental::{DocumentSession, SessionEdit};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const BENCHMARK_CASE_SCHEMA_VERSION: u32 = 1;
pub const BENCHMARK_REPORT_SCHEMA_VERSION: u32 = 1;
pub const INCREMENTAL_GOLDEN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkCategory {
    Recognition,
    Conversion,
    Roundtrip,
    Fidelity,
    Incremental,
    Performance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaEditCase {
    pub stable_id: String,
    pub latex: String,
}

/// A file-backed, versioned incremental equivalence fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncrementalGoldenCase {
    pub schema_version: u32,
    pub id: String,
    pub initial_source_file: String,
    pub edits_file: String,
    pub expected_document_file: String,
    pub expected_changed_nodes_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedChangedNodes {
    pub revision: u64,
    pub changed_stable_ids: BTreeSet<String>,
    pub reparsed_nodes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncrementalGoldenResult {
    pub id: String,
    pub revision: u64,
    pub changed_stable_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkCase {
    pub schema_version: u32,
    pub id: String,
    pub category: BenchmarkCategory,
    /// Optional link to an existing OCR evaluation corpus task.
    #[serde(default)]
    pub corpus_task: Option<CorpusTask>,
    /// A controlled runner name. New runners are additive.
    pub runner: String,
    #[serde(default = "default_iterations")]
    pub iterations: u32,
    #[serde(default)]
    pub formula: Option<String>,
    #[serde(default)]
    pub output_format: Option<String>,
    #[serde(default)]
    pub initial_latex: Option<String>,
    #[serde(default)]
    pub edits: Vec<FormulaEditCase>,
    #[serde(default)]
    pub formula_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkMetric {
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkResult {
    pub id: String,
    pub category: BenchmarkCategory,
    pub runner: String,
    pub iterations: u32,
    pub passed: bool,
    pub metrics: BTreeMap<String, BenchmarkMetric>,
}

#[derive(Debug, Error)]
pub enum BenchmarkError {
    #[error("unsupported benchmark schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("benchmark case id is required")]
    MissingId,
    #[error("iterations must be greater than zero")]
    InvalidIterations,
    #[error("runner '{0}' is unsupported")]
    UnsupportedRunner(String),
    #[error("benchmark field '{0}' is required")]
    MissingField(&'static str),
    #[error("unknown output format '{0}'")]
    UnknownOutputFormat(String),
    #[error("benchmark execution failed: {0}")]
    Execution(String),
    #[error("golden fixture error at '{path}': {message}")]
    GoldenFixture { path: String, message: String },
}

/// Runs a file-backed incremental golden case rooted at `case_dir`.
///
/// The result must both equal a clean full rebuild and match the checked-in
/// final `Document` plus touched-node expectations.
pub fn run_incremental_golden_case(
    case_dir: impl AsRef<Path>,
) -> Result<IncrementalGoldenResult, BenchmarkError> {
    let case_dir = case_dir.as_ref();
    let manifest: IncrementalGoldenCase = read_json(&case_dir.join("case.json"))?;
    if manifest.schema_version != INCREMENTAL_GOLDEN_SCHEMA_VERSION {
        return Err(BenchmarkError::UnsupportedSchemaVersion(
            manifest.schema_version,
        ));
    }
    if manifest.id.trim().is_empty() {
        return Err(BenchmarkError::MissingId);
    }

    let initial_source = read_text(&case_dir.join(&manifest.initial_source_file))?;
    let edits: Vec<FormulaEditCase> = read_json(&case_dir.join(&manifest.edits_file))?;
    let expected_document: serde_json::Value =
        read_json(&case_dir.join(&manifest.expected_document_file))?;
    let expected_changed: ExpectedChangedNodes =
        read_json(&case_dir.join(&manifest.expected_changed_nodes_file))?;

    let mut session = DocumentSession::from_latex(&manifest.id, initial_source)
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    let mut changed_stable_ids = BTreeSet::new();
    for edit in edits {
        let outcome = session
            .apply_edit(SessionEdit::ReplaceFormulaSource {
                expected_revision: session.revision,
                stable_id: edit.stable_id,
                latex: edit.latex,
            })
            .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
        changed_stable_ids.extend(outcome.invalidation.dirty_nodes);
        if !session
            .verify_full_equivalence()
            .map_err(|error| BenchmarkError::Execution(error.to_string()))?
        {
            return Err(BenchmarkError::Execution(
                "incremental result diverged from full reconcile".to_string(),
            ));
        }
    }

    let actual_document = serde_json::to_value(session.document())
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    if actual_document != expected_document {
        return Err(BenchmarkError::Execution(format!(
            "golden document mismatch for '{}'",
            manifest.id
        )));
    }
    if session.revision != expected_changed.revision
        || changed_stable_ids != expected_changed.changed_stable_ids
        || session.metrics().reparsed_nodes != expected_changed.reparsed_nodes
    {
        return Err(BenchmarkError::Execution(format!(
            "golden touched-node expectation mismatch for '{}'",
            manifest.id
        )));
    }

    Ok(IncrementalGoldenResult {
        id: manifest.id,
        revision: session.revision,
        changed_stable_ids,
    })
}

pub fn validate_case(case: &BenchmarkCase) -> Result<(), BenchmarkError> {
    if case.schema_version != BENCHMARK_CASE_SCHEMA_VERSION {
        return Err(BenchmarkError::UnsupportedSchemaVersion(
            case.schema_version,
        ));
    }
    if case.id.trim().is_empty() {
        return Err(BenchmarkError::MissingId);
    }
    if case.iterations == 0 {
        return Err(BenchmarkError::InvalidIterations);
    }
    match case.runner.as_str() {
        "formula_conversion" => {
            if case.formula.as_deref().is_none_or(str::is_empty) {
                return Err(BenchmarkError::MissingField("formula"));
            }
            if case.output_format.as_deref().is_none_or(str::is_empty) {
                return Err(BenchmarkError::MissingField("outputFormat"));
            }
        }
        "formula_incremental" => {
            if case.initial_latex.as_deref().is_none_or(str::is_empty) {
                return Err(BenchmarkError::MissingField("initialLatex"));
            }
        }
        "incremental_scale" => {
            if case.formula_count.is_none_or(|count| count == 0) {
                return Err(BenchmarkError::MissingField("formulaCount"));
            }
        }
        "incremental_formula_edit_scale" => {
            if case.formula_count.is_none_or(|count| count == 0) {
                return Err(BenchmarkError::MissingField("formulaCount"));
            }
        }
        other => return Err(BenchmarkError::UnsupportedRunner(other.to_string())),
    }
    Ok(())
}

pub fn run_case(case: &BenchmarkCase) -> Result<BenchmarkResult, BenchmarkError> {
    validate_case(case)?;
    match case.runner.as_str() {
        "formula_conversion" => run_formula_conversion(case),
        "formula_incremental" => run_formula_incremental(case),
        "incremental_scale" => run_incremental_scale(case),
        "incremental_formula_edit_scale" => run_incremental_formula_edit_scale(case),
        other => Err(BenchmarkError::UnsupportedRunner(other.to_string())),
    }
}

fn run_formula_conversion(case: &BenchmarkCase) -> Result<BenchmarkResult, BenchmarkError> {
    let formula = case
        .formula
        .as_deref()
        .ok_or(BenchmarkError::MissingField("formula"))?;
    let format = output_format(
        case.output_format
            .as_deref()
            .ok_or(BenchmarkError::MissingField("outputFormat"))?,
    )?;
    let mut samples = Vec::with_capacity(case.iterations as usize);
    for _ in 0..case.iterations {
        let started = Instant::now();
        let output = DocumentConverter::convert_latex_string(formula, format)
            .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
        if output.is_empty() {
            return Err(BenchmarkError::Execution(
                "conversion produced empty output".to_string(),
            ));
        }
        samples.push(started.elapsed());
    }
    Ok(BenchmarkResult {
        id: case.id.clone(),
        category: case.category,
        runner: case.runner.clone(),
        iterations: case.iterations,
        passed: true,
        metrics: latency_metrics(&samples),
    })
}

fn run_formula_incremental(case: &BenchmarkCase) -> Result<BenchmarkResult, BenchmarkError> {
    let initial_latex = case
        .initial_latex
        .as_deref()
        .ok_or(BenchmarkError::MissingField("initialLatex"))?;
    let started = Instant::now();
    let mut session = DocumentSession::from_latex(&case.id, initial_latex)
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    for edit in &case.edits {
        let revision = session.revision;
        session
            .apply_edit(SessionEdit::ReplaceFormulaSource {
                expected_revision: revision,
                stable_id: edit.stable_id.clone(),
                latex: edit.latex.clone(),
            })
            .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
        if !session
            .verify_full_equivalence()
            .map_err(|error| BenchmarkError::Execution(error.to_string()))?
        {
            return Err(BenchmarkError::Execution(
                "incremental result diverged from full reconcile".to_string(),
            ));
        }
    }
    let mut metrics = latency_metrics(&[started.elapsed()]);
    metrics.insert(
        "edits_applied".to_string(),
        count_metric(session.metrics().edits_applied),
    );
    metrics.insert(
        "reparsed_nodes".to_string(),
        count_metric(session.metrics().reparsed_nodes),
    );
    metrics.insert(
        "converted_nodes".to_string(),
        count_metric(session.metrics().converted_nodes),
    );
    metrics.insert(
        "rendered_nodes".to_string(),
        count_metric(session.metrics().rendered_nodes),
    );
    metrics.insert(
        "semantic_cache_hits".to_string(),
        count_metric(session.metrics().semantic_cache_hits),
    );
    metrics.insert(
        "render_cache_hits".to_string(),
        count_metric(session.metrics().render_cache_hits),
    );
    insert_incremental_metrics(&mut metrics, session.metrics());
    Ok(BenchmarkResult {
        id: case.id.clone(),
        category: case.category,
        runner: case.runner.clone(),
        iterations: case.iterations,
        passed: true,
        metrics,
    })
}

fn run_incremental_scale(case: &BenchmarkCase) -> Result<BenchmarkResult, BenchmarkError> {
    let formula_count = case
        .formula_count
        .ok_or(BenchmarkError::MissingField("formulaCount"))?;
    let initial_latex = (0..formula_count)
        .map(|index| format!("$x_{{{index}}}$"))
        .collect::<Vec<_>>()
        .join(" ");
    let started = Instant::now();
    let mut session = DocumentSession::from_latex(&case.id, initial_latex)
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    let last_id = formula_ids(&session)
        .last()
        .cloned()
        .ok_or_else(|| BenchmarkError::Execution("scale source has no formulas".to_string()))?;
    session
        .apply_edit(SessionEdit::ReplaceFormulaSource {
            expected_revision: session.revision,
            stable_id: last_id.clone(),
            latex: format!("y_{{{formula_count}}}"),
        })
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    session
        .convert_formula(&last_id, OutputFormat::OMML)
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    session
        .apply_edit(SessionEdit::ReplaceSourceRange {
            expected_revision: session.revision,
            span: Span::new(0, 0),
            replacement: "$z$ ".to_string(),
        })
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    session
        .convert_formula(&last_id, OutputFormat::OMML)
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    for index in 0..100 {
        session
            .apply_edit(SessionEdit::ReplaceFormulaSource {
                expected_revision: session.revision,
                stable_id: last_id.clone(),
                latex: format!("q_{{{index}}}"),
            })
            .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    }
    if !session
        .verify_full_equivalence()
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?
    {
        return Err(BenchmarkError::Execution(
            "incremental scale result diverged from full parse".to_string(),
        ));
    }
    let mut metrics = latency_metrics(&[started.elapsed()]);
    metrics.insert(
        "formula_count".to_string(),
        count_metric(formula_count as u64),
    );
    insert_incremental_metrics(&mut metrics, session.metrics());
    Ok(BenchmarkResult {
        id: case.id.clone(),
        category: case.category,
        runner: case.runner.clone(),
        iterations: case.iterations,
        passed: true,
        metrics,
    })
}

fn run_incremental_formula_edit_scale(
    case: &BenchmarkCase,
) -> Result<BenchmarkResult, BenchmarkError> {
    let formula_count = case
        .formula_count
        .ok_or(BenchmarkError::MissingField("formulaCount"))?;
    let initial_latex = (0..formula_count)
        .map(|index| format!("$x_{{{index}}}$"))
        .collect::<Vec<_>>()
        .join(" ");
    let setup_started = Instant::now();
    let mut session = DocumentSession::from_latex(&case.id, initial_latex)
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    let last_id = formula_ids(&session)
        .last()
        .cloned()
        .ok_or_else(|| BenchmarkError::Execution("scale source has no formulas".to_string()))?;
    let setup_elapsed = setup_started.elapsed();
    let edit_started = Instant::now();
    session
        .apply_edit(SessionEdit::ReplaceFormulaSource {
            expected_revision: 0,
            stable_id: last_id,
            latex: format!("y_{{{formula_count}}}"),
        })
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?;
    let edit_elapsed = edit_started.elapsed();
    let verify_started = Instant::now();
    if !session
        .verify_full_equivalence()
        .map_err(|error| BenchmarkError::Execution(error.to_string()))?
    {
        return Err(BenchmarkError::Execution(
            "formula fast path diverged from full parse".to_string(),
        ));
    }
    let verify_elapsed = verify_started.elapsed();
    let mut metrics = latency_metrics(&[edit_elapsed]);
    metrics.insert(
        "setup_latency_ns".to_string(),
        duration_metric(setup_elapsed),
    );
    metrics.insert("edit_latency_ns".to_string(), duration_metric(edit_elapsed));
    metrics.insert(
        "verify_latency_ns".to_string(),
        duration_metric(verify_elapsed),
    );
    metrics.insert(
        "formula_count".to_string(),
        count_metric(formula_count as u64),
    );
    insert_incremental_metrics(&mut metrics, session.metrics());
    Ok(BenchmarkResult {
        id: case.id.clone(),
        category: case.category,
        runner: case.runner.clone(),
        iterations: case.iterations,
        passed: true,
        metrics,
    })
}

fn formula_ids(session: &DocumentSession) -> Vec<String> {
    session
        .document()
        .all_blocks()
        .into_iter()
        .filter_map(|block| match block {
            Block::Formula(_) => block.source().and_then(|source| source.stable_id.clone()),
            _ => None,
        })
        .collect()
}

fn insert_incremental_metrics(
    metrics: &mut BTreeMap<String, BenchmarkMetric>,
    session: &latexsnipper_incremental::SessionMetrics,
) {
    for (name, value) in [
        ("edits_applied", session.edits_applied),
        ("reparsed_nodes", session.reparsed_nodes),
        ("converted_nodes", session.converted_nodes),
        ("rendered_nodes", session.rendered_nodes),
        ("semantic_cache_hits", session.semantic_cache_hits),
        ("semantic_cache_misses", session.semantic_cache_misses),
        ("render_cache_hits", session.render_cache_hits),
        ("render_cache_misses", session.render_cache_misses),
        ("semantic_cache_evictions", session.semantic_cache_evictions),
        ("render_cache_evictions", session.render_cache_evictions),
        ("semantic_cache_bytes", session.semantic_cache_bytes),
        ("render_cache_bytes", session.render_cache_bytes),
        ("reconcile_matched_nodes", session.reconcile_matched_nodes),
        ("reconcile_replaced_nodes", session.reconcile_replaced_nodes),
        (
            "cache_evictions",
            session.semantic_cache_evictions + session.render_cache_evictions,
        ),
    ] {
        metrics.insert(name.to_string(), count_metric(value));
    }
}

fn output_format(value: &str) -> Result<OutputFormat, BenchmarkError> {
    match value.to_ascii_lowercase().as_str() {
        "latex" => Ok(OutputFormat::Latex),
        "typst" => Ok(OutputFormat::Typst),
        "mathml" => Ok(OutputFormat::MathML),
        "omml" => Ok(OutputFormat::OMML),
        "markdown" => Ok(OutputFormat::MarkdownBlock),
        "html" => Ok(OutputFormat::Html),
        _ => Err(BenchmarkError::UnknownOutputFormat(value.to_string())),
    }
}

fn latency_metrics(samples: &[Duration]) -> BTreeMap<String, BenchmarkMetric> {
    let mut nanos: Vec<u128> = samples.iter().map(Duration::as_nanos).collect();
    nanos.sort_unstable();
    let sample =
        |quantile: f64| nanos[((nanos.len() - 1) as f64 * quantile).round() as usize] as f64;
    BTreeMap::from([
        (
            "latency_p50_ns".to_string(),
            BenchmarkMetric {
                value: sample(0.50),
                unit: "ns".to_string(),
            },
        ),
        (
            "latency_p95_ns".to_string(),
            BenchmarkMetric {
                value: sample(0.95),
                unit: "ns".to_string(),
            },
        ),
    ])
}

fn count_metric(value: u64) -> BenchmarkMetric {
    BenchmarkMetric {
        value: value as f64,
        unit: "count".to_string(),
    }
}

fn duration_metric(value: Duration) -> BenchmarkMetric {
    BenchmarkMetric {
        value: value.as_nanos() as f64,
        unit: "ns".to_string(),
    }
}

fn default_iterations() -> u32 {
    10
}

fn read_text(path: &Path) -> Result<String, BenchmarkError> {
    fs::read_to_string(path).map_err(|error| BenchmarkError::GoldenFixture {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, BenchmarkError> {
    let contents = read_text(path)?;
    serde_json::from_str(&contents).map_err(|error| BenchmarkError::GoldenFixture {
        path: path.display().to_string(),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_case_produces_latency_metrics() {
        let result = run_case(&BenchmarkCase {
            schema_version: BENCHMARK_CASE_SCHEMA_VERSION,
            id: "formula-omml".to_string(),
            category: BenchmarkCategory::Conversion,
            corpus_task: Some(CorpusTask::PrintedFormula),
            runner: "formula_conversion".to_string(),
            iterations: 2,
            formula: Some(r"\frac{a}{b}".to_string()),
            output_format: Some("omml".to_string()),
            initial_latex: None,
            edits: Vec::new(),
            formula_count: None,
        })
        .unwrap();
        assert!(result.passed);
        assert!(result.metrics.contains_key("latency_p95_ns"));
    }

    #[test]
    fn incremental_case_requires_full_reconcile_equivalence() {
        let result = run_case(&BenchmarkCase {
            schema_version: BENCHMARK_CASE_SCHEMA_VERSION,
            id: "formula-edit".to_string(),
            category: BenchmarkCategory::Incremental,
            corpus_task: Some(CorpusTask::PrintedFormula),
            runner: "formula_incremental".to_string(),
            iterations: 1,
            formula: None,
            output_format: None,
            initial_latex: Some("$x^2$".to_string()),
            edits: vec![FormulaEditCase {
                stable_id: "session:formula-edit:node:0".to_string(),
                latex: "y^3".to_string(),
            }],
            formula_count: None,
        })
        .unwrap();
        assert_eq!(result.metrics["edits_applied"].value, 1.0);
    }

    #[test]
    fn incremental_golden_case_matches_checked_in_document() {
        let case_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmark/golden/incremental/formula-edit-v1");
        let result = run_incremental_golden_case(case_dir).unwrap();
        assert_eq!(result.revision, 1);
        assert_eq!(
            result.changed_stable_ids,
            BTreeSet::from(["session:formula-edit-v1:node:0".to_string()])
        );
    }

    #[test]
    fn scale_case_records_reconcile_and_cache_metrics() {
        let result = run_case(&BenchmarkCase {
            schema_version: BENCHMARK_CASE_SCHEMA_VERSION,
            id: "scale-10".to_string(),
            category: BenchmarkCategory::Performance,
            corpus_task: None,
            runner: "incremental_scale".to_string(),
            iterations: 1,
            formula: None,
            output_format: None,
            initial_latex: None,
            edits: Vec::new(),
            formula_count: Some(10),
        })
        .unwrap();
        assert!(result.metrics["semantic_cache_hits"].value >= 1.0);
        assert!(result.metrics.contains_key("reconcile_matched_nodes"));
    }

    #[test]
    fn formula_edit_scale_touches_one_reparsed_node() {
        let result = run_case(&BenchmarkCase {
            schema_version: BENCHMARK_CASE_SCHEMA_VERSION,
            id: "formula-edit-scale-10".to_string(),
            category: BenchmarkCategory::Performance,
            corpus_task: None,
            runner: "incremental_formula_edit_scale".to_string(),
            iterations: 1,
            formula: None,
            output_format: None,
            initial_latex: None,
            edits: Vec::new(),
            formula_count: Some(10),
        })
        .unwrap();
        assert_eq!(result.metrics["reparsed_nodes"].value, 1.0);
    }
}
