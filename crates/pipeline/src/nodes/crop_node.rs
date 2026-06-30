use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Crops detected regions from the image.
/// Stores cropped regions in context metadata for downstream recognition.
pub struct CropNode {
    name: String,
    min_size: u32,
}

impl CropNode {
    pub fn new(min_size: u32) -> Self {
        Self { name: "crop".into(), min_size }
    }

    pub fn default() -> Self {
        Self::new(4)
    }
}

#[async_trait]
impl PipelineNode for CropNode {
    fn name(&self) -> &str { &self.name }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        // Cropping is performed by the Engine using detection results
        // This node acts as a marker in the pipeline graph
        log::info!("Pipeline: {} node executed (min_size={})", self.name, self.min_size);
        Ok(())
    }
}
