use async_trait::async_trait;
use latexsnipper_foundation::Result;
use latexsnipper_ast::SourceInfo;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Detects regions (formulas or text) in the image.
/// Stores detection results in context metadata.
pub struct DetectorNode {
    name: String,
    detector_type: DetectorType,
}

pub enum DetectorType {
    Formula,
    Text,
}

impl DetectorNode {
    pub fn formula() -> Self {
        Self { name: "detect_formula".into(), detector_type: DetectorType::Formula }
    }

    pub fn text() -> Self {
        Self { name: "detect_text".into(), detector_type: DetectorType::Text }
    }
}

#[async_trait]
impl PipelineNode for DetectorNode {
    fn name(&self) -> &str { &self.name }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        // Detection is performed by the Engine which passes results via metadata
        // This node acts as a marker in the pipeline graph
        log::info!("Pipeline: {} node executed", self.name);
        Ok(())
    }
}
