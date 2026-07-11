use crate::generator::Generator;
use crate::render_tree::RenderTree;
use crate::svg::SvgGenerator;
use latexsnipper_ast::GeneratedContent;
use latexsnipper_foundation::{Result, SnipperError};
use resvg::{tiny_skia, usvg};

/// Deterministic local SVG-to-PNG renderer.
pub struct PngGenerator;

impl Generator for PngGenerator {
    fn generate(&self, tree: &RenderTree) -> Result<GeneratedContent> {
        let generated = SvgGenerator.generate(tree)?;
        let svg = generated.as_text().ok_or_else(|| {
            SnipperError::Export("SVG generator returned binary content".to_string())
        })?;
        let options = usvg::Options::default();
        let parsed = usvg::Tree::from_str(svg, &options)
            .map_err(|e| SnipperError::Export(format!("Failed to parse generated SVG: {e}")))?;
        let size = parsed.size().to_int_size();
        let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).ok_or_else(|| {
            SnipperError::Export(format!(
                "PNG dimensions are too large: {}x{}",
                size.width(),
                size.height()
            ))
        })?;

        resvg::render(
            &parsed,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        let bytes = pixmap
            .encode_png()
            .map_err(|e| SnipperError::Export(format!("Failed to encode PNG: {e}")))?;
        Ok(GeneratedContent::Binary(bytes))
    }

    fn extension(&self) -> &str {
        "png"
    }

    fn mime_type(&self) -> &str {
        "image/png"
    }

    fn name(&self) -> &str {
        "png"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_tree::{RenderNode, RenderTree};

    #[test]
    fn png_generator_produces_reopenable_binary() {
        let tree = RenderTree {
            nodes: vec![RenderNode::Page(vec![RenderNode::Text("PNG".into())])],
            diagnostics: Vec::new(),
        };
        let output = PngGenerator.generate(&tree).unwrap();
        let bytes = output.as_bytes();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert!(resvg::tiny_skia::Pixmap::decode_png(bytes).is_ok());
    }
}
