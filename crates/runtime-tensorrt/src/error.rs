use latexsnipper_foundation::SnipperError;

pub fn tensorrt_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Runtime(format!("[tensorrt] {}", message.into()))
}

pub type TensorRtResult<T> = Result<T, SnipperError>;
