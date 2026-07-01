use crate::acceleration::AccelerationMode;
use crate::backend::RuntimeBackend;
use crate::model_handle::ModelHandle;
use crate::session::InferenceSession;
use latexsnipper_foundation::Result;
use latexsnipper_tensor::Tensor;

/// Stub runtime for testing and development.
/// Returns placeholder results without actual inference.
pub struct StubRuntime;

impl StubRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StubRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeBackend for StubRuntime {
    fn create_session(
        &self,
        _handle: &ModelHandle,
        _acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>> {
        Ok(Box::new(StubSession))
    }

    fn name(&self) -> &str {
        "stub"
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// Stub session that returns empty results.
struct StubSession;

impl InferenceSession for StubSession {
    fn run(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>> {
        // Return empty output tensors matching input names
        let outputs = inputs
            .iter()
            .map(|input| {
                Tensor::float32(
                    format!("{}_output", input.name()),
                    input.shape().to_vec(),
                    vec![0.0; input.len()],
                )
            })
            .collect();
        Ok(outputs)
    }

    fn input_names(&self) -> Vec<String> {
        vec![]
    }

    fn output_names(&self) -> Vec<String> {
        vec![]
    }

    fn release(&mut self) {}
}
