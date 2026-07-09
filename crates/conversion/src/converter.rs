use latexsnipper_ast::{Block, Diagnostic, DiagnosticLevel, Document, W_BLOCK_DOWNGRADED};
use latexsnipper_foundation::Result;

/// Trait for converting Document AST to a target format string.
///
/// Unlike `syntax::Renderer` which handles syntax-level rendering,
/// `Converter` handles format-level transformation (e.g., AST → OMML XML).
///
/// NOTE: Image asset resolution is now handled via `asset_helper::resolve_asset_ref`
///   and friends, which are called by all format converters.
///
/// TODO(phase4): deprecate this trait in favor of `crate::SemanticConverter` from the AST crate,
///   which provides richer context via `ConversionContext` and works alongside `Exporter`/`Renderer`.
pub trait Converter {
    /// Convert a Document to the target format.
    fn convert(&self, doc: &Document) -> Result<String>;

    /// Target format name (e.g., "latex", "omml", "mathml").
    fn name(&self) -> &str;

    /// Output file extension (e.g., "tex", "xml").
    fn extension(&self) -> &str;

    /// MIME type (e.g., "application/x-latex", "application/mathml+xml").
    fn mime_type(&self) -> &str;
}

/// Collect diagnostics about blocks that converters will downgrade to placeholders.
/// This is used by DocumentConverter::convert_artifact() to populate ExportArtifact.diagnostics.
pub fn collect_converter_diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for page in &doc.pages {
        for block in &page.blocks {
            check_block_for_downgrade(block, &mut diags);
        }
    }
    diags
}

fn check_block_for_downgrade(block: &Block, diags: &mut Vec<Diagnostic>) {
    match block {
        Block::Chart(c) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    format!(
                        "ChartBlock ({:?}) downgraded to placeholder in semantic converter",
                        c.chart_type
                    ),
                )
                .with_recoverable(true),
            );
        }
        Block::Shape(s) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    format!(
                        "ShapeBlock ({:?}) downgraded to placeholder in semantic converter",
                        s.shape_type
                    ),
                )
                .with_recoverable(true),
            );
        }
        Block::EmbeddedObject(e) => {
            diags.push(
                Diagnostic::new(DiagnosticLevel::Warning, W_BLOCK_DOWNGRADED,
                    format!("EmbeddedObjectBlock ({:?}) downgraded to placeholder in semantic converter", e.kind))
                    .with_recoverable(true),
            );
        }
        Block::Annotation(a) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    format!(
                        "AnnotationBlock ({:?}) downgraded to placeholder in semantic converter",
                        a.kind
                    ),
                )
                .with_recoverable(true),
            );
        }
        Block::FormField(_) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    "FormFieldBlock downgraded to placeholder in semantic converter",
                )
                .with_recoverable(true),
            );
        }
        Block::ChemicalFormula(_) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    "ChemicalFormulaBlock downgraded to placeholder in semantic converter",
                )
                .with_recoverable(true),
            );
        }
        Block::Graph(_) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    "GraphBlock downgraded to placeholder in semantic converter",
                )
                .with_recoverable(true),
            );
        }
        Block::QrCode(_) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    "QrCodeBlock downgraded to placeholder in semantic converter",
                )
                .with_recoverable(true),
            );
        }
        Block::Bibliography(_) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    "BibliographyBlock downgraded to placeholder in semantic converter",
                )
                .with_recoverable(true),
            );
        }
        Block::HeaderFooter(hf) => {
            diags.push(
                Diagnostic::new(
                    DiagnosticLevel::Warning,
                    W_BLOCK_DOWNGRADED,
                    format!(
                        "HeaderFooterBlock ({:?}) downgraded to placeholder in semantic converter",
                        hf.kind
                    ),
                )
                .with_recoverable(true),
            );
        }
        Block::PageBreak(_) | Block::SectionBreak(_) => {
            // These are structural, not downgrades -- skip
        }
        // Recurse into container blocks
        Block::TextBox(tb) => {
            for child in &tb.content {
                check_block_for_downgrade(child, diags);
            }
        }
        Block::Quote(q) => {
            for child in &q.blocks {
                check_block_for_downgrade(child, diags);
            }
        }
        Block::Minipage(m) => {
            for child in &m.content {
                check_block_for_downgrade(child, diags);
            }
        }
        Block::Float(f) => {
            for child in &f.content {
                check_block_for_downgrade(child, diags);
            }
        }
        Block::Theorem(t) => {
            for child in &t.content {
                check_block_for_downgrade(child, diags);
            }
        }
        Block::Proof(p) => {
            for child in &p.content {
                check_block_for_downgrade(child, diags);
            }
        }
        _ => {}
    }
}
