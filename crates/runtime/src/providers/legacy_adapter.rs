//! Legacy adapter: wraps a v1 RuntimeBackend (Arc) as a RuntimeFactory.

use std::sync::Arc;

use crate::{
    DeviceKind, InferenceSession, RunRequest, RunResponse, RuntimeArtifacts, RuntimeBackend,
    RuntimeDevice, RuntimeFactory, RuntimeKind, RuntimeOptions, RuntimeProbe, RuntimeSession,
    SessionMetadata, SessionTensorSpec, TensorMap,
};
use latexsnipper_foundation::{Result, SnipperError};

pub struct LegacyRuntimeAdapter {
    backend: Arc<dyn RuntimeBackend>,
}

impl LegacyRuntimeAdapter {
    pub fn new(backend: Arc<dyn RuntimeBackend>) -> Self {
        Self { backend }
    }
}

impl RuntimeFactory for LegacyRuntimeAdapter {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::from_id(self.backend.name())
    }

    fn probe(&self) -> RuntimeProbe {
        let available = self.backend.is_available();
        RuntimeProbe {
            available,
            version: None,
            devices: if available {
                vec![RuntimeDevice {
                    name: "cpu".to_string(),
                    kind: DeviceKind::Cpu,
                    memory_bytes: None,
                }]
            } else {
                vec![]
            },
            reason_unavailable: if available {
                None
            } else {
                Some("Legacy backend unavailable".to_string())
            },
            capabilities: Default::default(),
        }
    }

    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        let model_path = artifacts
            .files
            .get("model")
            .or_else(|| artifacts.files.get("primary"))
            .or_else(|| artifacts.files.values().next())
            .ok_or_else(|| {
                latexsnipper_foundation::SnipperError::Runtime("No model file in artifacts".into())
            })?;

        let model_id = artifact_string(artifacts, "legacyModelId").unwrap_or("legacy-model");
        let mut handle = if let Some(bytes) = artifacts.buffers.get("model") {
            crate::model_handle::ModelHandle::with_bytes(model_id, bytes.clone())
        } else if artifact_bool(artifacts, "legacyHasExplicitPath").unwrap_or(true) {
            crate::model_handle::ModelHandle::with_path(model_id, model_path.clone())
        } else {
            crate::model_handle::ModelHandle::new(
                model_id,
                artifact_string(artifacts, "legacyCategory").unwrap_or_default(),
                artifact_string(artifacts, "legacyVariant").unwrap_or_default(),
            )
        };
        if let Some(shape) = artifacts.options.get("legacyInputShape") {
            let shape = serde_json::from_value::<Vec<usize>>(shape.clone()).map_err(|error| {
                SnipperError::Runtime(format!("invalid preserved legacy input shape: {error}"))
            })?;
            handle = handle.with_input_shape(shape);
        }
        let inner = self.backend.create_session_with_threads(
            &handle,
            options.legacy_acceleration(),
            options.max_threads,
        )?;
        let input_names = inner.input_names();
        let output_names = inner.output_names();

        Ok(Box::new(LegacySession {
            inner,
            metadata: SessionMetadata {
                runtime: self.kind(),
                model_id: model_path.to_str().map(|s| s.to_string()),
                methods: Vec::new(),
                inputs: input_names
                    .iter()
                    .map(|n| SessionTensorSpec {
                        name: n.clone(),
                        shape: vec![],
                        dtype: "unknown".into(),
                    })
                    .collect(),
                outputs: output_names
                    .iter()
                    .map(|n| SessionTensorSpec {
                        name: n.clone(),
                        shape: vec![],
                        dtype: "unknown".into(),
                    })
                    .collect(),
            },
        }))
    }

    fn clear_sessions(&self) {
        self.backend.clear_sessions();
    }
}

fn artifact_string<'a>(artifacts: &'a RuntimeArtifacts, key: &str) -> Option<&'a str> {
    artifacts
        .options
        .get(key)
        .and_then(serde_json::Value::as_str)
}

fn artifact_bool(artifacts: &RuntimeArtifacts, key: &str) -> Option<bool> {
    artifacts
        .options
        .get(key)
        .and_then(serde_json::Value::as_bool)
}

struct LegacySession {
    inner: Box<dyn InferenceSession>,
    metadata: SessionMetadata,
}

impl RuntimeSession for LegacySession {
    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        if let Some(method) = request.method.as_deref() {
            return Err(SnipperError::Runtime(format!(
                "legacy runtime does not expose named method '{method}'"
            )));
        }
        let ordered_inputs: Vec<_> = self
            .metadata
            .inputs
            .iter()
            .filter_map(|spec| request.inputs.get(&spec.name).cloned())
            .collect();
        if ordered_inputs.len() != self.metadata.inputs.len() {
            return Err(SnipperError::Inference(
                "legacy runtime request is missing one or more named inputs".to_owned(),
            ));
        }
        let outputs = self.inner.run(&ordered_inputs)?;
        let requested = request.requested_outputs.as_ref();
        let output_map: TensorMap = self
            .metadata
            .outputs
            .iter()
            .enumerate()
            .filter(|(_, spec)| requested.is_none_or(|names| names.contains(&spec.name)))
            .filter_map(|(i, spec)| outputs.get(i).map(|t| (spec.name.clone(), t.clone())))
            .collect();
        Ok(RunResponse {
            outputs: output_map,
        })
    }
}
