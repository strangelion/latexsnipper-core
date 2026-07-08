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

/// Fidelity level for format conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FidelityLevel {
    Lossless,
    MostlyLossless,
    SemanticOnly,
    VisualOnly,
    BestEffort,
}

/// Capability information for a format conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatCapability {
    pub input: Option<String>,
    pub output: Option<String>,
    pub supports_formula: bool,
    pub supports_table: bool,
    pub supports_image: bool,
    pub supports_svg: bool,
    pub supports_style: bool,
    pub supports_layout: bool,
    pub supports_office_objects: bool,
    pub fidelity: FidelityLevel,
    pub notes: Vec<String>,
}
