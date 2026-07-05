use crate::acceleration::AccelerationMode;
use crate::model_handle::ModelHandle;
use crate::session::InferenceSession;
use latexsnipper_foundation::Result;

/// Abstraction over inference runtimes (ONNX Runtime, TensorRT, etc.).
/// Core only knows this trait, never OrtSession directly.
pub trait RuntimeBackend: Send + Sync {
    /// Create an inference session from a model handle.
    fn create_session(
        &self,
        handle: &ModelHandle,
        acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>>;

    /// Create an inference session with thread count hint.
    fn create_session_with_threads(
        &self,
        handle: &ModelHandle,
        acceleration: AccelerationMode,
        _max_threads: usize,
    ) -> Result<Box<dyn InferenceSession>> {
        self.create_session(handle, acceleration)
    }

    /// Clear all cached sessions, forcing fresh model loads on next inference.
    /// This is the mechanism for model hot-reload.
    fn clear_sessions(&self) {
        // Default: no-op for backends without session cache
    }

    /// Get the name of this runtime (e.g., "onnxruntime", "tensorrt").
    fn name(&self) -> &str;

    /// Check if this runtime is available on the current platform.
    fn is_available(&self) -> bool;
}
