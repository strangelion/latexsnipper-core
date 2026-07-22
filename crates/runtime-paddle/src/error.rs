//! Error types for Paddle Inference runtime.

use latexsnipper_foundation::SnipperError;

/// Convert a Paddle error message into a SnipperError.
pub fn paddle_error(msg: impl Into<String>) -> SnipperError {
    SnipperError::Runtime(format!("[paddle] {}", msg.into()))
}

/// Result type alias for Paddle operations.
pub type PaddleResult<T> = Result<T, SnipperError>;
