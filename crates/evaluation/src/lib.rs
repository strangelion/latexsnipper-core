//! Reproducible OCR evaluation contracts and metrics.

pub mod metrics;
pub mod schema;
pub mod validation;

pub use metrics::{evaluate_corpus, MetricError};
pub use schema::*;
pub use validation::{
    load_and_validate_index, validate_evidence, LoadedCorpus, LoadedCorpusIndex, ValidationError,
};
