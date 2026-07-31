//! Stable, UI-independent engine readiness and acceptance contracts.

use latexsnipper_ast::Diagnostic;
use serde::{Deserialize, Deserializer, Serialize};

pub const READINESS_SCHEMA_VERSION: u32 = 2;

/// Stable error codes shared by readiness, diagnostics, SDK, and Office
/// consumers. Human-readable error text is supplementary and must not be
/// parsed to make decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CoreErrorCode {
    ModelNotFound,
    ModelManifestInvalid,
    ModelArtifactMissing,
    ModelArtifactHashMismatch,
    ModelBaselineMissing,
    ModelBaselineFailed,
    ModelQualityNotValidated,
    AutoAcceptNotRecommended,
    CroppedFormulaModelMissing,
    RuntimeNotFound,
    ProviderUnavailable,
    ProviderLibraryMissing,
    ProviderLibraryNotFound,
    ProviderLoadFailed,
    ProviderValidationStale,
    ProviderValidationRequired,
    SessionCreateFailed,
    ProviderSessionCreateFailed,
    ProviderSmokeInferenceFailed,
    ProviderOutputMismatch,
    InputShapeMismatch,
    DecoderCacheSchemaMismatch,
    DecoderIncrementalDivergence,
    DecoderArtifactMissing,
    DecoderStateCaptureUnavailable,
    RealDatasetMissing,
    TableQualityBaselineMissing,
    OutputValidationFailed,
    PostprocessReviewRequired,
}

impl CoreErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ModelNotFound => "MODEL_NOT_FOUND",
            Self::ModelManifestInvalid => "MODEL_MANIFEST_INVALID",
            Self::ModelArtifactMissing => "MODEL_ARTIFACT_MISSING",
            Self::ModelArtifactHashMismatch => "MODEL_ARTIFACT_HASH_MISMATCH",
            Self::ModelBaselineMissing => "MODEL_BASELINE_MISSING",
            Self::ModelBaselineFailed => "MODEL_BASELINE_FAILED",
            Self::ModelQualityNotValidated => "MODEL_QUALITY_NOT_VALIDATED",
            Self::AutoAcceptNotRecommended => "AUTO_ACCEPT_NOT_RECOMMENDED",
            Self::CroppedFormulaModelMissing => "CROPPED_FORMULA_MODEL_MISSING",
            Self::RuntimeNotFound => "RUNTIME_NOT_FOUND",
            Self::ProviderUnavailable => "PROVIDER_UNAVAILABLE",
            Self::ProviderLibraryMissing => "PROVIDER_LIBRARY_MISSING",
            Self::ProviderLibraryNotFound => "PROVIDER_LIBRARY_NOT_FOUND",
            Self::ProviderLoadFailed => "PROVIDER_LOAD_FAILED",
            Self::ProviderValidationStale => "PROVIDER_VALIDATION_STALE",
            Self::ProviderValidationRequired => "PROVIDER_VALIDATION_REQUIRED",
            Self::SessionCreateFailed => "SESSION_CREATE_FAILED",
            Self::ProviderSessionCreateFailed => "PROVIDER_SESSION_CREATE_FAILED",
            Self::ProviderSmokeInferenceFailed => "PROVIDER_SMOKE_INFERENCE_FAILED",
            Self::ProviderOutputMismatch => "PROVIDER_OUTPUT_MISMATCH",
            Self::InputShapeMismatch => "INPUT_SHAPE_MISMATCH",
            Self::DecoderCacheSchemaMismatch => "DECODER_CACHE_SCHEMA_MISMATCH",
            Self::DecoderIncrementalDivergence => "DECODER_INCREMENTAL_DIVERGENCE",
            Self::DecoderArtifactMissing => "DECODER_ARTIFACT_MISSING",
            Self::DecoderStateCaptureUnavailable => "DECODER_STATE_CAPTURE_UNAVAILABLE",
            Self::RealDatasetMissing => "REAL_DATASET_MISSING",
            Self::TableQualityBaselineMissing => "TABLE_QUALITY_BASELINE_MISSING",
            Self::OutputValidationFailed => "OUTPUT_VALIDATION_FAILED",
            Self::PostprocessReviewRequired => "POSTPROCESS_REVIEW_REQUIRED",
        }
    }
}

/// Consumer-facing readiness snapshot. Unknown fields are intentionally
/// ignored so an older Office or SDK consumer can read a newer producer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineReadiness {
    #[serde(default = "readiness_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub core_version: String,
    #[serde(default, deserialize_with = "null_default")]
    pub modes: Vec<ModeReadiness>,
    #[serde(default, deserialize_with = "null_default")]
    pub runtimes: Vec<RuntimeReadiness>,
    #[serde(default, deserialize_with = "null_default")]
    pub models: Vec<ModelReadiness>,
    #[serde(default, deserialize_with = "null_default")]
    pub quality: Vec<ModelQualityReadiness>,
    #[serde(default, deserialize_with = "null_default")]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeReadiness {
    #[serde(default)]
    pub mode: String,
    /// `ready` is accepted only as a v1 input alias. v2 never emits it.
    #[serde(default, alias = "ready", deserialize_with = "null_default")]
    pub technical_ready: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub quality_ready: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub production_recommended: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub tasks: Vec<TaskReadiness>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskReadiness {
    #[serde(default)]
    pub task: String,
    /// `ready` is accepted only as a v1 input alias. v2 never emits it.
    #[serde(default, alias = "ready", deserialize_with = "null_default")]
    pub technical_ready: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub quality_ready: bool,
    #[serde(default)]
    pub selected_model: Option<String>,
    #[serde(default)]
    pub code: Option<CoreErrorCode>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeReadiness {
    #[serde(default)]
    pub id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub available: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub providers: Vec<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub provider_validations: Vec<ProviderValidationReport>,
    #[serde(default, deserialize_with = "null_default")]
    pub devices: Vec<String>,
    #[serde(default)]
    pub code: Option<CoreErrorCode>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderValidationLevel {
    #[default]
    Declared,
    LibraryDetected,
    ProbePassed,
    SessionCreated,
    SmokeInferencePassed,
    BenchmarkMeasured,
    BenchmarkValidated,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderValidationPolicy {
    #[default]
    ProbeOnly,
    CreateSession,
    SmokeInference,
    Benchmark,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderValidationKey {
    pub core_version: String,
    pub runtime_version: String,
    pub provider: String,
    pub provider_library_fingerprint: String,
    pub os: String,
    pub architecture: String,
    pub device_driver_fingerprint: String,
    pub smoke_model_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderValidationRequest {
    pub provider: String,
    #[serde(default, deserialize_with = "null_default")]
    pub policy: ProviderValidationPolicy,
    #[serde(default)]
    pub key: Option<ProviderValidationKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderValidationReport {
    #[serde(default)]
    pub provider: String,
    #[serde(default, deserialize_with = "null_default")]
    pub validation_level: ProviderValidationLevel,
    #[serde(default, deserialize_with = "null_default")]
    pub library_detected: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub probe_passed: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub session_created: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub smoke_inference_passed: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub benchmark_measured: bool,
    /// Reserved for a benchmark evaluated against explicit, versioned
    /// acceptance criteria. Merely collecting timing samples does not set it.
    #[serde(default, deserialize_with = "null_default")]
    pub benchmark_validated: bool,
    #[serde(default)]
    pub key: Option<ProviderValidationKey>,
    #[serde(default, deserialize_with = "null_default")]
    pub stale: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelQualityStatus {
    #[default]
    Unknown,
    BaselineMissing,
    BaselineFailed,
    Experimental,
    Validated,
}

impl ModelQualityStatus {
    pub const fn is_quality_ready(self) -> bool {
        matches!(self, Self::Experimental | Self::Validated)
    }

    pub const fn is_production_validated(self) -> bool {
        matches!(self, Self::Validated)
    }
}

impl ModelReadiness {
    pub const fn observed_technical_facts_ready(&self) -> bool {
        self.manifest_valid
            && self.artifacts_valid
            && self.runtime_resolved
            && self.executor_created
            && self.session_created
            && self.smoke_inference_passed
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelQualityReadiness {
    #[serde(default)]
    pub model_id: String,
    #[serde(default)]
    pub model_version: String,
    #[serde(default)]
    pub task: String,
    #[serde(default, deserialize_with = "null_default")]
    pub status: ModelQualityStatus,
    #[serde(default)]
    pub dataset_version: Option<String>,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub baseline_sha256: Option<String>,
    #[serde(default)]
    pub code: Option<CoreErrorCode>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelReadiness {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub version: String,
    #[serde(default, deserialize_with = "null_default")]
    pub manifest_valid: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub artifacts_valid: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub runtime_resolved: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub executor_created: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub session_created: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub smoke_inference_passed: bool,
    /// `ready` is accepted only as a v1 input alias. v2 never emits it.
    #[serde(default, alias = "ready", deserialize_with = "null_default")]
    pub technical_ready: bool,
    #[serde(default, deserialize_with = "null_default")]
    pub quality_status: ModelQualityStatus,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub code: Option<CoreErrorCode>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecognitionAction {
    AutoAccept,
    RequireReview,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionAcceptance {
    pub technically_valid: bool,
    pub quality_status: ModelQualityStatus,
    pub confidence: f32,
    pub parse_valid: bool,
    pub structure_valid: bool,
    pub review_required: bool,
    pub recommended_action: RecognitionAction,
    #[serde(default, deserialize_with = "null_default")]
    pub reasons: Vec<CoreErrorCode>,
}

impl RecognitionAcceptance {
    pub fn decide(
        technically_valid: bool,
        quality_status: ModelQualityStatus,
        confidence: f32,
        parse_valid: bool,
        structure_valid: bool,
        review_required: bool,
    ) -> Self {
        let mut reasons = Vec::new();
        if !technically_valid || !parse_valid || !structure_valid {
            reasons.push(CoreErrorCode::OutputValidationFailed);
        }
        if !quality_status.is_production_validated() {
            reasons.push(CoreErrorCode::ModelQualityNotValidated);
        }
        if review_required {
            reasons.push(CoreErrorCode::PostprocessReviewRequired);
        }
        let recommended_action = if !technically_valid || !parse_valid || !structure_valid {
            RecognitionAction::Reject
        } else if quality_status.is_production_validated() && confidence >= 0.90 && !review_required
        {
            RecognitionAction::AutoAccept
        } else {
            reasons.push(CoreErrorCode::AutoAcceptNotRecommended);
            RecognitionAction::RequireReview
        };
        reasons.sort_by_key(|code| code.as_str());
        reasons.dedup();
        Self {
            technically_valid,
            quality_status,
            confidence,
            parse_valid,
            structure_valid,
            review_required,
            recommended_action,
            reasons,
        }
    }
}

const fn readiness_schema_version() -> u32 {
    READINESS_SCHEMA_VERSION
}

fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_v2_is_forward_compatible_and_accepts_v1() {
        let v1 = serde_json::json!({
            "schemaVersion": 1,
            "coreVersion": "3.1.0",
            "modes": [{
                "mode": "formula",
                "ready": true,
                "tasks": [{
                    "task": "formula-recognition",
                    "ready": true,
                    "selectedModel": "trocr-deit",
                    "code": null,
                    "message": null,
                    "futureTaskField": {"nested": true}
                }],
                "futureModeField": 42
            }],
            "runtimes": [],
            "models": [],
            "diagnostics": [],
            "futureRootField": true
        });
        let parsed: EngineReadiness = serde_json::from_value(v1).unwrap();
        assert_eq!(parsed.schema_version, 1);
        assert!(parsed.modes[0].technical_ready);
        assert!(parsed.modes[0].tasks[0].technical_ready);
        assert!(!parsed.modes[0].quality_ready);
        assert!(parsed.quality.is_empty());
    }

    #[test]
    fn v2_output_has_split_readiness_and_no_legacy_ready_field() {
        let readiness = EngineReadiness {
            schema_version: READINESS_SCHEMA_VERSION,
            core_version: "3.1.0".to_owned(),
            modes: vec![ModeReadiness {
                mode: "formula".to_owned(),
                technical_ready: true,
                quality_ready: false,
                production_recommended: false,
                tasks: vec![TaskReadiness {
                    task: "formula-recognition".to_owned(),
                    technical_ready: true,
                    quality_ready: false,
                    selected_model: Some("trocr-deit".to_owned()),
                    code: Some(CoreErrorCode::ModelBaselineFailed),
                    message: None,
                }],
            }],
            runtimes: Vec::new(),
            models: Vec::new(),
            quality: Vec::new(),
            diagnostics: Vec::new(),
        };
        let value = serde_json::to_value(&readiness).unwrap();
        assert_eq!(value["schemaVersion"], 2);
        assert!(value["modes"][0].get("ready").is_none());
        assert_eq!(value["modes"][0]["technicalReady"], true);

        let reordered_with_nulls = serde_json::json!({
            "diagnostics": [],
            "quality": null,
            "models": [],
            "runtimes": [],
            "modes": [],
            "coreVersion": "3.1.0",
            "schemaVersion": 2
        });
        let parsed: EngineReadiness = serde_json::from_value(reordered_with_nulls).unwrap();
        assert!(parsed.quality.is_empty());
    }

    #[test]
    fn acceptance_never_auto_accepts_an_unvalidated_model() {
        let acceptance = RecognitionAcceptance::decide(
            true,
            ModelQualityStatus::BaselineFailed,
            0.99,
            true,
            true,
            false,
        );
        assert_eq!(
            acceptance.recommended_action,
            RecognitionAction::RequireReview
        );
        assert!(acceptance
            .reasons
            .contains(&CoreErrorCode::ModelQualityNotValidated));
    }

    #[test]
    fn office_consumer_fixture_is_tolerant_and_fail_closed_on_quality() {
        let readiness: EngineReadiness = serde_json::from_str(include_str!(
            "../../../contracts/fixtures/office-readiness-v2.json"
        ))
        .unwrap();
        let mode = &readiness.modes[0];
        assert!(mode.technical_ready);
        assert!(!mode.quality_ready);
        assert!(!mode.production_recommended);
        assert_eq!(
            readiness.models[0].quality_status,
            ModelQualityStatus::BaselineFailed
        );
    }

    #[test]
    fn technical_readiness_requires_executor_session_and_smoke() {
        let mut model = ModelReadiness {
            id: "demo".to_owned(),
            task: "formula-recognition".to_owned(),
            version: "1".to_owned(),
            manifest_valid: true,
            artifacts_valid: true,
            runtime_resolved: true,
            executor_created: false,
            session_created: false,
            smoke_inference_passed: false,
            technical_ready: false,
            quality_status: ModelQualityStatus::Validated,
            runtime: Some("onnx-runtime".to_owned()),
            provider: Some("cpu".to_owned()),
            code: None,
            message: None,
        };
        assert!(!model.observed_technical_facts_ready());
        model.executor_created = true;
        assert!(!model.observed_technical_facts_ready());
        model.session_created = true;
        assert!(!model.observed_technical_facts_ready());
        model.smoke_inference_passed = true;
        assert!(model.observed_technical_facts_ready());
    }

    #[test]
    fn every_error_code_has_the_exact_wire_spelling() {
        let codes = [
            CoreErrorCode::ModelNotFound,
            CoreErrorCode::ModelManifestInvalid,
            CoreErrorCode::ModelArtifactMissing,
            CoreErrorCode::ModelArtifactHashMismatch,
            CoreErrorCode::ModelBaselineMissing,
            CoreErrorCode::ModelBaselineFailed,
            CoreErrorCode::ModelQualityNotValidated,
            CoreErrorCode::AutoAcceptNotRecommended,
            CoreErrorCode::CroppedFormulaModelMissing,
            CoreErrorCode::RuntimeNotFound,
            CoreErrorCode::ProviderUnavailable,
            CoreErrorCode::ProviderLibraryMissing,
            CoreErrorCode::ProviderLibraryNotFound,
            CoreErrorCode::ProviderLoadFailed,
            CoreErrorCode::ProviderValidationStale,
            CoreErrorCode::ProviderValidationRequired,
            CoreErrorCode::SessionCreateFailed,
            CoreErrorCode::ProviderSessionCreateFailed,
            CoreErrorCode::ProviderSmokeInferenceFailed,
            CoreErrorCode::ProviderOutputMismatch,
            CoreErrorCode::InputShapeMismatch,
            CoreErrorCode::DecoderCacheSchemaMismatch,
            CoreErrorCode::DecoderIncrementalDivergence,
            CoreErrorCode::DecoderArtifactMissing,
            CoreErrorCode::DecoderStateCaptureUnavailable,
            CoreErrorCode::RealDatasetMissing,
            CoreErrorCode::TableQualityBaselineMissing,
            CoreErrorCode::OutputValidationFailed,
            CoreErrorCode::PostprocessReviewRequired,
        ];
        for code in codes {
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::Value::String(code.as_str().to_owned())
            );
        }
    }
}
