use std::collections::{BTreeMap, BTreeSet, HashMap};

use thiserror::Error;

use crate::schema::{
    Annotation, CorpusManifest, CorpusTask, MetricValue, PredictionSet, Region, TableCellValue,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MetricError {
    #[error("prediction is missing for sample '{0}'")]
    MissingPrediction(String),
    #[error("duplicate prediction for sample '{0}'")]
    DuplicatePrediction(String),
    #[error("annotation kind mismatch for sample '{0}'")]
    AnnotationKindMismatch(String),
    #[error("prediction references unknown sample '{0}'")]
    UnknownSample(String),
}

pub fn evaluate_corpus(
    corpus: &CorpusManifest,
    predictions: &PredictionSet,
) -> Result<BTreeMap<String, MetricValue>, MetricError> {
    let mut by_id = HashMap::new();
    for prediction in &predictions.predictions {
        if by_id
            .insert(prediction.sample_id.as_str(), &prediction.prediction)
            .is_some()
        {
            return Err(MetricError::DuplicatePrediction(
                prediction.sample_id.clone(),
            ));
        }
    }
    let sample_ids: BTreeSet<_> = corpus
        .samples
        .iter()
        .map(|sample| sample.id.as_str())
        .collect();
    if let Some(unknown) = by_id.keys().find(|id| !sample_ids.contains(**id)) {
        return Err(MetricError::UnknownSample((*unknown).to_string()));
    }

    match corpus.task {
        CorpusTask::LatinText
        | CorpusTask::SimplifiedChineseText
        | CorpusTask::MixedCjkLatinText => evaluate_text(corpus, &by_id),
        CorpusTask::PrintedFormula | CorpusTask::HandwrittenFormula => {
            evaluate_formula(corpus, &by_id)
        }
        CorpusTask::DocumentLayout => evaluate_layout(corpus, &by_id),
        CorpusTask::TableStructure => evaluate_table(corpus, &by_id),
        CorpusTask::Orientation => evaluate_orientation(corpus, &by_id),
        CorpusTask::MixedFormulaText => evaluate_document(corpus, &by_id),
    }
}

fn evaluate_text(
    corpus: &CorpusManifest,
    predictions: &HashMap<&str, &Annotation>,
) -> Result<BTreeMap<String, MetricValue>, MetricError> {
    let mut char_edits = 0usize;
    let mut char_total = 0usize;
    let mut word_edits = 0usize;
    let mut word_total = 0usize;
    for sample in &corpus.samples {
        let predicted = predictions
            .get(sample.id.as_str())
            .ok_or_else(|| MetricError::MissingPrediction(sample.id.clone()))?;
        let (Annotation::Text { text: expected }, Annotation::Text { text: actual }) =
            (&sample.annotation, *predicted)
        else {
            return Err(MetricError::AnnotationKindMismatch(sample.id.clone()));
        };
        let expected_chars: Vec<_> = expected.chars().collect();
        let actual_chars: Vec<_> = actual.chars().collect();
        char_edits += levenshtein(&expected_chars, &actual_chars);
        char_total += expected_chars.len();
        let expected_words: Vec<_> = expected.split_whitespace().collect();
        let actual_words: Vec<_> = actual.split_whitespace().collect();
        word_edits += levenshtein(&expected_words, &actual_words);
        word_total += expected_words.len();
    }
    Ok(BTreeMap::from([
        (
            "cer".to_string(),
            ratio_metric(char_edits, char_total, corpus.samples.len()),
        ),
        (
            "wer".to_string(),
            ratio_metric(word_edits, word_total, corpus.samples.len()),
        ),
    ]))
}

fn evaluate_formula(
    corpus: &CorpusManifest,
    predictions: &HashMap<&str, &Annotation>,
) -> Result<BTreeMap<String, MetricValue>, MetricError> {
    let mut exact = 0usize;
    let mut structural = 0.0f64;
    for sample in &corpus.samples {
        let predicted = predictions
            .get(sample.id.as_str())
            .ok_or_else(|| MetricError::MissingPrediction(sample.id.clone()))?;
        let (Annotation::Formula { latex: expected }, Annotation::Formula { latex: actual }) =
            (&sample.annotation, *predicted)
        else {
            return Err(MetricError::AnnotationKindMismatch(sample.id.clone()));
        };
        let expected = normalize_formula(expected);
        let actual = normalize_formula(actual);
        exact += usize::from(expected == actual);
        let expected_tokens = formula_tokens(&expected);
        let actual_tokens = formula_tokens(&actual);
        let denominator = expected_tokens.len().max(actual_tokens.len()).max(1);
        structural +=
            1.0 - levenshtein(&expected_tokens, &actual_tokens) as f64 / denominator as f64;
    }
    let count = corpus.samples.len();
    Ok(BTreeMap::from([
        (
            "formula_normalized_exact_match".to_string(),
            score_metric(exact as f64 / count.max(1) as f64, count),
        ),
        (
            "formula_structural_similarity".to_string(),
            score_metric(structural / count.max(1) as f64, count),
        ),
    ]))
}

fn evaluate_layout(
    corpus: &CorpusManifest,
    predictions: &HashMap<&str, &Annotation>,
) -> Result<BTreeMap<String, MetricValue>, MetricError> {
    let mut by_class: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    for sample in &corpus.samples {
        let predicted = predictions
            .get(sample.id.as_str())
            .ok_or_else(|| MetricError::MissingPrediction(sample.id.clone()))?;
        let (Annotation::Layout { regions: expected }, Annotation::Layout { regions: actual }) =
            (&sample.annotation, *predicted)
        else {
            return Err(MetricError::AnnotationKindMismatch(sample.id.clone()));
        };
        let classes: BTreeSet<_> = expected
            .iter()
            .chain(actual.iter())
            .map(|region| region.class.as_str())
            .collect();
        for class in classes {
            let expected: Vec<_> = expected.iter().filter(|r| r.class == class).collect();
            let actual: Vec<_> = actual.iter().filter(|r| r.class == class).collect();
            let matches = match_regions(&expected, &actual, 0.5);
            let entry = by_class.entry(class.to_string()).or_default();
            entry.0 += matches;
            entry.1 += actual.len().saturating_sub(matches);
            entry.2 += expected.len().saturating_sub(matches);
        }
    }
    let macro_f1 = if by_class.is_empty() {
        1.0
    } else {
        by_class
            .values()
            .map(|(tp, fp, fn_count)| f1(*tp, *fp, *fn_count))
            .sum::<f64>()
            / by_class.len() as f64
    };
    Ok(BTreeMap::from([(
        "layout_macro_f1".to_string(),
        score_metric(macro_f1, corpus.samples.len()),
    )]))
}

fn evaluate_table(
    corpus: &CorpusManifest,
    predictions: &HashMap<&str, &Annotation>,
) -> Result<BTreeMap<String, MetricValue>, MetricError> {
    let mut structure_f1 = 0.0;
    let mut tree_similarity = 0.0;
    for sample in &corpus.samples {
        let predicted = predictions
            .get(sample.id.as_str())
            .ok_or_else(|| MetricError::MissingPrediction(sample.id.clone()))?;
        let (Annotation::Table { cells: expected }, Annotation::Table { cells: actual }) =
            (&sample.annotation, *predicted)
        else {
            return Err(MetricError::AnnotationKindMismatch(sample.id.clone()));
        };
        let expected_structure: BTreeSet<_> = expected.iter().map(cell_structure).collect();
        let actual_structure: BTreeSet<_> = actual.iter().map(cell_structure).collect();
        let tp = expected_structure.intersection(&actual_structure).count();
        structure_f1 += f1(
            tp,
            actual_structure.len().saturating_sub(tp),
            expected_structure.len().saturating_sub(tp),
        );
        let expected_tree = table_tokens(expected);
        let actual_tree = table_tokens(actual);
        let denominator = expected_tree.len().max(actual_tree.len()).max(1);
        tree_similarity +=
            1.0 - levenshtein(&expected_tree, &actual_tree) as f64 / denominator as f64;
    }
    let count = corpus.samples.len();
    Ok(BTreeMap::from([
        (
            "table_structure_f1".to_string(),
            score_metric(structure_f1 / count.max(1) as f64, count),
        ),
        (
            "table_tree_similarity".to_string(),
            score_metric(tree_similarity / count.max(1) as f64, count),
        ),
    ]))
}

fn evaluate_orientation(
    corpus: &CorpusManifest,
    predictions: &HashMap<&str, &Annotation>,
) -> Result<BTreeMap<String, MetricValue>, MetricError> {
    let mut correct = 0usize;
    for sample in &corpus.samples {
        let predicted = predictions
            .get(sample.id.as_str())
            .ok_or_else(|| MetricError::MissingPrediction(sample.id.clone()))?;
        let (
            Annotation::Orientation { degrees: expected },
            Annotation::Orientation { degrees: actual },
        ) = (&sample.annotation, *predicted)
        else {
            return Err(MetricError::AnnotationKindMismatch(sample.id.clone()));
        };
        correct += usize::from(expected == actual);
    }
    Ok(BTreeMap::from([(
        "orientation_accuracy".to_string(),
        score_metric(
            correct as f64 / corpus.samples.len().max(1) as f64,
            corpus.samples.len(),
        ),
    )]))
}

fn evaluate_document(
    corpus: &CorpusManifest,
    predictions: &HashMap<&str, &Annotation>,
) -> Result<BTreeMap<String, MetricValue>, MetricError> {
    let mut semantic_f1 = 0.0;
    let mut order_similarity = 0.0;
    for sample in &corpus.samples {
        let predicted = predictions
            .get(sample.id.as_str())
            .ok_or_else(|| MetricError::MissingPrediction(sample.id.clone()))?;
        let (Annotation::Document { blocks: expected }, Annotation::Document { blocks: actual }) =
            (&sample.annotation, *predicted)
        else {
            return Err(MetricError::AnnotationKindMismatch(sample.id.clone()));
        };
        let expected_semantics = block_counts(expected);
        let actual_semantics = block_counts(actual);
        let tp: usize = expected_semantics
            .iter()
            .map(|(block, expected_count)| {
                expected_count.min(actual_semantics.get(block).unwrap_or(&0))
            })
            .sum();
        semantic_f1 += f1(
            tp,
            actual.len().saturating_sub(tp),
            expected.len().saturating_sub(tp),
        );
        let expected_order: Vec<_> = expected.iter().map(block_signature).collect();
        let actual_order: Vec<_> = actual.iter().map(block_signature).collect();
        let denominator = expected_order.len().max(actual_order.len()).max(1);
        order_similarity += lcs_len(&expected_order, &actual_order) as f64 / denominator as f64;
    }
    let count = corpus.samples.len();
    Ok(BTreeMap::from([
        (
            "document_block_semantics_f1".to_string(),
            score_metric(semantic_f1 / count.max(1) as f64, count),
        ),
        (
            "document_reading_order_similarity".to_string(),
            score_metric(order_similarity / count.max(1) as f64, count),
        ),
    ]))
}

fn block_counts(blocks: &[crate::schema::DocumentBlockValue]) -> BTreeMap<(String, String), usize> {
    let mut counts = BTreeMap::new();
    for block in blocks {
        *counts.entry(block_signature(block)).or_default() += 1;
    }
    counts
}

fn block_signature(block: &crate::schema::DocumentBlockValue) -> (String, String) {
    (
        block.kind.trim().to_ascii_lowercase(),
        block.text.split_whitespace().collect::<Vec<_>>().join(" "),
    )
}

fn normalize_formula(value: &str) -> String {
    value
        .replace("\\left", "")
        .replace("\\right", "")
        .replace("\\dfrac", "\\frac")
        .replace("\\tfrac", "\\frac")
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn formula_tokens(value: &str) -> Vec<String> {
    let chars: Vec<_> = value.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\\' {
            let start = index;
            index += 1;
            while index < chars.len() && chars[index].is_alphabetic() {
                index += 1;
            }
            tokens.push(chars[start..index].iter().collect());
        } else if chars[index].is_alphanumeric() {
            let start = index;
            index += 1;
            while index < chars.len() && chars[index].is_alphanumeric() {
                index += 1;
            }
            tokens.push(chars[start..index].iter().collect());
        } else {
            tokens.push(chars[index].to_string());
            index += 1;
        }
    }
    tokens
}

fn match_regions(expected: &[&Region], actual: &[&Region], threshold: f32) -> usize {
    let mut candidates = Vec::new();
    for (expected_index, expected_region) in expected.iter().enumerate() {
        for (actual_index, actual_region) in actual.iter().enumerate() {
            let overlap = iou(expected_region.bbox, actual_region.bbox);
            if overlap >= threshold {
                candidates.push((overlap, expected_index, actual_index));
            }
        }
    }
    candidates.sort_by(|left, right| right.0.total_cmp(&left.0));
    let mut expected_used = BTreeSet::new();
    let mut actual_used = BTreeSet::new();
    for (_, expected_index, actual_index) in candidates {
        if !expected_used.contains(&expected_index) && !actual_used.contains(&actual_index) {
            expected_used.insert(expected_index);
            actual_used.insert(actual_index);
        }
    }
    expected_used.len()
}

fn iou(left: [f32; 4], right: [f32; 4]) -> f32 {
    let intersection_left = left[0].max(right[0]);
    let intersection_top = left[1].max(right[1]);
    let intersection_right = (left[0] + left[2]).min(right[0] + right[2]);
    let intersection_bottom = (left[1] + left[3]).min(right[1] + right[3]);
    let width = (intersection_right - intersection_left).max(0.0);
    let height = (intersection_bottom - intersection_top).max(0.0);
    let intersection = width * height;
    let left_area = left[2].max(0.0) * left[3].max(0.0);
    let right_area = right[2].max(0.0) * right[3].max(0.0);
    let union = left_area + right_area - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn cell_structure(cell: &TableCellValue) -> (usize, usize, usize, usize) {
    (cell.row, cell.col, cell.rowspan, cell.colspan)
}

fn table_tokens(cells: &[TableCellValue]) -> Vec<String> {
    let mut cells = cells.to_vec();
    cells.sort();
    cells
        .into_iter()
        .map(|cell| {
            format!(
                "{}:{}:{}:{}:{}",
                cell.row, cell.col, cell.rowspan, cell.colspan, cell.text
            )
        })
        .collect()
}

fn f1(tp: usize, fp: usize, fn_count: usize) -> f64 {
    let denominator = (2 * tp + fp + fn_count) as f64;
    if denominator == 0.0 {
        1.0
    } else {
        2.0 * tp as f64 / denominator
    }
}

fn ratio_metric(numerator: usize, denominator: usize, sample_count: usize) -> MetricValue {
    MetricValue {
        value: numerator as f64 / denominator.max(1) as f64,
        unit: "ratio".to_string(),
        sample_count,
    }
}

fn score_metric(value: f64, sample_count: usize) -> MetricValue {
    MetricValue {
        value: value.clamp(0.0, 1.0),
        unit: "score".to_string(),
        sample_count,
    }
}

fn levenshtein<T: Eq>(left: &[T], right: &[T]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
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

fn lcs_len<T: Eq>(left: &[T], right: &[T]) -> usize {
    let mut row = vec![0usize; right.len() + 1];
    for left_value in left {
        let mut diagonal = 0usize;
        for (index, right_value) in right.iter().enumerate() {
            let above = row[index + 1];
            row[index + 1] = if left_value == right_value {
                diagonal + 1
            } else {
                row[index + 1].max(row[index])
            };
            diagonal = above;
        }
    }
    row[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_and_lcs_are_deterministic() {
        assert_eq!(levenshtein(&['a', 'b', 'c'], &['a', 'x', 'c']), 1);
        assert_eq!(lcs_len(&["a", "b", "c"], &["b", "a", "c"]), 2);
    }

    #[test]
    fn formula_normalization_preserves_structure() {
        assert_eq!(
            normalize_formula(r"\left( \dfrac{a}{b} \right)"),
            r"(\frac{a}{b})"
        );
        assert_eq!(
            formula_tokens(r"\frac{a1}{b}"),
            vec!["\\frac", "{", "a1", "}", "{", "b", "}"]
        );
    }

    #[test]
    fn intersection_over_union_handles_overlap() {
        assert!((iou([0.0, 0.0, 10.0, 10.0], [0.0, 0.0, 10.0, 10.0]) - 1.0).abs() < f32::EPSILON);
        assert_eq!(iou([0.0, 0.0, 1.0, 1.0], [2.0, 2.0, 1.0, 1.0]), 0.0);
    }

    #[test]
    fn error_ratios_can_exceed_one() {
        assert_eq!(ratio_metric(3, 2, 1).value, 1.5);
    }
}
