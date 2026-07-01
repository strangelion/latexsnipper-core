use crate::generator::Generator;
use crate::render_tree::{RenderNode, RenderTree};
use latexsnipper_foundation::Result;

/// SVG generator — produces SVG output from RenderTree.
pub struct SvgGenerator;

impl Generator for SvgGenerator {
    fn generate(&self, tree: &RenderTree) -> Result<String> {
        let mut svg = String::from(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\" height=\"200\">\n",
        );

        for node in &tree.nodes {
            if let RenderNode::Page(nodes) = node {
                let mut y = 20;
                for child in nodes {
                    y = render_node(child, &mut svg, y);
                }
            }
        }

        svg.push_str("</svg>");
        Ok(svg)
    }

    fn extension(&self) -> &str {
        "svg"
    }
    fn mime_type(&self) -> &str {
        "image/svg+xml"
    }
    fn name(&self) -> &str {
        "svg"
    }
}

fn render_node(node: &RenderNode, svg: &mut String, y: i32) -> i32 {
    match node {
        RenderNode::Text(text) => {
            if !text.is_empty() {
                svg.push_str(&format!(
                    "  <text x=\"20\" y=\"{}\" font-family=\"serif\" font-size=\"14\">{}</text>\n",
                    y,
                    escape_xml(text)
                ));
            }
            y + 20
        }
        RenderNode::Formula {
            latex,
            display_mode,
        } => {
            let size = if *display_mode { 16 } else { 14 };
            svg.push_str(&format!("  <text x=\"20\" y=\"{}\" font-family=\"serif\" font-size=\"{}\" font-style=\"italic\">{}</text>\n", y, size, escape_xml(latex)));
            y + 25
        }
        RenderNode::Paragraph(inlines) => {
            let mut current_y = y;
            for inline in inlines {
                current_y = render_node(inline, svg, current_y);
            }
            current_y
        }
        RenderNode::Page(_) => y,
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
