//! Plugin loading infrastructure.
//!
//! This module provides functionality to load ModelPlugin implementations
//! from various sources:
//! - Compiled Rust crates (feature-gated)
//! - Dynamic libraries (.so/.dll) - future
//! - WASM modules - future
//!
//! # Safety
//!
//! Plugin loading involves executing external code. The plugin system
//! enforces safety through:
//! - Sandboxed execution environment
//! - Restricted file system access
//! - No network access (unless explicitly granted)
//! - Resource limits

use crate::model_plugin::{ModelPlugin, ModelPluginRegistry};
use latexsnipper_foundation::{Result, SnipperError};
use std::path::Path;

/// Load plugins from a directory.
///
/// Scans the directory for plugin files and registers them.
pub fn load_plugins_from_dir(
    registry: &mut ModelPluginRegistry,
    dir: &Path,
) -> Result<Vec<String>> {
    let mut loaded = Vec::new();

    if !dir.is_dir() {
        return Err(SnipperError::Model(format!(
            "Plugin directory not found: {}",
            dir.display()
        )));
    }

    for entry in std::fs::read_dir(dir).map_err(|e| {
        SnipperError::Model(format!("Failed to read plugin directory: {}", e))
    })? {
        let entry = entry.map_err(|e| {
            SnipperError::Model(format!("Failed to read entry: {}", e))
        })?;

        let path = entry.path();

        // Skip hidden files and directories
        if path.file_name().map_or(false, |n| n.to_string_lossy().starts_with('.')) {
            continue;
        }

        // Try to load as different plugin types
        if path.extension().map_or(false, |e| e == "toml") {
            // Try to load as plugin manifest
            match load_plugin_from_manifest(&path) {
                Ok(plugin) => {
                    let name = plugin.name().to_string();
                    registry.register(plugin)?;
                    loaded.push(name);
                }
                Err(e) => {
                    log::warn!("Failed to load plugin from {}: {}", path.display(), e);
                }
            }
        }
    }

    Ok(loaded)
}

/// Load a plugin from a manifest file.
fn load_plugin_from_manifest(path: &Path) -> Result<Box<dyn ModelPlugin>> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        SnipperError::Model(format!("Failed to read manifest: {}", e))
    })?;

    let manifest: PluginManifest = toml::from_str(&content).map_err(|e| {
        SnipperError::Model(format!("Failed to parse manifest: {}", e))
    })?;

    log::info!(
        "Loading plugin '{}' v{} (type: {}, adapters: {:?}){}",
        manifest.name,
        manifest.version,
        manifest.plugin_type,
        manifest.adapters,
        manifest.description.as_deref().map(|d| format!(" - {}", d)).unwrap_or_default()
    );

    // For now, only support built-in plugins
    // Dynamic loading will be added later
    match manifest.plugin_type.as_str() {
        "builtin" => {
            // Look for built-in plugin by name
            create_builtin_plugin(&manifest.name, &manifest.adapters)
        }
        _ => Err(SnipperError::Model(format!(
            "Unsupported plugin type: {}",
            manifest.plugin_type
        ))),
    }
}

/// Create a built-in plugin by name.
fn create_builtin_plugin(name: &str, adapters: &[String]) -> Result<Box<dyn ModelPlugin>> {
    // This is where built-in plugins would be registered
    // For now, return an error
    Err(SnipperError::Model(format!(
        "Built-in plugin '{}' not found (adapters: {:?})",
        name, adapters
    )))
}

/// Plugin manifest file format.
#[derive(Debug, serde::Deserialize)]
struct PluginManifest {
    /// Plugin name.
    name: String,
    /// Plugin version.
    version: String,
    /// Plugin type (builtin, wasm, dynamic).
    #[serde(rename = "type")]
    plugin_type: String,
    /// Supported adapters.
    #[serde(default)]
    adapters: Vec<String>,
    /// Description.
    #[serde(default)]
    description: Option<String>,
}

/// Load a WASM plugin from a file.
///
/// This is a placeholder for future WASM plugin support.
/// WASM plugins would need to implement specific exported functions:
/// - `plugin_name() -> String`
/// - `plugin_version() -> String`
/// - `supported_adapters() -> String` (JSON array)
/// - `create_package(adapter: String, manifest: String) -> Result<String>`
pub fn load_wasm_plugin(_path: &Path) -> Result<Box<dyn ModelPlugin>> {
    Err(SnipperError::Other(
        "WASM plugin loading not yet implemented".into(),
    ))
}

/// Load a dynamic library plugin.
///
/// This is a placeholder for future dynamic library support.
pub fn load_dynamic_plugin(_path: &Path) -> Result<Box<dyn ModelPlugin>> {
    Err(SnipperError::Other(
        "Dynamic library plugin loading not yet implemented".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manifest_parsing() {
        let toml = r#"
name = "test-plugin"
version = "1.0.0"
type = "builtin"
adapters = ["test-adapter-v1"]
description = "A test plugin"
"#;

        let manifest: PluginManifest = toml::from_str(toml).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.plugin_type, "builtin");
        assert_eq!(manifest.adapters, vec!["test-adapter-v1"]);
    }
}