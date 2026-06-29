use thiserror::Error;

/// Unified error type for all LaTeXSnipper Core operations.
#[derive(Error, Debug, Clone)]
pub enum SnipperError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("Model error: {0}")]
    Model(String),

    #[error("Runtime error: {0}")]
    Runtime(String),

    #[error("Inference error: {0}")]
    Inference(String),

    #[error("Pipeline error: {0}")]
    Pipeline(String),

    #[error("Image error: {0}")]
    Image(String),

    #[error("Conversion error: {0}")]
    Conversion(String),

    #[error("Export error: {0}")]
    Export(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Timeout after {0}ms")]
    Timeout(u64),

    #[error("Cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, SnipperError>;

/// Convenience trait for converting external errors into SnipperError.
pub trait IntoSnipper<T> {
    fn into_snipper(self) -> Result<T>;
}

impl<T, E: std::fmt::Display> IntoSnipper<T> for std::result::Result<T, E> {
    fn into_snipper(self) -> Result<T> {
        self.map_err(|e| SnipperError::Other(e.to_string()))
    }
}
