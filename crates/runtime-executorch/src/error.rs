//! ExecuTorch runtime errors.

use latexsnipper_foundation::SnipperError;

pub fn executorch_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Runtime(format!("[executorch] {}", message.into()))
}

pub type ExecuTorchResult<T> = Result<T, SnipperError>;
