use latexsnipper_foundation::Result;
use crate::render_tree::{RenderTree, RenderNode};
use crate::generator::Generator;

/// Plain text generator — produces plain text output from RenderTree.
pub struct TextGenerator;

impl Generator for TextGenerator {
    fn generate(&self, tree: &RenderTree) -> Result<String> {
        let mut parts = Vec::new();

        for node in &tree.nodes {
            match node {
                RenderNode::Page(nodes) => {
                    for child in nodes {
                        match child {
                            RenderNode::Text(text) => {
                                parts.push(text.clone());
                            }
                            RenderNode::Formula { latex, .. } => {
                                parts.push(latex.clone());
                            }
                            RenderNode::Paragraph(inlines) => {
                                for inline in inlines {
                                    match inline {
                                        RenderNode::Text(text) => parts.push(text.clone()),
                                        RenderNode::Formula { latex, .. } => parts.push(latex.clone()),
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(parts.join("\n"))
    }

    fn extension(&self) -> &str { "txt" }
    fn mime_type(&self) -> &str { "text/plain" }
    fn name(&self) -> &str { "text" }
}
