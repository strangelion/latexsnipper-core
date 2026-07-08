use async_trait::async_trait;
use latexsnipper_ast::*;
use latexsnipper_foundation::Result;

use crate::context::PipelineContext;
use crate::node::PipelineNode;

/// Assembles multiple pages into a final Document.
///
/// After all pages have been processed, this node:
/// 1. Collects blocks from artifacts for each page
/// 2. Sorts blocks within each page by reading order
/// 3. Builds the final multi-page Document
pub struct PageAssemblyNode {
    name: String,
}

impl PageAssemblyNode {
    pub fn new() -> Self {
        Self {
            name: "page_assembly".into(),
        }
    }
}

impl Default for PageAssemblyNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for PageAssemblyNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        if !ctx.is_multipage() {
            return Ok(());
        }

        log::info!("PageAssembly: assembling {} pages", ctx.page_images.len());

        let mut pages: Vec<Page> = Vec::new();

        for (page_idx, page_image) in ctx.page_images.iter().enumerate() {
            let page_number = (page_idx + 1) as u32;

            // Collect blocks for this page from artifacts
            let blocks = self.collect_page_blocks(ctx, page_idx);

            // Sort blocks by reading order (y-coordinate, then x-coordinate)
            let mut sorted_blocks = blocks;
            sorted_blocks.sort_by(|a, b| {
                let ay = a.geometry().map_or(0.0, |g| g.y);
                let by = b.geometry().map_or(0.0, |g| g.y);
                ay.partial_cmp(&by)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        let ax = a.geometry().map_or(0.0, |g| g.x);
                        let bx = b.geometry().map_or(0.0, |g| g.x);
                        ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
                    })
            });

            log::info!(
                "PageAssembly: page {} has {} blocks",
                page_number,
                sorted_blocks.len()
            );

            pages.push(Page {
                width: page_image.width() as f32,
                height: page_image.height() as f32,
                blocks: sorted_blocks,
                page_number: Some(page_number),
            });
        }

        // Build the final document
        ctx.document = Document {
            metadata: Metadata::default(),
            pages,
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
        };

        log::info!(
            "PageAssembly: assembled document with {} pages, {} total blocks",
            ctx.document.pages.len(),
            ctx.document.block_count()
        );

        Ok(())
    }
}

impl PageAssemblyNode {
    /// Collect blocks for a specific page from artifacts.
    fn collect_page_blocks(&self, ctx: &PipelineContext, page_idx: usize) -> Vec<Block> {
        let mut blocks = Vec::new();

        // Collect formula blocks
        for block in &ctx.artifacts.formula_blocks {
            if self.belongs_to_page(block, page_idx) {
                blocks.push(block.clone());
            }
        }

        // Collect text blocks
        for block in &ctx.artifacts.text_blocks {
            if self.belongs_to_page(block, page_idx) {
                blocks.push(block.clone());
            }
        }

        // Collect handwriting blocks
        for block in &ctx.artifacts.handwriting_blocks {
            if self.belongs_to_page(block, page_idx) {
                blocks.push(block.clone());
            }
        }

        // Collect table blocks
        for block in &ctx.artifacts.table_blocks {
            if self.belongs_to_page(block, page_idx) {
                blocks.push(block.clone());
            }
        }

        blocks
    }

    /// Check if a block belongs to a specific page.
    ///
    /// Uses `SourceInfo::page` (set by recognizer nodes during pipeline execution).
    /// Blocks without a page tag (`None`) are assigned to page 0 as fallback.
    fn belongs_to_page(&self, block: &Block, page_idx: usize) -> bool {
        match block.source() {
            Some(src) => src.page.map_or(page_idx == 0, |p| p == page_idx),
            None => page_idx == 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_image::color::PixelFormat;
    use latexsnipper_image::SnipperImage;

    fn make_test_image(w: u32, h: u32) -> SnipperImage {
        let pixels = vec![255u8; (w * h * 3) as usize];
        SnipperImage::new(w, h, PixelFormat::Rgb, pixels)
    }

    #[tokio::test]
    async fn test_page_assembly_single_page() {
        let mut ctx = PipelineContext::with_image(make_test_image(100, 100));
        let node = PageAssemblyNode::new();

        node.process(&mut ctx).await.unwrap();

        // Single page should not modify document
        assert_eq!(ctx.document.pages.len(), 0);
    }

    #[tokio::test]
    async fn test_page_assembly_multi_page() {
        let pages = vec![make_test_image(100, 100), make_test_image(200, 200)];
        let mut ctx = PipelineContext::with_pages(pages);

        // Add blocks directly to artifacts
        ctx.artifacts.formula_blocks = vec![
            Block::Formula(FormulaBlock {
                formula: Formula::latex("E=mc^2"),
                geometry: Some(Rect::new(10.0, 20.0, 100.0, 30.0)),
                source: Some(SourceInfo::new().with_page(0)),
            }),
            Block::Formula(FormulaBlock {
                formula: Formula::latex("F=ma"),
                geometry: Some(Rect::new(10.0, 10.0, 80.0, 25.0)),
                source: Some(SourceInfo::new().with_page(1)),
            }),
        ];

        let node = PageAssemblyNode::new();
        node.process(&mut ctx).await.unwrap();

        assert_eq!(ctx.document.pages.len(), 2);
        assert_eq!(ctx.document.pages[0].blocks.len(), 1);
        assert_eq!(ctx.document.pages[1].blocks.len(), 1);
        assert_eq!(ctx.document.block_count(), 2);

        // Verify block-to-page assignment is correct
        if let Block::Formula(f0) = &ctx.document.pages[0].blocks[0] {
            assert_eq!(f0.formula.as_latex(), "E=mc^2");
        } else {
            panic!("Expected Formula block on page 0");
        }
        if let Block::Formula(f1) = &ctx.document.pages[1].blocks[0] {
            assert_eq!(f1.formula.as_latex(), "F=ma");
        } else {
            panic!("Expected Formula block on page 1");
        }
    }
}
