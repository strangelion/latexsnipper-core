use latexsnipper_ast::Document;
use latexsnipper_foundation::Result;

/// Trait for rendering Document AST to syntax string.
pub trait Renderer {
    /// Render a Document to a string.
    fn render(&self, doc: &Document) -> Result<String>;

    /// Get the name of this renderer (e.g., "latex", "typst").
    fn name(&self) -> &str;
}
