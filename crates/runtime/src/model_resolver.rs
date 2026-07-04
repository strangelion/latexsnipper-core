use crate::model_handle::ModelHandle;
use latexsnipper_foundation::{Result, SnipperError};
use std::path::PathBuf;
use std::sync::Arc;

/// A logical model identifier.
#[derive(Debug, Clone)]
pub struct ModelId {
    pub category: String,
    pub variant: String,
}

impl ModelId {
    pub fn new(category: impl Into<String>, variant: impl Into<String>) -> Self {
        Self {
            category: category.into(),
            variant: variant.into(),
        }
    }

    /// Create from a composite key like "formula-det/yolov8-mfd".
    pub fn from_composite_key(composite_key: &str) -> Self {
        let parts: Vec<&str> = composite_key.splitn(2, '/').collect();
        Self {
            category: parts[0].to_string(),
            variant: parts.get(1).unwrap_or(&"").to_string(),
        }
    }

    pub fn composite_key(&self) -> String {
        if self.variant.is_empty() {
            self.category.clone()
        } else {
            format!("{}/{}", self.category, self.variant)
        }
    }
}

/// Resolves logical model identifiers to concrete ModelHandles.
///
/// Native: ModelId → file path → ModelHandle::with_path
/// WASM: ModelId → model_store bytes → ModelHandle::with_bytes
pub trait ModelResolver: Send + Sync {
    /// Resolve a model ID to a ModelHandle.
    fn resolve(&self, id: &ModelId) -> Result<ModelHandle>;

    /// Check if a model is available.
    fn is_available(&self, id: &ModelId) -> bool;
}

/// Filesystem-based model resolver (native).
pub struct FsModelResolver {
    models_dir: PathBuf,
}

impl FsModelResolver {
    pub fn new(models_dir: PathBuf) -> Self {
        Self { models_dir }
    }
}

impl ModelResolver for FsModelResolver {
    fn resolve(&self, id: &ModelId) -> Result<ModelHandle> {
        let cat_dir = self.models_dir.join(&id.category);
        let variant_dir = cat_dir.join(&id.variant);

        // Try common model file names
        let candidates = [
            variant_dir.join("model.onnx"),
            variant_dir.join("model_int8.onnx"),
            variant_dir.join("inference.onnx"),
            variant_dir.join(format!("{}.onnx", id.category)),
        ];

        for path in &candidates {
            if path.exists() {
                return Ok(ModelHandle::with_path(
                    id.composite_key(),
                    path.clone(),
                ));
            }
        }

        // Try any .onnx file in the variant directory
        if let Ok(entries) = std::fs::read_dir(&variant_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("onnx") {
                    return Ok(ModelHandle::with_path(id.composite_key(), path));
                }
            }
        }

        Err(SnipperError::Model(format!(
            "Model not found: {}",
            id.composite_key()
        )))
    }

    fn is_available(&self, id: &ModelId) -> bool {
        self.resolve(id).is_ok()
    }
}

/// In-memory model resolver (WASM).
pub struct MemoryModelResolver {
    store: std::sync::Mutex<HashMap<String, Vec<u8>>>,
}

impl MemoryModelResolver {
    pub fn new() -> Self {
        Self {
            store: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Store a model by key.
    pub fn store(&self, key: impl Into<String>, bytes: Vec<u8>) {
        if let Ok(mut store) = self.store.lock() {
            store.insert(key.into(), bytes);
        }
    }

    /// Check if a model exists.
    pub fn has(&self, key: &str) -> bool {
        self.store
            .lock()
            .map(|s| s.contains_key(key))
            .unwrap_or(false)
    }

    /// Get a model by key.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.store.lock().ok()?.get(key).cloned()
    }

    /// List all loaded model keys.
    pub fn list(&self) -> Vec<String> {
        self.store
            .lock()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default()
    }
}

impl Default for MemoryModelResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelResolver for MemoryModelResolver {
    fn resolve(&self, id: &ModelId) -> Result<ModelHandle> {
        let key = id.composite_key();

        // Try exact key first
        if let Some(bytes) = self.get(&key) {
            return Ok(ModelHandle::with_bytes(key, bytes));
        }

        // Try category only
        if let Some(bytes) = self.get(&id.category) {
            return Ok(ModelHandle::with_bytes(key, bytes));
        }

        // Try with common suffixes
        for suffix in &["model.onnx", "model_int8.onnx", "inference.onnx"] {
            let full_key = format!("{}/{}", key, suffix);
            if let Some(bytes) = self.get(&full_key) {
                return Ok(ModelHandle::with_bytes(key, bytes));
            }
        }

        Err(SnipperError::Model(format!(
            "Model not found in memory store: {}",
            key
        )))
    }

    fn is_available(&self, id: &ModelId) -> bool {
        self.has(&id.composite_key()) || self.has(&id.category)
    }
}

use std::collections::HashMap;

/// A shared model resolver that can be used across the pipeline.
pub type SharedModelResolver = Arc<dyn ModelResolver>;
