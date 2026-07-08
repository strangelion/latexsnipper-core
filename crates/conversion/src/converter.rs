use latexsnipper_ast::Document;
use latexsnipper_foundation::Result;

/// Trait for converting Document AST to a target format string.
///
/// Unlike `syntax::Renderer` which handles syntax-level rendering,
/// `Converter` handles format-level transformation (e.g., AST → OMML XML).
///
/// NOTE: Image asset resolution is now handled via `asset_helper::resolve_asset_ref`
///   and friends, which are called by all format converters.
///
/// TODO(phase4): deprecate this trait in favor of `crate::SemanticConverter` from the AST crate,
///   which provides richer context via `ConversionContext` and works alongside `Exporter`/`Renderer`.
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
