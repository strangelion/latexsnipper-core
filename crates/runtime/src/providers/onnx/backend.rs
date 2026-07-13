use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ort::ep::{ExecutionProvider, ExecutionProviderDispatch};
use ort::{environment::Environment, session::Session, value::Value};

use super::platform::{Acceleration, Platform};
use crate::acceleration::AccelerationMode;
use crate::backend::RuntimeBackend;
use crate::model_handle::ModelHandle;
use crate::session::InferenceSession;
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
    sessions: Mutex<HashMap<String, Arc<Mutex<Session>>>>,
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
        acceleration: AccelerationMode,
        max_threads: usize,
    ) -> Result<Arc<Mutex<Session>>> {
        let cache_key = format!("{}::{acceleration:?}", model_path.to_string_lossy());

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

        let (configured, selected, fallback) = configure_execution_provider(builder, acceleration);
        builder = configured;
        if let Ok(mut provider) = self.selected_provider.lock() {
            *provider = selected.to_string();
        }
        if let Some(fallback) = fallback {
            log::warn!("{fallback}");
            if let Ok(mut fallbacks) = self.provider_fallbacks.lock() {
                fallbacks.push(fallback);
            }
        }

        let thread_count = max_threads.max(1);

        // Configure thread count via ORT 2.0 API
        builder = builder
            .with_intra_threads(thread_count)
            .map_err(|e| SnipperError::Runtime(format!("Failed to set thread count: {}", e)))?;

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

fn configure_execution_provider(
    mut builder: ort::session::builder::SessionBuilder,
    mode: AccelerationMode,
) -> (
    ort::session::builder::SessionBuilder,
    &'static str,
    Option<String>,
) {
    if mode == AccelerationMode::Cpu {
        return (builder, "CPU", None);
    }

    let mut last_failure = None;

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        let cuda = ort::ep::CUDA::default();
        if cuda.is_available().unwrap_or(false) {
            let (next, enabled, failure) = try_execution_provider(builder, cuda.build(), "CUDA");
            builder = next;
            if enabled {
                return (builder, "CUDA", None);
            }
            if failure.is_some() {
                last_failure = failure;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let directml = ort::ep::DirectML::default();
        if directml.is_available().unwrap_or(false) {
            let (next, enabled, failure) =
                try_execution_provider(builder, directml.build(), "DirectML");
            builder = next;
            if enabled {
                return (builder, "DirectML", None);
            }
            if failure.is_some() {
                last_failure = failure;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let coreml = ort::ep::CoreML::default();
        if coreml.is_available().unwrap_or(false) {
            let (next, enabled, failure) =
                try_execution_provider(builder, coreml.build(), "CoreML");
            builder = next;
            if enabled {
                return (builder, "CoreML", None);
            }
            if failure.is_some() {
                last_failure = failure;
            }
        }
    }

    let fallback = last_failure.or_else(|| {
        (mode == AccelerationMode::Gpu).then(|| {
            "GPU provider requested but no usable provider was available; using CPU".to_string()
        })
    });
    (builder, "CPU", fallback)
}

fn try_execution_provider(
    builder: ort::session::builder::SessionBuilder,
    provider: ExecutionProviderDispatch,
    name: &str,
) -> (ort::session::builder::SessionBuilder, bool, Option<String>) {
    match builder.with_execution_providers([provider.error_on_failure()]) {
        Ok(builder) => (builder, true, None),
        Err(error) => {
            let message = format!("{name} provider registration failed: {error}; using CPU");
            (error.recover(), false, Some(message))
        }
    }
}

fn available_execution_providers() -> Vec<String> {
    let mut providers = vec!["CPU".to_string()];
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
        let model_path = self.resolve_model_path(handle);
        let shared = self.get_or_create_session(&model_path, acceleration, max_threads)?;
        Ok(Box::new(OnnxSession { session: shared }))
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

    fn release(&mut self) {
        // Clear all cached sessions to release ONNX session resources.
        // Sessions are stored in OnnxRuntimeBackend.sessions. OnnxSession.session
        // is just an Arc clone pointing to the same Mutex<Session>. Once the Arc
        // refcount drops to zero, the underlying Session is freed.
        // Here we only clear OnnxSession's own reference. The backend cache is
        // out of scope; use SessionCache::clear() at the backend level instead.
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
