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
    PluginWasiPermissionDenied,
    PluginWasiProtocolMismatch,
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
            Self::PluginWasiPermissionDenied => "PLUGIN_WASI_PERMISSION_DENIED",
            Self::PluginWasiProtocolMismatch => "PLUGIN_WASI_PROTOCOL_MISMATCH",
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
}

impl WasiDiagnostic {
    pub fn new(code: WasiDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
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
