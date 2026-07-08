use latexsnipper_ast::{Document, OfficeApp, OfficeInsertKind};

use crate::clipboard::ClipboardBundle;
use crate::omml::latex_to_omml;

/// Result of preparing content for Office insertion.
#[derive(Debug, Clone)]
pub struct OfficeInsertResult {
    /// The content to insert, as bytes.
    pub content: Vec<u8>,
    /// Best Office insertion kind for this content.
    pub insert_kind: OfficeInsertKind,
    /// MIME type for clipboard/image fallback.
    pub mime_type: &'static str,
    /// Human-readable description of what was produced.
    pub description: String,
}

/// Unified service for preparing Document content for Office insertion.
///
/// Automatically selects the best format for the target Office application
/// based on the priority table from the platform plan:
///
/// | App | Priority |
/// |---|---|
/// | Word | OMath → OOXML Table → ClipboardBundle → PNG/SVG |
/// | PowerPoint | SVG → PNG → ClipboardBundle |
/// | Excel | CSV/TSV → PlainText → ClipboardBundle |
pub struct OfficeInsertService;

impl OfficeInsertService {
    /// Prepare a Document for insertion into the specified Office app.
    ///
    /// Returns the best available content along with metadata about what was produced.
    pub fn prepare(doc: &Document, app: OfficeApp) -> OfficeInsertResult {
        match app {
            OfficeApp::Word => Self::prepare_for_word(doc),
            OfficeApp::PowerPoint => Self::prepare_for_powerpoint(doc),
            OfficeApp::Excel => Self::prepare_for_excel(doc),
        }
    }

    /// Prepare a single formula (LaTeX string) for Office insertion.
    pub fn prepare_formula(latex: &str, app: OfficeApp) -> OfficeInsertResult {
        match app {
            OfficeApp::Word => {
                // OMath via OMML
                let omml = latex_to_omml(latex);
                OfficeInsertResult {
                    content: omml.into_bytes(),
                    insert_kind: OfficeInsertKind::OMath,
                    mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.math",
                    description: "Editable OMML equation for Word".to_string(),
                }
            }
            OfficeApp::PowerPoint => {
                // SVG is best for PowerPoint
                let svg = format!(
                    r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40">
                       <text x="10" y="30" font-family="serif" font-size="20" font-style="italic">{}</text>
                    </svg>"#,
                    latex
                );
                OfficeInsertResult {
                    content: svg.into_bytes(),
                    insert_kind: OfficeInsertKind::ImageSvg,
                    mime_type: "image/svg+xml",
                    description: "SVG formula for PowerPoint".to_string(),
                }
            }
            OfficeApp::Excel => {
                // Plain text formula for Excel
                OfficeInsertResult {
                    content: latex.as_bytes().to_vec(),
                    insert_kind: OfficeInsertKind::OoxmlFragment,
                    mime_type: "text/plain",
                    description: "Formula text for Excel".to_string(),
                }
            }
        }
    }

    // ── Word ──────────────────────────────────────────────────────────

    fn prepare_for_word(doc: &Document) -> OfficeInsertResult {
        // Priority 1: If the document contains only formulas → OMath
        if doc.block_count() > 0 && doc.all_blocks().iter().all(|b| matches!(b, latexsnipper_ast::Block::Formula(_))) {
            let latex = doc
                .all_blocks()
                .iter()
                .map(|b| match b {
                    latexsnipper_ast::Block::Formula(f) => f.formula.as_latex().to_string(),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join("\n");
            let omml = latex_to_omml(&latex);
            if !omml.is_empty() {
                return OfficeInsertResult {
                    content: omml.into_bytes(),
                    insert_kind: OfficeInsertKind::OMath,
                    mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.math",
                    description: "Editable OMML equation for Word".to_string(),
                };
            }
        }

        // Priority 2: Clipboard bundle (HTML + RTF + text)
        let bundle = ClipboardBundle::from_document(doc);
        let html = bundle.html;
        OfficeInsertResult {
            content: html.into_bytes(),
            insert_kind: OfficeInsertKind::HtmlClipboard,
            mime_type: "text/html",
            description: "HTML fragment with formatting for Word".to_string(),
        }
    }

    // ── PowerPoint ────────────────────────────────────────────────────

    fn prepare_for_powerpoint(_doc: &Document) -> OfficeInsertResult {
        // SVG is the best format for PowerPoint (natively supported).
        // For now, return a clipboard bundle fallback since full SVG rendering
        // of arbitrary documents requires the export crate's RenderTree pipeline.
        let bundle = ClipboardBundle::from_document(_doc);
        let html = bundle.html;
        OfficeInsertResult {
            content: html.into_bytes(),
            insert_kind: OfficeInsertKind::HtmlClipboard,
            mime_type: "text/html",
            description: "HTML content for PowerPoint (SVG export available via ExportService)".to_string(),
        }
    }

    // ── Excel ─────────────────────────────────────────────────────────

    fn prepare_for_excel(doc: &Document) -> OfficeInsertResult {
        // Priority: CSV/TSV for tables, plain text otherwise
        let plain_text = doc
            .all_blocks()
            .iter()
            .map(|b| match b {
                latexsnipper_ast::Block::Table(t) => {
                    let mut rows = Vec::new();
                    for row in &t.rows {
                        let cells: Vec<String> = row
                            .iter()
                            .map(|cell| {
                                cell.inlines
                                    .iter()
                                    .map(|i| match i {
                                        latexsnipper_ast::Inline::Text(t) => t.text.clone(),
                                        latexsnipper_ast::Inline::Formula(f) => f.as_latex().to_string(),
                                        _ => String::new(),
                                    })
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .collect();
                        rows.push(cells.join("\t"));
                    }
                    rows.join("\n")
                }
                latexsnipper_ast::Block::Formula(f) => f.formula.as_latex().to_string(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        OfficeInsertResult {
            content: plain_text.into_bytes(),
            insert_kind: OfficeInsertKind::OoxmlFragment,
            mime_type: "text/plain",
            description: "Tab-separated table / formula text for Excel".to_string(),
        }
    }
}
