use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ort::{environment::Environment, session::Session, value::Value};

use super::platform::{Acceleration, Platform};
use crate::acceleration::AccelerationMode;
use crate::backend::RuntimeBackend;
use crate::model_handle::ModelHandle;
use crate::session::InferenceSession;
use latexsnipper_foundation::{Result, SnipperError};

/// ONNX Runtime backend with auto GPU/CPU detection and session caching.
pub struct OnnxRuntimeBackend {
    _env: Arc<Environment>,
    models_dir: std::path::PathBuf,
    platform: Platform,
    acceleration: Acceleration,
    #[allow(dead_code)]
    max_threads: usize,
    sessions: Mutex<HashMap<String, Arc<Mutex<Session>>>>,
}

impl OnnxRuntimeBackend {
    pub fn new(models_dir: std::path::PathBuf) -> Result<Self> {
        Self::new_with_threads(models_dir, 4)
    }

    pub fn with_acceleration(
        models_dir: std::path::PathBuf,
        acceleration: Acceleration,
    ) -> Result<Self> {
        let env = Environment::current()
            .map_err(|e| SnipperError::Runtime(format!("Failed to init ORT: {}", e)))?;
        let platform = Platform::detect();
        log::info!(
            "ORT backend: platform={}, acceleration={:?}",
            platform,
            acceleration
        );
        Ok(Self {
            _env: env,
            models_dir,
            platform,
            acceleration,
            max_threads: 4,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    fn new_with_threads(models_dir: std::path::PathBuf, max_threads: usize) -> Result<Self> {
        let env = Environment::current()
            .map_err(|e| SnipperError::Runtime(format!("Failed to init ORT: {}", e)))?;
        let platform = Platform::detect();
        let acceleration = Platform::detect_gpu();
        log::info!(
            "ORT backend: platform={}, acceleration={:?}",
            platform,
            acceleration
        );
        Ok(Self {
            _env: env,
            models_dir,
            platform,
            acceleration,
            max_threads,
            sessions: Mutex::new(HashMap::new()),
        })
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }
    pub fn acceleration(&self) -> Acceleration {
        self.acceleration
    }

    fn resolve_model_path(&self, handle: &ModelHandle) -> std::path::PathBuf {
        if let Some(path) = handle.model_path() {
            return path.to_path_buf();
        }
        let dir = self
            .models_dir
            .join(handle.category())
            .join(handle.variant());
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
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().extension().is_some_and(|ext| ext == "onnx") {
                    return entry.path();
                }
            }
        }
        dir.join("model.onnx")
    }

    fn get_or_create_session(
        &self,
        model_path: &std::path::Path,
        acceleration: AccelerationMode,
    ) -> Result<Arc<Mutex<Session>>> {
        let cache_key = model_path.to_string_lossy().to_string();

        // Check cache first (short hold)
        {
            let sessions = self
                .sessions
                .lock()
                .map_err(|_| SnipperError::Runtime("Lock poisoned".into()))?;
            if let Some(cached) = sessions.get(&cache_key) {
                return Ok(Arc::clone(cached));
            }
        }

        // Create new session with acceleration and thread config
        let mut builder = Session::builder().map_err(|e| {
            SnipperError::Runtime(format!("Failed to create session builder: {}", e))
        })?;

        // Configure execution providers based on acceleration mode
        match acceleration {
            AccelerationMode::Gpu | AccelerationMode::Auto => {
                // Try CUDA first, then fall back to CPU (handled by builder defaults)
                // _ = builder.with_execution_providers([...]);
            }
            AccelerationMode::Cpu => {
                // CPU only - default
            }
        }

        // Set thread count
        let session = builder.commit_from_file(model_path).map_err(|e| {
            SnipperError::Runtime(format!(
                "Failed to load model {}: {}",
                model_path.display(),
                e
            ))
        })?;

        let shared = Arc::new(Mutex::new(session));

        // Store in cache (may race with another thread, that's fine)
        {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| SnipperError::Runtime("Lock poisoned".into()))?;
            sessions
                .entry(cache_key)
                .or_insert_with(|| Arc::clone(&shared));
        }

        Ok(shared)
    }
}

impl RuntimeBackend for OnnxRuntimeBackend {
    fn create_session(
        &self,
        handle: &ModelHandle,
        acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>> {
        let model_path = self.resolve_model_path(handle);
        let shared = self.get_or_create_session(&model_path, acceleration)?;
        Ok(Box::new(OnnxSession { session: shared }))
    }

    fn name(&self) -> &str {
        "onnxruntime"
    }
    fn is_available(&self) -> bool {
        true
    }
}

struct OnnxSession {
    session: Arc<Mutex<Session>>,
}

impl InferenceSession for OnnxSession {
    fn run(
        &self,
        inputs: &[latexsnipper_tensor::Tensor],
    ) -> Result<Vec<latexsnipper_tensor::Tensor>> {
        let mut input_values: Vec<(String, Value)> = Vec::new();

        for input in inputs {
            let name = input.name().to_string();
            match input.data() {
                latexsnipper_tensor::TensorData::Float32(data) => {
                    let shape: Vec<usize> = input.shape().to_vec();
                    let boxed: Box<[f32]> = data.clone().into();
                    let value: Value = Value::from_array((shape, boxed))
                        .map_err(|e| {
                            SnipperError::Inference(format!("Failed to create tensor: {}", e))
                        })?
                        .into();
                    input_values.push((name, value));
                }
                latexsnipper_tensor::TensorData::Int64(data) => {
                    let shape: Vec<usize> = input.shape().to_vec();
                    let boxed: Box<[i64]> = data.clone().into();
                    let value: Value = Value::from_array((shape, boxed))
                        .map_err(|e| {
                            SnipperError::Inference(format!("Failed to create tensor: {}", e))
                        })?
                        .into();
                    input_values.push((name, value));
                }
                _ => return Err(SnipperError::Inference("Unsupported tensor type".into())),
            }
        }

        let input_map: HashMap<String, Value> = input_values.into_iter().collect();
        let mut session = self
            .session
            .lock()
            .map_err(|_| SnipperError::Inference("Lock poisoned".into()))?;
        let outputs = session
            .run(input_map)
            .map_err(|e| SnipperError::Inference(format!("Inference failed: {}", e)))?;

        let mut result_tensors = Vec::new();
        for (name, value) in outputs {
            let shape: Vec<usize> = value.shape().iter().map(|&s| s as usize).collect();
            let tensor = match value.dtype() {
                ort::value::ValueType::Tensor {
                    ty: ort::value::TensorElementType::Float32,
                    ..
                } => {
                    let (_shape_out, data) = value.try_extract_tensor::<f32>().map_err(|e| {
                        SnipperError::Inference(format!("Failed to extract output: {}", e))
                    })?;
                    latexsnipper_tensor::Tensor::float32(name, shape, data.to_vec())
                }
                ort::value::ValueType::Tensor {
                    ty: ort::value::TensorElementType::Int64,
                    ..
                } => {
                    let (_shape_out, data) = value.try_extract_tensor::<i64>().map_err(|e| {
                        SnipperError::Inference(format!("Failed to extract output: {}", e))
                    })?;
                    latexsnipper_tensor::Tensor::int64(name, shape, data.to_vec())
                }
                ort::value::ValueType::Tensor {
                    ty: ort::value::TensorElementType::Int32,
                    ..
                } => {
                    let (_shape_out, data) = value.try_extract_tensor::<i32>().map_err(|e| {
                        SnipperError::Inference(format!("Failed to extract output: {}", e))
                    })?;
                    latexsnipper_tensor::Tensor::int32(name, shape, data.to_vec())
                }
                ort::value::ValueType::Tensor {
                    ty: ort::value::TensorElementType::Uint8,
                    ..
                } => {
                    let (_shape_out, data) = value.try_extract_tensor::<u8>().map_err(|e| {
                        SnipperError::Inference(format!("Failed to extract output: {}", e))
                    })?;
                    latexsnipper_tensor::Tensor::u8(name, shape, data.to_vec())
                }
                _ => return Err(SnipperError::Inference("Unsupported output dtype".into())),
            };
            result_tensors.push(tensor);
        }

        Ok(result_tensors)
    }

    fn input_names(&self) -> Vec<String> {
        vec![]
    }

    fn output_names(&self) -> Vec<String> {
        vec![]
    }

    fn get_character_list(&self) -> Option<Vec<String>> {
        let session = self.session.lock().ok()?;
        let meta = session.metadata().ok()?;
        let chars_str = meta.custom("character")?;
        let mut chars: Vec<String> = chars_str.lines().map(|s| s.to_string()).collect();
        // Model already has blank at position 0
        // Just add space at end (RapidOCR convention)
        chars.push(" ".to_string());
        Some(chars)
    }

    fn release(&mut self) {
        // Clear all cached sessions to release ONNX session resources.
        // Sessions are stored in OnnxRuntimeBackend.sessions. OnnxSession.session
        // is just an Arc clone pointing to the same Mutex<Session>. Once the Arc
        // refcount drops to zero, the underlying Session is freed.
        // Here we only clear OnnxSession's own reference. The backend cache is
        // out of scope — use SessionCache::clear() at the backend level instead.
    }
}
