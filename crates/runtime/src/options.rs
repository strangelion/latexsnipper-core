//! Runtime-independent execution options.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::AccelerationMode;

/// Execution device preference. The concrete provider is runtime-specific.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    #[default]
    Auto,
    Cpu,
    Gpu,
    Npu,
}

/// Configuration for one runtime-specific execution provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionProviderSpec {
    /// Canonical provider identifier such as `cuda`, `directml`, or `cpu`.
    pub name: String,
    /// Provider-specific options. Ownership and interpretation belong to the
    /// selected runtime factory.
    #[serde(default)]
    pub options: BTreeMap<String, serde_json::Value>,
}

impl ExecutionProviderSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            options: BTreeMap::new(),
        }
    }

    pub fn with_option(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.options.insert(key.into(), value.into());
        self
    }

    pub fn cpu() -> Self {
        Self::new("cpu")
    }
}

/// Options used when a factory creates a runtime session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOptions {
    #[serde(default)]
    pub device: DeviceKind,
    /// Ordered provider preference. An empty list asks the runtime to choose.
    #[serde(default)]
    pub providers: Vec<ExecutionProviderSpec>,
    /// Maximum intra-op threads. Zero means runtime default.
    #[serde(default, alias = "max_threads")]
    pub max_threads: usize,
    #[serde(default = "default_true", alias = "graph_optimization")]
    pub graph_optimization: bool,
    /// Runtime-specific options not understood by the common layer.
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

const fn default_true() -> bool {
    true
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            device: DeviceKind::Auto,
            providers: Vec::new(),
            max_threads: 0,
            graph_optimization: true,
            extra: BTreeMap::new(),
        }
    }
}

impl RuntimeOptions {
    pub fn cpu() -> Self {
        Self {
            device: DeviceKind::Cpu,
            providers: vec![ExecutionProviderSpec::cpu()],
            ..Self::default()
        }
    }

    pub fn from_acceleration(mode: AccelerationMode) -> Self {
        match mode {
            AccelerationMode::Auto => Self::default(),
            AccelerationMode::Cpu => Self::cpu(),
            AccelerationMode::Gpu => Self {
                device: DeviceKind::Gpu,
                providers: legacy_gpu_provider_order(),
                ..Self::default()
            },
        }
    }

    /// Convert the common options back to the legacy three-state API.
    pub fn legacy_acceleration(&self) -> AccelerationMode {
        if self.device == DeviceKind::Cpu
            || (!self.providers.is_empty()
                && self
                    .providers
                    .iter()
                    .all(|provider| provider.name.eq_ignore_ascii_case("cpu")))
        {
            AccelerationMode::Cpu
        } else if self.device == DeviceKind::Gpu
            || self.providers.iter().any(|provider| {
                matches!(
                    provider.name.to_ascii_lowercase().as_str(),
                    "cuda" | "directml" | "openvino" | "tensorrt" | "coreml" | "qnn"
                )
            })
        {
            AccelerationMode::Gpu
        } else {
            AccelerationMode::Auto
        }
    }
}

impl From<AccelerationMode> for RuntimeOptions {
    fn from(value: AccelerationMode) -> Self {
        Self::from_acceleration(value)
    }
}

/// Deterministic compatibility mapping for the old `Gpu` switch. New callers
/// should provide their desired provider order explicitly.
fn legacy_gpu_provider_order() -> Vec<ExecutionProviderSpec> {
    vec![
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        ExecutionProviderSpec::new("cuda"),
        #[cfg(target_os = "windows")]
        ExecutionProviderSpec::new("directml"),
        #[cfg(target_vendor = "apple")]
        ExecutionProviderSpec::new("coreml"),
        ExecutionProviderSpec::cpu(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_acceleration_round_trips_without_becoming_a_runtime() {
        for mode in [
            AccelerationMode::Auto,
            AccelerationMode::Cpu,
            AccelerationMode::Gpu,
        ] {
            let options = RuntimeOptions::from(mode);
            assert_eq!(options.legacy_acceleration(), mode);
        }
    }
}
