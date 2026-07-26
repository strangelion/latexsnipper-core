//! Evidence-based selection for ambiguous table cells.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellRecognitionRoute {
    FormulaOnly,
    TextOnly,
    DualCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellCandidateKind {
    Formula,
    Text,
}

#[derive(Debug, Clone, Copy)]
pub struct CellCandidate<'a> {
    pub kind: CellCandidateKind,
    pub content: &'a str,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct CellGeometryEvidence {
    pub aspect_ratio: f32,
    pub detector_overlap: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellCandidateScore {
    pub kind: CellCandidateKind,
    pub total: f32,
    pub confidence: f32,
    pub syntax_validity: f32,
    pub character_reasonability: f32,
    pub geometry: f32,
    pub detector_overlap: f32,
    pub repeated_token_penalty: f32,
    pub hard_negative_adjustment: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellCandidateDecision {
    pub selected_candidate: CellCandidateKind,
    pub candidate_scores: Vec<CellCandidateScore>,
    pub selection_reason: String,
}

pub fn cell_recognition_route(formula_confidence: f32) -> CellRecognitionRoute {
    if formula_confidence >= 0.80 {
        CellRecognitionRoute::FormulaOnly
    } else if formula_confidence <= 0.35 {
        CellRecognitionRoute::TextOnly
    } else {
        CellRecognitionRoute::DualCandidate
    }
}

pub fn select_ambiguous_cell_candidate(
    formula: CellCandidate<'_>,
    text: CellCandidate<'_>,
    geometry: CellGeometryEvidence,
) -> CellCandidateDecision {
    debug_assert_eq!(formula.kind, CellCandidateKind::Formula);
    debug_assert_eq!(text.kind, CellCandidateKind::Text);
    let scores = vec![score_formula(formula, geometry), score_text(text, geometry)];
    let selected_candidate = if scores[0].total >= scores[1].total {
        CellCandidateKind::Formula
    } else {
        CellCandidateKind::Text
    };
    let selection_reason = if looks_like_hard_negative(text.content) {
        "text matches a table hard-negative pattern".to_owned()
    } else if scores[0].repeated_token_penalty > 0.0 {
        "formula candidate contains a repeated-token failure".to_owned()
    } else if scores[0].syntax_validity > scores[1].syntax_validity {
        "formula candidate has stronger syntax evidence".to_owned()
    } else {
        "weighted confidence and character evidence favored the candidate".to_owned()
    };
    CellCandidateDecision {
        selected_candidate,
        candidate_scores: scores,
        selection_reason,
    }
}

pub fn looks_like_hard_negative(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() == 1
        && (chars[0].is_alphabetic() || ('\u{0370}'..='\u{03ff}').contains(&chars[0]))
    {
        return true;
    }
    if is_date(value) || is_percentage(value) || is_cell_reference(value) {
        return true;
    }
    if value
        .strip_prefix("No.")
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit()))
    {
        return true;
    }
    if looks_like_chemical_formula(value) {
        return true;
    }
    chars.iter().any(|ch| {
        matches!(
            ch,
            '⁰' | '¹' | '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹' | '₀'..='₉'
        )
    }) && chars.iter().any(|ch| ch.is_alphabetic())
}

fn score_formula(
    candidate: CellCandidate<'_>,
    geometry: CellGeometryEvidence,
) -> CellCandidateScore {
    let confidence = candidate.confidence.clamp(0.0, 1.0);
    let syntax_validity = f32::from(crate::parse_formula_latex(candidate.content).is_ok());
    let character_reasonability = character_reasonability(candidate.content, true);
    let geometry_score = geometry_score(geometry.aspect_ratio);
    let overlap = geometry.detector_overlap.clamp(0.0, 1.0);
    let repeated = f32::from(
        crate::latex_quality_flags(candidate.content)
            .iter()
            .any(|flag| flag == "repeated_token_run"),
    );
    let hard_negative = if looks_like_hard_negative(candidate.content) {
        -0.35
    } else {
        0.0
    };
    let total = 0.50 * confidence
        + 0.20 * syntax_validity
        + 0.10 * character_reasonability
        + 0.10 * geometry_score
        + 0.10 * overlap
        - 0.25 * repeated
        + hard_negative;
    CellCandidateScore {
        kind: CellCandidateKind::Formula,
        total,
        confidence,
        syntax_validity,
        character_reasonability,
        geometry: geometry_score,
        detector_overlap: overlap,
        repeated_token_penalty: repeated,
        hard_negative_adjustment: hard_negative,
    }
}

fn score_text(candidate: CellCandidate<'_>, geometry: CellGeometryEvidence) -> CellCandidateScore {
    let confidence = candidate.confidence.clamp(0.0, 1.0);
    let character_reasonability = character_reasonability(candidate.content, false);
    let geometry_score = geometry_score(geometry.aspect_ratio);
    let overlap = geometry.detector_overlap.clamp(0.0, 1.0);
    let hard_negative = if looks_like_hard_negative(candidate.content) {
        0.20
    } else {
        0.0
    };
    CellCandidateScore {
        kind: CellCandidateKind::Text,
        total: 0.55 * confidence
            + 0.25 * character_reasonability
            + 0.10 * geometry_score
            + 0.10 * (1.0 - overlap)
            + hard_negative,
        confidence,
        syntax_validity: 0.0,
        character_reasonability,
        geometry: geometry_score,
        detector_overlap: overlap,
        repeated_token_penalty: 0.0,
        hard_negative_adjustment: hard_negative,
    }
}

fn character_reasonability(value: &str, formula: bool) -> f32 {
    let mut accepted = 0usize;
    let mut total = 0usize;
    for ch in value.chars().filter(|ch| !ch.is_whitespace()) {
        total += 1;
        if ch.is_alphanumeric()
            || (formula && "\\{}[]()_^+-=*/.,<>|&".contains(ch))
            || (!formula && ".,:%#-_/".contains(ch))
        {
            accepted += 1;
        }
    }
    accepted as f32 / total.max(1) as f32
}

fn geometry_score(aspect_ratio: f32) -> f32 {
    if !aspect_ratio.is_finite() || aspect_ratio <= 0.0 {
        0.0
    } else if (0.20..=12.0).contains(&aspect_ratio) {
        1.0
    } else {
        0.5
    }
}

fn is_date(value: &str) -> bool {
    let parts = value.split('-').collect::<Vec<_>>();
    parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts
            .iter()
            .all(|part| part.chars().all(|ch| ch.is_ascii_digit()))
}

fn is_percentage(value: &str) -> bool {
    value
        .strip_suffix('%')
        .is_some_and(|number| number.parse::<f64>().is_ok())
}

fn is_cell_reference(value: &str) -> bool {
    let letters = value
        .chars()
        .take_while(|ch| ch.is_ascii_uppercase())
        .count();
    letters > 0
        && letters <= 3
        && value[letters..].chars().all(|ch| ch.is_ascii_digit())
        && value[letters..].chars().next().is_some()
}

fn looks_like_chemical_formula(value: &str) -> bool {
    let mut has_letter = false;
    let mut has_digit = false;
    for ch in value.chars() {
        if ch.is_ascii_alphabetic() {
            has_letter = true;
        } else if ch.is_ascii_digit() {
            has_digit = true;
        } else {
            return false;
        }
    }
    has_letter && has_digit && value.len() <= 12
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_runs_both_candidates_only_in_the_ambiguous_band() {
        assert_eq!(
            cell_recognition_route(0.90),
            CellRecognitionRoute::FormulaOnly
        );
        assert_eq!(cell_recognition_route(0.20), CellRecognitionRoute::TextOnly);
        assert_eq!(
            cell_recognition_route(0.55),
            CellRecognitionRoute::DualCandidate
        );
    }

    #[test]
    fn required_hard_negatives_are_selected_as_text() {
        for value in ["A1", "x", "2026-07-26", "10%", "H2O", "No.3", "α", "x²"] {
            assert!(looks_like_hard_negative(value), "{value}");
            let decision = select_ambiguous_cell_candidate(
                CellCandidate {
                    kind: CellCandidateKind::Formula,
                    content: value,
                    confidence: 0.70,
                },
                CellCandidate {
                    kind: CellCandidateKind::Text,
                    content: value,
                    confidence: 0.70,
                },
                CellGeometryEvidence {
                    aspect_ratio: 2.0,
                    detector_overlap: 0.5,
                },
            );
            assert_eq!(
                decision.selected_candidate,
                CellCandidateKind::Text,
                "{value}: {decision:?}"
            );
        }
    }

    #[test]
    fn valid_structured_latex_beats_plain_ocr_noise() {
        let decision = select_ambiguous_cell_candidate(
            CellCandidate {
                kind: CellCandidateKind::Formula,
                content: r"\frac{a}{b}",
                confidence: 0.78,
            },
            CellCandidate {
                kind: CellCandidateKind::Text,
                content: "fraca b",
                confidence: 0.52,
            },
            CellGeometryEvidence {
                aspect_ratio: 2.0,
                detector_overlap: 0.8,
            },
        );
        assert_eq!(decision.selected_candidate, CellCandidateKind::Formula);
    }
}
