use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Mutex;

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_runtime::{AccelerationMode, InferenceSession, ModelHandle, RuntimeBackend};
use tract_onnx::prelude::*;

use crate::session::TractSession;

/// A `RuntimeBackend` implementation using the `tract` pure-Rust ONNX runtime.
///
/// Unlike `OnnxRuntimeBackend`, this does not depend on the ONNX Runtime C++
/// library and can compile to `wasm32-unknown-unknown`.
pub struct TractBackend {
    models_dir: Option<PathBuf>,
    session_cache: Mutex<HashMap<String, std::sync::Arc<TractSession>>>,
}

impl TractBackend {
    /// Create a new TractBackend.
    ///
    /// - `Some(path)`: load models from filesystem (native)
    /// - `None`: use byte-based loading only (WASM)
    pub fn new(models_dir: Option<PathBuf>) -> Self {
        Self {
            models_dir,
            session_cache: Mutex::new(HashMap::new()),
        }
    }
}

impl RuntimeBackend for TractBackend {
    fn clear_sessions(&self) {
        match self.session_cache.lock() {
            Ok(mut cache) => {
                cache.clear();
                log::info!("Tract session cache cleared");
            }
            Err(error) => {
                log::error!("Failed to clear Tract session cache: {}", error);
            }
        }
    }

    fn create_session(
        &self,
        handle: &ModelHandle,
        _acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>> {
        let cache_key = handle.id().to_string();

        // Check cache first
        {
            let cache = self.session_cache.lock().map_err(|e| {
                SnipperError::Runtime(format!("Session cache lock poisoned: {}", e))
            })?;
            if let Some(session) = cache.get(&cache_key) {
                return Ok(Box::new(TractSession::clone(session)));
            }
        }

        // Load model bytes
        let model_bytes = if let Some(bytes) = handle.model_bytes() {
            bytes.to_vec()
        } else if let Some(path) = handle.model_path() {
            std::fs::read(path)
                .map_err(|e| SnipperError::Runtime(format!("Failed to read model file: {}", e)))?
        } else if let (Some(models_dir), category, variant) =
            (&self.models_dir, handle.category(), handle.variant())
        {
            // Try to find model by category/variant
            let candidates = [
                models_dir.join(category).join(variant).join("model.onnx"),
                models_dir
                    .join(category)
                    .join(variant)
                    .join(format!("{}.onnx", category)),
                models_dir
                    .join(category)
                    .join(variant)
                    .join("model_int8.onnx"),
            ];
            let mut found = None;
            for path in &candidates {
                if path.exists() {
                    found = Some(std::fs::read(path).map_err(|e| {
                        SnipperError::Runtime(format!("Failed to read model: {}", e))
                    })?);
                    break;
                }
            }
            found.ok_or_else(|| {
                SnipperError::Runtime(format!("Model not found for {}/{}", category, variant))
            })?
        } else {
            return Err(SnipperError::Runtime("No model source available".into()));
        };

        // Load with tract
        let model = onnx()
            .model_for_read(&mut Cursor::new(&model_bytes))
            .map_err(|e| SnipperError::Runtime(format!("Tract model load failed: {}", e)))?;

        let model = model
            .into_optimized()
            .map_err(|e| SnipperError::Runtime(format!("Tract model optimize failed: {}", e)))?;

        let model = model
            .into_runnable()
            .map_err(|e| SnipperError::Runtime(format!("Tract model compile failed: {}", e)))?;

        let session = std::sync::Arc::new(TractSession::new(model));

        // Cache it
        {
            let mut cache = self.session_cache.lock().map_err(|e| {
                SnipperError::Runtime(format!("Session cache lock poisoned: {}", e))
            })?;
            cache.insert(cache_key, session.clone());
        }

        Ok(Box::new(TractSession::clone(&session)))
    }

    fn name(&self) -> &str {
        "tract"
    }

    fn is_available(&self) -> bool {
        true
    }
}
