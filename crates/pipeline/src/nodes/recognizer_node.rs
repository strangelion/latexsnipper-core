use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Recognizes content in cropped regions.
/// Stores recognition results in context document.
pub struct RecognizerNode {
    name: String,
    recognizer_type: RecognizerType,
}

pub enum RecognizerType {
    Formula,
    Text,
}

impl RecognizerNode {
    pub fn formula() -> Self {
        Self { name: "recognize_formula".into(), recognizer_type: RecognizerType::Formula }
    }

    pub fn text() -> Self {
        Self { name: "recognize_text".into(), recognizer_type: RecognizerType::Text }
    }
}

#[async_trait]
impl PipelineNode for RecognizerNode {
    fn name(&self) -> &str { &self.name }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        // Recognition is performed by the Engine which passes results via metadata
        // This node acts as a marker in the pipeline graph
        log::info!("Pipeline: {} node executed", self.name);
        Ok(())
    }
}
