//! Reproducible OCR evaluation contracts and metrics.

pub mod failure_corpus;
pub mod int8;
pub mod metrics;
pub mod schema;
pub mod trace;
pub mod validation;

pub use failure_corpus::*;
pub use int8::*;
pub use metrics::{evaluate_corpus, MetricError};
pub use schema::*;
pub use trace::{EvaluationTrace, EVALUATION_TRACE_SCHEMA_VERSION};
pub use validation::{
    load_and_validate_index, validate_evidence, LoadedCorpus, LoadedCorpusIndex, ValidationError,
};
