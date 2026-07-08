use latexsnipper_ast::Document;
use latexsnipper_foundation::Result;

use crate::export_format::ExportFormat;

/// Export artifact containing the exported content.
pub struct ExportArtifact {
    pub content: Vec<u8>,
    pub format: ExportFormat,
    pub mime_type: String,
    pub extension: String,
}

/// Options for export.
pub struct ExportOptions {
    pub include_assets: bool,
    pub assets_dir: Option<String>,
    pub embed_assets: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_assets: false,
            assets_dir: None,
            embed_assets: false,
        }
    }
}

/// Trait for exporting Document to file formats.
pub trait Exporter {
    /// Target export format.
    fn target_format(&self) -> ExportFormat;

    /// Export a Document to the target format.
    fn export(&self, doc: &Document, options: &ExportOptions) -> Result<ExportArtifact>;

    /// Format name (e.g., "pdf", "svg", "png").
    fn name(&self) -> &str;

    /// Output file extension (e.g., "pdf", "svg", "png").
    fn extension(&self) -> &str;

    /// MIME type (e.g., "application/pdf", "image/svg+xml").
    fn mime_type(&self) -> &str;
}