//! Runtime plugin host errors.

use latexsnipper_foundation::SnipperError;

pub fn plugin_runtime_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Runtime(format!("[runtime-plugin] {}", message.into()))
}

pub type PluginRuntimeResult<T> = Result<T, SnipperError>;
