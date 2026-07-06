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

        // Apply language-specific postprocessing recursively to all text blocks.
        // Recursively handles Paragraph, Heading, Table cells, List, Quote, etc.
        // Skips Formula and Code blocks.
        for block in &mut blocks {
            normalize_block_inlines(block);
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

/// Recursively apply CJK/Latin spacing normalization to all text inlines
/// within a block. Skips Formula and Code blocks.
fn normalize_block_inlines(block: &mut latexsnipper_ast::Block) {
    use latexsnipper_ast::Block;
    match block {
        Block::Paragraph(ref mut p) => normalize_inlines(&mut p.inlines),
        Block::Heading(ref mut h) => normalize_inlines(&mut h.inlines),
        Block::Table(ref mut t) => {
            for row in &mut t.rows {
                for cell in row.iter_mut() {
                    normalize_inlines(&mut cell.inlines);
                }
            }
        }
        Block::List(ref mut list) => {
            for item in &mut list.items {
                normalize_inlines(&mut item.inlines);
            }
        }
        Block::Quote(ref mut q) => {
            for content in &mut q.blocks {
                normalize_block_inlines(content);
            }
        }
        Block::DescriptionList(ref mut dl) => {
            for item in &mut dl.items {
                if let Some(ref mut label) = item.label {
                    normalize_inlines(label);
                }
                for content in &mut item.content {
                    normalize_block_inlines(content);
                }
            }
        }
        Block::Handwriting(ref mut hw) => normalize_inlines(&mut hw.inlines),
        // Formula, CodeBlock, HorizontalRule, etc. — skip
        _ => {}
    }
}

/// Apply LanguageDetector::postprocess to all Text inlines.
fn normalize_inlines(inlines: &mut [Inline]) {
    for inline in inlines.iter_mut() {
        if let Inline::Text(ref mut text_run) = inline {
            text_run.text = LanguageDetector::postprocess(&text_run.text);
        }
    }
}
