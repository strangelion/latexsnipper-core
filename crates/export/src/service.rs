use latexsnipper_ast::{Document, ExportArtifact};
use latexsnipper_foundation::Result;

use crate::generator::Generator;
use crate::pdf::PdfGenerator;
use crate::render_tree::RenderTree;
use crate::svg::SvgGenerator;
use crate::text::TextGenerator;

/// Supported visual export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualFormat {
    Svg,
    Pdf,
    Png,
    PlainText,
}

impl VisualFormat {
    /// Try to parse from a string label.
    pub fn from_label(label: &str) -> Option<Self> {
        match label.to_ascii_lowercase().as_str() {
            "svg" => Some(Self::Svg),
            "pdf" => Some(Self::Pdf),
            "png" => Some(Self::Png),
            "text" | "plain" | "plaintext" | "txt" => Some(Self::PlainText),
            _ => None,
        }
    }

    /// File extension for this format.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Pdf => "pdf",
            Self::Png => "png",
            Self::PlainText => "txt",
        }
    }

    /// MIME type for this format.
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Svg => "image/svg+xml",
            Self::Pdf => "application/pdf",
            Self::Png => "image/png",
            Self::PlainText => "text/plain",
        }
    }
}

/// Unified export entry point.
///
/// Takes a `Document` AST and an export format, builds the intermediate
/// `RenderTree`, then dispatches to the appropriate generator.
///
/// # Example
/// ```
/// use latexsnipper_export::{ExportService, VisualFormat};
/// use latexsnipper_ast::DocumentBuilder;
///
/// let doc = DocumentBuilder::new()
///     .page(400.0, 200.0, |page| {
///         page.text_paragraph("Hello, world!");
///     })
///     .build();
/// let artifact = ExportService::export(&doc, VisualFormat::PlainText).unwrap();
/// assert!(artifact.text.unwrap().contains("Hello"));
/// ```
pub struct ExportService;

impl ExportService {
    /// Export a Document to the specified visual format.
    ///
    /// Returns an `ExportArtifact` with the generated content,
    /// MIME type, and diagnostics.
    pub fn export(doc: &Document, format: VisualFormat) -> Result<ExportArtifact> {
        let tree = RenderTree::from_document(doc);

        let content = match format {
            VisualFormat::Svg => SvgGenerator.generate(&tree)?.into_bytes(),
            VisualFormat::Pdf => PdfGenerator.generate(&tree)?.into_bytes(),
            VisualFormat::Png => {
                return Err(latexsnipper_foundation::SnipperError::Export(
                    "PNG export requires SVG→PNG rasterization step. \
                     Use ExportFormat::Svg first then convert with a rasterizer."
                        .into(),
                ));
            }
            VisualFormat::PlainText => TextGenerator.generate(&tree)?.into_bytes(),
        };

        Ok(ExportArtifact {
            format: format.extension().to_string(),
            primary_path: None,
            text: String::from_utf8(content).ok(),
            assets: Vec::new(),
            diagnostics: Vec::new(),
        })
    }

    /// Export a Document to SVG (convenience method).
    pub fn to_svg(doc: &Document) -> Result<ExportArtifact> {
        Self::export(doc, VisualFormat::Svg)
    }

    /// Export a Document to PDF (convenience method).
    pub fn to_pdf(doc: &Document) -> Result<ExportArtifact> {
        Self::export(doc, VisualFormat::Pdf)
    }

    /// Export a Document to plain text (convenience method).
    pub fn to_text(doc: &Document) -> Result<ExportArtifact> {
        Self::export(doc, VisualFormat::PlainText)
    }

    /// List all supported visual formats.
    pub fn supported_formats() -> Vec<VisualFormat> {
        vec![
            VisualFormat::Svg,
            VisualFormat::Pdf,
            VisualFormat::PlainText,
        ]
    }
}
