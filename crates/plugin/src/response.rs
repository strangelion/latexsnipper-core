use std::collections::HashMap;
use latexsnipper_ast::Document;

/// A response from a plugin.
#[derive(Debug, Clone)]
pub struct PluginResponse {
    /// The processed document.
    pub document: Document,

    /// Additional metadata from the plugin.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PluginResponse {
    /// Create a new response with the given document.
    pub fn new(document: Document) -> Self {
        Self {
            document,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the response.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Get a metadata value.
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }
}
