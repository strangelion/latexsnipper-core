//! Core ML runtime errors.

use latexsnipper_foundation::SnipperError;

pub fn coreml_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Runtime(format!("[coreml] {}", message.into()))
}

pub type CoreMlResult<T> = Result<T, SnipperError>;
