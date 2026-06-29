use latexsnipper_foundation::Result;
use log::info;

use crate::node::PipelineNode;
use crate::context::PipelineContext;

/// A pipeline graph that executes nodes in sequence.
pub struct PipelineGraph {
    name: String,
    nodes: Vec<Box<dyn PipelineNode>>,
}

impl PipelineGraph {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Vec::new(),
        }
    }

    /// Add a node to the end of the pipeline.
    pub fn add_node(&mut self, node: Box<dyn PipelineNode>) -> &mut Self {
        self.nodes.push(node);
        self
    }

    /// Execute all nodes in sequence.
    pub async fn run(&self, ctx: &mut PipelineContext) -> Result<()> {
        info!("Pipeline '{}' starting with {} nodes", self.name, self.nodes.len());

        for (i, node) in self.nodes.iter().enumerate() {
            if ctx.cancelled {
                info!("Pipeline '{}' cancelled at node {}", self.name, i);
                break;
            }

            info!("Pipeline '{}' executing node {}: {}", self.name, i, node.name());
            node.process(ctx).await?;
        }

        info!("Pipeline '{}' completed", self.name);
        Ok(())
    }

    /// Get the number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the pipeline is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get the pipeline name.
    pub fn name(&self) -> &str {
        &self.name
    }
}
