use latexsnipper_tensor::Tensor;
use latexsnipper_foundation::Result;

/// An inference session for running models.
pub trait InferenceSession: Send + Sync {
    /// Run inference with input tensors, return output tensors.
    fn run(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>>;

    /// Get input names expected by the model.
    fn input_names(&self) -> Vec<String>;

    /// Get output names produced by the model.
    fn output_names(&self) -> Vec<String>;

    /// Get character list from model metadata (for text recognition models).
    /// Returns None if the model doesn't have embedded characters.
    fn get_character_list(&self) -> Option<Vec<String>> {
        None
    }

    /// Release resources.
    fn release(&mut self);
}

/// Blanket implementation for Box<dyn InferenceSession>.
impl InferenceSession for Box<dyn InferenceSession> {
    fn run(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>> {
        (**self).run(inputs)
    }

    fn input_names(&self) -> Vec<String> {
        (**self).input_names()
    }

    fn output_names(&self) -> Vec<String> {
        (**self).output_names()
    }

    fn get_character_list(&self) -> Option<Vec<String>> {
        (**self).get_character_list()
    }

    fn release(&mut self) {
        (**self).release()
    }
}
