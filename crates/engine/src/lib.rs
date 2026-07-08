pub mod api;
pub mod config;
pub mod engine;
pub mod job;
pub mod metrics;
pub mod sdk;
pub mod service;

#[cfg(feature = "remote-api")]
pub mod chart_understanding;
#[cfg(feature = "remote-api")]
pub mod diagram_understanding;

pub use config::EngineConfig;
pub use engine::{RecognizeMode, RecognizeRequest, RecognizeResponse, SnipperEngine, StreamItem};
pub use job::{Job, JobQueue, JobStatus};
pub use latexsnipper_pipeline::DocumentParseMode;
pub use metrics::{MetricsBuilder, RecognitionMetrics, SerializableMetrics};
pub use service::{Service, ServiceStatus};

#[cfg(feature = "remote-api")]
pub use chart_understanding::{ChartUnderstandingResult, ChartUnderstandingService};
#[cfg(feature = "remote-api")]
pub use diagram_understanding::{
    DiagramConnection, DiagramShape, DiagramUnderstandingResult, DiagramUnderstandingService,
};

// Re-export api-types for backward compatibility (wasm/ffi use these via engine)
pub use latexsnipper_api_types::{RecognizeMode as ApiRecognizeMode, StreamItem as ApiStreamItem};
