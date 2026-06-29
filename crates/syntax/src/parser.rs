use latexsnipper_ast::Document;
use latexsnipper_foundation::Result;

/// Trait for parsing syntax into Document AST.
pub trait Parser {
    /// Parse input string into a Document.
    fn parse(&self, input: &str) -> Result<Document>;

    /// Get the name of this parser (e.g., "latex", "omml").
    fn name(&self) -> &str;
}
