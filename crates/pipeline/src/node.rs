use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::context::PipelineContext;

/// A node in the pipeline graph.
/// Each node processes context and may produce side effects.
#[async_trait]
pub trait PipelineNode: Send + Sync {
    /// Unique name for this node.
    fn name(&self) -> &str;

    /// Process the pipeline context.
    async fn process(&self, ctx: &mut PipelineContext) -> Result<()>;
}

/// A node that transforms the document in the context.
pub struct TransformNode {
    name: String,
    transform: Box<dyn Fn(&mut PipelineContext) -> Result<()> + Send + Sync>,
}

impl TransformNode {
    pub fn new(
        name: impl Into<String>,
        transform: impl Fn(&mut PipelineContext) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            transform: Box::new(transform),
        }
    }
}

#[async_trait]
impl PipelineNode for TransformNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        (self.transform)(ctx)
    }
}
