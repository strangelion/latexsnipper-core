use crate::render_tree::RenderTree;
use latexsnipper_foundation::Result;

/// Trait for generating output from a RenderTree.
pub trait Generator {
    /// Generate output from a RenderTree.
    fn generate(&self, tree: &RenderTree) -> Result<String>;

    /// Get the file extension for this format.
    fn extension(&self) -> &str;

    /// Get the MIME type for this format.
    fn mime_type(&self) -> &str;

    /// Get the name of this generator.
    fn name(&self) -> &str;
}
