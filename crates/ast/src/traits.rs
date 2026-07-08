use crate::{
    ConversionContext, Document, ExportArtifact, ExportOptions, ImportOptions,
    InputSourceDescriptor, RenderOptions,
};

// ---------------------------------------------------------------------------
// Importer
// ---------------------------------------------------------------------------

/// Converts an external format (DOCX, PDF, HTML, etc.) into a Document AST.
pub trait Importer {
    fn input_format(&self) -> &str;
    fn import(
        &self,
        source: &InputSourceDescriptor,
        options: &ImportOptions,
    ) -> Result<Document, String>;
}

// ---------------------------------------------------------------------------
// SemanticConverter
// ---------------------------------------------------------------------------

/// Converts a Document AST to a semantic text format (LaTeX, Markdown, etc.).
pub trait SemanticConverter {
    fn target_format(&self) -> &str;
    fn convert(&self, doc: &Document, ctx: &ConversionContext) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// Exporter
// ---------------------------------------------------------------------------

/// Exports a Document AST to a file/visual format (SVG, PDF, DOCX fragment, etc.).
pub trait Exporter {
    fn target_format(&self) -> &str;
    fn export(
        &self,
        doc: &Document,
        options: &ExportOptions,
    ) -> Result<ExportArtifact, String>;
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// Renders a visual preview of a Document (for screen display, not for editing).
pub trait Renderer {
    fn render_preview(
        &self,
        doc: &Document,
        options: &RenderOptions,
    ) -> Result<ExportArtifact, String>;
}

// ---------------------------------------------------------------------------
// OfficeAdapter
// ---------------------------------------------------------------------------

/// Adapter for Office (Word/PowerPoint/Excel) read/insert operations.
pub trait OfficeAdapter {
    fn read_selection(&self) -> Result<InputSourceDescriptor, String>;
    fn insert_document(&self, doc: &Document, kind: &str) -> Result<(), String>;
    fn insert_artifact(
        &self,
        artifact: &ExportArtifact,
        kind: &str,
    ) -> Result<(), String>;
}
