use latexsnipper_ast::Document;
use latexsnipper_foundation::Result;

/// Trait for converting Document AST to a target format string.
///
/// Unlike `syntax::Renderer` which handles syntax-level rendering,
/// `Converter` handles format-level transformation (e.g., AST → OMML XML).
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
