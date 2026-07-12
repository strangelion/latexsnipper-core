use serde::{Deserialize, Serialize};

use crate::{Diagnostic, ExportedAsset};

// ---------------------------------------------------------------------------
// SemanticFormat — semantic conversion formats (for formula/text conversion)
// ---------------------------------------------------------------------------

/// Semantic conversion formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticFormat {
    Latex,
    LatexDisplay,
    LatexEquation,
    Typst,
    MarkdownInline,
    MarkdownBlock,
    MathML,
    OMML,
    Html,
    PlainText,
    JsonAst,
}

// ---------------------------------------------------------------------------
// ExportFormat — file/export formats (for document export)
// ---------------------------------------------------------------------------

/// File/export formats for document export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    AstJson,
    PlainText,
    Markdown,
    Latex,
    Typst,
    Html,
    MathML,
    OMML,
    Svg,
    Pdf,
    Png,
    Docx,
    Pptx,
    Xlsx,
    OoxmlFragment,
    ClipboardHtml,
    ClipboardRtf,
}

// ---------------------------------------------------------------------------
// TargetFormat — unified target format
// ---------------------------------------------------------------------------

/// Unified target format, combining semantic and export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetFormat {
    Semantic(SemanticFormat),
    Export(ExportFormat),
}

impl From<SemanticFormat> for TargetFormat {
    fn from(format: SemanticFormat) -> Self {
        TargetFormat::Semantic(format)
    }
}

impl From<ExportFormat> for TargetFormat {
    fn from(format: ExportFormat) -> Self {
        TargetFormat::Export(format)
    }
}

// ---------------------------------------------------------------------------
// ExportArtifact — the result of an export operation
// ---------------------------------------------------------------------------

/// Type-safe generated payload.
///
/// Binary formats must never be converted to UTF-8 text. Callers can inspect
/// the variant or use [`GeneratedContent::as_bytes`] for format-agnostic I/O.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum GeneratedContent {
    Text(String),
    Binary(Vec<u8>),
}

impl GeneratedContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Binary(_) => None,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Binary(bytes) => bytes,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Self::Text(text) => text.into_bytes(),
            Self::Binary(bytes) => bytes,
        }
    }

    pub fn is_binary(&self) -> bool {
        matches!(self, Self::Binary(_))
    }
}

/// The output of an Exporter, containing file references, text, assets,
/// and any diagnostics from the export process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportArtifact {
    /// Human-readable name of the export format (e.g. "svg", "pdf", "docx-fragment").
    pub format: String,
    /// Path to the primary output file (if any).
    pub primary_path: Option<String>,
    /// Type-safe primary content for both text and binary formats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<GeneratedContent>,
    /// Text content (for semantic formats like LaTeX, Markdown, etc.).
    ///
    /// Compatibility adapter for pre-2.1 callers. Binary exporters always
    /// leave this field as `None`; new callers should use `content`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Exported asset copies (images, SVGs, etc.).
    #[serde(default)]
    pub assets: Vec<ExportedAsset>,
    /// Diagnostics (warnings, errors) produced during export.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    /// MIME type of the primary content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// SHA-256 checksum of the exact primary content bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum_sha256: Option<String>,
    /// Exact primary content size in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

impl ExportArtifact {
    /// Return the exact primary output bytes without text transcoding.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.content.as_ref().map(GeneratedContent::as_bytes)
    }

    /// Write the primary output to a writer without changing its bytes.
    pub fn write_to(&self, mut writer: impl std::io::Write) -> std::io::Result<u64> {
        let bytes = self.as_bytes().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "artifact has no content")
        })?;
        writer.write_all(bytes)?;
        Ok(bytes.len() as u64)
    }

    /// Write the primary output to a path and record that path on success.
    pub fn write_to_path(&mut self, path: impl AsRef<std::path::Path>) -> std::io::Result<u64> {
        let path = path.as_ref();
        let file = std::fs::File::create(path)?;
        let written = self.write_to(file)?;
        self.primary_path = Some(path.to_string_lossy().into_owned());
        Ok(written)
    }
}

#[cfg(test)]
mod export_artifact_tests {
    use super::*;

    #[test]
    fn binary_content_is_written_byte_for_byte() {
        let expected = vec![0, 0xff, 0xfe, b'%', b'P', b'D', b'F', 0, 0x80];
        let artifact = ExportArtifact {
            format: "pdf".to_string(),
            primary_path: None,
            content: Some(GeneratedContent::Binary(expected.clone())),
            text: None,
            assets: Vec::new(),
            diagnostics: Vec::new(),
            mime_type: Some("application/pdf".to_string()),
            checksum_sha256: None,
            size_bytes: Some(expected.len() as u64),
        };
        let mut written = Vec::new();
        assert_eq!(
            artifact.write_to(&mut written).unwrap(),
            expected.len() as u64
        );
        assert_eq!(written, expected);
        assert!(artifact.text.is_none());
    }
}

// ---------------------------------------------------------------------------
// LossKind — describes what was lost during conversion/export
// ---------------------------------------------------------------------------

/// Categories of information loss during conversion or export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LossKind {
    StyleLoss,
    LayoutLoss,
    AssetRasterized,
    FormulaDowngraded,
    TableStructureLoss,
    OfficeObjectPreviewOnly,
    UnsupportedAnnotation,
}

// ---------------------------------------------------------------------------
// CapabilityMatrix — describes the conversion/export capabilities of the system
// ---------------------------------------------------------------------------

/// Describes what a particular format conversion supports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatCapability {
    /// Input format label (e.g. "AST", "DOCX", "Markdown").
    pub input: Option<String>,
    /// Output format label (e.g. "Markdown", "HTML", "SVG").
    pub output: Option<String>,
    /// Whether the importer/exporter pair is registered and callable.
    pub available: bool,
    pub supports_formula: bool,
    pub supports_table: bool,
    pub supports_image: bool,
    pub supports_svg: bool,
    pub supports_style: bool,
    pub supports_layout: bool,
    pub supports_office_objects: bool,
    pub fidelity: FidelityLevel,
    #[serde(default)]
    pub known_loss: Vec<LossKind>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub required_features: Vec<String>,
    #[serde(default)]
    pub external_dependencies: Vec<String>,
    #[serde(default)]
    pub platform_restrictions: Vec<String>,
    #[serde(default)]
    pub experimental: bool,
}

/// How faithfully the conversion preserves the original content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FidelityLevel {
    Lossless,
    MostlyLossless,
    SemanticOnly,
    VisualOnly,
    BestEffort,
}

/// A matrix of all known conversion/export capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMatrix {
    pub schema_version: String,
    pub entries: Vec<FormatCapability>,
}

impl CapabilityMatrix {
    /// Query the capability for a specific input → output conversion.
    ///
    /// Returns the first matching format capability whose `input` and `output`
    /// fields contain the given labels (case-insensitive substring match).
    pub fn query(&self, input: &str, output: &str) -> Option<&FormatCapability> {
        let input_lower = input.to_lowercase();
        let output_lower = output.to_lowercase();
        self.entries.iter().find(|e| {
            let i_matches = e
                .input
                .as_deref()
                .map(|i| i.to_lowercase().contains(&input_lower))
                .unwrap_or(false);
            let o_matches = e
                .output
                .as_deref()
                .map(|o| o.to_lowercase().contains(&output_lower))
                .unwrap_or(false);
            i_matches && o_matches
        })
    }

    /// Explain the known loss types for a specific input → output conversion.
    ///
    /// Returns a reference to the `known_loss` vector for the matching entry,
    /// or an empty slice if no match is found.
    pub fn explain_loss(&self, input: &str, output: &str) -> &[LossKind] {
        self.query(input, output)
            .map(|e| e.known_loss.as_slice())
            .unwrap_or(&[])
    }
}

// ---------------------------------------------------------------------------
// PdfExportOptions
// ---------------------------------------------------------------------------

/// Mode for PDF export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PdfExportMode {
    /// Reflow content into a new PDF layout.
    Reflow,
    /// Render a visual snapshot of the document.
    VisualRender,
    /// Overlay content onto an existing source PDF.
    OverlayOnSource,
}

/// Options for PDF export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfExportOptions {
    pub mode: PdfExportMode,
    pub preserve_original_background: bool,
    pub embed_fonts: bool,
    pub rasterize_unsupported_assets: bool,
}

// ---------------------------------------------------------------------------
// Conversion/export/import options
// ---------------------------------------------------------------------------

/// Options for importing a document from an external format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOptions {
    pub preserve_assets: bool,
    pub preserve_layout: bool,
    pub page_range: Option<crate::PageRange>,
    #[serde(default)]
    pub ocr_fallback: bool,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub preserve_unknown_parts: bool,
    #[serde(default = "default_max_decompressed_size")]
    pub max_decompressed_size: u64,
    #[serde(default = "default_max_text_size")]
    pub max_text_size: u64,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            preserve_assets: true,
            preserve_layout: true,
            page_range: None,
            ocr_fallback: false,
            strict: false,
            preserve_unknown_parts: false,
            max_decompressed_size: default_max_decompressed_size(),
            max_text_size: default_max_text_size(),
        }
    }
}

fn default_max_decompressed_size() -> u64 {
    512 * 1024 * 1024
}

fn default_max_text_size() -> u64 {
    64 * 1024 * 1024
}

/// Options for exporting a document.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExportOptions {
    pub assets_dir: Option<String>,
    pub embed_assets: bool,
    pub rasterize_vectors: bool,
}

/// Options for rendering a preview.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RenderOptions {
    pub scale: Option<f32>,
    pub page: Option<usize>,
    pub background_color: Option<String>,
}

/// Context passed through a conversion pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversionContext {
    pub asset_base_path: Option<String>,
    pub target_language: Option<String>,
    pub options: std::collections::HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// ModelProviderKind — classification of model execution backends
// ---------------------------------------------------------------------------

/// Classification of where and how a model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelProviderKind {
    LocalOnnx,
    LocalPlugin,
    RemoteApi,
    RemoteVlm,
}

// ---------------------------------------------------------------------------
// ModelCapability — describes the capability of a model/provider
// ---------------------------------------------------------------------------

/// Describes what a model or provider can do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapability {
    pub provider_kind: ModelProviderKind,
    pub tasks: Vec<String>,
    pub input_formats: Vec<String>,
    pub output_schema: Option<serde_json::Value>,
    pub supports_batch: bool,
    pub supports_streaming: bool,
    pub supports_quad: bool,
    pub supports_language_hints: bool,
    pub max_image_pixels: Option<u64>,
    pub max_pages: Option<u32>,
    pub remote_upload_required: bool,
}
