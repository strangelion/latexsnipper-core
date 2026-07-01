use serde::{Deserialize, Serialize};

/// Hardware acceleration mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccelerationMode {
    /// CPU-only inference.
    Cpu,
    /// GPU acceleration (NNAPI/CUDA/DML).
    Gpu,
    /// Auto-detect best available.
    #[default]
    Auto,
}
