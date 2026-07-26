//! Recognition provenance and transformation evidence.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

    #[test]
    fn provenance_json_is_backward_and_forward_tolerant() {
        let old_json = serde_json::json!({
            "source": {"format": "Latex", "content": "x+y"},
            "display_mode": true,
            "confidence": 0.8,
            "source_info": null,
            "layout": null
        });
        let old_formula: crate::Formula = serde_json::from_value(old_json.clone()).unwrap();
        let old_snapshot = serde_json::to_value(&old_formula).unwrap();
        assert!(old_formula.recognition_provenance.is_none());
        assert!(old_formula.recognition_evidence.is_none());

        let new_without_evidence = serde_json::json!({
            "source": {"format": "Latex", "content": "x+y"},
            "display_mode": true,
            "confidence": 0.8,
            "recognition_provenance": {
                "modelId": "trocr-deit",
                "modelVersion": "models-v3.1.0",
                "runtime": "onnxruntime",
                "provider": "cpu",
                "sourceRegion": null,
                "rawConfidence": 0.8,
                "normalizedConfidence": 0.8,
                "transformations": [{
                    "ruleId": "trim-v1",
                    "beforeSha256": "0".repeat(64),
                    "afterSha256": "1".repeat(64),
                    "reason": "trim",
                    "confidenceDelta": 0.0,
                    "mode": "automatic",
                    "version": "1",
                    "futureEvidenceField": {"version": 2}
                }],
                "futureProviderField": true
            }
        });
        let parsed: crate::Formula = serde_json::from_value(new_without_evidence.clone()).unwrap();
        assert!(parsed.recognition_provenance.is_some());
        assert!(parsed.recognition_evidence.is_none());

        let mut null_evidence = new_without_evidence;
        null_evidence["recognition_evidence"] = serde_json::Value::Null;
        null_evidence["futureFormulaField"] = serde_json::json!("ignored");
        assert!(serde_json::from_value::<crate::Formula>(null_evidence)
            .unwrap()
            .recognition_evidence
            .is_none());

        let mut legacy_snapshot = serde_json::to_value(parsed).unwrap();
        let object = legacy_snapshot.as_object_mut().unwrap();
        object.remove("recognition_provenance");
        object.remove("recognition_evidence");
        assert_eq!(legacy_snapshot, old_snapshot);
    }
}
