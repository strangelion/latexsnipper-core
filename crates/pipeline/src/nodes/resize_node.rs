use async_trait::async_trait;
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Resizes the image in the pipeline context to a target size.
pub struct ResizeNode {
    name: String,
    target_size: u32,
}

impl ResizeNode {
    pub fn new(target_size: u32) -> Self {
        Self {
            name: format!("resize_{}", target_size),
            target_size,
        }
    }
}

#[async_trait]
impl PipelineNode for ResizeNode {
    fn name(&self) -> &str { &self.name }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        if let Some(ref image) = ctx.image {
            let resized = operations::resize_to_fit(image, self.target_size);
            ctx.image = Some(resized);
        }
        Ok(())
    }
}
