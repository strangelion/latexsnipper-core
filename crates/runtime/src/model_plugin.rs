//! Model plugin system for extending ModelPackage with external adapters.
//!
//! This module provides traits and types for creating model plugins that
//! can be loaded dynamically (WASM, dynamic libraries) or compiled as
//! feature-gated Rust crates.
//!
//! # Safety Model
//!
//! Plugins are sandboxed and cannot:
//! - Access the file system directly (must use provided paths)
//! - Execute arbitrary code (only inference operations)
//! - Access network (must use provided endpoints)
//!
//! # Three-Phase Strategy
//!
//! - **Phase 1**: Built-in Rust adapters (completed)
//! - **Phase 2**: Configurable adapters via manifest (completed)
//! - **Phase 3**: Dynamic plugins via WASM/dynamic libraries (this module)

use crate::model_package::{ModelPackage, ModelTask};
use latexsnipper_foundation::Result;
use std::path::Path;

/// A model plugin that can be loaded dynamically.
///
/// This trait extends `ModelPlugin` with lifecycle management
/// for dynamic loading scenarios.
pub trait ModelPlugin: Send + Sync {
    /// Plugin name (unique identifier).
    fn name(&self) -> &str;

    /// Plugin version.
    fn version(&self) -> &str;

    /// Supported adapter names.
    ///
    /// Returns a list of adapter names this plugin can handle.
    /// For example: `["yolov8-detection-v1", "yolov8-detection-v2"]`
    fn supported_adapters(&self) -> Vec<&str>;

    /// Create a ModelPackage for the given adapter and manifest.
    ///
    /// The plugin should examine the manifest and create an appropriate
    /// ModelPackage implementation.
    fn create_package(
        &self,
        adapter: &str,
        manifest: &dyn ModelManifestView,
        model_dir: &Path,
    ) -> Result<Box<dyn ModelPackage>>;

    /// Initialize the plugin.
    /// Called once when the plugin is loaded.
    fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Cleanup resources.
    /// Called when the plugin is unloaded.
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A view of model manifest for plugins.
///
/// This provides read-only access to manifest data without
/// exposing the full ModelManifest type.
pub trait ModelManifestView {
    /// Model identifier (category/variant).
    fn id(&self) -> &str;

    /// What task this model performs.
    fn task(&self) -> ModelTask;

    /// Adapter type (e.g., "yolov8-detection-v1").
    fn adapter(&self) -> &str;

    /// Model version.
    fn version(&self) -> &str;

    /// Input tensor name.
    fn input_name(&self) -> &str;

    /// Input tensor shape.
    fn input_shape(&self) -> &[i64];

    /// Get a file path by name.
    fn get_file(&self, name: &str) -> Option<&str>;
}

/// Registry for model plugins.
pub struct ModelPluginRegistry {
    plugins: Vec<Box<dyn ModelPlugin>>,
    adapter_to_plugin: std::collections::HashMap<String, usize>,
}

impl ModelPluginRegistry {
    /// Create an empty plugin registry.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            adapter_to_plugin: std::collections::HashMap::new(),
        }
    }

    /// Register a model plugin.
    pub fn register(&mut self, mut plugin: Box<dyn ModelPlugin>) -> Result<()> {
        plugin.init()?;

        let idx = self.plugins.len();
        let adapters = plugin.supported_adapters();

        for adapter in adapters {
            self.adapter_to_plugin.insert(adapter.to_string(), idx);
        }

        self.plugins.push(plugin);
        Ok(())
    }

    /// Find a plugin that can handle the given adapter.
    pub fn find_plugin(&self, adapter: &str) -> Option<&dyn ModelPlugin> {
        let idx = self.adapter_to_plugin.get(adapter)?;
        self.plugins.get(*idx).map(|p| p.as_ref())
    }

    /// Create a package using a registered plugin.
    pub fn create_package(
        &self,
        adapter: &str,
        manifest: &dyn ModelManifestView,
        model_dir: &Path,
    ) -> Result<Option<Box<dyn ModelPackage>>> {
        let plugin = match self.find_plugin(adapter) {
            Some(p) => p,
            None => return Ok(None),
        };

        let package = plugin.create_package(adapter, manifest, model_dir)?;
        Ok(Some(package))
    }

    /// List all registered adapter names.
    pub fn registered_adapters(&self) -> Vec<&str> {
        self.adapter_to_plugin.keys().map(|s| s.as_str()).collect()
    }

    /// List all registered plugins.
    pub fn plugins(&self) -> Vec<&dyn ModelPlugin> {
        self.plugins.iter().map(|p| p.as_ref()).collect()
    }
}

impl Default for ModelPluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper to adapt ModelManifest for ModelManifestView trait.
pub struct ManifestAdapter<'a> {
    manifest: &'a crate::model_registry::ModelManifest,
}

impl<'a> ManifestAdapter<'a> {
    pub fn new(manifest: &'a crate::model_registry::ModelManifest) -> Self {
        Self { manifest }
    }
}

impl<'a> ModelManifestView for ManifestAdapter<'a> {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn task(&self) -> ModelTask {
        self.manifest.task
    }

    fn adapter(&self) -> &str {
        &self.manifest.adapter
    }

    fn version(&self) -> &str {
        &self.manifest.version
    }

    fn input_name(&self) -> &str {
        &self.manifest.input.name
    }

    fn input_shape(&self) -> &[i64] {
        &self.manifest.input.shape
    }

    fn get_file(&self, name: &str) -> Option<&str> {
        match name {
            "primary" => self.manifest.files.primary.as_deref(),
            "encoder" => self.manifest.files.encoder.as_deref(),
            "decoder" => self.manifest.files.decoder.as_deref(),
            "tokenizer" => self.manifest.files.tokenizer.as_deref(),
            "config" => self.manifest.files.config.as_deref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin {
        name: String,
    }

    impl ModelPlugin for TestPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn supported_adapters(&self) -> Vec<&str> {
            vec!["test-adapter-v1"]
        }

        fn create_package(
            &self,
            _adapter: &str,
            _manifest: &dyn ModelManifestView,
            _model_dir: &Path,
        ) -> Result<Box<dyn ModelPackage>> {
            Err(latexsnipper_foundation::SnipperError::Other(
                "Not implemented".into(),
            ))
        }
    }

    #[test]
    fn test_plugin_registry() {
        let mut registry = ModelPluginRegistry::new();
        let plugin = Box::new(TestPlugin {
            name: "test".to_string(),
        });

        registry.register(plugin).unwrap();

        assert!(registry.find_plugin("test-adapter-v1").is_some());
        assert!(registry.find_plugin("unknown").is_none());
        assert_eq!(registry.registered_adapters(), vec!["test-adapter-v1"]);
    }
}
