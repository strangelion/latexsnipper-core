use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::context::PipelineContext;
use crate::node::PipelineNode;

/// Normalizes image pixels using mean/std normalization.
pub struct NormalizeNode {
    name: String,
    mean: [f32; 3],
    std: [f32; 3],
}

impl NormalizeNode {
    pub fn new(mean: [f32; 3], std: [f32; 3]) -> Self {
        Self {
            name: "normalize".into(),
            mean,
            std,
        }
    }

    pub fn default_yolo() -> Self {
        Self::new([0.0, 0.0, 0.0], [1.0, 1.0, 1.0])
    }

    pub fn default_imagenet() -> Self {
        Self::new([0.485, 0.456, 0.406], [0.229, 0.224, 0.225])
    }
}

#[async_trait]
impl PipelineNode for NormalizeNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        // Store normalization params in metadata for downstream nodes
        ctx.set("norm_mean", serde_json::json!(self.mean));
        ctx.set("norm_std", serde_json::json!(self.std));
        Ok(())
    }
}
