//! ONNX Runtime factory for the canonical registry API.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use latexsnipper_foundation::{Result, SnipperError};

use crate::providers::onnx::OnnxRuntimeBackend;
use crate::{
    DeviceKind, RunRequest, RunResponse, RuntimeArtifacts, RuntimeBackend, RuntimeCapabilities,
    RuntimeDevice, RuntimeFactory, RuntimeKind, RuntimeOptions, RuntimeProbe, RuntimeSession,
    SessionMetadata, SessionTensorSpec, TensorMap,
};

pub struct OnnxRuntimeFactory {
    models_dir: PathBuf,
    backend: OnceLock<std::result::Result<Arc<OnnxRuntimeBackend>, SnipperError>>,
}

impl OnnxRuntimeFactory {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            backend: OnceLock::new(),
        }
    }

    fn backend(&self) -> Result<Arc<OnnxRuntimeBackend>> {
        self.backend
            .get_or_init(|| OnnxRuntimeBackend::new(self.models_dir.clone()).map(Arc::new))
            .clone()
    }
}

impl RuntimeFactory for OnnxRuntimeFactory {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::OnnxRuntime
    }

    fn probe(&self) -> RuntimeProbe {
        match self.backend() {
            Ok(backend) => {
                let providers = backend.available_providers();
                let devices = providers
                    .iter()
                    .map(|provider| RuntimeDevice {
                        name: provider.to_ascii_lowercase(),
                        kind: if provider.eq_ignore_ascii_case("cpu") {
                            DeviceKind::Cpu
                        } else {
                            DeviceKind::Gpu
                        },
                        memory_bytes: None,
                    })
                    .collect();
                let mut capabilities = RuntimeCapabilities::default();
                capabilities.tensor_dtypes.extend(
                    ["f32", "f16", "i64", "i32", "u8", "bool"]
                        .into_iter()
                        .map(str::to_owned),
                );
                capabilities.execution_providers.extend(
                    providers
                        .into_iter()
                        .map(|provider| provider.to_ascii_lowercase()),
                );
                capabilities.features.extend(
                    [
                        "artifact-format:onnx",
                        "artifact-format:ort",
                        "model-loading:file",
                    ]
                    .into_iter()
                    .map(str::to_owned),
                );
                RuntimeProbe {
                    available: true,
                    version: Some(format!("api-{} ({})", ort::MINOR_VERSION, ort::info())),
                    devices,
                    reason_unavailable: None,
                    capabilities,
                }
            }
            Err(error) => RuntimeProbe::unavailable(error.to_string()),
        }
    }

    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        if artifacts.runtime != RuntimeKind::OnnxRuntime {
            return Err(SnipperError::Runtime(format!(
                "ONNX factory received '{}' artifacts",
                artifacts.runtime
            )));
        }
        let model_path = select_model_path(artifacts, options)?;
        let backend = self.backend()?;
        let handle = crate::ModelHandle::with_path("onnx-model", model_path.clone());
        let inner = backend.create_session_with_options(&handle, options)?;
        let input_names = inner.input_names();
        let output_names = inner.output_names();
        Ok(Box::new(OnnxRegistrySession {
            inner,
            metadata: SessionMetadata {
                runtime: RuntimeKind::OnnxRuntime,
                model_id: Some(model_path.to_string_lossy().into_owned()),
                methods: Vec::new(),
                inputs: input_names
                    .into_iter()
                    .map(|name| SessionTensorSpec {
                        name,
                        shape: Vec::new(),
                        dtype: "unknown".to_owned(),
                    })
                    .collect(),
                outputs: output_names
                    .into_iter()
                    .map(|name| SessionTensorSpec {
                        name,
                        shape: Vec::new(),
                        dtype: "unknown".to_owned(),
                    })
                    .collect(),
            },
        }))
    }

    fn clear_sessions(&self) {
        if let Some(Ok(backend)) = self.backend.get() {
            backend.clear_sessions();
        }
    }
}

fn select_model_path(artifacts: &RuntimeArtifacts, options: &RuntimeOptions) -> Result<PathBuf> {
    if options
        .extra
        .get("memoryMapModel")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Err(SnipperError::Runtime(
            "ONNX_MODEL_MMAP_UNSUPPORTED: this ORT binding does not expose a verified model mmap lifecycle"
                .to_owned(),
        ));
    }
    let prefer_ort = options
        .extra
        .get("preferOrtFormat")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if let Some(role) = options
        .extra
        .get("artifact")
        .and_then(|value| value.as_str())
    {
        return select_role_path(artifacts, role, prefer_ort)?.ok_or_else(|| {
            SnipperError::Model(format!("ONNX artifact role '{role}' is not declared"))
        });
    }

    for role in ["model", "primary", "encoder", "decoder"] {
        if let Some(path) = select_role_path(artifacts, role, prefer_ort)? {
            return Ok(path);
        }
    }
    let mut models = artifacts
        .files
        .values()
        .filter(|path| supported_graph_format(path))
        .cloned();
    let first = models.next().ok_or_else(|| {
        SnipperError::Model("ONNX runtime variant declares no .onnx or .ort artifact".to_owned())
    })?;
    if models.next().is_some() {
        return Err(SnipperError::Model(
            "ONNX runtime variant contains multiple graphs; the model adapter must select an artifact role"
                .to_owned(),
        ));
    }
    Ok(first)
}

fn select_role_path(
    artifacts: &RuntimeArtifacts,
    role: &str,
    prefer_ort: bool,
) -> Result<Option<PathBuf>> {
    let suffixed = if prefer_ort {
        [format!("{role}.ort"), format!("{role}.onnx")]
    } else {
        [format!("{role}.onnx"), format!("{role}.ort")]
    };
    let candidates = if prefer_ort {
        [suffixed[0].as_str(), role, suffixed[1].as_str()]
    } else {
        [role, suffixed[0].as_str(), suffixed[1].as_str()]
    };
    for candidate in candidates {
        if let Some(path) = artifacts.files.get(candidate) {
            if !supported_graph_format(path) {
                return Err(SnipperError::Model(format!(
                    "ONNX artifact role '{candidate}' must use .onnx or .ort: {}",
                    path.display()
                )));
            }
            return Ok(Some(path.clone()));
        }
    }
    Ok(None)
}

fn supported_graph_format(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("onnx") || extension.eq_ignore_ascii_case("ort")
        })
}

struct OnnxRegistrySession {
    inner: Box<dyn crate::InferenceSession>,
    metadata: SessionMetadata,
}

impl RuntimeSession for OnnxRegistrySession {
    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        if let Some(method) = request.method.as_deref() {
            return Err(SnipperError::Runtime(format!(
                "ONNX Runtime session has no named method '{method}'"
            )));
        }

        let ordered_inputs: Vec<_> = self
            .metadata
            .inputs
            .iter()
            .map(|spec| {
                request.inputs.get(&spec.name).cloned().ok_or_else(|| {
                    SnipperError::Inference(format!(
                        "ONNX request is missing input tensor '{}'",
                        spec.name
                    ))
                })
            })
            .collect::<Result<_>>()?;
        let tensors = self.inner.run(&ordered_inputs)?;
        if tensors.len() != self.metadata.outputs.len() {
            return Err(SnipperError::Inference(format!(
                "ONNX Runtime returned {} outputs, metadata declares {}",
                tensors.len(),
                self.metadata.outputs.len()
            )));
        }
        let requested = request.requested_outputs.as_ref();
        let outputs: TensorMap = self
            .metadata
            .outputs
            .iter()
            .zip(tensors)
            .filter(|(spec, _)| requested.is_none_or(|names| names.contains(&spec.name)))
            .map(|(spec, tensor)| (spec.name.clone(), tensor))
            .collect();
        Ok(RunResponse { outputs })
    }
}

#[cfg(test)]
mod artifact_selection_tests {
    use super::*;

    fn alternatives() -> RuntimeArtifacts {
        RuntimeArtifacts::new(RuntimeKind::OnnxRuntime)
            .with_file("decoder.onnx", "decoder.onnx")
            .with_file("decoder.ort", "decoder.ort")
    }

    #[test]
    fn ort_format_is_opt_in_and_onnx_remains_the_default() {
        let artifacts = alternatives();
        let mut options = RuntimeOptions::default();
        options
            .extra
            .insert("artifact".to_owned(), serde_json::json!("decoder"));
        assert_eq!(
            select_model_path(&artifacts, &options).unwrap(),
            PathBuf::from("decoder.onnx")
        );

        options
            .extra
            .insert("preferOrtFormat".to_owned(), serde_json::json!(true));
        assert_eq!(
            select_model_path(&artifacts, &options).unwrap(),
            PathBuf::from("decoder.ort")
        );
    }

    #[test]
    fn mmap_request_fails_closed_until_the_binding_supports_it() {
        let artifacts = alternatives();
        let mut options = RuntimeOptions::default();
        options
            .extra
            .insert("memoryMapModel".to_owned(), serde_json::json!(true));
        let error = select_model_path(&artifacts, &options).unwrap_err();
        assert!(error.to_string().contains("ONNX_MODEL_MMAP_UNSUPPORTED"));
    }

    #[test]
    fn executable_or_unknown_graph_extension_is_rejected() {
        let artifacts =
            RuntimeArtifacts::new(RuntimeKind::OnnxRuntime).with_file("model", "provider.dll");
        let error = select_model_path(&artifacts, &RuntimeOptions::default()).unwrap_err();
        assert!(error.to_string().contains("must use .onnx or .ort"));
    }
}
