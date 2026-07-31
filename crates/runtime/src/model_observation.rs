//! Runtime observations emitted only by actual model execution events.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LastInferenceOutcome {
    #[default]
    NeverRun,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelRuntimeObservation {
    pub executor_created: bool,
    pub session_created: bool,
    pub session_generation: u64,
    pub inference_started: bool,
    pub inference_completed: bool,
    pub inference_failed: bool,
    pub last_inference_outcome: LastInferenceOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRuntimeEvent {
    ExecutorCreated,
    SessionCreated,
    InferenceStarted,
    InferenceCompleted,
    InferenceFailed,
}

pub trait ModelRuntimeObserver: Send + Sync {
    fn observe(&self, model_id: &str, event: ModelRuntimeEvent);
}

impl ModelRuntimeObservation {
    pub const fn latest_inference_succeeded(self) -> bool {
        matches!(self.last_inference_outcome, LastInferenceOutcome::Succeeded)
    }

    pub fn record(&mut self, event: ModelRuntimeEvent) {
        match event {
            ModelRuntimeEvent::ExecutorCreated => self.executor_created = true,
            ModelRuntimeEvent::SessionCreated => {
                self.session_created = true;
                self.session_generation = self.session_generation.saturating_add(1);
                self.inference_started = false;
                self.inference_completed = false;
                self.inference_failed = false;
                self.last_inference_outcome = LastInferenceOutcome::NeverRun;
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
            }
            ModelRuntimeEvent::InferenceFailed => {
                self.inference_completed = false;
                self.inference_failed = true;
                self.last_inference_outcome = LastInferenceOutcome::Failed;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_inference_result_replaces_older_success() {
        let mut observation = ModelRuntimeObservation::default();
        observation.record(ModelRuntimeEvent::SessionCreated);
        observation.record(ModelRuntimeEvent::InferenceStarted);
        observation.record(ModelRuntimeEvent::InferenceCompleted);
        assert!(observation.latest_inference_succeeded());

        observation.record(ModelRuntimeEvent::InferenceStarted);
        observation.record(ModelRuntimeEvent::InferenceFailed);
        assert!(!observation.latest_inference_succeeded());
        assert!(!observation.inference_completed);
        assert!(observation.inference_failed);
    }

    #[test]
    fn new_session_resets_prior_inference_evidence() {
        let mut observation = ModelRuntimeObservation::default();
        observation.record(ModelRuntimeEvent::SessionCreated);
        observation.record(ModelRuntimeEvent::InferenceStarted);
        observation.record(ModelRuntimeEvent::InferenceCompleted);
        observation.record(ModelRuntimeEvent::SessionCreated);

        assert_eq!(observation.session_generation, 2);
        assert_eq!(
            observation.last_inference_outcome,
            LastInferenceOutcome::NeverRun
        );
        assert!(!observation.latest_inference_succeeded());
    }
}
