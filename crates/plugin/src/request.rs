use latexsnipper_ast::Document;
use std::collections::HashMap;

use crate::manifest::PluginHook;

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

    /// Create a request for a typed lifecycle hook.
    pub fn for_hook(hook: PluginHook, document: Document) -> Self {
        Self::new(hook_label(hook), document)
    }

    /// Resolve the legacy action label to a typed lifecycle hook when known.
    pub fn hook(&self) -> Option<PluginHook> {
        parse_hook(&self.action)
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

fn hook_label(hook: PluginHook) -> &'static str {
    match hook {
        PluginHook::BeforeImport => "before_import",
        PluginHook::AfterImport => "after_import",
        PluginHook::BeforeRecognition => "before_recognition",
        PluginHook::AfterRecognition => "after_recognition",
        PluginHook::BeforeConversion => "before_conversion",
        PluginHook::AfterConversion => "after_conversion",
        PluginHook::BeforeExport => "before_export",
        PluginHook::AfterExport => "after_export",
        PluginHook::Validate => "validate",
        PluginHook::RegisterImporter => "register_importer",
        PluginHook::RegisterExporter => "register_exporter",
        PluginHook::RegisterRuntime => "register_runtime",
        PluginHook::RegisterModelAdapter => "register_model_adapter",
    }
}

fn parse_hook(action: &str) -> Option<PluginHook> {
    match action {
        "before_import" => Some(PluginHook::BeforeImport),
        "after_import" => Some(PluginHook::AfterImport),
        "before_recognition" => Some(PluginHook::BeforeRecognition),
        "after_recognition" => Some(PluginHook::AfterRecognition),
        "before_conversion" => Some(PluginHook::BeforeConversion),
        "after_conversion" => Some(PluginHook::AfterConversion),
        "before_export" => Some(PluginHook::BeforeExport),
        "after_export" | "post_process" | "transform" => Some(PluginHook::AfterExport),
        "validate" => Some(PluginHook::Validate),
        "register_importer" => Some(PluginHook::RegisterImporter),
        "register_exporter" => Some(PluginHook::RegisterExporter),
        "register_runtime" => Some(PluginHook::RegisterRuntime),
        "register_model_adapter" => Some(PluginHook::RegisterModelAdapter),
        _ => None,
    }
}
