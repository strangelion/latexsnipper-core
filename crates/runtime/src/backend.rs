use crate::acceleration::AccelerationMode;
use crate::model_handle::ModelHandle;
use crate::session::InferenceSession;
use latexsnipper_ast::Diagnostic;
use latexsnipper_foundation::Result;
use serde::{Deserialize, Serialize};

/// Machine-readable runtime and execution-provider status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDiagnostics {
    pub runtime: String,
    pub available: bool,
    pub selected_provider: String,
    pub available_providers: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

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

    /// Provider selected for the most recently created session.
    fn selected_provider(&self) -> String {
        self.name().to_string()
    }

    /// Providers currently available to this backend.
    fn available_providers(&self) -> Vec<String> {
        if self.is_available() {
            vec![self.name().to_string()]
        } else {
            Vec::new()
        }
    }

    /// Stable diagnostics emitted while selecting execution providers.
    fn provider_diagnostics(&self) -> Vec<Diagnostic> {
        Vec::new()
    }

    /// Return a complete snapshot suitable for SDK and CLI JSON output.
    fn runtime_diagnostics(&self) -> RuntimeDiagnostics {
        RuntimeDiagnostics {
            runtime: self.name().to_string(),
            available: self.is_available(),
            selected_provider: self.selected_provider(),
            available_providers: self.available_providers(),
            diagnostics: self.provider_diagnostics(),
        }
    }
}
