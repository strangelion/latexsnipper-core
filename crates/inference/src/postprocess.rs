//! Evidence-preserving, rule-based recognition postprocessing.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};
use thiserror::Error;

use latexsnipper_ast::{
    PostProcessResult, TextDiff, TransformationEvidence, TransformationMode, TriggerDecision,
    ValidationEvidence,
};

pub const POSTPROCESS_VERSION: &str = "formula-rules-v1";

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub raw: String,
    pub confidence: f32,
    pub unexpected_eos: bool,
    pub truncated: bool,
}

impl Candidate {
    pub fn new(raw: impl Into<String>, confidence: f32) -> Self {
        Self {
            raw: raw.into(),
            confidence,
            unexpected_eos: false,
            truncated: false,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PostProcessError {
    #[error("recognition candidate is empty")]
    EmptyCandidate,
    #[error("recognition confidence is not finite")]
    InvalidConfidence,
}

pub trait RecognitionPostProcessor {
    fn should_run(&self, candidate: &Candidate) -> TriggerDecision;

    fn process(&self, candidate: &Candidate) -> Result<PostProcessResult, PostProcessError>;
}

#[derive(Debug, Clone)]
pub struct RuleBasedRecognitionPostProcessor {
    pub confidence_threshold: f32,
}

impl Default for RuleBasedRecognitionPostProcessor {
    fn default() -> Self {
        Self {
            confidence_threshold: 0.75,
        }
    }
}

impl RecognitionPostProcessor for RuleBasedRecognitionPostProcessor {
    fn should_run(&self, candidate: &Candidate) -> TriggerDecision {
        let validation = validate(candidate);
        let mut triggers = Vec::new();
        if candidate.raw.trim().is_empty() {
            triggers.push("empty_output".to_owned());
        }
        if candidate.confidence < self.confidence_threshold {
            triggers.push("low_confidence".to_owned());
        }
        if !validation.balanced_groups {
            triggers.push("unbalanced_group".to_owned());
        }
        if !validation.environments_closed {
            triggers.push("environment_closure".to_owned());
        }
        if !validation.left_right_balanced {
            triggers.push("unbalanced_left_right".to_owned());
        }
        if validation.duplicate_token_run {
            triggers.push("duplicate_token_run".to_owned());
        }
        if validation.dangling_command {
            triggers.push("invalid_command".to_owned());
        }
        if validation.unexpected_eos {
            triggers.push("unexpected_eos".to_owned());
        }
        if validation.truncated {
            triggers.push("truncation".to_owned());
        }
        if !validation.matrix_shape_valid {
            triggers.push("matrix_shape".to_owned());
        }
        TriggerDecision {
            should_run: !triggers.is_empty(),
            triggers,
        }
    }

    fn process(&self, candidate: &Candidate) -> Result<PostProcessResult, PostProcessError> {
        if !candidate.confidence.is_finite() {
            return Err(PostProcessError::InvalidConfidence);
        }

        let trigger = self.should_run(candidate);
        let validation = validate(candidate);
        let normalized = candidate.raw.trim().to_owned();
        let mut corrected = candidate.raw.clone();
        let mut transformations = Vec::new();

        if trigger.should_run {
            if corrected != normalized {
                apply_rule(
                    &mut corrected,
                    &mut transformations,
                    "trim-surrounding-whitespace",
                    "remove surrounding whitespace without changing formula tokens",
                    |_| normalized.clone(),
                );
            }
            let group_balance = group_balance(&corrected);
            if group_balance > 0 {
                apply_rule(
                    &mut corrected,
                    &mut transformations,
                    "balance-closing-groups",
                    "append missing closing group delimiters without changing group content",
                    |value| format!("{value}{}", "}".repeat(group_balance as usize)),
                );
            }
            if !left_right_balanced(&corrected) {
                apply_rule(
                    &mut corrected,
                    &mut transformations,
                    "remove-unpaired-left-right-sizing",
                    "remove unmatched visual sizing prefixes while preserving delimiters",
                    |value| value.replace(r"\left", "").replace(r"\right", ""),
                );
            }
            if let Some(suffix) = missing_environment_suffix(&corrected) {
                apply_rule(
                    &mut corrected,
                    &mut transformations,
                    "close-open-environments",
                    "append matching closures for open environments in reverse order",
                    |value| format!("{value}{suffix}"),
                );
            }
        }

        let corrected_candidate = Candidate {
            raw: corrected.clone(),
            ..candidate.clone()
        };
        let corrected_validation = validate(&corrected_candidate);
        let review_required = corrected.trim().is_empty()
            || !corrected_validation.syntax_valid()
            || corrected_validation.unexpected_eos
            || corrected_validation.truncated;
        let diff = (corrected != candidate.raw).then(|| TextDiff {
            before: candidate.raw.clone(),
            after: corrected.clone(),
        });

        Ok(PostProcessResult {
            raw: candidate.raw.clone(),
            normalized,
            corrected,
            diff,
            trigger,
            raw_confidence: candidate.confidence,
            normalized_confidence: candidate.confidence.clamp(0.0, 1.0),
            validation,
            corrected_validation,
            transformations,
            review_required,
            status_code: review_required.then(|| "POSTPROCESS_REVIEW_REQUIRED".to_owned()),
        })
    }
}

fn apply_rule(
    value: &mut String,
    evidence: &mut Vec<TransformationEvidence>,
    rule_id: &str,
    reason: &str,
    transform: impl FnOnce(&str) -> String,
) {
    let before = value.clone();
    let after = transform(&before);
    if before == after {
        return;
    }
    evidence.push(TransformationEvidence {
        rule_id: rule_id.to_owned(),
        before_sha256: sha256(&before),
        after_sha256: sha256(&after),
        reason: reason.to_owned(),
        confidence_delta: 0.0,
        mode: TransformationMode::Automatic,
        version: POSTPROCESS_VERSION.to_owned(),
    });
    *value = after;
}

fn validate(candidate: &Candidate) -> ValidationEvidence {
    ValidationEvidence {
        balanced_groups: group_balance(&candidate.raw) == 0,
        environments_closed: environments_balanced(&candidate.raw),
        left_right_balanced: left_right_balanced(&candidate.raw),
        duplicate_token_run: duplicate_token_run(&candidate.raw),
        dangling_command: candidate.raw.trim_end().ends_with('\\'),
        unexpected_eos: candidate.unexpected_eos,
        truncated: candidate.truncated,
        matrix_shape_valid: matrix_shape_valid(&candidate.raw),
    }
}

fn group_balance(value: &str) -> i32 {
    let bytes = value.as_bytes();
    let mut balance = 0;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if escaped(bytes, index) {
            continue;
        }
        match byte {
            b'{' => balance += 1,
            b'}' => balance -= 1,
            _ => {}
        }
        if balance < 0 {
            return balance;
        }
    }
    balance
}

fn environment_stack(value: &str) -> Option<Vec<String>> {
    let mut stack = Vec::new();
    let mut rest = value;
    loop {
        let begin = rest.find(r"\begin{").map(|index| (index, true));
        let end = rest.find(r"\end{").map(|index| (index, false));
        let Some((index, is_begin)) = [begin, end].into_iter().flatten().min_by_key(|item| item.0)
        else {
            return Some(stack);
        };
        let start = index + if is_begin { 7 } else { 5 };
        let after = &rest[start..];
        let close = after.find('}')?;
        let name = &after[..close];
        if is_begin {
            stack.push(name.to_owned());
        } else if stack.pop().as_deref() != Some(name) {
            return None;
        }
        rest = &after[close + 1..];
    }
}

fn environments_balanced(value: &str) -> bool {
    environment_stack(value).is_some_and(|stack| stack.is_empty())
}

fn missing_environment_suffix(value: &str) -> Option<String> {
    let stack = environment_stack(value)?;
    (!stack.is_empty()).then(|| {
        stack
            .iter()
            .rev()
            .map(|name| format!(r"\end{{{name}}}"))
            .collect()
    })
}

fn left_right_balanced(value: &str) -> bool {
    value.matches(r"\left").count() == value.matches(r"\right").count()
}

fn duplicate_token_run(value: &str) -> bool {
    let mut previous = None;
    let mut run = 0;
    for token in tokenize(value) {
        if previous.as_deref() == Some(token.as_str()) {
            run += 1;
        } else {
            previous = Some(token);
            run = 1;
        }
        if run >= 8 {
            return true;
        }
    }
    false
}

fn tokenize(value: &str) -> Vec<String> {
    let characters: Vec<_> = value.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        if characters[index] == '\\' {
            let start = index;
            index += 1;
            while index < characters.len() && characters[index].is_alphabetic() {
                index += 1;
            }
            tokens.push(characters[start..index].iter().collect());
        } else if characters[index].is_whitespace() {
            index += 1;
        } else {
            tokens.push(characters[index].to_string());
            index += 1;
        }
    }
    tokens
}

fn matrix_shape_valid(value: &str) -> bool {
    let matrix_environments = [
        "matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix",
    ];
    for environment in matrix_environments {
        let begin = format!(r"\begin{{{environment}}}");
        let end = format!(r"\end{{{environment}}}");
        let mut rest = value;
        while let Some(start) = rest.find(&begin) {
            let body = &rest[start + begin.len()..];
            let Some(finish) = body.find(&end) else {
                return false;
            };
            let rows: Vec<_> = body[..finish].split(r"\\").collect();
            let widths: BTreeSet<_> = rows
                .iter()
                .filter(|row| !row.trim().is_empty())
                .map(|row| row.matches('&').count() + 1)
                .collect();
            if widths.len() > 1 {
                return false;
            }
            rest = &body[finish + end.len()..];
        }
    }
    true
}

fn escaped(bytes: &[u8], index: usize) -> bool {
    let mut cursor = index;
    let mut count = 0;
    while cursor > 0 && bytes[cursor - 1] == b'\\' {
        cursor -= 1;
        count += 1;
    }
    count % 2 == 1
}

fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_confidence_valid_formula_skips_postprocessing() {
        let processor = RuleBasedRecognitionPostProcessor::default();
        let candidate = Candidate::new(r"\frac{a}{b}", 0.99);
        assert!(!processor.should_run(&candidate).should_run);
        let result = processor.process(&candidate).unwrap();
        assert_eq!(result.corrected, candidate.raw);
        assert!(result.transformations.is_empty());
        assert!(!result.review_required);
    }

    #[test]
    fn missing_closing_group_is_corrected_with_hash_evidence() {
        let processor = RuleBasedRecognitionPostProcessor::default();
        let result = processor
            .process(&Candidate::new(r"\frac{a}{b", 0.4))
            .unwrap();
        assert_eq!(result.corrected, r"\frac{a}{b}");
        assert_eq!(result.transformations.len(), 1);
        assert_ne!(
            result.transformations[0].before_sha256,
            result.transformations[0].after_sha256
        );
        assert!(!result.review_required);
    }

    #[test]
    fn duplicate_run_is_flagged_without_silent_math_rewrite() {
        let processor = RuleBasedRecognitionPostProcessor::default();
        let raw = "x x x x x x x x";
        let result = processor.process(&Candidate::new(raw, 0.2)).unwrap();
        assert_eq!(result.corrected, raw);
        assert!(result.validation.duplicate_token_run);
        assert!(result.review_required);
        assert_eq!(
            result.status_code.as_deref(),
            Some("POSTPROCESS_REVIEW_REQUIRED")
        );
    }

    #[test]
    fn inconsistent_matrix_columns_require_review() {
        let processor = RuleBasedRecognitionPostProcessor::default();
        let result = processor
            .process(&Candidate::new(
                r"\begin{matrix}a&b\\c&d&e\end{matrix}",
                0.8,
            ))
            .unwrap();
        assert!(!result.validation.matrix_shape_valid);
        assert!(result.review_required);
    }
}
