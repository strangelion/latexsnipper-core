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
        let response = self.inner.run(RunRequest::from_tensors(inputs))?;
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
}
