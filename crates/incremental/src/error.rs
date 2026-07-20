use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("revision conflict: expected {expected}, current {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("unknown stable id: {0}")]
    UnknownStableId(String),
    #[error("stable id is empty or already bound: {0}")]
    DuplicateStableId(String),
    #[error("invalid source range")]
    InvalidRange,
    #[error("edit is unsupported for this node kind")]
    UnsupportedEdit,
    #[error("source-backed operation requires a source snapshot")]
    MissingSource,
    #[error("conversion failed: {0}")]
    Conversion(String),
    #[error("render failed: {0}")]
    Render(String),
    #[error("parse failed: {0}")]
    Parse(String),
}
