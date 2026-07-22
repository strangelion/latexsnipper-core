use std::collections::BTreeMap;
use std::path::PathBuf;

use latexsnipper_runtime::{DeviceKind, RuntimeOptions};
use serde::{Deserialize, Serialize};

use crate::error::{tensorrt_error, TensorRtResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TensorRtPrecision {
    #[default]
    Fp32,
    Fp16,
    Int8,
}

impl TensorRtPrecision {
    pub(crate) const fn native_value(self) -> i32 {
        match self {
            Self::Fp32 => 0,
            Self::Fp16 => 1,
            Self::Int8 => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeProfile {
    pub min: Vec<i64>,
    pub opt: Vec<i64>,
    pub max: Vec<i64>,
}

impl ShapeProfile {
    fn validate(&self, input: &str) -> TensorRtResult<()> {
        if self.min.is_empty()
            || self.min.len() != self.opt.len()
            || self.min.len() != self.max.len()
        {
            return Err(tensorrt_error(format!(
                "profile '{input}' must have non-empty min/opt/max shapes with identical ranks"
            )));
        }
        for (index, ((min, opt), max)) in self.min.iter().zip(&self.opt).zip(&self.max).enumerate()
        {
            if *min <= 0 || min > opt || opt > max {
                return Err(tensorrt_error(format!(
                    "profile '{input}' dimension {index} must satisfy 0 < min <= opt <= max, got {min}/{opt}/{max}"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct SerializedOptions {
    #[serde(default, alias = "library_path")]
    library_path: Option<PathBuf>,
    #[serde(default = "default_true")]
    cache: bool,
    #[serde(default, alias = "cache_dir")]
    cache_dir: Option<PathBuf>,
    #[serde(default)]
    precision: TensorRtPrecision,
    #[serde(default, alias = "workspace_bytes")]
    workspace_bytes: u64,
    #[serde(default, alias = "device_id")]
    device_id: i32,
    #[serde(default)]
    profiles: BTreeMap<String, ShapeProfile>,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TensorRtOptions {
    #[serde(skip)]
    pub library_path: Option<PathBuf>,
    pub cache: bool,
    pub cache_dir: PathBuf,
    pub precision: TensorRtPrecision,
    pub workspace_bytes: u64,
    pub device_id: i32,
    pub profiles: BTreeMap<String, ShapeProfile>,
}

impl TensorRtOptions {
    pub fn from_runtime(options: &RuntimeOptions) -> TensorRtResult<Self> {
        if !matches!(options.device, DeviceKind::Auto | DeviceKind::Gpu) {
            return Err(tensorrt_error(
                "native TensorRT requires device 'auto' or 'gpu'",
            ));
        }
        if !options.providers.is_empty() {
            return Err(tensorrt_error(
                "native TensorRT does not accept ONNX execution providers; use deviceId for the CUDA device",
            ));
        }
        let serialized: SerializedOptions = serde_json::from_value(serde_json::Value::Object(
            options
                .extra
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ))
        .map_err(|error| tensorrt_error(format!("invalid runtime options: {error}")))?;
        if serialized.device_id < 0 {
            return Err(tensorrt_error("deviceId must be non-negative"));
        }
        for (input, profile) in &serialized.profiles {
            if input.trim().is_empty() {
                return Err(tensorrt_error("profile input name must not be empty"));
            }
            profile.validate(input)?;
        }
        Ok(Self {
            library_path: serialized.library_path,
            cache: serialized.cache,
            cache_dir: serialized.cache_dir.unwrap_or_else(default_cache_dir),
            precision: serialized.precision,
            workspace_bytes: serialized.workspace_bytes,
            device_id: serialized.device_id,
            profiles: serialized.profiles,
        })
    }
}

fn default_cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(root) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join("LaTeXSnipper/cache/tensorrt");
    }
    #[cfg(not(target_os = "windows"))]
    if let Some(root) = std::env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join("latexsnipper/tensorrt");
    }
    #[cfg(not(target_os = "windows"))]
    if let Some(root) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(root).join(".cache/latexsnipper/tensorrt");
    }
    std::env::temp_dir().join("latexsnipper/cache/tensorrt")
}

#[cfg(test)]
mod tests {
    use latexsnipper_runtime::ExecutionProviderSpec;

    use super::*;

    #[test]
    fn parses_and_validates_profiles() {
        let mut options = RuntimeOptions::default();
        options.extra.insert(
            "precision".to_owned(),
            serde_json::Value::String("fp16".to_owned()),
        );
        options.extra.insert(
            "profiles".to_owned(),
            serde_json::json!({
                "input": { "min": [1, 1, 128, 128], "opt": [1, 1, 384, 384], "max": [4, 1, 1024, 1024] }
            }),
        );
        let parsed = TensorRtOptions::from_runtime(&options).unwrap();
        assert_eq!(parsed.precision, TensorRtPrecision::Fp16);
        assert_eq!(parsed.profiles["input"].max[3], 1024);
    }

    #[test]
    fn rejects_invalid_profile_order() {
        let mut options = RuntimeOptions::default();
        options.extra.insert(
            "profiles".to_owned(),
            serde_json::json!({ "input": { "min": [4], "opt": [2], "max": [8] } }),
        );
        assert!(TensorRtOptions::from_runtime(&options)
            .unwrap_err()
            .to_string()
            .contains("min <= opt <= max"));
    }

    #[test]
    fn provider_options_are_not_confused_with_native_runtime() {
        let options = RuntimeOptions {
            providers: vec![ExecutionProviderSpec::new("cuda")],
            ..RuntimeOptions::default()
        };
        assert!(TensorRtOptions::from_runtime(&options)
            .unwrap_err()
            .to_string()
            .contains("does not accept ONNX execution providers"));
    }
}
