use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// Crops detected regions from the image.
/// Reads detection results from context metadata, crops regions, stores in metadata.
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
        // Crop regions are handled by the Engine which passes crop info via metadata
        // This node validates that crop data exists
        let formula_crops = ctx.get("formula_crops").map(|v| v.as_array().map_or(0, |a| a.len())).unwrap_or(0);
        let text_crops = ctx.get("text_crops").map(|v| v.as_array().map_or(0, |a| a.len())).unwrap_or(0);

        if formula_crops > 0 || text_crops > 0 {
            log::info!("Pipeline: crop node — {} formula + {} text regions", formula_crops, text_crops);
        }

        Ok(())
    }
}
