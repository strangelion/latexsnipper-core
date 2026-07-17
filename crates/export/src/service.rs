use latexsnipper_ast::{Diagnostic, Document, ExportArtifact, GeneratedContent};
use latexsnipper_foundation::{Result, SnipperError};
use sha2::{Digest, Sha256};

use crate::bundle::{RenderBundle, RenderPreference};
use crate::generator::Generator;
use crate::pdf::PdfGenerator;
use crate::png::PngGenerator;
use crate::render_tree::RenderTree;
use crate::svg::SvgGenerator;
use crate::svg_policy::{normalize_svg, SvgContentPolicy};
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
    fn build_artifact(
        format: VisualFormat,
        content: GeneratedContent,
        diagnostics: Vec<Diagnostic>,
    ) -> ExportArtifact {
        let bytes = content.as_bytes();

        let checksum = format!("{:x}", Sha256::digest(bytes));

        let size_bytes = bytes.len() as u64;

        let text = match &content {
            GeneratedContent::Text(text) => Some(text.clone()),
            GeneratedContent::Binary(_) => None,
        };

        ExportArtifact {
            format: format.extension().to_string(),
            primary_path: None,
            content: Some(content),
            text,
            assets: Vec::new(),
            diagnostics,
            mime_type: Some(format.mime_type().to_string()),
            checksum_sha256: Some(checksum),
            size_bytes: Some(size_bytes),
        }
    }

    /// Export a Document to the specified visual format.
    ///
    /// Returns an `ExportArtifact` with the generated content,
    /// MIME type, and diagnostics.
    pub fn export(doc: &Document, format: VisualFormat) -> Result<ExportArtifact> {
        let tree = RenderTree::from_document(doc);

        let content = match format {
            VisualFormat::Svg => SvgGenerator.generate(&tree)?,
            VisualFormat::Pdf => PdfGenerator.generate(&tree)?,
            VisualFormat::Png => PngGenerator.generate(&tree)?,
            VisualFormat::PlainText => TextGenerator.generate(&tree)?,
        };

        Ok(Self::build_artifact(format, content, tree.diagnostics))
    }

    /// Export a Document to SVG (convenience method).
    pub fn to_svg(doc: &Document) -> Result<ExportArtifact> {
        Self::export(doc, VisualFormat::Svg)
    }

    /// Export a Document to PDF (convenience method).
    pub fn to_pdf(doc: &Document) -> Result<ExportArtifact> {
        Self::export(doc, VisualFormat::Pdf)
    }

    /// Export a Document to PNG (convenience method).
    pub fn to_png(doc: &Document) -> Result<ExportArtifact> {
        Self::export(doc, VisualFormat::Png)
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
            VisualFormat::Png,
            VisualFormat::PlainText,
        ]
    }

    /// Build a portable render bundle from a Document.
    ///
    /// The bundle provides a preferred artifact and optional fallback artifacts
    /// based on the specified render preference.
    pub fn render_bundle(doc: &Document, preference: RenderPreference) -> Result<RenderBundle> {
        let tree = RenderTree::from_document(doc);

        let generated = SvgGenerator.generate(&tree)?;

        let svg = generated.as_text().ok_or_else(|| {
            SnipperError::Export("SVG generator returned binary content".to_string())
        })?;

        Self::render_bundle_from_svg_internal(svg, preference, tree.diagnostics)
    }

    /// Build a portable render bundle from pre-existing SVG content.
    ///
    /// This is the recommended entry point when the caller already has
    /// high-quality SVG (e.g. from MathJax). The SVG is validated,
    /// normalized, and composed into a bundle with the appropriate fallback.
    pub fn render_bundle_from_svg(svg: &str, preference: RenderPreference) -> Result<RenderBundle> {
        Self::render_bundle_from_svg_internal(svg, preference, Vec::new())
    }

    fn render_bundle_from_svg_internal(
        svg: &str,
        preference: RenderPreference,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<RenderBundle> {
        let svg_policy = match preference {
            RenderPreference::VectorOnly => SvgContentPolicy::VectorOnly,
            RenderPreference::Auto | RenderPreference::RasterOnly => {
                SvgContentPolicy::AllowEmbeddedRaster
            }
        };

        let normalized = normalize_svg(svg, svg_policy)?;

        let dimensions = normalized.validation.dimensions;

        match preference {
            RenderPreference::VectorOnly => {
                let preferred = Self::build_artifact(
                    VisualFormat::Svg,
                    GeneratedContent::Text(normalized.svg),
                    diagnostics,
                );

                Ok(RenderBundle {
                    preferred,
                    fallbacks: Vec::new(),
                    dimensions,
                })
            }

            RenderPreference::RasterOnly => {
                let png = PngGenerator::generate_from_svg(&normalized.svg)?;

                let preferred = Self::build_artifact(VisualFormat::Png, png, diagnostics);

                Ok(RenderBundle {
                    preferred,
                    fallbacks: Vec::new(),
                    dimensions,
                })
            }

            RenderPreference::Auto => {
                let png = PngGenerator::generate_from_svg(&normalized.svg)?;

                let preferred = Self::build_artifact(
                    VisualFormat::Svg,
                    GeneratedContent::Text(normalized.svg),
                    diagnostics.clone(),
                );

                let fallback = Self::build_artifact(VisualFormat::Png, png, diagnostics);

                Ok(RenderBundle {
                    preferred,
                    fallbacks: vec![fallback],
                    dimensions,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::DocumentBuilder;

    fn document() -> Document {
        DocumentBuilder::new()
            .page(400.0, 200.0, |page| {
                page.text_paragraph("Hello");
                page.display_formula(r"\frac{a}{b}");
            })
            .build()
    }

    #[test]
    fn pdf_artifact_is_binary_and_self_describing() {
        let artifact = ExportService::to_pdf(&document()).unwrap();
        let bytes = artifact.as_bytes().unwrap();
        assert!(matches!(
            artifact.content,
            Some(GeneratedContent::Binary(_))
        ));
        assert!(artifact.text.is_none());
        assert_eq!(artifact.mime_type.as_deref(), Some("application/pdf"));
        assert_eq!(artifact.size_bytes, Some(bytes.len() as u64));
        assert_eq!(artifact.checksum_sha256.as_deref().map(str::len), Some(64));
        assert!(lopdf::Document::load_mem(bytes).is_ok());
    }

    #[test]
    fn png_is_reported_only_with_real_binary_output() {
        assert!(ExportService::supported_formats().contains(&VisualFormat::Png));
        let artifact = ExportService::to_png(&document()).unwrap();
        assert_eq!(&artifact.as_bytes().unwrap()[..8], b"\x89PNG\r\n\x1a\n");
        assert!(artifact.text.is_none());
    }

    #[test]
    fn svg_uses_dynamic_canvas_and_visual_formula_fallback() {
        let artifact = ExportService::to_svg(&document()).unwrap();
        let svg = artifact.content.as_ref().unwrap().as_text().unwrap();
        assert!(svg.contains("viewBox=\"0 0 400"));
        assert!(svg.contains("id=\"page-1\""));
        assert!(svg.contains("data-latex=\"\\frac{a}{b}\""));
        assert!(svg.contains(">(a)/(b)</text>"));
    }

    #[test]
    fn auto_bundle_has_svg_primary_and_png_fallback() {
        let bundle = ExportService::render_bundle(&document(), RenderPreference::Auto).unwrap();

        assert_eq!(bundle.preferred.format, "svg");

        assert_eq!(bundle.fallbacks.len(), 1);

        assert_eq!(bundle.fallbacks[0].format, "png");

        assert_eq!(
            &bundle.fallbacks[0].as_bytes().unwrap()[..8],
            b"\x89PNG\r\n\x1a\n"
        );
    }

    #[test]
    fn vector_only_bundle_has_no_fallback() {
        let bundle =
            ExportService::render_bundle(&document(), RenderPreference::VectorOnly).unwrap();

        assert_eq!(bundle.preferred.format, "svg");

        assert!(bundle.fallbacks.is_empty());
    }

    #[test]
    fn raster_only_bundle_returns_png() {
        let bundle =
            ExportService::render_bundle(&document(), RenderPreference::RasterOnly).unwrap();

        assert_eq!(bundle.preferred.format, "png");

        assert!(bundle.fallbacks.is_empty());
    }

    #[test]
    fn render_bundle_from_svg_auto() {
        let svg = r#"
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="200"
              height="100"
              viewBox="0 0 200 100">
              <rect width="200" height="100" fill="blue"/>
            </svg>
        "#;

        let bundle = ExportService::render_bundle_from_svg(svg, RenderPreference::Auto).unwrap();

        assert_eq!(bundle.preferred.format, "svg");
        assert_eq!(bundle.fallbacks.len(), 1);
        assert_eq!(bundle.fallbacks[0].format, "png");
        assert!((bundle.dimensions.width_px - 200.0).abs() < f32::EPSILON);
        assert!((bundle.dimensions.height_px - 100.0).abs() < f32::EPSILON);
    }
}
