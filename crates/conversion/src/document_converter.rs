use latexsnipper_ast::{
    Block, Document, ExportArtifact, Formula, FormulaBlock, FormulaLayout, FormulaSource,
    GeneratedContent, NodeIdGenerator, Page,
};
#[cfg(feature = "native")]
use latexsnipper_export::render_tree::RenderTree;
use latexsnipper_foundation::Result;
use sha2::{Digest, Sha256};

use crate::converter::collect_converter_diagnostics;
use crate::converter::Converter;
use crate::export_format::ExportFormat;
use crate::{
    HtmlConverter, LatexConverter, LatexDisplayConverter, LatexEquationConverter,
    MarkdownBlockConverter, MarkdownInlineConverter, MathmlConverter, OmmlConverter,
    TypstConverter,
};

/// Supported output formats.
///
/// NOTE: These are semantic conversion formats, not file export formats.
/// For file export (SVG, PDF, PNG, DOCX, etc.), use `ExportFormat` in the export module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Latex,
    LatexDisplay,
    LatexEquation,
    Typst,
    MarkdownInline,
    MarkdownBlock,
    MathML,
    OMML,
    Html,
}

impl OutputFormat {
    pub fn all() -> &'static [OutputFormat] {
        &[
            OutputFormat::Latex,
            OutputFormat::LatexDisplay,
            OutputFormat::LatexEquation,
            OutputFormat::Typst,
            OutputFormat::MarkdownInline,
            OutputFormat::MarkdownBlock,
            OutputFormat::MathML,
            OutputFormat::OMML,
            OutputFormat::Html,
        ]
    }

    pub fn name(&self) -> &str {
        match self {
            OutputFormat::Latex => "latex",
            OutputFormat::LatexDisplay => "latex_display",
            OutputFormat::LatexEquation => "latex_equation",
            OutputFormat::Typst => "typst",
            OutputFormat::MarkdownInline => "markdown_inline",
            OutputFormat::MarkdownBlock => "markdown_block",
            OutputFormat::MathML => "mathml",
            OutputFormat::OMML => "omml",
            OutputFormat::Html => "html",
        }
    }

    pub fn extension(&self) -> &str {
        match self {
            OutputFormat::Latex | OutputFormat::LatexDisplay | OutputFormat::LatexEquation => "tex",
            OutputFormat::Typst => "typ",
            OutputFormat::MarkdownInline | OutputFormat::MarkdownBlock => "md",
            OutputFormat::MathML | OutputFormat::OMML => "xml",
            OutputFormat::Html => "html",
        }
    }

    /// Convert to ExportFormat.
    pub fn to_export_format(&self) -> ExportFormat {
        match self {
            OutputFormat::Latex | OutputFormat::LatexDisplay | OutputFormat::LatexEquation => {
                ExportFormat::Latex
            }
            OutputFormat::Typst => ExportFormat::Typst,
            OutputFormat::MarkdownInline | OutputFormat::MarkdownBlock => ExportFormat::Markdown,
            OutputFormat::MathML => ExportFormat::MathML,
            OutputFormat::OMML => ExportFormat::OMML,
            OutputFormat::Html => ExportFormat::Html,
        }
    }
}

/// Unified converter that can convert Document AST to any supported format.
pub struct DocumentConverter {
    format: OutputFormat,
}

impl DocumentConverter {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    pub fn convert(&self, doc: &Document) -> Result<String> {
        let converter: Box<dyn Converter> = match self.format {
            OutputFormat::Latex => Box::new(LatexConverter),
            OutputFormat::LatexDisplay => Box::new(LatexDisplayConverter),
            OutputFormat::LatexEquation => Box::new(LatexEquationConverter),
            OutputFormat::Typst => Box::new(TypstConverter),
            OutputFormat::MarkdownInline => Box::new(MarkdownInlineConverter),
            OutputFormat::MarkdownBlock => Box::new(MarkdownBlockConverter),
            OutputFormat::MathML => Box::new(MathmlConverter),
            OutputFormat::OMML => Box::new(OmmlConverter),
            OutputFormat::Html => Box::new(HtmlConverter),
        };
        converter.convert(doc)
    }

    /// Convert one formula through the existing document conversion pipeline.
    ///
    /// The fragment is intentionally represented as a minimal Document so all
    /// current converters retain their existing behavior.
    pub fn convert_formula(formula: &Formula, format: OutputFormat) -> Result<String> {
        let block = Block::Formula(FormulaBlock {
            formula: formula.clone(),
            label: None,
            number: None,
            environment: None,
            geometry: None,
            source: None,
        });
        Self::convert_block(&Document::new(), &block, format)
    }

    /// Build the shared structural layout from any supported source format.
    pub fn formula_layout(formula: &Formula) -> Result<FormulaLayout> {
        formula
            .layout
            .clone()
            .map(Ok)
            .unwrap_or_else(|| crate::parse_formula_source_to_layout(&formula.source))
    }

    /// Convert an existing layout through its canonical LaTeX projection.
    ///
    /// This is additive; source-first conversion remains the default until
    /// cross-format round-trip fidelity gates are established.
    pub fn convert_formula_layout(layout: &FormulaLayout, format: OutputFormat) -> Result<String> {
        Self::convert_latex_string(&layout.canonical_latex(), format)
    }

    /// Convert one block while preserving the document metadata and assets
    /// required by existing converters.
    pub fn convert_block(
        document: &Document,
        block: &Block,
        format: OutputFormat,
    ) -> Result<String> {
        let fragment = Self::fragment_document(document, block.clone());
        Self::new(format).convert(&fragment)
    }

    /// Build the minimal document used by fragment conversion APIs.
    pub fn fragment_document(document: &Document, block: Block) -> Document {
        Document {
            metadata: document.metadata.clone(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks: vec![block],
                page_number: None,
                layout: None,
                background_asset_id: None,
            }],
            assets: document.assets.clone(),
            diagnostics: Vec::new(),
            id_gen: NodeIdGenerator::new(),
            schema_version: document.schema_version.clone(),
            notes: document.notes.clone(),
            outline: None,
        }
    }

    /// Convert specific pages (0-based indices) of a Document.
    pub fn convert_pages(&self, doc: &Document, pages: &[usize]) -> Result<String> {
        let filtered = doc.filter_pages(pages);
        self.convert(&filtered)
    }

    /// Convert a Document to the target format, returning an ExportArtifact
    /// with diagnostics, assets, and the converted text.
    pub fn convert_artifact(&self, doc: &Document) -> std::result::Result<ExportArtifact, String> {
        let text = self.convert(doc).map_err(|e| e.to_string())?;
        let format_str = self.format.name().to_string();
        let checksum = format!("{:x}", Sha256::digest(text.as_bytes()));
        let size_bytes = text.len() as u64;

        // Collect diagnostics from all sources
        let mut diagnostics = doc.diagnostics.clone();

        // RenderTree warns about unsupported blocks in native visual export.
        #[cfg(feature = "native")]
        {
            let tree = RenderTree::from_document(doc);
            diagnostics.extend(tree.diagnostics);
        }

        // 2. Converter-level diagnostics for placeholder-rendered blocks
        diagnostics.extend(collect_converter_diagnostics(doc));

        Ok(ExportArtifact {
            format: format_str,
            primary_path: None,
            content: Some(GeneratedContent::Text(text.clone())),
            text: Some(text),
            assets: Vec::new(),
            diagnostics,
            mime_type: Some(crate::semantic_mime_type(self.format).to_string()),
            checksum_sha256: Some(checksum),
            size_bytes: Some(size_bytes),
        })
    }

    pub fn convert_all(doc: &Document) -> Result<Vec<(OutputFormat, String)>> {
        let mut results = Vec::new();
        for &format in OutputFormat::all() {
            let converter = DocumentConverter::new(format);
            let output = converter.convert(doc)?;
            results.push((format, output));
        }
        Ok(results)
    }

    /// Convert a raw LaTeX string to the target format.
    /// Wraps the LaTeX into a minimal Document AST, then converts.
    pub fn convert_latex_string(latex: &str, format: OutputFormat) -> Result<String> {
        let doc = Document {
            metadata: latexsnipper_ast::Metadata::default(),
            pages: vec![Page {
                width: 0.0,
                height: 0.0,
                blocks: vec![Block::Formula(FormulaBlock {
                    formula: Formula {
                        source: FormulaSource::Latex(latex.to_string()),
                        display_mode: true,
                        confidence: 1.0,
                        source_info: None,
                        layout: None,
                    },
                    label: None,
                    number: None,
                    environment: None,
                    geometry: None,
                    source: None,
                })],
                page_number: None,
                layout: None,
                background_asset_id: None,
            }],
            assets: Vec::new(),
            diagnostics: Vec::new(),
            id_gen: NodeIdGenerator::new(),
            schema_version: "1.0.0".to_string(),
            notes: Vec::new(),
            outline: None,
        };
        DocumentConverter::new(format).convert(&doc)
    }

    /// Parse a MathML XML string, convert to LaTeX, then to the target format.
    pub fn convert_mathml_string(mathml: &str, format: OutputFormat) -> Result<String> {
        let latex = crate::mathml_parser::parse_mathml_to_latex(mathml)
            .map_err(latexsnipper_foundation::SnipperError::Conversion)?;
        Self::convert_latex_string(&latex, format)
    }

    /// Parse an OMML XML string, convert to LaTeX, then to the target format.
    pub fn convert_omml_string(omml: &str, format: OutputFormat) -> Result<String> {
        let latex = crate::omml_parser::parse_omml_to_latex(omml)
            .map_err(latexsnipper_foundation::SnipperError::Conversion)?;
        Self::convert_latex_string(&latex, format)
    }

    /// Parse a Typst math string, convert to LaTeX, then to the target format.
    pub fn convert_typst_string(typst: &str, format: OutputFormat) -> Result<String> {
        let latex = crate::typst_parser::parse_typst_to_latex(typst);
        Self::convert_latex_string(&latex, format)
    }

    /// Parse a Markdown string (with $...$ and $$...$$ math) to the target format.
    pub fn convert_markdown_string(md: &str, format: OutputFormat) -> Result<String> {
        let doc = crate::markdown_parser::parse_markdown_to_document(md);
        DocumentConverter::new(format).convert(&doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::DocumentBuilder;

    fn test_doc() -> Document {
        DocumentBuilder::new()
            .page(800.0, 600.0, |page| {
                page.heading(1, "Math Document");
                page.paragraph(|p| {
                    p.text("The equation ");
                    p.formula("\\frac{a}{b}");
                    p.text(" is important.");
                });
                page.display_formula("E = mc^2");
                page.unordered_list(|l| {
                    l.text_item("Item 1");
                    l.text_item("Item 2");
                });
            })
            .build()
    }

    #[test]
    fn convert_to_latex() {
        let doc = test_doc();
        let converter = DocumentConverter::new(OutputFormat::Latex);
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("\\section{Math Document}"));
        assert!(result.contains("\\frac{a}{b}"));
    }

    #[test]
    fn formula_layout_adapter_supports_all_formula_sources() {
        let sources = [
            FormulaSource::Latex("\\frac{a}{b}".to_string()),
            FormulaSource::MathML("<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>".to_string()),
            FormulaSource::Typst("frac(a, b)".to_string()),
        ];
        for source in sources {
            let formula = Formula {
                source,
                display_mode: true,
                confidence: 1.0,
                source_info: None,
                layout: None,
            };
            let layout = DocumentConverter::formula_layout(&formula).unwrap();
            assert_eq!(layout.canonical_latex(), "\\frac{a}{b}");
            assert!(
                DocumentConverter::convert_formula_layout(&layout, OutputFormat::MathML)
                    .unwrap()
                    .contains("mfrac")
            );
        }
    }

    #[test]
    fn convert_to_markdown() {
        let doc = test_doc();
        let converter = DocumentConverter::new(OutputFormat::MarkdownBlock);
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("# Math Document"));
        assert!(result.contains("\\frac{a}{b}"));
        assert!(result.contains("- Item 1"));
    }

    #[test]
    fn convert_to_typst() {
        let doc = test_doc();
        let converter = DocumentConverter::new(OutputFormat::Typst);
        let result = converter.convert(&doc).unwrap();
        assert!(result.contains("Math Document"));
        assert!(result.contains("frac(a, b)") || result.contains("(a)/(b)"));
    }

    #[test]
    fn convert_all_formats() {
        let doc = test_doc();
        let results = DocumentConverter::convert_all(&doc).unwrap();
        assert_eq!(results.len(), 9);
        for (format, output) in &results {
            assert!(!output.is_empty(), "Empty output for {:?}", format);
        }
    }
}
