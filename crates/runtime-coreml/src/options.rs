//! Core ML execution and compilation-cache options.

use std::path::PathBuf;

use latexsnipper_runtime::{DeviceKind, RuntimeOptions};

use crate::error::{coreml_error, CoreMlResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CoreMlComputeUnits {
    #[default]
    All,
    CpuOnly,
    CpuAndGpu,
    CpuAndNeuralEngine,
}

impl CoreMlComputeUnits {
    #[cfg(target_vendor = "apple")]
    pub(crate) const fn native_code(self) -> i32 {
        match self {
            Self::All => 0,
            Self::CpuOnly => 1,
            Self::CpuAndGpu => 2,
            Self::CpuAndNeuralEngine => 3,
        }
    }

    fn parse(value: &str) -> CoreMlResult<Self> {
        match value.to_ascii_lowercase().replace('_', "-").as_str() {
            "all" | "auto" | "coreml" => Ok(Self::All),
            "cpu" | "cpu-only" => Ok(Self::CpuOnly),
            "gpu" | "cpu-gpu" | "cpu-and-gpu" => Ok(Self::CpuAndGpu),
            "npu" | "neural-engine" | "cpu-neural-engine" | "cpu-and-neural-engine" => {
                Ok(Self::CpuAndNeuralEngine)
            }
            other => Err(coreml_error(format!(
                "unsupported Core ML computeUnits value '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreMlOptions {
    pub compute_units: CoreMlComputeUnits,
    pub cache: bool,
    pub cache_dir: PathBuf,
}

impl CoreMlOptions {
    pub fn from_runtime(options: &RuntimeOptions) -> CoreMlResult<Self> {
        let explicit = options
            .extra
            .get("computeUnits")
            .and_then(serde_json::Value::as_str)
            .map(CoreMlComputeUnits::parse)
            .transpose()?;
        let provider = provider_compute_units(options)?;
        if explicit.is_some() && provider.is_some() && explicit != provider {
            return Err(coreml_error(
                "computeUnits conflicts with the requested execution provider",
            ));
        }
        let device = match options.device {
            DeviceKind::Auto => None,
            DeviceKind::Cpu => Some(CoreMlComputeUnits::CpuOnly),
            DeviceKind::Gpu => Some(CoreMlComputeUnits::CpuAndGpu),
            DeviceKind::Npu => Some(CoreMlComputeUnits::CpuAndNeuralEngine),
        };
        let selected = explicit.or(provider).or(device).unwrap_or_default();
        if let Some(device) = device {
            if device != selected {
                return Err(coreml_error(
                    "Core ML computeUnits/provider conflicts with the common device option",
                ));
            }
        }

        let cache = options
            .extra
            .get("cache")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let cache_dir = options
            .extra
            .get("cacheDir")
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("latexsnipper-coreml-cache"));
        Ok(Self {
            compute_units: selected,
            cache,
            cache_dir,
        })
    }
}

fn provider_compute_units(options: &RuntimeOptions) -> CoreMlResult<Option<CoreMlComputeUnits>> {
    if options.providers.is_empty() {
        return Ok(None);
    }
    let mut selected = None;
    for provider in &options.providers {
        let units = CoreMlComputeUnits::parse(&provider.name)?;
        if selected.is_some_and(|selected| selected != units) {
            return Err(coreml_error(
                "native Core ML accepts one compute-unit policy, not an ordered provider chain",
            ));
        }
        selected = Some(units);
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use latexsnipper_runtime::ExecutionProviderSpec;

    use super::*;

    #[test]
    fn maps_common_devices_to_compute_units() {
        let runtime = RuntimeOptions {
            device: DeviceKind::Npu,
            ..RuntimeOptions::default()
        };
        assert_eq!(
            CoreMlOptions::from_runtime(&runtime).unwrap().compute_units,
            CoreMlComputeUnits::CpuAndNeuralEngine
        );
    }

    #[test]
    fn rejects_provider_priority_that_coreml_cannot_represent() {
        let runtime = RuntimeOptions {
            providers: vec![
                ExecutionProviderSpec::new("cpu-gpu"),
                ExecutionProviderSpec::new("cpu"),
            ],
            ..RuntimeOptions::default()
        };
        assert!(CoreMlOptions::from_runtime(&runtime).is_err());
    }

    #[test]
    fn explicit_cache_configuration_is_preserved() {
        let mut runtime = RuntimeOptions::default();
        runtime.extra.insert("cache".to_owned(), false.into());
        runtime
            .extra
            .insert("cacheDir".to_owned(), "/tmp/coreml".into());
        let parsed = CoreMlOptions::from_runtime(&runtime).unwrap();
        assert!(!parsed.cache);
        assert_eq!(parsed.cache_dir, PathBuf::from("/tmp/coreml"));
    }
}
