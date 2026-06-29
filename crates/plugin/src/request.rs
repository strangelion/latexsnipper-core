use std::collections::HashMap;
use latexsnipper_ast::Document;

/// A request sent to a plugin.
#[derive(Debug, Clone)]
pub struct PluginRequest {
    /// The action to perform (e.g., "post_process", "detect", "transform").
    pub action: String,

    /// The document to process.
    pub document: Document,

    /// Additional metadata for the request.
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PluginRequest {
    /// Create a new request with the given action and document.
    pub fn new(action: impl Into<String>, document: Document) -> Self {
        Self {
            action: action.into(),
            document,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the request.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Get a metadata value.
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }
}
