use latexsnipper_ast::{Diagnostic, DOCUMENT_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};

pub const API_ENVELOPE_VERSION_V3: u32 = 3;
pub const CAPABILITY_SCHEMA_VERSION_V3: u32 = 3;
pub const DIAGNOSTIC_SCHEMA_VERSION_V3: u32 = 1;

/// Independent protocol versions carried by a v3 API envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiContractVersionsV3 {
    pub api_envelope_version: u32,
    pub capability_schema_version: u32,
    pub diagnostic_schema_version: u32,
    pub document_schema_version: String,
    pub core_version: String,
}

impl ApiContractVersionsV3 {
    pub fn current() -> Self {
        Self {
            api_envelope_version: API_ENVELOPE_VERSION_V3,
            capability_schema_version: CAPABILITY_SCHEMA_VERSION_V3,
            diagnostic_schema_version: DIAGNOSTIC_SCHEMA_VERSION_V3,
            document_schema_version: DOCUMENT_SCHEMA_VERSION.to_string(),
            core_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Stable v3 error payload. Engine-specific text belongs in `details`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiErrorV3 {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Strict v3 response envelope contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEnvelopeV3<T> {
    pub ok: bool,
    pub versions: ApiContractVersionsV3,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiErrorV3>,
}

impl<T> ApiEnvelopeV3<T> {
    pub fn success(data: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            ok: true,
            versions: ApiContractVersionsV3::current(),
            diagnostics,
            data: Some(data),
            error: None,
        }
    }

    pub fn failure(error: ApiErrorV3, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            ok: false,
            versions: ApiContractVersionsV3::current(),
            diagnostics,
            data: None,
            error: Some(error),
        }
    }

    pub fn has_valid_shape(&self) -> bool {
        matches!(
            (self.ok, self.data.is_some(), self.error.is_some()),
            (true, true, false) | (false, false, true)
        )
    }

    pub fn has_supported_versions(&self) -> bool {
        let supported_core = semver::Version::parse(&self.versions.core_version)
            .is_ok_and(|version| version.major == 3);
        self.versions.api_envelope_version == API_ENVELOPE_VERSION_V3
            && self.versions.capability_schema_version == CAPABILITY_SCHEMA_VERSION_V3
            && self.versions.diagnostic_schema_version == DIAGNOSTIC_SCHEMA_VERSION_V3
            && self.versions.document_schema_version == DOCUMENT_SCHEMA_VERSION
            && supported_core
    }

    pub fn has_valid_contract(&self) -> bool {
        let error_is_valid = self
            .error
            .as_ref()
            .is_none_or(|error| !error.code.trim().is_empty() && !error.message.trim().is_empty());
        self.has_valid_shape() && self.has_supported_versions() && error_is_valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_versions_are_independent_and_shape_is_strict() {
        let success = ApiEnvelopeV3::success("ok", Vec::new());
        assert!(success.has_valid_contract());
        assert_eq!(success.versions.api_envelope_version, 3);
        assert_eq!(success.versions.document_schema_version, "1.0.0");

        let mut malformed = success;
        malformed.error = Some(ApiErrorV3 {
            code: "E_TEST".to_string(),
            message: "fixture".to_string(),
            recoverable: false,
            details: None,
        });
        assert!(!malformed.has_valid_shape());

        let mut wrong_version = ApiEnvelopeV3::success("ok", Vec::new());
        wrong_version.versions.capability_schema_version = 2;
        assert!(!wrong_version.has_valid_contract());

        let invalid_error = ApiEnvelopeV3::<()>::failure(
            ApiErrorV3 {
                code: String::new(),
                message: "fixture".to_string(),
                recoverable: false,
                details: None,
            },
            Vec::new(),
        );
        assert!(!invalid_error.has_valid_contract());
    }
}
