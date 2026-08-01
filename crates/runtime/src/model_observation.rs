//! Runtime observations emitted only by actual model execution events.

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LastInferenceOutcome {
    #[default]
    NeverRun,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelRuntimeObservation {
    pub executor_created: bool,
    pub session_created: bool,
    pub session_generation: u64,
    pub runtime: Option<String>,
    pub effective_provider: Option<String>,
    pub inference_started: bool,
    pub inference_completed: bool,
    pub inference_failed: bool,
    pub last_inference_outcome: LastInferenceOutcome,
    pub last_failure_code: Option<String>,
    pub last_failure_at: Option<u64>,
    pub last_success_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelRuntimeEvent {
    ExecutorCreated,
    SessionCreated {
        runtime: String,
        effective_provider: String,
    },
    InferenceStarted,
    InferenceCompleted,
    InferenceFailed {
        code: Option<String>,
    },
}

pub trait ModelRuntimeObserver: Send + Sync {
    fn observe(&self, model_id: &str, event: ModelRuntimeEvent);
}

impl ModelRuntimeObservation {
    pub const fn latest_inference_succeeded(&self) -> bool {
        matches!(self.last_inference_outcome, LastInferenceOutcome::Succeeded)
    }

    pub fn record(&mut self, event: ModelRuntimeEvent) {
        match event {
            ModelRuntimeEvent::ExecutorCreated => self.executor_created = true,
            ModelRuntimeEvent::SessionCreated {
                runtime,
                effective_provider,
            } => {
                self.session_created = true;
                self.session_generation = self.session_generation.saturating_add(1);
                self.runtime = Some(runtime);
                self.effective_provider = Some(effective_provider);
                self.inference_started = false;
                self.inference_completed = false;
                self.inference_failed = false;
                self.last_inference_outcome = LastInferenceOutcome::NeverRun;
                self.last_failure_code = None;
                self.last_failure_at = None;
                self.last_success_at = None;
            }
            ModelRuntimeEvent::InferenceStarted => {
                self.inference_started = true;
                self.inference_completed = false;
                self.inference_failed = false;
                self.last_inference_outcome = LastInferenceOutcome::Running;
            }
            ModelRuntimeEvent::InferenceCompleted => {
                self.inference_completed = true;
                self.inference_failed = false;
                self.last_inference_outcome = LastInferenceOutcome::Succeeded;
                self.last_failure_code = None;
                self.last_success_at = Some(now_unix_millis());
            }
            ModelRuntimeEvent::InferenceFailed { code } => {
                self.inference_completed = false;
                self.inference_failed = true;
                self.last_inference_outcome = LastInferenceOutcome::Failed;
                self.last_failure_code = code;
                self.last_failure_at = Some(now_unix_millis());
            }
        }
    }
}

fn now_unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_inference_result_replaces_older_success() {
        let mut observation = ModelRuntimeObservation::default();
        observation.record(ModelRuntimeEvent::SessionCreated {
            runtime: "onnxruntime".to_owned(),
            effective_provider: "cpu".to_owned(),
        });
        observation.record(ModelRuntimeEvent::InferenceStarted);
        observation.record(ModelRuntimeEvent::InferenceCompleted);
        assert!(observation.latest_inference_succeeded());

        observation.record(ModelRuntimeEvent::InferenceStarted);
        observation.record(ModelRuntimeEvent::InferenceFailed {
            code: Some("INFERENCE_FAILED".to_owned()),
        });
        assert!(!observation.latest_inference_succeeded());
        assert!(!observation.inference_completed);
        assert!(observation.inference_failed);
    }

    #[test]
    fn new_session_resets_prior_inference_evidence() {
        let mut observation = ModelRuntimeObservation::default();
        observation.record(ModelRuntimeEvent::SessionCreated {
            runtime: "onnxruntime".to_owned(),
            effective_provider: "cpu".to_owned(),
        });
        observation.record(ModelRuntimeEvent::InferenceStarted);
        observation.record(ModelRuntimeEvent::InferenceCompleted);
        observation.record(ModelRuntimeEvent::SessionCreated {
            runtime: "onnxruntime".to_owned(),
            effective_provider: "directml".to_owned(),
        });

        assert_eq!(observation.session_generation, 2);
        assert_eq!(
            observation.last_inference_outcome,
            LastInferenceOutcome::NeverRun
        );
        assert!(!observation.latest_inference_succeeded());
        assert_eq!(observation.effective_provider.as_deref(), Some("directml"));
        assert!(observation.last_success_at.is_none());
    }
}
