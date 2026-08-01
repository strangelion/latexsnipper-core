use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use ort::ep::ExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::{environment::Environment, session::Session, value::Value};

use super::platform::{Acceleration, Platform};
use super::provider;
use crate::acceleration::AccelerationMode;
use crate::legacy::{InferenceSession, RuntimeBackend};
use crate::model_handle::ModelHandle;
use crate::{ProviderAttempt, RuntimeOptions};
use latexsnipper_ast::{Diagnostic, DiagnosticLevel, W_GPU_PROVIDER_FALLBACK};
use latexsnipper_foundation::{Result, SnipperError};

/// ONNX Runtime backend with auto GPU/CPU detection and session caching.
pub struct OnnxRuntimeBackend {
    _env: Arc<Environment>,
    models_dir: std::path::PathBuf,
    platform: Platform,
    acceleration: Acceleration,
    #[allow(dead_code)]
    max_threads: usize,
    sessions: Mutex<HashMap<String, CachedSession>>,
    selected_provider: Mutex<String>,
    provider_fallbacks: Mutex<Vec<String>>,
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
            selected_provider: Mutex::new("CPU".to_string()),
            provider_fallbacks: Mutex::new(Vec::new()),
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
            selected_provider: Mutex::new("CPU".to_string()),
            provider_fallbacks: Mutex::new(Vec::new()),
        })
    }

    pub fn platform(&self) -> Platform {
        self.platform
    }
    pub fn acceleration(&self) -> Acceleration {
        self.acceleration
    }

    pub fn provider_fallbacks(&self) -> Vec<String> {
        self.provider_fallbacks
            .lock()
            .map(|fallbacks| fallbacks.clone())
            .unwrap_or_default()
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
        options: &RuntimeOptions,
    ) -> Result<CachedSession> {
        let options_key = serde_json::to_string(options).map_err(|error| {
            SnipperError::Runtime(format!("Failed to serialize ONNX Runtime options: {error}"))
        })?;
        let cache_key = format!("{}::{options_key}", model_path.to_string_lossy());

        // Check cache first (short hold)
        {
            let sessions = self
                .sessions
                .lock()
                .map_err(|_| SnipperError::Runtime("Lock poisoned".into()))?;
            if let Some(cached) = sessions.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        // Create new session with acceleration and thread config
        let mut builder = Session::builder().map_err(|e| {
            SnipperError::Runtime(format!("Failed to create session builder: {}", e))
        })?;

        let configured = provider::configure(builder, options, self.acceleration)?;
        builder = configured.builder;
        let selected = configured.selected;
        let attempts = configured.attempts;
        let diagnostics = configured.diagnostics;
        if let Ok(mut provider) = self.selected_provider.lock() {
            *provider = selected.clone();
        }
        for fallback in &diagnostics {
            log::warn!("{fallback}");
            if let Ok(mut fallbacks) = self.provider_fallbacks.lock() {
                fallbacks.push(fallback.clone());
            }
        }

        if configured.active.iter().any(|name| name == "DirectML") {
            builder = builder
                .with_memory_pattern(false)
                .map_err(|error| {
                    SnipperError::Runtime(format!(
                        "Failed to disable memory patterns for DirectML: {error}"
                    ))
                })?
                .with_parallel_execution(false)
                .map_err(|error| {
                    SnipperError::Runtime(format!(
                        "Failed to select sequential execution for DirectML: {error}"
                    ))
                })?;
        }

        builder = builder
            .with_optimization_level(if options.graph_optimization {
                GraphOptimizationLevel::All
            } else {
                GraphOptimizationLevel::Disable
            })
            .map_err(|error| {
                SnipperError::Runtime(format!(
                    "Failed to configure ONNX graph optimization: {error}"
                ))
            })?;
        if options.max_threads > 0 {
            builder = builder
                .with_intra_threads(options.max_threads)
                .map_err(|error| {
                    SnipperError::Runtime(format!("Failed to set thread count: {error}"))
                })?;
        }

        let _provider_guard = provider_operation_guard(&selected)?;
        let session = builder.commit_from_file(model_path).map_err(|e| {
            SnipperError::Runtime(format!(
                "Failed to load model {}: {}",
                model_path.display(),
                e
            ))
        })?;

        let cached = CachedSession {
            session: Arc::new(Mutex::new(session)),
            provider: selected,
            attempts,
            diagnostics,
        };

        // Store in cache (may race with another thread, that's fine)
        {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| SnipperError::Runtime("Lock poisoned".into()))?;
            sessions.entry(cache_key).or_insert_with(|| cached.clone());
        }

        Ok(cached)
    }
}

fn available_execution_providers() -> Vec<String> {
    let mut providers = vec!["CPU".to_string()];
    #[cfg(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    ))]
    if ort::ep::TensorRT::default().is_available().unwrap_or(false) {
        providers.push("TensorRT".to_string());
    }
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if ort::ep::CUDA::default().is_available().unwrap_or(false) {
        providers.push("CUDA".to_string());
    }
    #[cfg(target_os = "windows")]
    if ort::ep::DirectML::default().is_available().unwrap_or(false) {
        providers.push("DirectML".to_string());
    }
    #[cfg(target_os = "macos")]
    if ort::ep::CoreML::default().is_available().unwrap_or(false) {
        providers.push("CoreML".to_string());
    }
    providers
}

#[derive(Clone)]
struct CachedSession {
    session: Arc<Mutex<Session>>,
    provider: String,
    attempts: Vec<ProviderAttempt>,
    diagnostics: Vec<String>,
}

impl RuntimeBackend for OnnxRuntimeBackend {
    fn create_session(
        &self,
        handle: &ModelHandle,
        acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>> {
        self.create_session_with_threads(handle, acceleration, self.max_threads)
    }

    fn create_session_with_threads(
        &self,
        handle: &ModelHandle,
        acceleration: AccelerationMode,
        max_threads: usize,
    ) -> Result<Box<dyn InferenceSession>> {
        let mut options = RuntimeOptions::from_acceleration(acceleration);
        options.max_threads = max_threads;
        self.create_session_with_options(handle, &options)
    }

    fn name(&self) -> &str {
        "onnxruntime"
    }

    fn is_available(&self) -> bool {
        Environment::current().is_ok()
    }

    fn selected_provider(&self) -> String {
        self.selected_provider
            .lock()
            .map(|provider| provider.clone())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    fn available_providers(&self) -> Vec<String> {
        available_execution_providers()
    }

    fn provider_diagnostics(&self) -> Vec<Diagnostic> {
        self.provider_fallbacks()
            .into_iter()
            .map(|message| {
                Diagnostic::new(DiagnosticLevel::Warning, W_GPU_PROVIDER_FALLBACK, message)
                    .with_recoverable(true)
                    .with_remediation("Install a compatible GPU provider or explicitly select CPU")
            })
            .collect()
    }

    fn clear_sessions(&self) {
        if let Ok(mut sessions) = self.sessions.lock() {
            let count = sessions.len();
            sessions.clear();
            log::info!("Cleared {} cached ONNX sessions", count);
        }
    }
}

impl OnnxRuntimeBackend {
    pub fn create_session_with_options(
        &self,
        handle: &ModelHandle,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn InferenceSession>> {
        let model_path = self.resolve_model_path(handle);
        let cached = self.get_or_create_session(&model_path, options)?;
        Ok(Box::new(OnnxSession {
            session: cached.session,
            provider: cached.provider,
            attempts: cached.attempts,
            diagnostics: cached.diagnostics,
        }))
    }
}

struct OnnxSession {
    session: Arc<Mutex<Session>>,
    provider: String,
    attempts: Vec<ProviderAttempt>,
    diagnostics: Vec<String>,
}

impl InferenceSession for OnnxSession {
    fn run(
        &self,
        inputs: &[latexsnipper_tensor::Tensor],
    ) -> Result<Vec<latexsnipper_tensor::Tensor>> {
        let _provider_guard = provider_operation_guard(&self.provider)?;
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
                latexsnipper_tensor::TensorData::Float16(data) => {
                    let shape: Vec<usize> = input.shape().to_vec();
                    let boxed: Box<[half::f16]> = data
                        .iter()
                        .copied()
                        .map(half::f16::from_bits)
                        .collect::<Vec<_>>()
                        .into();
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
                latexsnipper_tensor::TensorData::Int32(data) => {
                    let shape: Vec<usize> = input.shape().to_vec();
                    let boxed: Box<[i32]> = data.clone().into();
                    let value: Value = Value::from_array((shape, boxed))
                        .map_err(|e| {
                            SnipperError::Inference(format!("Failed to create tensor: {}", e))
                        })?
                        .into();
                    input_values.push((name, value));
                }
                latexsnipper_tensor::TensorData::UInt8(data) => {
                    let shape: Vec<usize> = input.shape().to_vec();
                    let boxed: Box<[u8]> = data.clone().into();
                    let value: Value = Value::from_array((shape, boxed))
                        .map_err(|e| {
                            SnipperError::Inference(format!("Failed to create tensor: {}", e))
                        })?
                        .into();
                    input_values.push((name, value));
                }
                latexsnipper_tensor::TensorData::Bool(data) => {
                    let shape: Vec<usize> = input.shape().to_vec();
                    let boxed: Box<[bool]> = data.clone().into();
                    let value: Value = Value::from_array((shape, boxed))
                        .map_err(|e| {
                            SnipperError::Inference(format!("Failed to create tensor: {}", e))
                        })?
                        .into();
                    input_values.push((name, value));
                }
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
                    ty: ort::value::TensorElementType::Float16,
                    ..
                } => {
                    let (_shape_out, data) =
                        value.try_extract_tensor::<half::f16>().map_err(|e| {
                            SnipperError::Inference(format!("Failed to extract output: {}", e))
                        })?;
                    latexsnipper_tensor::Tensor::float16_bits(
                        name,
                        shape,
                        data.iter().map(|value| value.to_bits()).collect(),
                    )
                }
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
                ort::value::ValueType::Tensor {
                    ty: ort::value::TensorElementType::Bool,
                    ..
                } => {
                    let (_shape_out, data) = value.try_extract_tensor::<bool>().map_err(|e| {
                        SnipperError::Inference(format!("Failed to extract output: {}", e))
                    })?;
                    latexsnipper_tensor::Tensor::boolean(name, shape, data.to_vec())
                }
                _ => return Err(SnipperError::Inference("Unsupported output dtype".into())),
            };
            result_tensors.push(tensor);
        }

        Ok(result_tensors)
    }

    fn input_names(&self) -> Vec<String> {
        self.session
            .lock()
            .map(|session| {
                session
                    .inputs()
                    .iter()
                    .map(|input| input.name().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn output_names(&self) -> Vec<String> {
        self.session
            .lock()
            .map(|session| {
                session
                    .outputs()
                    .iter()
                    .map(|output| output.name().to_string())
                    .collect()
            })
            .unwrap_or_default()
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

    fn effective_provider(&self) -> Option<String> {
        Some(self.provider.clone())
    }

    fn provider_attempts(&self) -> Vec<ProviderAttempt> {
        self.attempts.clone()
    }

    fn fallback_diagnostics(&self) -> Vec<String> {
        self.diagnostics.clone()
    }

    fn release(&mut self) {
        // Clear all cached sessions to release ONNX session resources.
        // Sessions are stored in OnnxRuntimeBackend.sessions. OnnxSession.session
        // is just an Arc clone pointing to the same Mutex<Session>. Once the Arc
        // refcount drops to zero, the underlying Session is freed.
        // Here we only clear OnnxSession's own reference. The backend cache is
        // out of scope; use SessionCache::clear() at the backend level instead.
    }
}

fn provider_operation_guard(provider: &str) -> Result<Option<MutexGuard<'static, ()>>> {
    static GPU_OPERATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    if provider == "CPU" {
        return Ok(None);
    }
    GPU_OPERATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map(Some)
        .map_err(|_| SnipperError::Runtime("GPU provider operation lock poisoned".to_string()))
}

impl Drop for OnnxRuntimeBackend {
    fn drop(&mut self) {
        // ORT GPU execution providers may release device resources from Session::drop.
        // Keep destruction serialized with session creation and inference in other backends.
        let Ok(_guard) = provider_operation_guard("GPU") else {
            return;
        };
        if let Ok(sessions) = self.sessions.get_mut() {
            sessions.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_diagnostics_always_report_cpu() {
        let backend = OnnxRuntimeBackend::new(std::env::temp_dir()).unwrap();
        assert!(backend.is_available());
        assert_eq!(backend.selected_provider(), "CPU");
        assert!(backend
            .available_providers()
            .iter()
            .any(|item| item == "CPU"));
        assert!(backend.provider_fallbacks().is_empty());
        let diagnostics = backend.runtime_diagnostics();
        assert_eq!(diagnostics.runtime, "onnxruntime");
        assert!(diagnostics.available);
        assert_eq!(diagnostics.selected_provider, "CPU");
        assert!(diagnostics.available_providers.contains(&"CPU".to_string()));
    }
}
