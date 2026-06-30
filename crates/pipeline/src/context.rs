use std::collections::HashMap;
use latexsnipper_ast::Document;
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::RuntimeBackend;

/// Context passed through the pipeline.
/// Each node reads from and writes to this context.
pub struct PipelineContext {
    /// The input image (if any).
    pub image: Option<SnipperImage>,
    /// The document being built.
    pub document: Document,
    /// Key-value metadata for passing data between nodes.
    pub metadata: HashMap<String, serde_json::Value>,
    /// Whether the pipeline was cancelled.
    pub cancelled: bool,
    /// Models directory path.
    pub models_dir: Option<std::path::PathBuf>,
}

impl PipelineContext {
    pub fn new() -> Self {
        Self {
            image: None,
            document: Document::new(),
            metadata: HashMap::new(),
            cancelled: false,
            models_dir: None,
        }
    }

    pub fn with_image(image: SnipperImage) -> Self {
        let mut ctx = Self::new();
        ctx.image = Some(image);
        ctx
    }

    pub fn with_models_dir(models_dir: std::path::PathBuf) -> Self {
        let mut ctx = Self::new();
        ctx.models_dir = Some(models_dir);
        ctx
    }

    /// Set a metadata value.
    pub fn set(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Get a metadata value.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }

    /// Cancel the pipeline.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }
}

impl Default for PipelineContext {
    fn default() -> Self {
        Self::new()
    }
}
