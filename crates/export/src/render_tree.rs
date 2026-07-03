use latexsnipper_ast::{Block, Document, Inline};

/// An intermediate representation between AST and final output.
/// Avoids re-traversing the AST for each export format.
#[derive(Debug, Clone)]
pub struct RenderTree {
    pub nodes: Vec<RenderNode>,
}

#[derive(Debug, Clone)]
pub enum RenderNode {
    Text(String),
    Formula {
        latex: String,
        display_mode: bool,
    },
    Paragraph(Vec<RenderNode>),
    Heading {
        level: u8,
        nodes: Vec<RenderNode>,
    },
    Table {
        rows: Vec<Vec<Vec<RenderNode>>>,
    },
    List {
        ordered: bool,
        items: Vec<Vec<RenderNode>>,
    },
    Code {
        language: Option<String>,
        code: String,
    },
    Quote(Vec<RenderNode>),
    HorizontalRule,
    Page(Vec<RenderNode>),
}

impl RenderTree {
    /// Build a RenderTree from a Document.
    pub fn from_document(doc: &Document) -> Self {
        let mut nodes = Vec::new();

        for page in &doc.pages {
            let mut page_nodes = Vec::new();
            for block in &page.blocks {
                if let Some(node) = convert_block(block) {
                    page_nodes.push(node);
                }
            }
            nodes.push(RenderNode::Page(page_nodes));
        }

        Self { nodes }
    }

    /// Build a RenderTree from specific pages of a Document.
    pub fn from_document_pages(doc: &Document, page_indices: &[usize]) -> Self {
        let mut nodes = Vec::new();

        for &idx in page_indices {
            if let Some(page) = doc.get_page(idx) {
                let mut page_nodes = Vec::new();
                for block in &page.blocks {
                    if let Some(node) = convert_block(block) {
                        page_nodes.push(node);
                    }
                }
                nodes.push(RenderNode::Page(page_nodes));
            }
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

fn convert_block(block: &Block) -> Option<RenderNode> {
    match block {
        Block::Heading(h) => {
            let inlines = convert_inlines(&h.inlines);
            Some(RenderNode::Heading {
                level: h.level,
                nodes: inlines,
            })
        }
        Block::Paragraph(p) => {
            let inlines = convert_inlines(&p.inlines);
            Some(RenderNode::Paragraph(inlines))
        }
        Block::Formula(f) => Some(RenderNode::Formula {
            latex: f.formula.as_latex().to_string(),
            display_mode: f.formula.display_mode,
        }),
        Block::Table(t) => {
            let rows: Vec<Vec<Vec<RenderNode>>> = t
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| convert_inlines(&cell.inlines))
                        .collect()
                })
                .collect();
            Some(RenderNode::Table { rows })
        }
        Block::List(l) => {
            let items: Vec<Vec<RenderNode>> = l
                .items
                .iter()
                .map(|item| convert_inlines(&item.inlines))
                .collect();
            Some(RenderNode::List {
                ordered: l.ordered,
                items,
            })
        }
        Block::Code(c) => Some(RenderNode::Code {
            language: c.language.clone(),
            code: c.code.clone(),
        }),
        Block::Quote(q) => {
            let inner: Vec<RenderNode> = q.blocks.iter().filter_map(convert_block).collect();
            Some(RenderNode::Quote(inner))
        }
        Block::HorizontalRule(_) => Some(RenderNode::HorizontalRule),
        Block::Figure(_) => None,
    }
}

fn convert_inlines(inlines: &[Inline]) -> Vec<RenderNode> {
    inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t) => RenderNode::Text(t.text.clone()),
            Inline::Formula(f) => RenderNode::Formula {
                latex: f.as_latex().to_string(),
                display_mode: f.display_mode,
            },
            Inline::Image(_) => RenderNode::Text(String::new()),
        })
        .collect()
}
