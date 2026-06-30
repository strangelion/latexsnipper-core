use async_trait::async_trait;
use latexsnipper_foundation::Result;
use latexsnipper_ast::SourceInfo;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Detects regions (formulas or text) in the image.
/// Stores detection results in context metadata for downstream nodes.
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
        // Detection results are passed via metadata by the Engine
        // This node validates that detection results exist
        let key = match &self.detector_type {
            DetectorType::Formula => "formula_detections",
            DetectorType::Text => "text_detections",
        };

        if let Some(detections) = ctx.get(key) {
            let count = detections.as_array().map_or(0, |a| a.len());
            log::info!("Pipeline: {} found {} regions", self.name, count);
        } else {
            log::info!("Pipeline: {} — no detections in context", self.name);
        }

        Ok(())
    }
}
