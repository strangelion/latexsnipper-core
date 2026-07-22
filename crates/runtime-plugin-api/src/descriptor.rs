//! Installed runtime plugin descriptor schema.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::error::{plugin_runtime_error, PluginRuntimeResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePluginDescriptor {
    pub schema_version: u32,
    pub runtime_id: String,
    pub plugin_version: String,
    /// Package-relative dynamic-library path. It is never resolved relative to
    /// a model directory.
    pub library: String,
    pub sha256: String,
}

impl RuntimePluginDescriptor {
    pub fn validate(&self) -> PluginRuntimeResult<()> {
        if self.schema_version != 1 {
            return Err(plugin_runtime_error(format!(
                "unsupported runtime plugin descriptor schema version {}",
                self.schema_version
            )));
        }
        validate_runtime_id(&self.runtime_id)?;
        if self.plugin_version.trim().is_empty() || self.plugin_version.len() > 128 {
            return Err(plugin_runtime_error(
                "runtime plugin version must contain 1 to 128 characters",
            ));
        }
        let library = Path::new(&self.library);
        if self.library.trim().is_empty()
            || library.is_absolute()
            || library.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(plugin_runtime_error(
                "runtime plugin library must be a package-relative path without '..'",
            ));
        }
        validate_sha256(&self.sha256)
    }
}

pub(crate) fn validate_runtime_id(runtime_id: &str) -> PluginRuntimeResult<()> {
    if runtime_id.is_empty()
        || runtime_id.len() > 128
        || runtime_id.starts_with("custom:")
        || !runtime_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(plugin_runtime_error(
            "custom runtime id must contain 1 to 128 ASCII letters, digits, '.', '_', or '-' and omit the 'custom:' prefix",
        ));
    }
    if matches!(
        runtime_id,
        "onnx-runtime"
            | "onnxruntime"
            | "onnx"
            | "paddle-inference"
            | "paddle"
            | "executorch"
            | "tensorrt"
            | "tensorrt-rtx"
            | "coreml"
            | "core-ml"
    ) {
        return Err(plugin_runtime_error(format!(
            "runtime id '{runtime_id}' is reserved by a built-in runtime"
        )));
    }
    Ok(())
}

pub(crate) fn validate_sha256(value: &str) -> PluginRuntimeResult<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(plugin_runtime_error(
            "runtime plugin SHA-256 must contain exactly 64 hexadecimal characters",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> RuntimePluginDescriptor {
        RuntimePluginDescriptor {
            schema_version: 1,
            runtime_id: "vendor.npu".to_owned(),
            plugin_version: "1.0.0".to_owned(),
            library: "bin/vendor_npu.dll".to_owned(),
            sha256: "a".repeat(64),
        }
    }

    #[test]
    fn accepts_a_scoped_relative_library() {
        assert!(descriptor().validate().is_ok());
    }

    #[test]
    fn rejects_builtin_ids_and_path_traversal() {
        let mut value = descriptor();
        value.runtime_id = "onnx-runtime".to_owned();
        assert!(value.validate().is_err());
        value.runtime_id = "vendor.npu".to_owned();
        value.library = "../plugin.dll".to_owned();
        assert!(value.validate().is_err());
    }
}
