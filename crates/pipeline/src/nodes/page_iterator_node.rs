use async_trait::async_trait;
use latexsnipper_foundation::Result;

use crate::context::PipelineContext;
use crate::node::PipelineNode;

/// Iterates over multiple pages and processes each through the pipeline.
///
/// **This node is a structural placeholder** — it sets up page metadata but does
/// NOT actually run sub-pipelines for each page. The real multi-page iteration
/// happens at a higher level (e.g. `SnipperEngine::recognize_pdf`), which loops
/// over pages externally and runs the full pipeline for each page independently.
///
/// What this node currently does:
/// 1. Clears per-page detection/crop/block metadata for each page
/// 2. Initializes `page_results` entries with "pending" status
/// 3. Sets `total_pages` metadata
///
/// The actual detection/recognition is handled by downstream nodes that read
/// `ctx.image` (set per-page by the external loop or `set_current_page`).
pub struct PageIteratorNode {
    name: String,
}

impl PageIteratorNode {
    pub fn new() -> Self {
        Self {
            name: "page_iterator".into(),
        }
    }
}

impl Default for PageIteratorNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for PageIteratorNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        if !ctx.is_multipage() {
            // Single page - nothing to iterate
            return Ok(());
        }

        let page_count = ctx.page_images.len();
        log::info!("PageIterator: processing {} pages", page_count);

        let mut page_results: Vec<serde_json::Value> = Vec::new();

        for page_idx in 0..page_count {
            if ctx.cancelled {
                log::info!("PageIterator: cancelled at page {}", page_idx);
                break;
            }

            log::info!(
                "PageIterator: processing page {}/{}",
                page_idx + 1,
                page_count
            );

            // Set current page
            ctx.set_current_page(page_idx);

            // Clear previous page's detection results
            ctx.set("formula_detections", serde_json::json!([]));
            ctx.set("text_detections", serde_json::json!([]));
            ctx.set("formula_crops", serde_json::json!([]));
            ctx.set("text_crops", serde_json::json!([]));
            ctx.set("formula_blocks", serde_json::json!([]));
            ctx.set("text_blocks", serde_json::json!([]));

            // NOTE: This node does NOT run detection/recognition sub-pipelines.
            // Multi-page iteration happens externally via Engine::recognize_pdf or
            // similar loops that invoke the full pipeline per page. This node only
            // initializes tracking metadata for downstream consumers.

            page_results.push(serde_json::json!({
                "page_number": page_idx + 1,
                "page_index": page_idx,
                "status": "pending"
            }));
        }

        ctx.set("page_results", serde_json::json!(page_results));
        ctx.set("total_pages", serde_json::json!(page_count));

        log::info!("PageIterator: initialized {} pages", page_count);
        Ok(())
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
    async fn test_page_iterator_single_page() {
        let mut ctx = PipelineContext::with_image(make_test_image(100, 100));
        let node = PageIteratorNode::new();

        // Single page should not modify context
        node.process(&mut ctx).await.unwrap();

        assert!(!ctx.is_multipage());
        assert!(ctx.get("page_results").is_none());
    }

    #[tokio::test]
    async fn test_page_iterator_multi_page() {
        let pages = vec![
            make_test_image(100, 100),
            make_test_image(200, 200),
            make_test_image(300, 300),
        ];
        let mut ctx = PipelineContext::with_pages(pages);
        let node = PageIteratorNode::new();

        node.process(&mut ctx).await.unwrap();

        assert!(ctx.is_multipage());
        assert_eq!(ctx.page_count(), 3);

        let page_results = ctx.get("page_results").unwrap();
        assert!(page_results.is_array());
        assert_eq!(page_results.as_array().unwrap().len(), 3);
    }
}
