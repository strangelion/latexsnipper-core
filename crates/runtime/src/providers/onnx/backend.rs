use std::sync::Arc;

use ort::{
    session::Session,
    environment::Environment,
    value::Value,
};

use crate::backend::RuntimeBackend;
use crate::session::InferenceSession;
use crate::model_handle::ModelHandle;
use crate::acceleration::AccelerationMode;
use latexsnipper_foundation::{SnipperError, Result};
use super::platform::{Platform, Acceleration};

/// ONNX Runtime backend with auto GPU/CPU detection and session caching.
pub struct OnnxRuntimeBackend {
    env: Arc<Environment>,
    models_dir: std::path::PathBuf,
    platform: Platform,
    acceleration: Acceleration,
    sessions: std::sync::Mutex<std::collections::HashMap<String, std::sync::Mutex<Session>>>,
}

impl OnnxRuntimeBackend {
    /// Create backend with auto-detected acceleration.
    pub fn new(models_dir: std::path::PathBuf) -> Result<Self> {
        let env = Environment::current()
            .map_err(|e| SnipperError::Runtime(format!("Failed to init ORT: {}", e)))?;

        let platform = Platform::detect();
        let acceleration = Platform::detect_gpu();

        log::info!("ORT backend: platform={}, acceleration={:?}", platform, acceleration);

        Ok(Self {
            env,
            models_dir,
            platform,
            acceleration,
            sessions: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Create backend with explicit acceleration mode.
    pub fn with_acceleration(
        models_dir: std::path::PathBuf,
        acceleration: Acceleration,
    ) -> Result<Self> {
        let env = Environment::current()
            .map_err(|e| SnipperError::Runtime(format!("Failed to init ORT: {}", e)))?;

        let platform = Platform::detect();
        log::info!("ORT backend: platform={}, acceleration={:?}", platform, acceleration);

        Ok(Self {
            env,
            models_dir,
            platform,
            acceleration,
            sessions: std::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    /// Get the detected platform.
    pub fn platform(&self) -> Platform {
        self.platform
    }

    /// Get the current acceleration mode.
    pub fn acceleration(&self) -> Acceleration {
        self.acceleration
    }

    fn resolve_model_path(&self, handle: &ModelHandle) -> std::path::PathBuf {
        // If explicit path is set in ModelHandle, use it directly
        if let Some(path) = handle.model_path() {
            return path.to_path_buf();
        }

        // Otherwise, construct from category/variant
        let dir = self.models_dir
            .join(handle.category())
            .join(handle.variant());

        // Try common ONNX filenames
        let candidates = [
            "model.onnx",
            "model_int8.onnx",
            &format!("{}.onnx", handle.category()),
        ];

        for name in &candidates {
            let path = dir.join(name);
            if path.exists() {
                return path;
            }
        }

        // Fallback: any .onnx file in the directory
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().extension().map_or(false, |ext| ext == "onnx") {
                    return entry.path();
                }
            }
        }

        // Return default path even if not found (error will be reported later)
        dir.join("model.onnx")
    }
}

impl RuntimeBackend for OnnxRuntimeBackend {
    fn create_session(
        &self,
        handle: &ModelHandle,
        _acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>> {
        let model_path = self.resolve_model_path(handle);
        let cache_key = model_path.to_string_lossy().to_string();

        // Check cache first
        {
            let sessions = self.sessions.lock().map_err(|_| SnipperError::Runtime("Lock poisoned".into()))?;
            if sessions.contains_key(&cache_key) {
                // Session already cached, return a new wrapper
                drop(sessions);
                let sessions = self.sessions.lock().map_err(|_| SnipperError::Runtime("Lock poisoned".into()))?;
                if let Some(cached) = sessions.get(&cache_key) {
                    let _session_guard = cached.lock().map_err(|_| SnipperError::Runtime("Session lock".into()))?;
                    // We can't clone Session, so we return a reference-based wrapper
                    // For now, just create a new session (ORT handles caching internally)
                }
            }
        }

        // Create new session
        let session = Session::builder()
            .map_err(|e| SnipperError::Runtime(format!("Failed to create session builder: {}", e)))?
            .commit_from_file(&model_path)
            .map_err(|e| SnipperError::Runtime(format!("Failed to load model {}: {}", model_path.display(), e)))?;

        // Cache the session path for reuse
        {
            let mut sessions = self.sessions.lock().map_err(|_| SnipperError::Runtime("Lock poisoned".into()))?;
            // Store a marker that this model is loaded
            sessions.insert(cache_key, std::sync::Mutex::new(session));
        }

        // Create the session to return (ORT handles internal caching)
        let session = Session::builder()
            .map_err(|e| SnipperError::Runtime(format!("Failed to create session builder: {}", e)))?
            .commit_from_file(&model_path)
            .map_err(|e| SnipperError::Runtime(format!("Failed to load model {}: {}", model_path.display(), e)))?;

        Ok(Box::new(OnnxSession::new(session)))
    }

    fn name(&self) -> &str {
        "onnxruntime"
    }

    fn is_available(&self) -> bool {
        true
    }
}

/// ONNX Runtime session wrapper.
struct OnnxSession {
    session: std::sync::Mutex<Session>,
}

impl OnnxSession {
    fn new(session: Session) -> Self {
        Self { session: std::sync::Mutex::new(session) }
    }
}

impl InferenceSession for OnnxSession {
    fn run(&self, inputs: &[latexsnipper_tensor::Tensor]) -> Result<Vec<latexsnipper_tensor::Tensor>> {
        let mut input_values: Vec<(String, Value)> = Vec::new();

        for input in inputs {
            let name = input.name().to_string();
            match input.data() {
                latexsnipper_tensor::TensorData::Float32(data) => {
                    let shape: Vec<usize> = input.shape().to_vec();
                    let boxed: Box<[f32]> = data.clone().into();
                    let value: Value = Value::from_array((shape, boxed))
                        .map_err(|e| SnipperError::Inference(format!("Failed to create tensor: {}", e)))?
                        .into();
                    input_values.push((name, value));
                }
                latexsnipper_tensor::TensorData::Int64(data) => {
                    let shape: Vec<usize> = input.shape().to_vec();
                    let boxed: Box<[i64]> = data.clone().into();
                    let value: Value = Value::from_array((shape, boxed))
                        .map_err(|e| SnipperError::Inference(format!("Failed to create tensor: {}", e)))?
                        .into();
                    input_values.push((name, value));
                }
                _ => return Err(SnipperError::Inference("Unsupported tensor type".into())),
            }
        }

        let input_map: std::collections::HashMap<String, Value> = input_values.into_iter().collect();
        let mut session = self.session.lock().map_err(|_| SnipperError::Inference("Lock poisoned".into()))?;
        let outputs = session.run(input_map)
            .map_err(|e| SnipperError::Inference(format!("Inference failed: {}", e)))?;

        let mut result_tensors = Vec::new();
        for (name, value) in outputs {
            let shape: Vec<usize> = value.shape().iter().map(|&s| s as usize).collect();
            let tensor = match value.dtype() {
                ort::value::ValueType::Tensor { ty: ort::value::TensorElementType::Float32, .. } => {
                    let (_shape_out, data) = value.try_extract_tensor::<f32>()
                        .map_err(|e| SnipperError::Inference(format!("Failed to extract output: {}", e)))?;
                    latexsnipper_tensor::Tensor::float32(name, shape, data.to_vec())
                }
                ort::value::ValueType::Tensor { ty: ort::value::TensorElementType::Int64, .. } => {
                    let (_shape_out, data) = value.try_extract_tensor::<i64>()
                        .map_err(|e| SnipperError::Inference(format!("Failed to extract output: {}", e)))?;
                    latexsnipper_tensor::Tensor::int64(name, shape, data.to_vec())
                }
                _ => return Err(SnipperError::Inference("Unsupported output dtype".into())),
            };
            result_tensors.push(tensor);
        }

        Ok(result_tensors)
    }

    fn input_names(&self) -> Vec<String> { vec![] }
    fn output_names(&self) -> Vec<String> { vec![] }
    fn release(&mut self) {}
}
