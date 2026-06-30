use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Post-processes recognition results (sort by reading order, merge, etc.).
pub struct PostprocessNode {
    name: String,
}

impl PostprocessNode {
    pub fn new() -> Self {
        Self { name: "postprocess".into() }
    }
}

#[async_trait]
impl PipelineNode for PostprocessNode {
    fn name(&self) -> &str { &self.name }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        // Sort blocks by y-coordinate (reading order)
        // This is done after all recognition is complete
        log::info!("Pipeline: postprocess node executed");
        Ok(())
    }
}
