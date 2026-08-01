//! The canonical named-tensor runtime session API.

use std::collections::BTreeMap;

use latexsnipper_foundation::Result;
use latexsnipper_tensor::Tensor;
use serde::{Deserialize, Serialize};

use crate::RuntimeKind;

pub type TensorMap = BTreeMap<String, Tensor>;

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub method: Option<String>,
    pub inputs: TensorMap,
    pub requested_outputs: Option<Vec<String>>,
}

impl RunRequest {
    pub fn new(inputs: TensorMap) -> Self {
        Self {
            method: None,
            inputs,
            requested_outputs: None,
        }
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    pub fn requesting(mut self, outputs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.requested_outputs = Some(outputs.into_iter().map(Into::into).collect());
        self
    }

    /// Compatibility helper for positional callers. Tensor names remain the
    /// source of truth; duplicate names are rejected by overwriting only in
    /// legacy code paths that already lacked a way to distinguish them.
    pub fn from_tensors(tensors: &[Tensor]) -> Self {
        Self::new(
            tensors
                .iter()
                .map(|tensor| (tensor.name().to_owned(), tensor.clone()))
                .collect(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct RunResponse {
    pub outputs: TensorMap,
}

impl RunResponse {
    pub fn first_output(&self) -> Option<&Tensor> {
        self.outputs.values().next()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub runtime: RuntimeKind,
    pub model_id: Option<String>,
    /// Ordered providers requested by the caller. This is diagnostic intent,
    /// never evidence that a provider actually created the session.
    #[serde(default)]
    pub requested_providers: Vec<String>,
    /// Provider selected by the runtime after a session was successfully
    /// created. `None` is intentionally fail-closed for runtimes that cannot
    /// expose this fact.
    #[serde(default)]
    pub effective_provider: Option<String>,
    /// Every provider considered while creating this session, in order.
    #[serde(default)]
    pub fallback_chain: Vec<ProviderAttempt>,
    /// Runtime-owned diagnostics explaining rejected providers or fallback.
    #[serde(default)]
    pub fallback_diagnostics: Vec<String>,
    #[serde(default)]
    pub methods: Vec<String>,
    pub inputs: Vec<TensorSpec>,
    pub outputs: Vec<TensorSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAttempt {
    pub provider: String,
    #[serde(default)]
    pub selected: bool,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TensorSpec {
    pub name: String,
    pub shape: Vec<Option<i64>>,
    pub dtype: String,
}

/// Runtime-neutral inference session. Model-specific preprocessing,
/// generation, tokenization, and postprocessing remain in model adapters.
pub trait RuntimeSession: Send + Sync {
    fn metadata(&self) -> &SessionMetadata;
    fn run(&self, request: RunRequest) -> Result<RunResponse>;
}
