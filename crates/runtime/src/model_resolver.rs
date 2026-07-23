use crate::model_handle::ModelHandle;
use latexsnipper_foundation::{Result, SnipperError};
use std::collections::HashMap;
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

    /// Resolve a named artifact within a model package.
    fn resolve_artifact(&self, id: &ModelId, artifact: &str) -> Result<ModelHandle> {
        let key = format!("{}/{}", id.composite_key(), artifact);
        self.resolve(&ModelId::from_composite_key(&key))
    }

    /// Read a UTF-8 artifact such as a tokenizer or model configuration.
    fn read_text_artifact(&self, id: &ModelId, artifact: &str) -> Result<String> {
        let handle = self.resolve_artifact(id, artifact)?;
        let bytes = handle.model_bytes().ok_or_else(|| {
            SnipperError::Model(format!(
                "Text artifact is not available as bytes: {}/{}",
                id.composite_key(),
                artifact
            ))
        })?;
        String::from_utf8(bytes.to_vec()).map_err(|error| {
            SnipperError::Model(format!(
                "Text artifact is not valid UTF-8 ({}/{}): {}",
                id.composite_key(),
                artifact,
                error
            ))
        })
    }

    /// List the artifacts currently available for a model package.
    fn list_artifacts(&self, _id: &ModelId) -> Vec<String> {
        Vec::new()
    }
}

/// Normalize model-store keys across browser and native callers.
pub fn normalize_key(key: &str) -> String {
    key.replace('\\', "/").trim_start_matches('/').to_string()
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
                return Ok(ModelHandle::with_path(id.composite_key(), path.clone()));
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

    fn resolve_artifact(&self, id: &ModelId, artifact: &str) -> Result<ModelHandle> {
        let path = self
            .models_dir
            .join(&id.category)
            .join(&id.variant)
            .join(artifact);
        if path.exists() {
            Ok(ModelHandle::with_path(
                format!("{}/{}", id.composite_key(), artifact),
                path,
            ))
        } else {
            Err(SnipperError::Model(format!(
                "Artifact not found: {}/{}",
                id.composite_key(),
                artifact
            )))
        }
    }

    fn read_text_artifact(&self, id: &ModelId, artifact: &str) -> Result<String> {
        let path = self
            .models_dir
            .join(&id.category)
            .join(&id.variant)
            .join(artifact);
        std::fs::read_to_string(&path).map_err(|error| {
            SnipperError::Model(format!(
                "Failed to read {}/{}: {}",
                id.composite_key(),
                artifact,
                error
            ))
        })
    }

    fn list_artifacts(&self, id: &ModelId) -> Vec<String> {
        let variant_dir = self.models_dir.join(&id.category).join(&id.variant);
        std::fs::read_dir(variant_dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect()
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
            store.insert(normalize_key(&key.into()), bytes);
        }
    }

    /// Check if a model exists.
    pub fn has(&self, key: &str) -> bool {
        self.store
            .lock()
            .map(|s| s.contains_key(&normalize_key(key)))
            .unwrap_or(false)
    }

    /// Get a model by key.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.store.lock().ok()?.get(&normalize_key(key)).cloned()
    }

    /// List all loaded model keys.
    pub fn list(&self) -> Vec<String> {
        self.store
            .lock()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Remove every loaded artifact.
    pub fn clear(&self) {
        if let Ok(mut store) = self.store.lock() {
            store.clear();
        }
    }

    /// Remove one loaded artifact.
    pub fn remove(&self, key: &str) -> bool {
        self.store
            .lock()
            .map(|mut store| store.remove(&normalize_key(key)).is_some())
            .unwrap_or(false)
    }

    /// Return the number of loaded artifacts.
    pub fn len(&self) -> usize {
        self.store.lock().map(|store| store.len()).unwrap_or(0)
    }

    /// Return whether the store has no loaded artifacts.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn attach_package_input_shape(&self, id: &ModelId, handle: ModelHandle) -> ModelHandle {
        let config_key = format!("{}/config.json", id.composite_key());
        let Some(config_bytes) = self.get(&config_key) else {
            return handle;
        };
        let Ok(config) = serde_json::from_slice::<serde_json::Value>(&config_bytes) else {
            return handle;
        };
        let Some(dimensions) = config
            .pointer("/input/shape")
            .and_then(serde_json::Value::as_array)
        else {
            return handle;
        };
        let shape: Option<Vec<usize>> = dimensions
            .iter()
            .map(|dimension| {
                dimension
                    .as_i64()
                    .filter(|value| *value > 0)
                    .and_then(|value| usize::try_from(value).ok())
            })
            .collect();
        if let Some(shape) = shape {
            handle.with_input_shape(shape)
        } else {
            handle
        }
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
            return Ok(self.attach_package_input_shape(id, ModelHandle::with_bytes(key, bytes)));
        }

        // Try category only
        if let Some(bytes) = self.get(&id.category) {
            return Ok(self.attach_package_input_shape(id, ModelHandle::with_bytes(key, bytes)));
        }

        // Try with common suffixes
        for suffix in &["model.onnx", "model_int8.onnx", "inference.onnx"] {
            let full_key = format!("{}/{}", key, suffix);
            if let Some(bytes) = self.get(&full_key) {
                return Ok(self.attach_package_input_shape(id, ModelHandle::with_bytes(key, bytes)));
            }
        }

        Err(SnipperError::Model(format!(
            "Model not found in memory store: {}",
            key
        )))
    }

    fn is_available(&self, id: &ModelId) -> bool {
        let key = id.composite_key();
        self.has(&key)
            || self.has(&id.category)
            || ["model.onnx", "model_int8.onnx", "inference.onnx"]
                .iter()
                .any(|suffix| self.has(&format!("{key}/{suffix}")))
    }

    fn resolve_artifact(&self, id: &ModelId, artifact: &str) -> Result<ModelHandle> {
        let key = format!("{}/{}", id.composite_key(), artifact);
        if let Some(bytes) = self.get(&key) {
            return Ok(self.attach_package_input_shape(id, ModelHandle::with_bytes(key, bytes)));
        }

        Err(SnipperError::Model(format!(
            "Artifact not found in memory store: {}",
            key
        )))
    }

    fn list_artifacts(&self, id: &ModelId) -> Vec<String> {
        let prefix = format!("{}/", id.composite_key());
        self.list()
            .into_iter()
            .filter(|key| key.starts_with(&prefix))
            .map(|key| key[prefix.len()..].to_string())
            .collect()
    }
}

/// A shared model resolver that can be used across the pipeline.
pub type SharedModelResolver = Arc<dyn ModelResolver>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_resolver_attaches_static_input_shape_from_package_config() {
        let resolver = MemoryModelResolver::new();
        let id = ModelId::new("third-party-det", "custom-640");
        resolver.store(
            "third-party-det/custom-640/config.json",
            br#"{"input":{"shape":[1,3,640,960]}}"#.to_vec(),
        );
        resolver.store("third-party-det/custom-640/model.onnx", vec![1, 2, 3]);

        let handle = resolver.resolve_artifact(&id, "model.onnx").unwrap();
        assert_eq!(handle.input_shape(), Some([1, 3, 640, 960].as_slice()));
    }

    #[test]
    fn memory_resolver_preserves_dynamic_package_shapes() {
        let resolver = MemoryModelResolver::new();
        let id = ModelId::new("third-party-det", "dynamic");
        resolver.store(
            "third-party-det/dynamic/config.json",
            br#"{"input":{"shape":[1,3,-1,-1]}}"#.to_vec(),
        );
        resolver.store("third-party-det/dynamic/model.onnx", vec![1, 2, 3]);

        let handle = resolver.resolve_artifact(&id, "model.onnx").unwrap();
        assert_eq!(handle.input_shape(), None);
    }
}
