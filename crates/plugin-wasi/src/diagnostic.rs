use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WasiDiagnosticCode {
    PluginWasiTrap,
    PluginWasiTimeout,
    PluginWasiCancelled,
    PluginWasiMemoryLimit,
    PluginWasiOutputLimit,
    PluginWasiResourcePolicy,
    PluginWasiPermissionDenied,
    PluginWasiCapabilityMismatch,
    PluginWasiInvocationNotDeclared,
    PluginWasiProtocolMismatch,
    PluginWasiInvalidInput,
    PluginWasiInvalidPatch,
    PluginWasiHostFailure,
}

impl WasiDiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PluginWasiTrap => "PLUGIN_WASI_TRAP",
            Self::PluginWasiTimeout => "PLUGIN_WASI_TIMEOUT",
            Self::PluginWasiCancelled => "PLUGIN_WASI_CANCELLED",
            Self::PluginWasiMemoryLimit => "PLUGIN_WASI_MEMORY_LIMIT",
            Self::PluginWasiOutputLimit => "PLUGIN_WASI_OUTPUT_LIMIT",
            Self::PluginWasiResourcePolicy => "PLUGIN_WASI_RESOURCE_POLICY",
            Self::PluginWasiPermissionDenied => "PLUGIN_WASI_PERMISSION_DENIED",
            Self::PluginWasiCapabilityMismatch => "PLUGIN_WASI_CAPABILITY_MISMATCH",
            Self::PluginWasiInvocationNotDeclared => "PLUGIN_WASI_INVOCATION_NOT_DECLARED",
            Self::PluginWasiProtocolMismatch => "PLUGIN_WASI_PROTOCOL_MISMATCH",
            Self::PluginWasiInvalidInput => "PLUGIN_WASI_INVALID_INPUT",
            Self::PluginWasiInvalidPatch => "PLUGIN_WASI_INVALID_PATCH",
            Self::PluginWasiHostFailure => "PLUGIN_WASI_HOST_FAILURE",
        }
    }
}

impl fmt::Display for WasiDiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasiDiagnostic {
    pub code: WasiDiagnosticCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<WasiDiagnosticDetail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WasiDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasiDiagnosticDetail {
    pub code: WasiDiagnosticCode,
    pub severity: WasiDiagnosticSeverity,
    pub message: String,
    pub field: Option<String>,
}

impl WasiDiagnostic {
    pub fn new(code: WasiDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }

    pub fn with_details(mut self, details: Vec<WasiDiagnosticDetail>) -> Self {
        self.details = details;
        self
    }
}

impl fmt::Display for WasiDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for WasiDiagnostic {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_machine_values() {
        assert_eq!(
            WasiDiagnosticCode::PluginWasiProtocolMismatch.as_str(),
            "PLUGIN_WASI_PROTOCOL_MISMATCH"
        );
        assert_eq!(
            serde_json::to_string(&WasiDiagnosticCode::PluginWasiTimeout).unwrap(),
            "\"PLUGIN_WASI_TIMEOUT\""
        );
    }
}
