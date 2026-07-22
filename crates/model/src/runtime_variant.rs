use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One executable artifact set for a model package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeVariant {
    pub id: String,
    /// Canonical RuntimeKind identifier. Kept as a string here to avoid a
    /// dependency cycle between the model schema and runtime implementation.
    pub runtime: String,
    #[serde(default)]
    pub status: VariantStatus,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub artifacts: BTreeMap<String, String>,
    /// Common RuntimeOptions plus runtime-specific flattened keys.
    #[serde(default)]
    pub options: Option<BTreeMap<String, serde_json::Value>>,
    /// Target constraints such as `windows`, `linux-x86_64`, `macos`, or
    /// `apple`. Empty means every platform.
    #[serde(default)]
    pub platforms: Vec<String>,
    /// Capability identifiers required from RuntimeProbe.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Ordered, explicit fallback variant ids.
    #[serde(default)]
    pub fallbacks: Vec<String>,
}

impl RuntimeVariant {
    pub fn new(id: impl Into<String>, runtime: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            runtime: runtime.into(),
            status: VariantStatus::Stable,
            priority: 0,
            artifacts: BTreeMap::new(),
            options: None,
            platforms: Vec::new(),
            capabilities: Vec::new(),
            fallbacks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VariantStatus {
    #[default]
    Stable,
    Experimental,
    Deprecated,
    Disabled,
    Broken,
}

impl VariantStatus {
    pub fn is_selectable(self) -> bool {
        matches!(self, Self::Stable | Self::Experimental)
    }
}
