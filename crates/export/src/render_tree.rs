use latexsnipper_ast::{AssetId, Block, Document, Inline};

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
    /// An inline image referenced by its asset ID.
    Image {
        asset_id: Option<AssetId>,
        width: Option<f32>,
        height: Option<f32>,
        alt_text: Option<String>,
    },
    /// A figure block with an optional asset and caption.
    Figure {
        asset_id: Option<AssetId>,
        caption: Vec<RenderNode>,
    },
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
        Block::Figure(f) => {
            let caption = f
                .caption
                .as_ref()
                .map(|c| {
                    // Simple text render of caption
                    vec![RenderNode::Text(c.clone())]
                })
                .unwrap_or_default();
            Some(RenderNode::Figure {
                asset_id: f.asset_id.clone(),
                caption,
            })
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
        Block::Handwriting(hw) => {
            let inlines = convert_inlines(&hw.inlines);
            Some(RenderNode::Paragraph(inlines))
        }
        Block::DescriptionList(dl) => {
            let items: Vec<Vec<RenderNode>> = dl
                .items
                .iter()
                .flat_map(|item| {
                    let mut nodes = Vec::new();
                    if let Some(label) = &item.label {
                        nodes.extend(convert_inlines(label));
                    }
                    for block in &item.content {
                        if let Some(node) = convert_block(block) {
                            nodes.push(node);
                        }
                    }
                    if nodes.is_empty() {
                        None
                    } else {
                        Some(nodes)
                    }
                })
                .collect();
            Some(RenderNode::List {
                ordered: false,
                items,
            })
        }
        Block::TableOfContents => Some(RenderNode::Paragraph(vec![RenderNode::Text(
            "目录".to_string(),
        )])),
        Block::Theorem(t) => {
            let mut nodes = vec![RenderNode::Text(format!("{}.", t.name))];
            for block in &t.content {
                if let Some(node) = convert_block(block) {
                    nodes.push(node);
                }
            }
            Some(RenderNode::Paragraph(nodes))
        }
        Block::Proof(p) => {
            let mut nodes = vec![RenderNode::Text("Proof.".to_string())];
            for block in &p.content {
                if let Some(node) = convert_block(block) {
                    nodes.push(node);
                }
            }
            nodes.push(RenderNode::Text("□".to_string()));
            Some(RenderNode::Paragraph(nodes))
        }
        Block::Minipage(m) => {
            let nodes: Vec<RenderNode> = m.content.iter().filter_map(convert_block).collect();
            Some(RenderNode::Paragraph(nodes))
        }
        Block::Float(f) => {
            let nodes: Vec<RenderNode> = f.content.iter().filter_map(convert_block).collect();
            Some(RenderNode::Paragraph(nodes))
        }
        Block::TextBox(tb) => {
            let nodes: Vec<RenderNode> = tb.content.iter().filter_map(convert_block).collect();
            Some(RenderNode::Paragraph(nodes))
        }
        Block::Chart(_) | Block::Shape(_) | Block::EmbeddedObject(_) | Block::Annotation(_) => {
            None
        }
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
            Inline::Image(img) => RenderNode::Image {
                asset_id: img.asset_id.clone(),
                width: img.width,
                height: img.height,
                alt_text: img.alt_text.clone(),
            },
            Inline::Footnote { content } => {
                let inner = convert_inlines(&[*content.clone()]);
                RenderNode::Text(format!(
                    "[^{}]",
                    inner
                        .iter()
                        .map(|n| match n {
                            RenderNode::Text(t) => t.clone(),
                            _ => String::new(),
                        })
                        .collect::<String>()
                ))
            }
            Inline::Label { key } => RenderNode::Text(format!("[label={}]", key)),
            Inline::Reference { key, .. } => RenderNode::Text(format!("({})", key)),
            Inline::Citation { key, .. } => RenderNode::Text(format!("[{}]", key)),
            Inline::LineBreak | Inline::SoftBreak => RenderNode::Text("\n".to_string()),
            Inline::Span(s) => {
                let nodes = convert_inlines(&s.content);
                if nodes.len() == 1 {
                    nodes.into_iter().next().unwrap_or(RenderNode::Text(String::new()))
                } else {
                    RenderNode::Text(
                        nodes
                            .iter()
                            .map(|n| match n {
                                RenderNode::Text(t) => t.clone(),
                                _ => String::new(),
                            })
                            .collect(),
                    )
                }
            }
            Inline::Link(l) => {
                let text = convert_inlines(&l.content)
                    .iter()
                    .map(|n| match n {
                        RenderNode::Text(t) => t.clone(),
                        _ => String::new(),
                    })
                    .collect::<String>();
                RenderNode::Text(format!("[{}](l.target)", text))
            }
            Inline::Code(c) => RenderNode::Text(c.code.clone()),
            Inline::Superscript(inner) | Inline::Subscript(inner) => {
                let nodes = convert_inlines(inner);
                RenderNode::Text(
                    nodes
                        .iter()
                        .map(|n| match n {
                            RenderNode::Text(t) => t.clone(),
                            _ => String::new(),
                        })
                        .collect(),
                )
            }
        })
        .collect()
}
