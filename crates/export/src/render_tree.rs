use latexsnipper_ast::{
    AssetId, Block, Diagnostic, DiagnosticLevel, Document, Inline, W_BLOCK_DOWNGRADED,
};

/// An intermediate representation between AST and final output.
/// Avoids re-traversing the AST for each export format.
#[derive(Debug, Clone)]
pub struct RenderTree {
    pub nodes: Vec<RenderNode>,
    pub diagnostics: Vec<Diagnostic>,
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
    /// An unsupported block type that could not be rendered.
    /// Includes a diagnostic message for observability.
    Unsupported {
        block_type: &'static str,
        message: String,
    },
    Page(Vec<RenderNode>),
}

impl RenderTree {
    /// Build a RenderTree from a Document.
    pub fn from_document(doc: &Document) -> Self {
        let mut nodes = Vec::new();
        let mut diagnostics = Vec::new();

        for page in &doc.pages {
            let mut page_nodes = Vec::new();
            for block in &page.blocks {
                if let Some(node) = convert_block(block, &mut diagnostics) {
                    page_nodes.push(node);
                }
            }
            nodes.push(RenderNode::Page(page_nodes));
        }

        diagnostics.extend(doc.diagnostics.clone());

        Self { nodes, diagnostics }
    }

    /// Build a RenderTree from specific pages of a Document.
    pub fn from_document_pages(doc: &Document, page_indices: &[usize]) -> Self {
        let mut nodes = Vec::new();
        let mut diagnostics = Vec::new();

        for &idx in page_indices {
            if let Some(page) = doc.get_page(idx) {
                let mut page_nodes = Vec::new();
                for block in &page.blocks {
                    if let Some(node) = convert_block(block, &mut diagnostics) {
                        page_nodes.push(node);
                    }
                }
                nodes.push(RenderNode::Page(page_nodes));
            }
        }

        diagnostics.extend(doc.diagnostics.clone());

        Self { nodes, diagnostics }
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

fn convert_block(block: &Block, diags: &mut Vec<Diagnostic>) -> Option<RenderNode> {
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
                    row.cells
                        .iter()
                        .map(|cell| {
                            let inlines = cell.collect_inlines();
                            convert_inlines(&inlines)
                        })
                        .collect()
                })
                .collect();
            Some(RenderNode::Table { rows })
        }
        Block::Figure(f) => {
            let caption_text = f.caption_plain_text();
            let caption = if caption_text.is_empty() {
                vec![]
            } else {
                vec![RenderNode::Text(caption_text)]
            };
            Some(RenderNode::Figure {
                asset_id: f.asset_id.clone(),
                caption,
            })
        }
        Block::List(l) => {
            let items: Vec<Vec<RenderNode>> = l
                .items
                .iter()
                .map(|item| {
                    item.content
                        .iter()
                        .filter_map(|b| convert_block(b, diags))
                        .collect()
                })
                .collect();
            Some(RenderNode::List {
                ordered: l.is_ordered(),
                items,
            })
        }
        Block::Code(c) => Some(RenderNode::Code {
            language: c.language.clone(),
            code: c.code.clone(),
        }),
        Block::Quote(q) => {
            let inner: Vec<RenderNode> = q
                .blocks
                .iter()
                .filter_map(|b| convert_block(b, diags))
                .collect();
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
                        if let Some(node) = convert_block(block, diags) {
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
                if let Some(node) = convert_block(block, diags) {
                    nodes.push(node);
                }
            }
            Some(RenderNode::Paragraph(nodes))
        }
        Block::Proof(p) => {
            let mut nodes = vec![RenderNode::Text("Proof.".to_string())];
            for block in &p.content {
                if let Some(node) = convert_block(block, diags) {
                    nodes.push(node);
                }
            }
            nodes.push(RenderNode::Text("□".to_string()));
            Some(RenderNode::Paragraph(nodes))
        }
        Block::Minipage(m) => {
            let nodes: Vec<RenderNode> = m
                .content
                .iter()
                .filter_map(|b| convert_block(b, diags))
                .collect();
            Some(RenderNode::Paragraph(nodes))
        }
        Block::Float(f) => {
            let nodes: Vec<RenderNode> = f
                .content
                .iter()
                .filter_map(|b| convert_block(b, diags))
                .collect();
            Some(RenderNode::Paragraph(nodes))
        }
        Block::TextBox(tb) => {
            let nodes: Vec<RenderNode> = tb
                .content
                .iter()
                .filter_map(|b| convert_block(b, diags))
                .collect();
            Some(RenderNode::Paragraph(nodes))
        }
        Block::Chart(c) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "chart block ({:?}) is not supported in render tree output",
                    c.chart_type
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "chart",
                message: format!("{:?}", c.chart_type),
            })
        }
        Block::Shape(s) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "shape block ({:?}) is not supported in render tree output",
                    s.shape_type
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "shape",
                message: format!("{:?}", s.shape_type),
            })
        }
        Block::EmbeddedObject(e) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "embedded object ({:?}) is not supported in render tree output",
                    e.kind
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "embedded_object",
                message: format!("{:?}", e.kind),
            })
        }
        Block::Annotation(a) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "annotation block ({:?}) is not supported in render tree output",
                    a.kind
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "annotation",
                message: format!("{:?}", a.kind),
            })
        }
        Block::PageBreak(_) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                "page break block is not supported in render tree output",
            ));
            Some(RenderNode::Unsupported {
                block_type: "page_break",
                message: String::new(),
            })
        }
        Block::SectionBreak(sb) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "section break block ({:?}) is not supported in render tree output",
                    sb.kind
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "section_break",
                message: format!("{:?}", sb.kind),
            })
        }
        Block::HeaderFooter(hf) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "header/footer block ({:?} {:?}) is not supported in render tree output",
                    hf.kind, hf.applies_to
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "header_footer",
                message: format!("{:?} {:?}", hf.kind, hf.applies_to),
            })
        }
        Block::Bibliography(bb) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "bibliography block ({} entries) is not supported in render tree output",
                    bb.entries.len()
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "bibliography",
                message: format!("{} entries", bb.entries.len()),
            })
        }
        Block::FormField(ff) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "form field block ({:?}) is not supported in render tree output",
                    ff.kind
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "form_field",
                message: format!("{:?}", ff.kind),
            })
        }
        Block::Revision(r) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "revision block ({:?}) is not supported in render tree output",
                    r.kind
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "revision",
                message: format!("{:?}", r.kind),
            })
        }
        Block::ChemicalFormula(cf) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "chemical formula block ({}) is not supported in render tree output",
                    cf.formula
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "chemical_formula",
                message: cf.formula.clone(),
            })
        }
        Block::QrCode(_) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                "QR code block is not supported in render tree output",
            ));
            Some(RenderNode::Unsupported {
                block_type: "qr_code",
                message: String::new(),
            })
        }
        Block::Graph(g) => {
            diags.push(Diagnostic::new(
                DiagnosticLevel::Warning,
                W_BLOCK_DOWNGRADED,
                format!(
                    "graph block ({:?}) is not supported in render tree output",
                    g.graph_type
                ),
            ));
            Some(RenderNode::Unsupported {
                block_type: "graph",
                message: format!("{:?}", g.graph_type),
            })
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
            Inline::NoteRef(n) => RenderNode::Text(format!("[note: {}]", n.note_id)),
            Inline::Label { key } => RenderNode::Text(format!("[label={}]", key)),
            Inline::Reference { key, .. } => RenderNode::Text(format!("({})", key)),
            Inline::Citation { key, .. } => RenderNode::Text(format!("[{}]", key)),
            Inline::LineBreak | Inline::SoftBreak => RenderNode::Text("\n".to_string()),
            Inline::Span(s) => {
                let nodes = convert_inlines(&s.content);
                if nodes.len() == 1 {
                    nodes
                        .into_iter()
                        .next()
                        .unwrap_or(RenderNode::Text(String::new()))
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
                RenderNode::Text(format!("[{}]({})", text, l.target))
            }
            Inline::Code(c) => RenderNode::Text(c.code.clone()),
            Inline::Anchor(a) => RenderNode::Text(format!("[anchor: {}]", a.id)),
            Inline::CrossReference(x) => RenderNode::Text(format!("[xref: {}]", x.target_id)),
            Inline::CitationGroup(c) => {
                let keys: Vec<&str> = c.citations.iter().map(|ci| ci.key.as_str()).collect();
                RenderNode::Text(format!("[cite: {}]", keys.join(",")))
            }
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
