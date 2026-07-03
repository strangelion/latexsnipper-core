use crate::generator::Generator;
use crate::render_tree::{RenderNode, RenderTree};
use latexsnipper_foundation::Result;

/// Plain text generator — produces plain text output from RenderTree.
pub struct TextGenerator;

impl Generator for TextGenerator {
    fn generate(&self, tree: &RenderTree) -> Result<String> {
        let mut parts = Vec::new();

        for node in &tree.nodes {
            if let RenderNode::Page(nodes) = node {
                for child in nodes {
                    parts.push(render_text(child));
                }
            }
        }

        Ok(parts.join("\n"))
    }

    fn extension(&self) -> &str {
        "txt"
    }
    fn mime_type(&self) -> &str {
        "text/plain"
    }
    fn name(&self) -> &str {
        "text"
    }
}

fn render_nodes_to_text(nodes: &[RenderNode]) -> String {
    nodes.iter().map(render_text).collect()
}

fn render_text(node: &RenderNode) -> String {
    match node {
        RenderNode::Text(text) => text.clone(),
        RenderNode::Formula { latex, .. } => latex.clone(),
        RenderNode::Paragraph(nodes) => render_nodes_to_text(nodes),
        RenderNode::Heading { nodes, level } => {
            let prefix = "#".repeat(*level as usize);
            let text = render_nodes_to_text(nodes);
            format!("{} {}", prefix, text)
        }
        RenderNode::Table { rows } => {
            let mut result = String::new();
            for (i, row) in rows.iter().enumerate() {
                let cells: Vec<String> = row.iter().map(|c| render_nodes_to_text(c)).collect();
                result.push_str(&cells.join(" | "));
                if i + 1 < rows.len() {
                    result.push('\n');
                    result.push_str(&"-".repeat(40));
                    result.push('\n');
                }
            }
            result
        }
        RenderNode::List { ordered, items } => {
            let mut result = String::new();
            for (i, item) in items.iter().enumerate() {
                let text = render_nodes_to_text(item);
                if *ordered {
                    result.push_str(&format!("{}. {}", i + 1, text));
                } else {
                    result.push_str(&format!("- {}", text));
                }
                result.push('\n');
            }
            result
        }
        RenderNode::Code { code, .. } => code.clone(),
        RenderNode::Quote(nodes) => {
            let text = render_nodes_to_text(nodes);
            text.lines()
                .map(|l| format!("> {}", l))
                .collect::<Vec<_>>()
                .join("\n")
        }
        RenderNode::HorizontalRule => "---".to_string(),
        RenderNode::Page(_) => String::new(),
    }
}
