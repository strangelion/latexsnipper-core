//! Compatibility API for pre-registry callers.
//!
//! New code must use [`crate::RuntimeRegistry`] and [`crate::RuntimeSession`].

use std::collections::BTreeMap;
use std::sync::Arc;

use latexsnipper_ast::Diagnostic;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_tensor::Tensor;
use serde::{Deserialize, Serialize};

use crate::{
    AccelerationMode, ArtifactValidation, RunRequest, RuntimeKind, RuntimeOptions, RuntimeRegistry,
    RuntimeResolver,
};

/// Positional session retained for source compatibility.
pub trait InferenceSession: Send + Sync {
    fn run(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>>;
    fn input_names(&self) -> Vec<String>;
    fn output_names(&self) -> Vec<String>;

    fn get_character_list(&self) -> Option<Vec<String>> {
        None
    }

    fn release(&mut self);
}

impl InferenceSession for Box<dyn InferenceSession> {
    fn run(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>> {
        (**self).run(inputs)
    }

    fn input_names(&self) -> Vec<String> {
        (**self).input_names()
    }

    fn output_names(&self) -> Vec<String> {
        (**self).output_names()
    }

    fn get_character_list(&self) -> Option<Vec<String>> {
        (**self).get_character_list()
    }

    fn release(&mut self) {
        (**self).release();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDiagnostics {
    pub runtime: String,
    pub available: bool,
    pub selected_provider: String,
    pub available_providers: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
}

/// Legacy backend retained only as an input adapter into RuntimeRegistry.
pub trait RuntimeBackend: Send + Sync {
    fn create_session(
        &self,
        handle: &crate::ModelHandle,
        acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>>;

    fn create_session_with_threads(
        &self,
        handle: &crate::ModelHandle,
        acceleration: AccelerationMode,
        _max_threads: usize,
    ) -> Result<Box<dyn InferenceSession>> {
        self.create_session(handle, acceleration)
    }

    fn clear_sessions(&self) {}
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;

    fn selected_provider(&self) -> String {
        self.name().to_owned()
    }

    fn available_providers(&self) -> Vec<String> {
        self.is_available()
            .then(|| self.name().to_owned())
            .into_iter()
            .collect()
    }

    fn provider_diagnostics(&self) -> Vec<Diagnostic> {
        Vec::new()
    }

    fn runtime_diagnostics(&self) -> RuntimeDiagnostics {
        RuntimeDiagnostics {
            runtime: self.name().to_owned(),
            available: self.is_available(),
            selected_provider: self.selected_provider(),
            available_providers: self.available_providers(),
            diagnostics: self.provider_diagnostics(),
        }
    }
}

/// A compatibility view over the canonical registry. It does not own a second
/// runtime implementation; every session still resolves through the registry.
#[derive(Clone)]
pub struct RegistryRuntimeBackend {
    registry: Arc<RuntimeRegistry>,
    default_runtime: RuntimeKind,
}

impl RegistryRuntimeBackend {
    pub fn new(registry: Arc<RuntimeRegistry>, default_runtime: RuntimeKind) -> Self {
        Self {
            registry,
            default_runtime,
        }
    }

    pub fn registry(&self) -> &RuntimeRegistry {
        &self.registry
    }
}

impl RuntimeBackend for RegistryRuntimeBackend {
    fn create_session(
        &self,
        handle: &crate::ModelHandle,
        acceleration: AccelerationMode,
    ) -> Result<Box<dyn InferenceSession>> {
        self.create_session_with_threads(handle, acceleration, 0)
    }

    fn create_session_with_threads(
        &self,
        handle: &crate::ModelHandle,
        acceleration: AccelerationMode,
        max_threads: usize,
    ) -> Result<Box<dyn InferenceSession>> {
        let has_explicit_path = handle.model_path().is_some();
        let model_path = handle
            .model_path()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(handle.category())
                    .join(handle.variant())
                    .join("model.onnx")
            });
        let model_dir = model_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        let variant = latexsnipper_model::RuntimeVariant {
            id: "legacy-compatibility".to_owned(),
            runtime: self.default_runtime.to_string(),
            status: latexsnipper_model::VariantStatus::Stable,
            priority: 0,
            artifacts: BTreeMap::from([(
                "model".to_owned(),
                model_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| model_path.to_string_lossy().into_owned()),
            )]),
            options: None,
            platforms: Vec::new(),
            capabilities: Vec::new(),
            fallbacks: Vec::new(),
        };

        let mut resolved = RuntimeResolver::new(&self.registry)
            .with_artifact_validation(ArtifactValidation::AllowMissing)
            .resolve(handle.id(), std::slice::from_ref(&variant), model_dir, None)?;
        resolved.artifacts.options.insert(
            "legacyModelId".to_owned(),
            serde_json::Value::String(handle.id().to_owned()),
        );
        resolved.artifacts.options.insert(
            "legacyCategory".to_owned(),
            serde_json::Value::String(handle.category().to_owned()),
        );
        resolved.artifacts.options.insert(
            "legacyVariant".to_owned(),
            serde_json::Value::String(handle.variant().to_owned()),
        );
        resolved.artifacts.options.insert(
            "legacyHasExplicitPath".to_owned(),
            serde_json::Value::Bool(has_explicit_path),
        );
        if let Some(shape) = handle.input_shape() {
            resolved.artifacts.options.insert(
                "legacyInputShape".to_owned(),
                serde_json::to_value(shape).map_err(|error| {
                    SnipperError::Runtime(format!("failed to preserve legacy input shape: {error}"))
                })?,
            );
        }
        if let Some(bytes) = handle.model_bytes() {
            resolved
                .artifacts
                .buffers
                .insert("model".to_owned(), bytes.to_vec());
        }
        let mut options = RuntimeOptions::from(acceleration);
        options.max_threads = max_threads;
        let session =
            self.registry
                .create_session(&resolved.runtime, &resolved.artifacts, &options)?;
        Ok(Box::new(RuntimeSessionCompatibility::new(session)))
    }

    fn clear_sessions(&self) {
        self.registry.clear_sessions();
    }

    fn name(&self) -> &str {
        match &self.default_runtime {
            RuntimeKind::OnnxRuntime => "onnxruntime",
            _ => self.default_runtime.as_str(),
        }
    }

    fn is_available(&self) -> bool {
        self.registry.is_available(&self.default_runtime)
    }

    fn available_providers(&self) -> Vec<String> {
        self.registry
            .probe(&self.default_runtime)
            .map(|probe| probe.capabilities.execution_providers.into_iter().collect())
            .unwrap_or_default()
    }
}

struct RuntimeSessionCompatibility {
    inner: Box<dyn crate::RuntimeSession>,
    input_names: Vec<String>,
    output_names: Vec<String>,
}

impl RuntimeSessionCompatibility {
    fn new(inner: Box<dyn crate::RuntimeSession>) -> Self {
        let input_names = inner
            .metadata()
            .inputs
            .iter()
            .map(|spec| spec.name.clone())
            .collect();
        let output_names = inner
            .metadata()
            .outputs
            .iter()
            .map(|spec| spec.name.clone())
            .collect();
        Self {
            inner,
            input_names,
            output_names,
        }
    }
}

impl InferenceSession for RuntimeSessionCompatibility {
    fn run(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>> {
        if inputs.len() != self.input_names.len() {
            return Err(SnipperError::Inference(format!(
                "runtime expected {} positional inputs, got {}",
                self.input_names.len(),
                inputs.len(),
            )));
        }

        // The legacy InferenceSession API is positional.
        // Preserve that contract by binding each positional tensor
        // to the canonical input name exposed by RuntimeSession,
        // ignoring whatever name the legacy caller placed on the tensor.
        let named_inputs: crate::TensorMap = self
            .input_names
            .iter()
            .cloned()
            .zip(inputs.iter().cloned())
            .collect();

        let response = self.inner.run(RunRequest::new(named_inputs))?;
        self.output_names
            .iter()
            .map(|name| {
                response.outputs.get(name).cloned().ok_or_else(|| {
                    SnipperError::Inference(format!(
                        "runtime omitted declared output tensor '{name}'"
                    ))
                })
            })
            .collect()
    }

    fn input_names(&self) -> Vec<String> {
        self.input_names.clone()
    }

    fn output_names(&self) -> Vec<String> {
        self.output_names.clone()
    }

    fn release(&mut self) {}
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::providers::legacy_adapter::LegacyRuntimeAdapter;

    struct CaptureBackend {
        captured: Arc<Mutex<Option<crate::ModelHandle>>>,
    }

    impl RuntimeBackend for CaptureBackend {
        fn create_session(
            &self,
            handle: &crate::ModelHandle,
            _acceleration: AccelerationMode,
        ) -> Result<Box<dyn InferenceSession>> {
            *self.captured.lock().unwrap() = Some(handle.clone());
            Ok(Box::new(EmptySession))
        }

        fn name(&self) -> &str {
            "capture"
        }

        fn is_available(&self) -> bool {
            true
        }
    }

    struct EmptySession;

    impl InferenceSession for EmptySession {
        fn run(&self, _inputs: &[Tensor]) -> Result<Vec<Tensor>> {
            Ok(Vec::new())
        }

        fn input_names(&self) -> Vec<String> {
            Vec::new()
        }

        fn output_names(&self) -> Vec<String> {
            Vec::new()
        }

        fn release(&mut self) {}
    }

    #[test]
    fn compatibility_registry_preserves_unresolved_legacy_handle() {
        let captured = Arc::new(Mutex::new(None));
        let backend: Arc<dyn RuntimeBackend> = Arc::new(CaptureBackend {
            captured: captured.clone(),
        });
        let registry = Arc::new(RuntimeRegistry::with_factory(LegacyRuntimeAdapter::new(
            backend,
        )));
        let compatibility =
            RegistryRuntimeBackend::new(registry, RuntimeKind::Custom("capture".to_owned()));

        compatibility
            .create_session(
                &crate::ModelHandle::new("model-id", "formula-rec", "legacy-variant")
                    .with_input_shape(vec![1, 3, 48, 320]),
                AccelerationMode::Cpu,
            )
            .unwrap();

        let handle = captured.lock().unwrap().clone().unwrap();
        assert_eq!(handle.id(), "model-id");
        assert_eq!(handle.category(), "formula-rec");
        assert_eq!(handle.variant(), "legacy-variant");
        assert!(handle.model_path().is_none());
        assert_eq!(handle.input_shape(), Some([1, 3, 48, 320].as_slice()));
    }

    #[test]
    fn compatibility_registry_preserves_in_memory_model_bytes() {
        let captured = Arc::new(Mutex::new(None));
        let backend: Arc<dyn RuntimeBackend> = Arc::new(CaptureBackend {
            captured: captured.clone(),
        });
        let registry = Arc::new(RuntimeRegistry::with_factory(LegacyRuntimeAdapter::new(
            backend,
        )));
        let compatibility =
            RegistryRuntimeBackend::new(registry, RuntimeKind::Custom("capture".to_owned()));

        compatibility
            .create_session(
                &crate::ModelHandle::with_bytes("memory-model", vec![7, 8, 9]),
                AccelerationMode::Auto,
            )
            .unwrap();

        let handle = captured.lock().unwrap().clone().unwrap();
        assert_eq!(handle.id(), "memory-model");
        assert_eq!(handle.model_bytes(), Some([7, 8, 9].as_slice()));
        assert!(handle.model_path().is_none());
    }

    /// Regression: legacy callers place arbitrary names on their tensors
    /// (e.g. "x", "pixel_values"). The compatibility layer must bind by
    /// position, not reject the request because the names differ from the
    /// canonical RuntimeSession input names (e.g. "input_0").
    #[test]
    fn positional_compatibility_rebinds_runtime_input_names() {
        use crate::session::{RunRequest, SessionMetadata, TensorSpec};
        use crate::{RuntimeKind, RuntimeSession};
        use std::sync::Arc;

        let last_request: Arc<Mutex<Option<RunRequest>>> = Arc::new(Mutex::new(None));
        let captured = last_request.clone();

        struct NamedSession {
            metadata: SessionMetadata,
            on_run: Box<dyn Fn(RunRequest) -> Result<crate::RunResponse> + Send + Sync>,
        }

        impl RuntimeSession for NamedSession {
            fn run(&self, request: RunRequest) -> Result<crate::RunResponse> {
                (self.on_run)(request)
            }

            fn metadata(&self) -> &SessionMetadata {
                &self.metadata
            }
        }

        let session = NamedSession {
            metadata: SessionMetadata {
                runtime: RuntimeKind::OnnxRuntime,
                model_id: None,
                inputs: vec![
                    TensorSpec {
                        name: "input_0".to_owned(),
                        shape: vec![Some(1), Some(3), Some(48), Some(320)],
                        dtype: "float32".to_owned(),
                    },
                    TensorSpec {
                        name: "input_1".to_owned(),
                        shape: vec![Some(1), Some(128)],
                        dtype: "int64".to_owned(),
                    },
                ],
                outputs: vec![TensorSpec {
                    name: "output".to_owned(),
                    shape: vec![Some(1)],
                    dtype: "float32".to_owned(),
                }],
                methods: Vec::new(),
            },
            on_run: Box::new(move |request: RunRequest| {
                *captured.lock().unwrap() = Some(request);
                Ok(crate::RunResponse {
                    outputs: std::collections::BTreeMap::from([(
                        "output".to_owned(),
                        Tensor::float32("output", vec![1], vec![42.0]),
                    )]),
                })
            }),
        };

        let compat = RuntimeSessionCompatibility {
            inner: Box::new(session),
            input_names: vec!["input_0".to_owned(), "input_1".to_owned()],
            output_names: vec!["output".to_owned()],
        };

        // Legacy caller provides tensors with completely different names.
        // The compatibility layer must map position 0 → "input_0",
        // position 1 → "input_1", regardless of the tensor's own name.
        let result = InferenceSession::run(
            &compat,
            &[
                Tensor::float32("pixel_values", vec![1, 3, 48, 320], vec![0.5; 46080]),
                Tensor::int64("input_ids", vec![1, 128], vec![0; 128]),
            ],
        )
        .unwrap();

        assert_eq!(result.len(), 1);
        assert!((result[0].as_f32_slice().unwrap()[0] - 42.0).abs() < 1e-6);

        // Verify the inner session received the tensors under canonical names.
        let last = last_request.lock().unwrap().clone().unwrap();
        assert!(last.inputs.contains_key("input_0"));
        assert!(last.inputs.contains_key("input_1"));
        assert!(!last.inputs.contains_key("pixel_values"));
        assert!(!last.inputs.contains_key("input_ids"));
    }

    #[test]
    fn positional_compatibility_rejects_wrong_input_count() {
        use crate::session::{SessionMetadata, TensorSpec};
        use crate::{RuntimeKind, RuntimeSession};

        struct SingleInputSession {
            metadata: SessionMetadata,
        }

        impl RuntimeSession for SingleInputSession {
            fn run(&self, _request: crate::session::RunRequest) -> Result<crate::RunResponse> {
                Ok(crate::RunResponse {
                    outputs: std::collections::BTreeMap::new(),
                })
            }

            fn metadata(&self) -> &SessionMetadata {
                &self.metadata
            }
        }

        let session = SingleInputSession {
            metadata: SessionMetadata {
                runtime: RuntimeKind::OnnxRuntime,
                model_id: None,
                inputs: vec![TensorSpec {
                    name: "input_0".to_owned(),
                    shape: vec![Some(1), Some(3), Some(48), Some(320)],
                    dtype: "float32".to_owned(),
                }],
                outputs: vec![],
                methods: vec![],
            },
        };

        let compat = RuntimeSessionCompatibility {
            inner: Box::new(session),
            input_names: vec!["input_0".to_owned()],
            output_names: vec![],
        };

        let err = InferenceSession::run(
            &compat,
            &[
                Tensor::float32("a", vec![1], vec![1.0]),
                Tensor::float32("b", vec![1], vec![2.0]),
            ],
        )
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("expected 1 positional inputs, got 2"));
    }
}
