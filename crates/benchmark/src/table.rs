//! Versioned table-recognition benchmark with an ordered-tree TEDS metric.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const TABLE_BENCHMARK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableBenchmarkManifest {
    pub schema_version: u32,
    pub dataset_id: String,
    pub dataset_version: String,
    #[serde(default = "minimum_table_samples")]
    pub minimum_sample_count: usize,
    pub samples: Vec<TableBenchmarkSample>,
}

fn minimum_table_samples() -> usize {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableBenchmarkSample {
    pub id: String,
    pub image: String,
    pub image_sha256: String,
    pub source: String,
    pub license: String,
    pub categories: Vec<String>,
    pub cells: Vec<TableCellTruth>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCellTruth {
    pub row: usize,
    pub column: usize,
    pub rowspan: usize,
    pub colspan: usize,
    pub kind: TableCellKind,
    pub content: String,
    pub reading_order: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableCellKind {
    Empty,
    Text,
    Formula,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TablePredictionBundle {
    pub schema_version: u32,
    pub dataset_id: String,
    pub dataset_version: String,
    pub core_commit: String,
    pub model_id: String,
    pub model_sha256: String,
    pub runtime: String,
    pub provider: String,
    pub predictions: Vec<TablePrediction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TablePrediction {
    pub sample_id: String,
    pub cells: Vec<TableCellTruth>,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableMetrics {
    pub sample_count: usize,
    pub teds: f64,
    pub structure_exact_match: f64,
    pub rowspan_accuracy: f64,
    pub colspan_accuracy: f64,
    pub cell_count_accuracy: f64,
    pub empty_cell_accuracy: f64,
    pub cell_text_character_error_rate: f64,
    pub cell_formula_normalized_exact_match: f64,
    pub reading_order_accuracy: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableBenchmarkReport {
    pub schema_version: u32,
    pub dataset_id: String,
    pub dataset_version: String,
    pub core_commit: String,
    pub model_id: String,
    pub model_sha256: String,
    pub runtime: String,
    pub provider: String,
    pub metrics: TableMetrics,
    pub latency_by_cell_count: BTreeMap<String, TableMetrics>,
    pub samples: Vec<TableSampleResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableSampleResult {
    pub sample_id: String,
    pub expected_cell_count: usize,
    pub actual_cell_count: usize,
    pub teds: f64,
    pub structure_exact_match: bool,
    pub rowspan_correct: usize,
    pub colspan_correct: usize,
    pub matched_cells: usize,
    pub empty_correct: usize,
    pub empty_total: usize,
    pub text_character_edits: usize,
    pub text_character_count: usize,
    pub formula_correct: usize,
    pub formula_total: usize,
    pub reading_order_correct: bool,
    pub latency_ms: f64,
}

#[derive(Debug, Error)]
pub enum TableBenchmarkError {
    #[error("unsupported table benchmark schema {0}")]
    UnsupportedSchema(u32),
    #[error("table benchmark requires at least {required} samples, found {actual}")]
    TooFewSamples { required: usize, actual: usize },
    #[error("prediction bundle dataset does not match the manifest")]
    DatasetMismatch,
    #[error("missing prediction for '{0}'")]
    MissingPrediction(String),
    #[error("duplicate prediction for '{0}'")]
    DuplicatePrediction(String),
    #[error("invalid latency for '{0}'")]
    InvalidLatency(String),
    #[error("unsafe image path for '{0}'")]
    UnsafeImagePath(String),
    #[error("image checksum mismatch for '{0}'")]
    ImageHashMismatch(String),
    #[error("image I/O for '{sample}': {source}")]
    ImageIo {
        sample: String,
        source: std::io::Error,
    },
}

pub fn evaluate_table_benchmark(
    manifest: &TableBenchmarkManifest,
    bundle: &TablePredictionBundle,
) -> Result<TableBenchmarkReport, TableBenchmarkError> {
    validate_table_contract(manifest, bundle)?;
    let predictions = bundle
        .predictions
        .iter()
        .map(|prediction| (prediction.sample_id.as_str(), prediction))
        .collect::<HashMap<_, _>>();
    let samples = manifest
        .samples
        .iter()
        .map(|sample| {
            evaluate_sample(
                sample,
                predictions
                    .get(sample.id.as_str())
                    .expect("validated prediction"),
            )
        })
        .collect::<Vec<_>>();
    let metrics = aggregate(samples.iter().collect());
    let mut grouped: BTreeMap<String, Vec<&TableSampleResult>> = BTreeMap::new();
    for sample in &samples {
        grouped
            .entry(cell_count_bucket(sample.expected_cell_count).to_owned())
            .or_default()
            .push(sample);
    }
    Ok(TableBenchmarkReport {
        schema_version: TABLE_BENCHMARK_SCHEMA_VERSION,
        dataset_id: manifest.dataset_id.clone(),
        dataset_version: manifest.dataset_version.clone(),
        core_commit: bundle.core_commit.clone(),
        model_id: bundle.model_id.clone(),
        model_sha256: bundle.model_sha256.clone(),
        runtime: bundle.runtime.clone(),
        provider: bundle.provider.clone(),
        metrics,
        latency_by_cell_count: grouped
            .into_iter()
            .map(|(key, samples)| (key, aggregate(samples)))
            .collect(),
        samples,
    })
}

pub fn validate_table_manifest_files(
    manifest_path: &Path,
    manifest: &TableBenchmarkManifest,
) -> Result<(), TableBenchmarkError> {
    let root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    for sample in &manifest.samples {
        let relative = Path::new(&sample.image);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(TableBenchmarkError::UnsafeImagePath(sample.id.clone()));
        }
        let bytes =
            fs::read(root.join(relative)).map_err(|source| TableBenchmarkError::ImageIo {
                sample: sample.id.clone(),
                source,
            })?;
        if format!("{:x}", Sha256::digest(bytes)) != sample.image_sha256 {
            return Err(TableBenchmarkError::ImageHashMismatch(sample.id.clone()));
        }
    }
    Ok(())
}

fn validate_table_contract(
    manifest: &TableBenchmarkManifest,
    bundle: &TablePredictionBundle,
) -> Result<(), TableBenchmarkError> {
    if manifest.schema_version != TABLE_BENCHMARK_SCHEMA_VERSION
        || bundle.schema_version != TABLE_BENCHMARK_SCHEMA_VERSION
    {
        return Err(TableBenchmarkError::UnsupportedSchema(
            manifest.schema_version.max(bundle.schema_version),
        ));
    }
    if manifest.samples.len() < manifest.minimum_sample_count {
        return Err(TableBenchmarkError::TooFewSamples {
            required: manifest.minimum_sample_count,
            actual: manifest.samples.len(),
        });
    }
    if manifest.dataset_id != bundle.dataset_id
        || manifest.dataset_version != bundle.dataset_version
    {
        return Err(TableBenchmarkError::DatasetMismatch);
    }
    let mut predictions = HashMap::new();
    for prediction in &bundle.predictions {
        if !prediction.latency_ms.is_finite() || prediction.latency_ms < 0.0 {
            return Err(TableBenchmarkError::InvalidLatency(
                prediction.sample_id.clone(),
            ));
        }
        if predictions
            .insert(prediction.sample_id.as_str(), prediction)
            .is_some()
        {
            return Err(TableBenchmarkError::DuplicatePrediction(
                prediction.sample_id.clone(),
            ));
        }
    }
    for sample in &manifest.samples {
        if !predictions.contains_key(sample.id.as_str()) {
            return Err(TableBenchmarkError::MissingPrediction(sample.id.clone()));
        }
    }
    Ok(())
}

fn evaluate_sample(
    sample: &TableBenchmarkSample,
    prediction: &TablePrediction,
) -> TableSampleResult {
    let expected = cells_by_position(&sample.cells);
    let actual = cells_by_position(&prediction.cells);
    let mut matched = 0;
    let mut rowspan_correct = 0;
    let mut colspan_correct = 0;
    let mut empty_correct = 0;
    let mut empty_total = 0;
    let mut text_edits = 0;
    let mut text_count = 0;
    let mut formula_correct = 0;
    let mut formula_total = 0;
    for (position, expected) in &expected {
        let actual = actual.get(position);
        if let Some(actual) = actual {
            matched += 1;
            rowspan_correct += usize::from(expected.rowspan == actual.rowspan);
            colspan_correct += usize::from(expected.colspan == actual.colspan);
        }
        match expected.kind {
            TableCellKind::Empty => {
                empty_total += 1;
                empty_correct += usize::from(actual.is_some_and(|cell| {
                    cell.kind == TableCellKind::Empty || cell.content.trim().is_empty()
                }));
            }
            TableCellKind::Text => {
                let expected_chars = expected.content.chars().collect::<Vec<_>>();
                let actual_chars = actual
                    .map(|cell| cell.content.chars().collect::<Vec<_>>())
                    .unwrap_or_default();
                text_edits += levenshtein(&expected_chars, &actual_chars);
                text_count += expected_chars.len();
            }
            TableCellKind::Formula => {
                formula_total += 1;
                formula_correct += usize::from(actual.is_some_and(|cell| {
                    normalize_formula(&expected.content) == normalize_formula(&cell.content)
                }));
            }
        }
    }
    let structure_exact_match = sample.cells.len() == prediction.cells.len()
        && sample.cells.iter().all(|cell| {
            actual
                .get(&(cell.row, cell.column))
                .is_some_and(|other| cell.rowspan == other.rowspan && cell.colspan == other.colspan)
        });
    let expected_order = reading_order(&sample.cells);
    let actual_order = reading_order(&prediction.cells);
    TableSampleResult {
        sample_id: sample.id.clone(),
        expected_cell_count: sample.cells.len(),
        actual_cell_count: prediction.cells.len(),
        teds: table_teds(&sample.cells, &prediction.cells),
        structure_exact_match,
        rowspan_correct,
        colspan_correct,
        matched_cells: matched,
        empty_correct,
        empty_total,
        text_character_edits: text_edits,
        text_character_count: text_count,
        formula_correct,
        formula_total,
        reading_order_correct: expected_order == actual_order,
        latency_ms: prediction.latency_ms,
    }
}

fn aggregate(samples: Vec<&TableSampleResult>) -> TableMetrics {
    if samples.is_empty() {
        return TableMetrics::default();
    }
    let count = samples.len() as f64;
    let mut latencies = samples
        .iter()
        .map(|sample| sample.latency_ms)
        .collect::<Vec<_>>();
    latencies.sort_by(f64::total_cmp);
    let ratio = |numerator: usize, denominator: usize| numerator as f64 / denominator.max(1) as f64;
    let matched = samples.iter().map(|sample| sample.matched_cells).sum();
    let cell_count_accuracy = samples
        .iter()
        .map(|sample| {
            1.0 - sample
                .expected_cell_count
                .abs_diff(sample.actual_cell_count) as f64
                / sample
                    .expected_cell_count
                    .max(sample.actual_cell_count)
                    .max(1) as f64
        })
        .sum::<f64>()
        / count;
    TableMetrics {
        sample_count: samples.len(),
        teds: samples.iter().map(|sample| sample.teds).sum::<f64>() / count,
        structure_exact_match: samples
            .iter()
            .filter(|sample| sample.structure_exact_match)
            .count() as f64
            / count,
        rowspan_accuracy: ratio(
            samples.iter().map(|sample| sample.rowspan_correct).sum(),
            matched,
        ),
        colspan_accuracy: ratio(
            samples.iter().map(|sample| sample.colspan_correct).sum(),
            matched,
        ),
        cell_count_accuracy,
        empty_cell_accuracy: ratio(
            samples.iter().map(|sample| sample.empty_correct).sum(),
            samples.iter().map(|sample| sample.empty_total).sum(),
        ),
        cell_text_character_error_rate: ratio(
            samples
                .iter()
                .map(|sample| sample.text_character_edits)
                .sum(),
            samples
                .iter()
                .map(|sample| sample.text_character_count)
                .sum(),
        ),
        cell_formula_normalized_exact_match: ratio(
            samples.iter().map(|sample| sample.formula_correct).sum(),
            samples.iter().map(|sample| sample.formula_total).sum(),
        ),
        reading_order_accuracy: samples
            .iter()
            .filter(|sample| sample.reading_order_correct)
            .count() as f64
            / count,
        latency_p50_ms: quantile(&latencies, 0.50),
        latency_p95_ms: quantile(&latencies, 0.95),
    }
}

/// Ordered tree-edit similarity over table → row → cell → content nodes.
fn table_teds(expected: &[TableCellTruth], actual: &[TableCellTruth]) -> f64 {
    let expected_rows = rows(expected);
    let actual_rows = rows(actual);
    let distance = sequence_distance(
        &expected_rows,
        &actual_rows,
        |row| row_size(row),
        |left, right| row_distance(left, right),
    );
    let normalizer = (1 + expected_rows.iter().map(|row| row_size(row)).sum::<usize>())
        .max(1 + actual_rows.iter().map(|row| row_size(row)).sum::<usize>());
    (1.0 - distance / normalizer.max(1) as f64).clamp(0.0, 1.0)
}

fn rows(cells: &[TableCellTruth]) -> Vec<Vec<&TableCellTruth>> {
    let mut rows = BTreeMap::<usize, Vec<&TableCellTruth>>::new();
    for cell in cells {
        rows.entry(cell.row).or_default().push(cell);
    }
    rows.into_values()
        .map(|mut cells| {
            cells.sort_by_key(|cell| cell.column);
            cells
        })
        .collect()
}

fn row_size(row: &[&TableCellTruth]) -> usize {
    1 + row.len() * 2
}

fn row_distance(left: &[&TableCellTruth], right: &[&TableCellTruth]) -> f64 {
    sequence_distance(
        left,
        right,
        |_| 2,
        |left, right| {
            let span_cost =
                f64::from(left.rowspan != right.rowspan) + f64::from(left.colspan != right.colspan);
            let kind_cost = f64::from(left.kind != right.kind);
            let left_chars = left.content.chars().collect::<Vec<_>>();
            let right_chars = right.content.chars().collect::<Vec<_>>();
            let text_cost = levenshtein(&left_chars, &right_chars) as f64
                / left_chars.len().max(right_chars.len()).max(1) as f64;
            (span_cost + kind_cost + text_cost).min(2.0)
        },
    )
}

fn sequence_distance<T>(
    left: &[T],
    right: &[T],
    size: impl Fn(&T) -> usize,
    substitute: impl Fn(&T, &T) -> f64,
) -> f64 {
    let mut previous = vec![0.0; right.len() + 1];
    for index in 0..right.len() {
        previous[index + 1] = previous[index] + size(&right[index]) as f64;
    }
    let mut current = vec![0.0; right.len() + 1];
    for left_item in left {
        current[0] = previous[0] + size(left_item) as f64;
        for right_index in 0..right.len() {
            current[right_index + 1] = (previous[right_index + 1] + size(left_item) as f64)
                .min(current[right_index] + size(&right[right_index]) as f64)
                .min(previous[right_index] + substitute(left_item, &right[right_index]));
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

fn cells_by_position(cells: &[TableCellTruth]) -> HashMap<(usize, usize), &TableCellTruth> {
    cells
        .iter()
        .map(|cell| ((cell.row, cell.column), cell))
        .collect()
}

fn reading_order(cells: &[TableCellTruth]) -> Vec<(usize, usize)> {
    let mut cells = cells.iter().collect::<Vec<_>>();
    cells.sort_by_key(|cell| cell.reading_order);
    cells.iter().map(|cell| (cell.row, cell.column)).collect()
}

fn normalize_formula(value: &str) -> String {
    value.split_ascii_whitespace().collect()
}

fn cell_count_bucket(count: usize) -> &'static str {
    match count {
        0..=20 => "small",
        21..=100 => "medium",
        _ => "large",
    }
}

fn quantile(sorted: &[f64], fraction: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

fn levenshtein<T: Eq>(left: &[T], right: &[T]) -> usize {
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_value) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_value) in right.iter().enumerate() {
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + usize::from(left_value != right_value));
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(row: usize, column: usize, content: &str) -> TableCellTruth {
        TableCellTruth {
            row,
            column,
            rowspan: 1,
            colspan: 1,
            kind: TableCellKind::Text,
            content: content.to_owned(),
            reading_order: row * 2 + column,
        }
    }

    #[test]
    fn ordered_tree_teds_is_exact_for_identical_tables() {
        let cells = vec![cell(0, 0, "a"), cell(0, 1, "b"), cell(1, 0, "c")];
        assert_eq!(table_teds(&cells, &cells), 1.0);
    }

    #[test]
    fn ordered_tree_teds_penalizes_span_and_content_changes() {
        let expected = vec![cell(0, 0, "a"), cell(0, 1, "b")];
        let mut actual = expected.clone();
        actual[0].rowspan = 2;
        actual[1].content = "wrong".to_owned();
        let score = table_teds(&expected, &actual);
        assert!(score > 0.0 && score < 1.0);
    }
}
