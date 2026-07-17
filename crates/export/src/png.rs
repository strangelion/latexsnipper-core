use crate::generator::Generator;
use crate::render_tree::RenderTree;
use crate::svg::SvgGenerator;
use crate::svg_policy::{parse_svg_tree, SvgContentPolicy};
use latexsnipper_ast::GeneratedContent;
use latexsnipper_foundation::{Result, SnipperError};
use resvg::tiny_skia;

/// Deterministic local SVG-to-PNG renderer.
pub struct PngGenerator;

impl PngGenerator {
    pub(crate) fn generate_from_svg(svg: &str) -> Result<GeneratedContent> {
        let (parsed, _) = parse_svg_tree(svg, SvgContentPolicy::AllowEmbeddedRaster)?;

        let size = parsed.size().to_int_size();

        let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height()).ok_or_else(|| {
            SnipperError::Export(format!(
                "PNG dimensions are too large: {}x{}",
                size.width(),
                size.height(),
            ))
        })?;

        resvg::render(
            &parsed,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );

        let bytes = pixmap
            .encode_png()
            .map_err(|error| SnipperError::Export(format!("Failed to encode PNG: {error}")))?;

        Ok(GeneratedContent::Binary(bytes))
    }
}

impl Generator for PngGenerator {
    fn generate(&self, tree: &RenderTree) -> Result<GeneratedContent> {
        let generated = SvgGenerator.generate(tree)?;

        let svg = generated.as_text().ok_or_else(|| {
            SnipperError::Export("SVG generator returned binary content".to_string())
        })?;

        Self::generate_from_svg(svg)
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

    #[test]
    fn generate_from_svg_produces_valid_png() {
        let svg = r#"
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="100"
              height="50">
              <rect width="100" height="50" fill="blue"/>
            </svg>
        "#;
        let output = PngGenerator::generate_from_svg(svg).unwrap();
        let bytes = output.as_bytes();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }
}
