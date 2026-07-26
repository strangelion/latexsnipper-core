//! Stable, UI-independent engine readiness contract.

use latexsnipper_ast::Diagnostic;
use serde::{Deserialize, Serialize};

pub const READINESS_SCHEMA_VERSION: u32 = 1;

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
    RuntimeNotFound,
    ProviderUnavailable,
    ProviderLibraryMissing,
    SessionCreateFailed,
    InputShapeMismatch,
    DecoderCacheSchemaMismatch,
    DecoderIncrementalDivergence,
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
            Self::RuntimeNotFound => "RUNTIME_NOT_FOUND",
            Self::ProviderUnavailable => "PROVIDER_UNAVAILABLE",
            Self::ProviderLibraryMissing => "PROVIDER_LIBRARY_MISSING",
            Self::SessionCreateFailed => "SESSION_CREATE_FAILED",
            Self::InputShapeMismatch => "INPUT_SHAPE_MISMATCH",
            Self::DecoderCacheSchemaMismatch => "DECODER_CACHE_SCHEMA_MISMATCH",
            Self::DecoderIncrementalDivergence => "DECODER_INCREMENTAL_DIVERGENCE",
            Self::OutputValidationFailed => "OUTPUT_VALIDATION_FAILED",
            Self::PostprocessReviewRequired => "POSTPROCESS_REVIEW_REQUIRED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineReadiness {
    #[serde(default = "readiness_schema_version")]
    pub schema_version: u32,
    pub core_version: String,
    #[serde(default)]
    pub modes: Vec<ModeReadiness>,
    #[serde(default)]
    pub runtimes: Vec<RuntimeReadiness>,
    #[serde(default)]
    pub models: Vec<ModelReadiness>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModeReadiness {
    pub mode: String,
    pub ready: bool,
    #[serde(default)]
    pub tasks: Vec<TaskReadiness>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskReadiness {
    pub task: String,
    pub ready: bool,
    pub selected_model: Option<String>,
    pub code: Option<CoreErrorCode>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeReadiness {
    pub id: String,
    pub available: bool,
    pub version: Option<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub devices: Vec<String>,
    pub code: Option<CoreErrorCode>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelReadiness {
    pub id: String,
    pub task: String,
    pub version: String,
    pub ready: bool,
    pub runtime: Option<String>,
    pub provider: Option<String>,
    pub code: Option<CoreErrorCode>,
    pub message: Option<String>,
}

const fn readiness_schema_version() -> u32 {
    READINESS_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_json_snapshot_is_stable_and_strict() {
        let readiness = EngineReadiness {
            schema_version: READINESS_SCHEMA_VERSION,
            core_version: "3.1.0".to_owned(),
            modes: vec![ModeReadiness {
                mode: "formula".to_owned(),
                ready: false,
                tasks: vec![TaskReadiness {
                    task: "formula-recognition".to_owned(),
                    ready: false,
                    selected_model: None,
                    code: Some(CoreErrorCode::ModelNotFound),
                    message: Some("no runnable model".to_owned()),
                }],
            }],
            runtimes: Vec::new(),
            models: Vec::new(),
            diagnostics: Vec::new(),
        };

        let value = serde_json::to_value(&readiness).unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["modes"][0]["tasks"][0]["code"], "MODEL_NOT_FOUND");
        let mut unknown = value;
        unknown["privateSession"] = serde_json::json!(true);
        assert!(serde_json::from_value::<EngineReadiness>(unknown).is_err());
    }

    #[test]
    fn every_error_code_has_the_exact_wire_spelling() {
        let codes = [
            CoreErrorCode::ModelNotFound,
            CoreErrorCode::ModelManifestInvalid,
            CoreErrorCode::ModelArtifactMissing,
            CoreErrorCode::ModelArtifactHashMismatch,
            CoreErrorCode::RuntimeNotFound,
            CoreErrorCode::ProviderUnavailable,
            CoreErrorCode::ProviderLibraryMissing,
            CoreErrorCode::SessionCreateFailed,
            CoreErrorCode::InputShapeMismatch,
            CoreErrorCode::DecoderCacheSchemaMismatch,
            CoreErrorCode::DecoderIncrementalDivergence,
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
