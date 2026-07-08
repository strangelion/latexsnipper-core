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

/// The output of an Exporter, containing file references, text, assets,
/// and any diagnostics from the export process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportArtifact {
    /// Human-readable name of the export format (e.g. "svg", "pdf", "docx-fragment").
    pub format: String,
    /// Path to the primary output file (if any).
    pub primary_path: Option<String>,
    /// Text content (for semantic formats like LaTeX, Markdown, etc.).
    pub text: Option<String>,
    /// Exported asset copies (images, SVGs, etc.).
    #[serde(default)]
    pub assets: Vec<ExportedAsset>,
    /// Diagnostics (warnings, errors) produced during export.
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportOptions {
    pub preserve_assets: bool,
    pub preserve_layout: bool,
    pub page_range: Option<crate::PageRange>,
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
