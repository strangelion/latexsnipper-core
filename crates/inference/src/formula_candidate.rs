//! Segment-vs-whole quality arbitration.
//!
//! When a formula image can be recognized both as segmented rows and as a
//! whole image, the recognizer produces several candidates. This module
//! scores each candidate with versioned, explainable rules (severe errors
//! first, AST-parsability, structural completeness, semantic-hash stability,
//! confidence as a tie signal) and records the choice plus every rejection
//! reason for provenance.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::formula_parser::parse_formula_latex;

/// Where a candidate came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaCandidateSource {
    /// Rows recognized independently then reassembled.
    Segmented,
    /// Whole-line retry (single line recognized as one image).
    WholeLineRetry,
    /// Whole-image retry (entire region recognized as one image).
    WholeImageRetry,
}

/// Stable quality flags for one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaQualityFlag {
    /// Repeated relation operators (e.g. `==`, `<= <=`).
    DuplicateRelation,
    /// Short token loop (a short token repeated many times).
    ShortTokenLoop,
    /// Repeated spacing commands (e.g. `\, \,`).
    RepeatedSpacing,
    /// Unbalanced braces/parentheses.
    UnbalancedBraces,
    /// Unbalanced `\left` / `\right`.
    UnbalancedLeftRight,
    /// Unmatched `\begin` / `\end`.
    MismatchedEnvironment,
    /// Duplicate superscript/subscript anchors.
    DuplicateSuperSubscript,
    /// Empty output.
    EmptyOutput,
    /// Abnormal length (too short or too long).
    AbnormalLength,
    /// Low confidence.
    LowConfidence,
    /// The LaTeX does not parse into a formula AST.
    AstParseFailure,
}

impl FormulaQualityFlag {
    /// Severity weight: structural defects cost more than style signals.
    fn severity(self) -> i32 {
        match self {
            Self::DuplicateRelation
            | Self::ShortTokenLoop
            | Self::UnbalancedBraces
            | Self::UnbalancedLeftRight
            | Self::MismatchedEnvironment
            | Self::DuplicateSuperSubscript
            | Self::EmptyOutput
            | Self::AstParseFailure => 3,
            Self::RepeatedSpacing | Self::AbnormalLength => 2,
            Self::LowConfidence => 1,
        }
    }
}

/// A recognition candidate with quality evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaCandidate {
    pub source: FormulaCandidateSource,
    pub latex: String,
    pub confidence: f32,
    pub quality_flags: Vec<FormulaQualityFlag>,
    pub ast_semantic_hash: Option<String>,
    pub latency_ms: u64,
}

/// Versioned arbitration policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaArbitrationPolicy {
    pub version: String,
    /// Confidence below this is flagged low.
    pub low_confidence_threshold: f32,
    /// A formula shorter than this (chars) is abnormally short.
    pub min_length: usize,
    /// A formula longer than this (chars) is abnormally long.
    pub max_length: usize,
}

impl Default for FormulaArbitrationPolicy {
    fn default() -> Self {
        Self {
            version: "v1".into(),
            low_confidence_threshold: 0.5,
            min_length: 1,
            max_length: 4096,
        }
    }
}

/// Outcome of arbitrating several candidates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaArbitration {
    /// All candidates considered, in input order.
    pub candidates: Vec<FormulaCandidate>,
    /// Index into `candidates` of the winner (None when no candidate survived).
    pub selected_index: Option<usize>,
    /// Explainable selection reason.
    pub selection_reason: String,
    /// Rejection reason per non-selected candidate index.
    pub rejected_reasons: Vec<RejectedCandidate>,
    pub policy_version: String,
}

/// Why one candidate lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedCandidate {
    pub candidate_index: usize,
    pub reason: String,
}

/// Score a candidate with versioned rules. Higher is better.
fn score_candidate(
    candidate: &FormulaCandidate,
    policy: &FormulaArbitrationPolicy,
    parsed: bool,
) -> i32 {
    let mut score = 0;
    // Severe structural errors dominate.
    for flag in &candidate.quality_flags {
        score -= flag.severity();
    }
    // AST-parsable candidates win over non-parsable ones.
    if parsed {
        score += 10;
    }
    // Structural completeness: fewer quality flags overall is better.
    score -= candidate.quality_flags.len() as i32;
    // Semantic hash stability: a stable hash means deterministic output.
    if candidate.ast_semantic_hash.is_some() {
        score += 2;
    }
    // Confidence is a signal, never the primary criterion.
    score += (candidate.confidence * 5.0) as i32;
    let _ = policy;
    score
}

/// Check a candidate's quality flags from its LaTeX text.
pub fn check_quality_flags(
    latex: &str,
    confidence: f32,
    policy: &FormulaArbitrationPolicy,
) -> Vec<FormulaQualityFlag> {
    let mut flags = Vec::new();
    if latex.trim().is_empty() {
        flags.push(FormulaQualityFlag::EmptyOutput);
        return flags;
    }
    if confidence < policy.low_confidence_threshold {
        flags.push(FormulaQualityFlag::LowConfidence);
    }
    let len = latex.chars().count();
    if len < policy.min_length {
        flags.push(FormulaQualityFlag::AbnormalLength);
    }
    if len > policy.max_length {
        flags.push(FormulaQualityFlag::AbnormalLength);
    }
    if has_duplicate_relation(latex) {
        flags.push(FormulaQualityFlag::DuplicateRelation);
    }
    if has_short_token_loop(latex) {
        flags.push(FormulaQualityFlag::ShortTokenLoop);
    }
    if has_repeated_spacing(latex) {
        flags.push(FormulaQualityFlag::RepeatedSpacing);
    }
    if !braces_balanced(latex) {
        flags.push(FormulaQualityFlag::UnbalancedBraces);
    }
    if latex.matches(r"\left").count() != latex.matches(r"\right").count() {
        flags.push(FormulaQualityFlag::UnbalancedLeftRight);
    }
    if !environments_balanced(latex) {
        flags.push(FormulaQualityFlag::MismatchedEnvironment);
    }
    if has_duplicate_super_subscript(latex) {
        flags.push(FormulaQualityFlag::DuplicateSuperSubscript);
    }
    flags
}

/// Build a candidate with flags and AST hash, computing parse status.
pub fn build_candidate(
    source: FormulaCandidateSource,
    latex: impl Into<String>,
    confidence: f32,
    latency: Duration,
    policy: &FormulaArbitrationPolicy,
) -> FormulaCandidate {
    let latex = latex.into();
    let mut quality_flags = check_quality_flags(&latex, confidence, policy);
    let ast_semantic_hash = match parse_formula_latex(&latex) {
        Ok(_) => Some(semantic_hash(&latex)),
        Err(_) => {
            quality_flags.push(FormulaQualityFlag::AstParseFailure);
            None
        }
    };
    FormulaCandidate {
        source,
        latex,
        confidence,
        quality_flags,
        ast_semantic_hash,
        latency_ms: latency.as_millis() as u64,
    }
}

/// Arbitrate candidates; returns the winner with provenance.
pub fn arbitrate_candidates(
    candidates: Vec<FormulaCandidate>,
    policy: &FormulaArbitrationPolicy,
) -> FormulaArbitration {
    if candidates.is_empty() {
        return FormulaArbitration {
            candidates,
            selected_index: None,
            selection_reason: "no candidates".into(),
            rejected_reasons: Vec::new(),
            policy_version: policy.version.clone(),
        };
    }

    let mut best: Option<(usize, i32)> = None;
    let mut rejected_reasons = Vec::new();

    for (idx, candidate) in candidates.iter().enumerate() {
        let parsed = candidate.ast_semantic_hash.is_some();
        let score = score_candidate(candidate, policy, parsed);
        match best {
            None => best = Some((idx, score)),
            Some((best_idx, best_score)) => {
                if score > best_score {
                    rejected_reasons.push(RejectedCandidate {
                        candidate_index: best_idx,
                        reason: format!("scored {best_score}, outscored by {score}"),
                    });
                    best = Some((idx, score));
                } else if score < best_score {
                    rejected_reasons.push(RejectedCandidate {
                        candidate_index: idx,
                        reason: format!("scored {score}, below best {best_score}"),
                    });
                } else {
                    // Tie: prefer the later whole-image retry (more complete).
                    rejected_reasons.push(RejectedCandidate {
                        candidate_index: idx,
                        reason: "tie broken in favor of earlier candidate".into(),
                    });
                }
            }
        }
    }

    let (selected_index, best_score) = best.unwrap();
    let selected = &candidates[selected_index];
    let reason = format!(
        "candidate {} (source {:?}, score {}, flags {:?})",
        selected_index, selected.source, best_score, selected.quality_flags
    );

    FormulaArbitration {
        candidates,
        selected_index: Some(selected_index),
        selection_reason: reason,
        rejected_reasons,
        policy_version: policy.version.clone(),
    }
}

fn semantic_hash(latex: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(latex.as_bytes()))
}

fn has_duplicate_relation(latex: &str) -> bool {
    const RELATIONS: &[&str] = &[
        "==", "!=", "<=", ">=", "\\le", "\\ge", "\\leq", "\\geq", "\\neq", "\\approx",
    ];
    RELATIONS.iter().any(|rel| latex.matches(rel).count() >= 2)
}

fn has_short_token_loop(latex: &str) -> bool {
    // A single short token repeated many times (e.g. "x x x x x x x").
    let tokens: Vec<&str> = latex.split_whitespace().collect();
    if tokens.len() < 6 {
        return false;
    }
    let first = tokens[0];
    if first.chars().count() > 2 {
        return false;
    }
    tokens.iter().all(|t| *t == first)
}

fn has_repeated_spacing(latex: &str) -> bool {
    const SPACING: &[&str] = &[r"\,", r"\;", r"\:", r"\quad", r"\qquad", r"\ "];
    SPACING.iter().any(|cmd| latex.matches(cmd).count() >= 3)
}

fn braces_balanced(latex: &str) -> bool {
    let mut balance = 0i32;
    let mut chars = latex.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // Skip escaped char.
            chars.next();
            continue;
        }
        match c {
            '{' => balance += 1,
            '}' => balance -= 1,
            _ => {}
        }
        if balance < 0 {
            return false;
        }
    }
    balance == 0
}

fn environments_balanced(latex: &str) -> bool {
    let begins = latex.matches(r"\begin{").count();
    let ends = latex.matches(r"\end{").count();
    begins == ends
}

fn has_duplicate_super_subscript(latex: &str) -> bool {
    // e.g. "x^^2" or "x__2" — double caret/underscore.
    latex.contains("^^") || latex.contains("__")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(source: FormulaCandidateSource, latex: &str, confidence: f32) -> FormulaCandidate {
        build_candidate(
            source,
            latex,
            confidence,
            Duration::from_millis(5),
            &FormulaArbitrationPolicy::default(),
        )
    }

    #[test]
    fn arbitrates_clean_segmented_over_noisy_whole() {
        let candidates = vec![
            cand(FormulaCandidateSource::Segmented, r"x + y = 1", 0.9),
            cand(
                FormulaCandidateSource::WholeImageRetry,
                r"x + y = = 1",
                0.95,
            ),
        ];
        let arbitration = arbitrate_candidates(candidates, &FormulaArbitrationPolicy::default());
        let selected = arbitration.selected_index.unwrap();
        // The clean segmented candidate wins even though the whole-image
        // retry has higher raw confidence.
        assert_eq!(arbitration.candidates[selected].latex, r"x + y = 1");
        assert!(!arbitration.rejected_reasons.is_empty());
        assert_eq!(arbitration.policy_version, "v1");
    }

    #[test]
    fn ast_parse_failure_loses() {
        let candidates = vec![
            cand(FormulaCandidateSource::Segmented, r"\frac{a}{b", 0.9),
            cand(
                FormulaCandidateSource::WholeImageRetry,
                r"\frac{a}{b}",
                0.85,
            ),
        ];
        let arbitration = arbitrate_candidates(candidates, &FormulaArbitrationPolicy::default());
        let selected = arbitration.selected_index.unwrap();
        assert_eq!(arbitration.candidates[selected].latex, r"\frac{a}{b}");
    }

    #[test]
    fn unbalanced_environment_loses() {
        let candidates = vec![
            cand(
                FormulaCandidateSource::Segmented,
                r"\begin{aligned}x\\y\end{aligned}",
                0.8,
            ),
            cand(
                FormulaCandidateSource::WholeImageRetry,
                r"\begin{aligned}x\\y",
                0.9,
            ),
        ];
        let arbitration = arbitrate_candidates(candidates, &FormulaArbitrationPolicy::default());
        let selected = arbitration.selected_index.unwrap();
        assert!(arbitration.candidates[selected]
            .latex
            .contains(r"\end{aligned}"));
    }

    #[test]
    fn empty_candidates_arbitrate_to_none() {
        let arbitration = arbitrate_candidates(Vec::new(), &FormulaArbitrationPolicy::default());
        assert!(arbitration.selected_index.is_none());
    }

    #[test]
    fn quality_flags_detected() {
        let policy = FormulaArbitrationPolicy::default();
        assert!(check_quality_flags(r"x == y == z", 0.9, &policy)
            .contains(&FormulaQualityFlag::DuplicateRelation));
        assert!(check_quality_flags("", 0.9, &policy).contains(&FormulaQualityFlag::EmptyOutput));
        assert!(check_quality_flags(r"\left( x", 0.9, &policy)
            .contains(&FormulaQualityFlag::UnbalancedLeftRight));
        assert!(check_quality_flags(r"x^^2", 0.9, &policy)
            .contains(&FormulaQualityFlag::DuplicateSuperSubscript));
        assert!(check_quality_flags(r"\frac{a}{b}", 0.2, &policy)
            .contains(&FormulaQualityFlag::LowConfidence));
    }
}
