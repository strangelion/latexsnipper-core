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
            svg.push_str(&format!(
                "  <text x=\"20\" y=\"{}\" font-family=\"serif\" font-size=\"{}\" font-style=\"italic\">{}</text>\n",
                y, size, escape_xml(latex)
            ));
            y + 25
        }
        RenderNode::Paragraph(inlines) => {
            let mut current_y = y;
            for inline in inlines {
                current_y = render_node(inline, svg, current_y);
            }
            current_y
        }
        RenderNode::Heading { level, nodes } => {
            let size = 24 - (*level as i32) * 2;
            let text: String = nodes.iter().map(node_to_text).collect();
            svg.push_str(&format!(
                "  <text x=\"20\" y=\"{}\" font-family=\"serif\" font-size=\"{}\" font-weight=\"bold\">{}</text>\n",
                y, size, escape_xml(&text)
            ));
            y + size + 6
        }
        RenderNode::Table { rows } => {
            let mut current_y = y;
            for row in rows {
                let mut x = 20;
                let col_width = 80;
                for cell in row {
                    let text: String = cell.iter().map(node_to_text).collect();
                    svg.push_str(&format!(
                        "  <text x=\"{}\" y=\"{}\" font-family=\"serif\" font-size=\"10\">{}</text>\n",
                        x, current_y, escape_xml(&text)
                    ));
                    x += col_width;
                }
                current_y += 16;
            }
            current_y + 4
        }
        RenderNode::List { ordered, items } => {
            let mut current_y = y;
            for (i, item) in items.iter().enumerate() {
                let text: String = item.iter().map(node_to_text).collect();
                let bullet = if *ordered {
                    format!("{}.", i + 1)
                } else {
                    "\u{2022}".to_string()
                };
                svg.push_str(&format!(
                    "  <text x=\"20\" y=\"{}\" font-family=\"serif\" font-size=\"14\">{} {}</text>\n",
                    current_y, bullet, escape_xml(&text)
                ));
                current_y += 18;
            }
            current_y
        }
        RenderNode::Code { code, .. } => {
            svg.push_str(&format!(
                "  <text x=\"30\" y=\"{}\" font-family=\"monospace\" font-size=\"12\">{}</text>\n",
                y,
                escape_xml(code)
            ));
            y + 16
        }
        RenderNode::Quote(nodes) => {
            let mut current_y = y;
            svg.push_str(&format!(
                "  <line x1=\"20\" y1=\"{}\" x2=\"20\" y2=\"{}\" stroke=\"gray\" stroke-width=\"2\"/>\n",
                y, y + nodes.len() as i32 * 18
            ));
            for child in nodes {
                current_y = render_node(child, svg, current_y);
            }
            current_y + 4
        }
        RenderNode::HorizontalRule => {
            svg.push_str(&format!(
                "  <line x1=\"20\" y1=\"{}\" x2=\"380\" y2=\"{}\" stroke=\"black\" stroke-width=\"1\"/>\n",
                y, y
            ));
            y + 10
        }
        RenderNode::Page(_) => y,
        RenderNode::Image { alt_text, .. } => {
            let text = alt_text.as_deref().unwrap_or("[image]");
            svg.push_str(&format!(
                "  <text x=\"20\" y=\"{}\" font-family=\"serif\" font-size=\"14\" font-style=\"italic\">{}</text>\n",
                y, escape_xml(text)
            ));
            y + 20
        }
        RenderNode::Figure { caption, .. } => {
            let text = if caption.is_empty() {
                "[figure]".to_string()
            } else {
                render_nodes_to_text(caption)
            };
            svg.push_str(&format!(
                "  <text x=\"20\" y=\"{}\" font-family=\"serif\" font-size=\"14\">{}</text>\n",
                y, escape_xml(&text)
            ));
            y + 20
        }
    }
}

fn render_nodes_to_text(nodes: &[RenderNode]) -> String {
    nodes.iter().map(node_to_text).collect()
}

fn node_to_text(node: &RenderNode) -> String {
    match node {
        RenderNode::Text(t) => t.clone(),
        RenderNode::Formula { latex, .. } => format!("[{}]", latex),
        RenderNode::Paragraph(nodes) => render_nodes_to_text(nodes),
        RenderNode::Heading { nodes, .. } => render_nodes_to_text(nodes),
        RenderNode::Table { rows } => {
            let mut result = String::new();
            for row in rows {
                let cells: Vec<String> = row.iter().map(|c| render_nodes_to_text(c)).collect();
                result.push_str(&cells.join(" | "));
                result.push('\n');
            }
            result
        }
        RenderNode::List { items, .. } => items
            .iter()
            .map(|i| render_nodes_to_text(i))
            .collect::<Vec<_>>()
            .join("\n"),
        RenderNode::Code { code, .. } => code.clone(),
        RenderNode::Quote(nodes) => render_nodes_to_text(nodes),
        RenderNode::HorizontalRule => "---".to_string(),
        RenderNode::Page(_) => String::new(),
        RenderNode::Image { alt_text, .. } => alt_text.clone().unwrap_or_else(|| "[image]".to_string()),
        RenderNode::Figure { caption, .. } => {
            if caption.is_empty() {
                "[figure]".to_string()
            } else {
                render_nodes_to_text(caption)
            }
        }
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
