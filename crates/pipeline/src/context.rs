use latexsnipper_ast::Document;
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::{InferenceSession, RuntimeBackend};
use std::collections::HashMap;
use std::sync::Arc;

/// Cached ONNX session for reuse across pipeline nodes.
pub struct CachedSession {
    pub session: Arc<Box<dyn InferenceSession>>,
}

/// Context passed through the pipeline.
/// Each node reads from and writes to this context.
pub struct PipelineContext {
    /// The input image (if any). For multi-page, this is the current page.
    pub image: Option<SnipperImage>,
    /// All page images (for multi-page PDF input).
    pub page_images: Vec<SnipperImage>,
    /// Current page index (0-based) when processing multi-page input.
    pub current_page: usize,
    /// The document being built.
    pub document: Document,
    /// Key-value metadata for passing data between nodes.
    pub metadata: HashMap<String, serde_json::Value>,
    /// Whether the pipeline was cancelled.
    pub cancelled: bool,
    /// Models directory path.
    pub models_dir: Option<std::path::PathBuf>,
    /// Runtime backend for inference sessions (injected by engine).
    pub backend: Option<Arc<dyn RuntimeBackend>>,
    /// Cached ONNX sessions for reuse across nodes.
    pub sessions: HashMap<String, CachedSession>,
}

impl PipelineContext {
    pub fn new() -> Self {
        Self {
            image: None,
            page_images: Vec::new(),
            current_page: 0,
            document: Document::new(),
            metadata: HashMap::new(),
            cancelled: false,
            models_dir: None,
            backend: None,
            sessions: HashMap::new(),
        }
    }

    pub fn with_image(image: SnipperImage) -> Self {
        let mut ctx = Self::new();
        ctx.image = Some(image);
        ctx
    }

    /// Create context with multiple page images (for PDF input).
    pub fn with_pages(pages: Vec<SnipperImage>) -> Self {
        let mut ctx = Self::new();
        if !pages.is_empty() {
            ctx.image = Some(pages[0].clone());
        }
        ctx.page_images = pages;
        ctx
    }

    pub fn with_models_dir(models_dir: std::path::PathBuf) -> Self {
        let mut ctx = Self::new();
        ctx.models_dir = Some(models_dir);
        ctx
    }

    /// Check if this context has multiple pages.
    pub fn is_multipage(&self) -> bool {
        self.page_images.len() > 1
    }

    /// Get the total number of pages.
    pub fn page_count(&self) -> usize {
        if self.page_images.is_empty() {
            if self.image.is_some() {
                1
            } else {
                0
            }
        } else {
            self.page_images.len()
        }
    }

    /// Set the current page index and update the image reference.
    pub fn set_current_page(&mut self, index: usize) {
        if index < self.page_images.len() {
            self.current_page = index;
            self.image = Some(self.page_images[index].clone());
        }
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
