use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WasmErrorCode {
    Cancelled,
    InvalidArgument,
    InvalidImage,
    ImageLimitExceeded,
    TableElementLimitExceeded,
    ResultLimitExceeded,
    UnsupportedMode,
    UnsupportedFormat,
    ModelArtifactMissing,
    ModelArtifactInvalid,
    ModelChecksumMismatch,
    ModelMemoryLimit,
    UpdateNotActive,
    UpdateAlreadyActive,
    InferenceFailed,
    ConversionFailed,
    SerializationFailed,
    InternalError,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmError {
    pub code: WasmErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    pub recoverable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl WasmError {
    pub fn new(code: WasmErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            stage: None,
            recoverable: false,
            details: None,
        }
    }

    pub fn recoverable(code: WasmErrorCode, message: impl Into<String>) -> Self {
        Self {
            recoverable: true,
            ..Self::new(code, message)
        }
    }

    pub fn at_stage(mut self, stage: impl Into<String>) -> Self {
        self.stage = Some(stage.into());
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiDiagnostic {
    pub level: &'static str,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmResponse<T: Serialize> {
    pub ok: bool,
    pub api_version: u32,
    pub capability_version: u32,
    pub core_version: &'static str,
    pub schema_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<WasmError>,
    pub diagnostics: Vec<ApiDiagnostic>,
}

impl<T: Serialize> WasmResponse<T> {
    pub fn success(data: T, diagnostics: Vec<ApiDiagnostic>) -> Self {
        Self {
            ok: true,
            api_version: crate::api::WASM_API_VERSION,
            capability_version: crate::api::CAPABILITY_VERSION,
            core_version: env!("CARGO_PKG_VERSION"),
            schema_version: crate::api::AST_SCHEMA_VERSION,
            data: Some(data),
            error: None,
            diagnostics,
        }
    }

    pub fn failure(error: WasmError, diagnostics: Vec<ApiDiagnostic>) -> Self {
        Self {
            ok: false,
            api_version: crate::api::WASM_API_VERSION,
            capability_version: crate::api::CAPABILITY_VERSION,
            core_version: env!("CARGO_PKG_VERSION"),
            schema_version: crate::api::AST_SCHEMA_VERSION,
            data: None,
            error: Some(error),
            diagnostics,
        }
    }
}
