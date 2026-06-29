use latexsnipper_ast::Document;
use latexsnipper_foundation::Result;

use crate::request::PluginRequest;
use crate::response::PluginResponse;

/// Trait for extending Core capabilities with standard interfaces.
///
/// Plugins can:
/// - Post-process OCR results
/// - Add new detection/recognition capabilities
/// - Transform documents
/// - Add custom export formats
///
/// Plugins are isolated — one plugin's failure doesn't affect others.
pub trait Plugin: Send + Sync {
    /// Plugin name (unique identifier).
    fn name(&self) -> &str;

    /// Plugin version.
    fn version(&self) -> &str;

    /// Initialize the plugin.
    /// Called once when the plugin is registered.
    fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Handle a request and return a response.
    fn handle(&self, request: &PluginRequest) -> Result<PluginResponse>;

    /// Cleanup resources.
    /// Called when the plugin is unregistered.
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A simple plugin that transforms documents.
pub struct TransformPlugin {
    name: String,
    version: String,
    transform: Box<dyn Fn(&mut Document) -> Result<()> + Send + Sync>,
}

impl TransformPlugin {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        transform: impl Fn(&mut Document) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            transform: Box::new(transform),
        }
    }
}

impl Plugin for TransformPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn handle(&self, request: &PluginRequest) -> Result<PluginResponse> {
        let mut doc = request.document.clone();
        (self.transform)(&mut doc)?;

        Ok(PluginResponse {
            document: doc,
            metadata: request.metadata.clone(),
        })
    }
}
