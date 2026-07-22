//! Runtime probing and capability reporting.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::DeviceKind;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    /// Tensor dtype identifiers accepted by the runtime bridge.
    #[serde(default)]
    pub tensor_dtypes: BTreeSet<String>,
    /// Execution providers/delegates understood by this build.
    #[serde(default)]
    pub execution_providers: BTreeSet<String>,
    /// Named methods exposed by a loaded program, when known during probe.
    #[serde(default)]
    pub methods: BTreeSet<String>,
    /// Free-form stable capability identifiers.
    #[serde(default)]
    pub features: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDevice {
    pub name: String,
    pub kind: DeviceKind,
    pub memory_bytes: Option<u64>,
}

/// Availability snapshot returned by a runtime factory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProbe {
    pub available: bool,
    pub version: Option<String>,
    #[serde(default)]
    pub devices: Vec<RuntimeDevice>,
    pub reason_unavailable: Option<String>,
    #[serde(default)]
    pub capabilities: RuntimeCapabilities,
}

impl RuntimeProbe {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            version: None,
            devices: Vec::new(),
            reason_unavailable: Some(reason.into()),
            capabilities: RuntimeCapabilities::default(),
        }
    }

    pub fn available(version: Option<String>, devices: Vec<RuntimeDevice>) -> Self {
        Self {
            available: true,
            version,
            devices,
            reason_unavailable: None,
            capabilities: RuntimeCapabilities::default(),
        }
    }
}
