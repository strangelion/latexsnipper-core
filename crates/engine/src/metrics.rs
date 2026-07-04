use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Performance metrics for a recognition pipeline run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecognitionMetrics {
    /// Total pipeline execution time.
    pub total_time: Duration,
    /// Time spent in each pipeline node.
    pub node_times: HashMap<String, Duration>,
    /// Number of detected regions by type.
    pub detected_regions: HashMap<String, usize>,
    /// Number of recognized blocks by type.
    pub recognized_blocks: HashMap<String, usize>,
    /// Number of failed regions.
    pub failed_regions: usize,
    /// Runtime backend name.
    pub runtime: String,
    /// Loaded model versions.
    pub model_versions: HashMap<String, String>,
    /// Memory usage estimate (bytes).
    pub memory_usage: Option<usize>,
    /// Whether the pipeline completed successfully.
    pub success: bool,
    /// Any error message.
    pub error: Option<String>,
}

impl RecognitionMetrics {
    /// Create a new empty metrics instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Start timing a pipeline node.
    pub fn start_node(&mut self, name: &str) -> NodeTimer {
        NodeTimer {
            name: name.to_string(),
            start: Instant::now(),
        }
    }

    /// Record a node's execution time.
    pub fn record_node_time(&mut self, name: &str, duration: Duration) {
        self.node_times.insert(name.to_string(), duration);
    }

    /// Record detected regions.
    pub fn record_detections(&mut self, category: &str, count: usize) {
        *self.detected_regions
            .entry(category.to_string())
            .or_insert(0) += count;
    }

    /// Record recognized blocks.
    pub fn record_blocks(&mut self, category: &str, count: usize) {
        *self.recognized_blocks
            .entry(category.to_string())
            .or_insert(0) += count;
    }

    /// Record a failed region.
    pub fn record_failure(&mut self) {
        self.failed_regions += 1;
    }

    /// Set the runtime backend name.
    pub fn set_runtime(&mut self, name: impl Into<String>) {
        self.runtime = name.into();
    }

    /// Record a model version.
    pub fn record_model_version(&mut self, model_id: &str, version: &str) {
        self.model_versions
            .insert(model_id.to_string(), version.to_string());
    }

    /// Mark the pipeline as successful.
    pub fn mark_success(&mut self) {
        self.success = true;
    }

    /// Record an error.
    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.success = false;
    }

    /// Get total detection count.
    pub fn total_detections(&self) -> usize {
        self.detected_regions.values().sum()
    }

    /// Get total block count.
    pub fn total_blocks(&self) -> usize {
        self.recognized_blocks.values().sum()
    }

    /// Get a summary string.
    pub fn summary(&self) -> String {
        format!(
            "Runtime: {} | Detections: {} | Blocks: {} | Failed: {} | Time: {:?}",
            self.runtime,
            self.total_detections(),
            self.total_blocks(),
            self.failed_regions,
            self.total_time
        )
    }

    /// Convert to JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Timer for measuring node execution time.
pub struct NodeTimer {
    name: String,
    start: Instant,
}

impl NodeTimer {
    /// Stop the timer and return the elapsed duration.
    pub fn stop(self) -> (String, Duration) {
        (self.name, self.start.elapsed())
    }
}

/// Builder for RecognitionMetrics.
pub struct MetricsBuilder {
    metrics: RecognitionMetrics,
}

impl MetricsBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            metrics: RecognitionMetrics::new(),
        }
    }

    /// Set the runtime backend.
    pub fn runtime(mut self, name: impl Into<String>) -> Self {
        self.metrics.set_runtime(name);
        self
    }

    /// Record a model version.
    pub fn model_version(mut self, id: &str, version: &str) -> Self {
        self.metrics.record_model_version(id, version);
        self
    }

    /// Build the metrics.
    pub fn build(self) -> RecognitionMetrics {
        self.metrics
    }
}

impl Default for MetricsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable version of metrics for JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableMetrics {
    pub total_time_ms: u64,
    pub node_times: HashMap<String, u64>,
    pub detected_regions: HashMap<String, usize>,
    pub recognized_blocks: HashMap<String, usize>,
    pub failed_regions: usize,
    pub runtime: String,
    pub model_versions: HashMap<String, String>,
    pub memory_usage: Option<usize>,
    pub success: bool,
    pub error: Option<String>,
}

impl From<&RecognitionMetrics> for SerializableMetrics {
    fn from(m: &RecognitionMetrics) -> Self {
        Self {
            total_time_ms: m.total_time.as_millis() as u64,
            node_times: m
                .node_times
                .iter()
                .map(|(k, v)| (k.clone(), v.as_millis() as u64))
                .collect(),
            detected_regions: m.detected_regions.clone(),
            recognized_blocks: m.recognized_blocks.clone(),
            failed_regions: m.failed_regions,
            runtime: m.runtime.clone(),
            model_versions: m.model_versions.clone(),
            memory_usage: m.memory_usage,
            success: m.success,
            error: m.error.clone(),
        }
    }
}
