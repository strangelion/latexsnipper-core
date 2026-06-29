use crate::session::InferenceSession;
use crate::model_handle::ModelHandle;
use crate::acceleration::AccelerationMode;
use latexsnipper_foundation::Result;

/// Abstraction over inference runtimes (ONNX Runtime, TensorRT, etc.).
/// Core only knows this trait, never OrtSession directly.
pub trait RuntimeBackend: Send + Sync {
    /// Create an inference session from a model handle.
    /// Runtime doesn't know about file paths — ModelManager handles that.
    fn create_session(
        &self,
        handle: &ModelHandle,
        acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>>;

    /// Get the name of this runtime (e.g., "onnxruntime", "tensorrt").
    fn name(&self) -> &str;

    /// Check if this runtime is available on the current platform.
    fn is_available(&self) -> bool;
}
