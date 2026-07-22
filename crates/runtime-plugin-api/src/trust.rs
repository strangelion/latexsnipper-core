//! Application-owned trust enrollment for native runtime plugins.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::descriptor::{validate_runtime_id, validate_sha256};
use crate::error::{plugin_runtime_error, PluginRuntimeResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustedRuntimePlugin {
    pub runtime_id: String,
    pub library_path: PathBuf,
    pub sha256: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePluginTrustStore {
    schema_version: u32,
    entries: BTreeMap<String, TrustedRuntimePlugin>,
}

impl Default for RuntimePluginTrustStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            entries: BTreeMap::new(),
        }
    }
}

impl RuntimePluginTrustStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_json(bytes: &[u8]) -> PluginRuntimeResult<Self> {
        let store: Self = serde_json::from_slice(bytes).map_err(|error| {
            plugin_runtime_error(format!(
                "invalid runtime plugin trust registry JSON: {error}"
            ))
        })?;
        if store.schema_version != 1 {
            return Err(plugin_runtime_error(format!(
                "unsupported runtime plugin trust registry schema version {}",
                store.schema_version
            )));
        }
        for (key, entry) in &store.entries {
            validate_runtime_id(key)?;
            validate_runtime_id(&entry.runtime_id)?;
            validate_sha256(&entry.sha256)?;
            if key != &entry.runtime_id || !entry.library_path.is_absolute() {
                return Err(plugin_runtime_error(
                    "runtime plugin trust registry contains a mismatched id or non-absolute library path",
                ));
            }
        }
        Ok(store)
    }

    pub fn to_json_pretty(&self) -> PluginRuntimeResult<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|error| plugin_runtime_error(error.to_string()))
    }

    /// Enroll an already installed library. Enrollment verifies the exact
    /// bytes but leaves execution disabled until `set_enabled(id, true)`.
    pub fn enroll(
        &mut self,
        runtime_id: impl Into<String>,
        library_path: impl AsRef<Path>,
        expected_sha256: impl Into<String>,
    ) -> PluginRuntimeResult<&TrustedRuntimePlugin> {
        let runtime_id = runtime_id.into();
        validate_runtime_id(&runtime_id)?;
        let expected_sha256 = expected_sha256.into().to_ascii_lowercase();
        validate_sha256(&expected_sha256)?;
        let library_path = canonical_regular_file(library_path.as_ref())?;
        let actual = sha256_file(&library_path)?;
        if actual != expected_sha256 {
            return Err(plugin_runtime_error(format!(
                "refusing to trust runtime plugin '{runtime_id}': expected SHA-256 {expected_sha256}, found {actual}"
            )));
        }
        if self.entries.contains_key(&runtime_id) {
            return Err(plugin_runtime_error(format!(
                "runtime plugin '{runtime_id}' is already enrolled"
            )));
        }
        self.entries.insert(
            runtime_id.clone(),
            TrustedRuntimePlugin {
                runtime_id: runtime_id.clone(),
                library_path,
                sha256: actual,
                enabled: false,
            },
        );
        Ok(&self.entries[&runtime_id])
    }

    pub fn set_enabled(&mut self, runtime_id: &str, enabled: bool) -> PluginRuntimeResult<()> {
        let entry = self.entries.get_mut(runtime_id).ok_or_else(|| {
            plugin_runtime_error(format!(
                "runtime plugin '{runtime_id}' must be enrolled before it can be enabled"
            ))
        })?;
        entry.enabled = enabled;
        Ok(())
    }

    pub fn get(&self, runtime_id: &str) -> Option<&TrustedRuntimePlugin> {
        self.entries.get(runtime_id)
    }

    pub fn entries(&self) -> impl Iterator<Item = &TrustedRuntimePlugin> {
        self.entries.values()
    }
}

pub(crate) fn canonical_regular_file(path: &Path) -> PluginRuntimeResult<PathBuf> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        plugin_runtime_error(format!(
            "inspect runtime plugin library '{}': {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(plugin_runtime_error(format!(
            "runtime plugin library must be a real regular file: {}",
            path.display()
        )));
    }
    path.canonicalize().map_err(|error| {
        plugin_runtime_error(format!(
            "canonicalize runtime plugin library '{}': {error}",
            path.display()
        ))
    })
}

pub(crate) fn sha256_file(path: &Path) -> PluginRuntimeResult<String> {
    let mut file = File::open(path).map_err(|error| {
        plugin_runtime_error(format!(
            "open runtime plugin library '{}': {error}",
            path.display()
        ))
    })?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            plugin_runtime_error(format!(
                "read runtime plugin library '{}': {error}",
                path.display()
            ))
        })?;
        if read == 0 {
            return Ok(hex::encode(hash.finalize()));
        }
        hash.update(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrollment_is_disabled_until_explicit_enable() {
        let root =
            std::env::temp_dir().join(format!("latexsnipper-runtime-trust-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let library = root.join("plugin.bin");
        std::fs::write(&library, b"trusted plugin").unwrap();
        let digest = sha256_file(&library).unwrap();
        let mut trust = RuntimePluginTrustStore::new();
        let entry = trust.enroll("vendor.npu", &library, digest).unwrap();
        assert!(!entry.enabled);
        trust.set_enabled("vendor.npu", true).unwrap();
        assert!(trust.get("vendor.npu").unwrap().enabled);

        let encoded = trust.to_json_pretty().unwrap();
        let decoded = RuntimePluginTrustStore::from_json(&encoded).unwrap();
        assert_eq!(decoded.get("vendor.npu"), trust.get("vendor.npu"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
