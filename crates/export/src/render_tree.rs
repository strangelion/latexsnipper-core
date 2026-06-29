use latexsnipper_ast::{Document, Block, Inline, Formula};

/// An intermediate representation between AST and final output.
/// Avoids re-traversing the AST for each export format.
#[derive(Debug, Clone)]
pub struct RenderTree {
    pub nodes: Vec<RenderNode>,
}

#[derive(Debug, Clone)]
pub enum RenderNode {
    Text(String),
    Formula { latex: String, display_mode: bool },
    Paragraph(Vec<RenderNode>),
    Page(Vec<RenderNode>),
}

impl RenderTree {
    /// Build a RenderTree from a Document.
    pub fn from_document(doc: &Document) -> Self {
        let mut nodes = Vec::new();

        for page in &doc.pages {
            let mut page_nodes = Vec::new();
            for block in &page.blocks {
                match block {
                    Block::Paragraph(p) => {
                        let inlines: Vec<RenderNode> = p.inlines.iter().map(|i| {
                            match i {
                                Inline::Text(t) => RenderNode::Text(t.text.clone()),
                                Inline::Formula(f) => RenderNode::Formula {
                                    latex: f.as_latex().to_string(),
                                    display_mode: f.display_mode,
                                },
                                _ => RenderNode::Text(String::new()),
                            }
                        }).collect();
                        page_nodes.push(RenderNode::Paragraph(inlines));
                    }
                    Block::Formula(f) => {
                        page_nodes.push(RenderNode::Formula {
                            latex: f.formula.as_latex().to_string(),
                            display_mode: f.formula.display_mode,
                        });
                    }
                    _ => {}
                }
            }
            nodes.push(RenderNode::Page(page_nodes));
        }

        Self { nodes }
    }

    /// Get the number of pages.
    pub fn page_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of nodes in a page.
    pub fn node_count(&self, page: usize) -> usize {
        self.nodes.get(page).map_or(0, |n| match n {
            RenderNode::Page(nodes) => nodes.len(),
            _ => 0,
        })
    }
}
