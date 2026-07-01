use latexsnipper_ast::Document;
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use std::collections::HashMap;
use std::sync::Arc;

/// Cached ONNX session for reuse across pipeline nodes.
pub struct CachedSession {
    pub session: Arc<Box<dyn InferenceSession>>,
}

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
    /// Cached ONNX sessions for reuse across nodes.
    pub sessions: HashMap<String, CachedSession>,
}

impl PipelineContext {
    pub fn new() -> Self {
        Self {
            image: None,
            document: Document::new(),
            metadata: HashMap::new(),
            cancelled: false,
            models_dir: None,
            sessions: HashMap::new(),
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

    /// Get a cached session by key.
    pub fn get_session(&self, key: &str) -> Option<Arc<Box<dyn InferenceSession>>> {
        self.sessions.get(key).map(|c| Arc::clone(&c.session))
    }

    /// Cache a session for reuse.
    pub fn cache_session(&mut self, key: impl Into<String>, session: Box<dyn InferenceSession>) {
        self.sessions.insert(
            key.into(),
            CachedSession {
                session: Arc::new(session),
            },
        );
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
