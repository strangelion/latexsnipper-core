//! Model artifacts selected for one runtime variant.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::RuntimeKind;

/// Fully resolved files passed to a runtime factory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeArtifacts {
    pub runtime: RuntimeKind,
    /// Artifact role to resolved path (`model`, `encoder`, `params`, ...).
    pub files: BTreeMap<String, PathBuf>,
    /// Optional in-memory artifacts, primarily for WASM and embedders that
    /// cannot expose a filesystem path. Native runtime packages normally use
    /// `files` so large model data is not copied through this API.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub buffers: BTreeMap<String, Vec<u8>>,
    /// Artifact/load configuration that is not an execution-provider option.
    #[serde(default)]
    pub options: BTreeMap<String, serde_json::Value>,
}

impl RuntimeArtifacts {
    pub fn new(runtime: RuntimeKind) -> Self {
        Self {
            runtime,
            files: BTreeMap::new(),
            buffers: BTreeMap::new(),
            options: BTreeMap::new(),
        }
    }

    pub fn with_file(mut self, role: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.files.insert(role.into(), path.into());
        self
    }

    pub fn primary_model(&self) -> Option<&PathBuf> {
        self.files
            .get("model")
            .or_else(|| self.files.get("primary"))
    }

    pub fn with_buffer(mut self, role: impl Into<String>, bytes: Vec<u8>) -> Self {
        self.buffers.insert(role.into(), bytes);
        self
    }
}
