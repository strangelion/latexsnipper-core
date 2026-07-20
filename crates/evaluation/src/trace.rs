use latexsnipper_artifact::ArtifactTrace;
use serde::{Deserialize, Serialize};

pub const EVALUATION_TRACE_SCHEMA_VERSION: u32 = 1;

/// Associates runtime lineage with a corpus sample without altering evidence metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationTrace {
    pub schema_version: u32,
    pub corpus_id: String,
    pub sample_id: String,
    pub artifact_trace: ArtifactTrace,
}

impl EvaluationTrace {
    pub fn new(
        corpus_id: impl Into<String>,
        sample_id: impl Into<String>,
        artifact_trace: ArtifactTrace,
    ) -> Self {
        Self {
            schema_version: EVALUATION_TRACE_SCHEMA_VERSION,
            corpus_id: corpus_id.into(),
            sample_id: sample_id.into(),
            artifact_trace,
        }
    }
}

#[cfg(test)]
mod tests {
    use latexsnipper_artifact::ArtifactGraph;

    use super::*;

    #[test]
    fn evaluation_trace_is_versioned() {
        let trace = EvaluationTrace::new("formula", "sample-1", ArtifactGraph::default().trace());
        assert_eq!(
            serde_json::to_value(trace).unwrap()["schemaVersion"],
            EVALUATION_TRACE_SCHEMA_VERSION
        );
    }
}
