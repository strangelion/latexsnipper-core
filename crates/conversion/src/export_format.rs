// Re-export platform-level types from ast to avoid duplicate definitions.
// SemanticFormat, ExportFormat, and TargetFormat are defined locally for now
// since they carry conversion-specific variants that may not apply at the ast level.
pub use latexsnipper_ast::{FidelityLevel, FormatCapability};

use serde::{Deserialize, Serialize};

/// Semantic conversion formats (for formula/text conversion).
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

/// File/export formats (for document export).
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

/// Unified target format.
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
