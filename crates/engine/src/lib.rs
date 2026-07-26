pub mod api;
#[cfg(feature = "native")]
pub mod application;
pub mod config;
pub mod engine;
pub mod job;
pub mod metrics;
pub mod runtime_registry;
pub mod service;

#[cfg(feature = "native")]
pub mod sdk;
#[cfg(feature = "native")]
pub mod stage_runners;

#[cfg(feature = "remote-api")]
pub mod chart_understanding;
#[cfg(feature = "remote-api")]
pub mod diagram_understanding;

pub use config::EngineConfig;
pub use engine::{
    EngineWarmupEntry, RecognizeMode, RecognizeRequest, RecognizeResponse, SnipperEngine,
    StreamItem,
};
pub use job::{Job, JobQueue, JobStatus};
pub use latexsnipper_pipeline::DocumentParseMode;
pub use metrics::{MetricsBuilder, RecognitionMetrics, SerializableMetrics};
pub use runtime_registry::default_runtime_registry;
#[cfg(feature = "runtime-plugins")]
pub use runtime_registry::runtime_registry_with_plugins;
pub use service::{Service, ServiceStatus};

/// Experimental source-aware incremental document sessions.
///
/// This remains behind an opt-in feature while the session contract evolves.
#[cfg(feature = "experimental-incremental")]
pub use latexsnipper_incremental as incremental;

#[cfg(feature = "remote-api")]
pub use chart_understanding::{ChartUnderstandingResult, ChartUnderstandingService};
#[cfg(feature = "remote-api")]
pub use diagram_understanding::{
    DiagramConnection, DiagramShape, DiagramUnderstandingResult, DiagramUnderstandingService,
};

// Re-export api-types for backward compatibility (wasm/ffi use these via engine)
pub use latexsnipper_api_types::{
    CoreErrorCode, EngineReadiness, ModeReadiness, ModelReadiness, ProviderValidationLevel,
    ProviderValidationReport, RecognitionProfile, RecognizeMode as ApiRecognizeMode,
    RuntimeReadiness, StreamItem as ApiStreamItem, TaskReadiness, READINESS_SCHEMA_VERSION,
};
