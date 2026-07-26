//! Recognition provenance and transformation evidence.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecognitionProvenance {
    pub model_id: String,
    pub model_version: String,
    pub runtime: String,
    pub provider: String,
    pub source_region: Option<SourcePolygon>,
    pub raw_confidence: Option<f32>,
    pub normalized_confidence: Option<f32>,
    #[serde(default)]
    pub transformations: Vec<TransformationEvidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePolygon {
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransformationEvidence {
    pub rule_id: String,
    pub before_sha256: String,
    pub after_sha256: String,
    pub reason: String,
    pub confidence_delta: f32,
    pub mode: TransformationMode,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformationMode {
    Automatic,
    Manual,
}

impl TransformationEvidence {
    pub fn automatic(
        rule_id: impl Into<String>,
        before: &str,
        after: &str,
        reason: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            before_sha256: format!("{:x}", Sha256::digest(before.as_bytes())),
            after_sha256: format!("{:x}", Sha256::digest(after.as_bytes())),
            reason: reason.into(),
            confidence_delta: 0.0,
            mode: TransformationMode::Automatic,
            version: version.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TriggerDecision {
    pub should_run: bool,
    pub triggers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationEvidence {
    pub balanced_groups: bool,
    pub environments_closed: bool,
    pub left_right_balanced: bool,
    pub duplicate_token_run: bool,
    pub dangling_command: bool,
    pub unexpected_eos: bool,
    pub truncated: bool,
    pub matrix_shape_valid: bool,
}

impl ValidationEvidence {
    pub fn syntax_valid(&self) -> bool {
        self.balanced_groups
            && self.environments_closed
            && self.left_right_balanced
            && !self.duplicate_token_run
            && !self.dangling_command
            && self.matrix_shape_valid
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextDiff {
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PostProcessResult {
    pub raw: String,
    pub normalized: String,
    pub corrected: String,
    pub diff: Option<TextDiff>,
    pub trigger: TriggerDecision,
    pub raw_confidence: f32,
    pub normalized_confidence: f32,
    pub validation: ValidationEvidence,
    pub corrected_validation: ValidationEvidence,
    #[serde(default)]
    pub transformations: Vec<TransformationEvidence>,
    pub review_required: bool,
    pub status_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transformation_evidence_serializes_with_stable_hashes() {
        let evidence = TransformationEvidence::automatic(
            "trim-v1",
            " x ",
            "x",
            "trim surrounding whitespace",
            "1",
        );
        assert_eq!(evidence.before_sha256.len(), 64);
        assert_eq!(evidence.after_sha256.len(), 64);
        assert_ne!(evidence.before_sha256, evidence.after_sha256);
        let value = serde_json::to_value(&evidence).unwrap();
        assert_eq!(value["ruleId"], "trim-v1");
        assert_eq!(value["mode"], "automatic");
    }

    #[test]
    fn formula_serialization_keeps_render_source_separate_from_recognition_evidence() {
        let validation = ValidationEvidence {
            balanced_groups: true,
            environments_closed: true,
            left_right_balanced: true,
            duplicate_token_run: false,
            dangling_command: false,
            unexpected_eos: false,
            truncated: false,
            matrix_shape_valid: true,
        };
        let mut formula = crate::Formula::latex(r"\frac{a}{b}");
        formula.recognition_evidence = Some(Box::new(PostProcessResult {
            raw: r"\frac{a}{b".to_owned(),
            normalized: r"\frac{a}{b".to_owned(),
            corrected: r"\frac{a}{b}".to_owned(),
            diff: Some(TextDiff {
                before: r"\frac{a}{b".to_owned(),
                after: r"\frac{a}{b}".to_owned(),
            }),
            trigger: TriggerDecision {
                should_run: true,
                triggers: vec!["unbalanced_group".to_owned()],
            },
            raw_confidence: 0.4,
            normalized_confidence: 0.4,
            validation: ValidationEvidence {
                balanced_groups: false,
                ..validation.clone()
            },
            corrected_validation: validation,
            transformations: Vec::new(),
            review_required: false,
            status_code: None,
        }));
        let value = serde_json::to_value(formula).unwrap();
        assert_eq!(value["source"]["content"], r"\frac{a}{b}");
        assert_eq!(value["recognition_evidence"]["raw"], r"\frac{a}{b");
        assert_eq!(value["recognition_evidence"]["corrected"], r"\frac{a}{b}");
    }
}
