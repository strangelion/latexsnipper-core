use async_trait::async_trait;
use latexsnipper_ast::Inline;
use latexsnipper_foundation::Result;
use latexsnipper_inference::LanguageDetector;

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::reading_order::ReadingOrder;

/// Post-processes recognition results (sort by reading order, merge, and
/// apply language-specific text cleanup for multilingual output).
pub struct PostprocessNode {
    name: String,
    y_threshold: f32,
}

impl PostprocessNode {
    pub fn new() -> Self {
        Self {
            name: "postprocess".into(),
            y_threshold: 5.0,
        }
    }

    /// Create with custom y-bucket threshold.
    pub fn with_threshold(y_threshold: f32) -> Self {
        Self {
            name: "postprocess".into(),
            y_threshold,
        }
    }
}

impl Default for PostprocessNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for PostprocessNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        // Collect all blocks from artifacts
        let mut blocks = ctx.artifacts.all_blocks();

        if blocks.is_empty() {
            log::info!("Pipeline: postprocess — no blocks to sort");
            return Ok(());
        }

        let count = blocks.len();

        // Sort by reading order (y-bucket + x tie-breaker)
        ReadingOrder::sort(&mut blocks, self.y_threshold);

        // Apply language-specific postprocessing to text blocks
        for block in &mut blocks {
            if let latexsnipper_ast::Block::Paragraph(ref mut p) = block {
                for inline in &mut p.inlines {
                    if let Inline::Text(ref mut text_run) = inline {
                        text_run.text = LanguageDetector::postprocess(&text_run.text);
                    }
                }
            }
        }

        // Replace artifacts with sorted blocks
        ctx.artifacts.formula_blocks.clear();
        ctx.artifacts.text_blocks.clear();
        ctx.artifacts.handwriting_blocks.clear();
        ctx.artifacts.table_blocks.clear();

        for block in blocks {
            match &block {
                latexsnipper_ast::Block::Formula(_) => ctx.artifacts.formula_blocks.push(block),
                latexsnipper_ast::Block::Paragraph(_) | latexsnipper_ast::Block::Heading(_) => {
                    ctx.artifacts.text_blocks.push(block)
                }
                latexsnipper_ast::Block::Table(_) => ctx.artifacts.table_blocks.push(block),
                _ => ctx.artifacts.text_blocks.push(block),
            }
        }

        log::info!(
            "Pipeline: postprocess sorted {} blocks by reading order",
            count
        );
        Ok(())
    }
}
