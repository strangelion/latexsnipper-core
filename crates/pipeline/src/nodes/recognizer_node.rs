use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Recognizes content in cropped regions.
/// Recognition results are passed via metadata by the Engine.
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
        // Recognition results are passed via metadata by the Engine
        // This node validates that recognition results exist
        let key = match &self.recognizer_type {
            RecognizerType::Formula => "formula_blocks",
            RecognizerType::Text => "text_blocks",
        };

        if let Some(blocks) = ctx.get(key) {
            let count = blocks.as_array().map_or(0, |a| a.len());
            log::info!("Pipeline: {} recognized {} blocks", self.name, count);
        } else {
            log::info!("Pipeline: {} — no recognition results in context", self.name);
        }

        Ok(())
    }
}
