//! Runtime observations emitted only by actual model execution events.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelRuntimeObservation {
    pub executor_created: bool,
    pub session_created: bool,
    pub inference_started: bool,
    pub inference_completed: bool,
    pub inference_failed: bool,
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
    pub fn record(&mut self, event: ModelRuntimeEvent) {
        match event {
            ModelRuntimeEvent::ExecutorCreated => self.executor_created = true,
            ModelRuntimeEvent::SessionCreated => self.session_created = true,
            ModelRuntimeEvent::InferenceStarted => self.inference_started = true,
            ModelRuntimeEvent::InferenceCompleted => self.inference_completed = true,
            ModelRuntimeEvent::InferenceFailed => self.inference_failed = true,
        }
    }
}
