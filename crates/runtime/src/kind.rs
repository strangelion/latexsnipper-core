//! Stable identifiers for inference runtimes.
//!
//! A runtime is the program that executes a model. Hardware acceleration is
//! configured separately through [`crate::RuntimeOptions`]. For example,
//! ONNX Runtime with CUDA and ONNX Runtime with DirectML are both
//! [`RuntimeKind::OnnxRuntime`].

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Stable identifier for a model inference runtime.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuntimeKind {
    OnnxRuntime,
    PaddleInference,
    ExecuTorch,
    TensorRt,
    TensorRtRtx,
    CoreMl,
    /// A separately installed runtime plugin.
    Custom(String),
}

impl RuntimeKind {
    /// Canonical manifest/ABI identifier.
    pub fn as_str(&self) -> &str {
        match self {
            Self::OnnxRuntime => "onnx-runtime",
            Self::PaddleInference => "paddle-inference",
            Self::ExecuTorch => "executorch",
            Self::TensorRt => "tensorrt",
            Self::TensorRtRtx => "tensorrt-rtx",
            Self::CoreMl => "coreml",
            Self::Custom(id) => id,
        }
    }

    /// Parse a canonical identifier. Unknown identifiers are treated as custom
    /// runtimes so a model manifest can refer to an installed third-party
    /// runtime without changing this enum.
    pub fn from_id(id: impl AsRef<str>) -> Self {
        match id.as_ref() {
            "onnx-runtime" | "onnxruntime" | "onnx" => Self::OnnxRuntime,
            "paddle-inference" | "paddle" => Self::PaddleInference,
            "executorch" => Self::ExecuTorch,
            "tensorrt" => Self::TensorRt,
            "tensorrt-rtx" => Self::TensorRtRtx,
            "coreml" | "core-ml" => Self::CoreMl,
            custom => Self::Custom(custom.strip_prefix("custom:").unwrap_or(custom).to_owned()),
        }
    }

    /// Compatibility alias retained for callers of the P0 scaffold.
    pub fn name(&self) -> &str {
        self.as_str()
    }

    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
    }
}

impl fmt::Display for RuntimeKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::Custom(id) = self {
            write!(formatter, "custom:{id}")
        } else {
            formatter.write_str(self.as_str())
        }
    }
}

impl FromStr for RuntimeKind {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_id(value))
    }
}

impl From<&str> for RuntimeKind {
    fn from(value: &str) -> Self {
        Self::from_id(value)
    }
}

impl From<String> for RuntimeKind {
    fn from(value: String) -> Self {
        Self::from_id(value)
    }
}

impl Serialize for RuntimeKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RuntimeKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::from_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_kind_has_stable_string_serialization() {
        let cases = [
            (RuntimeKind::OnnxRuntime, "\"onnx-runtime\""),
            (RuntimeKind::PaddleInference, "\"paddle-inference\""),
            (
                RuntimeKind::Custom("vendor-npu".to_owned()),
                "\"custom:vendor-npu\"",
            ),
        ];

        for (kind, encoded) in cases {
            assert_eq!(serde_json::to_string(&kind).unwrap(), encoded);
            assert_eq!(serde_json::from_str::<RuntimeKind>(encoded).unwrap(), kind);
        }
    }
}
