use std::collections::HashMap;
use log::info;

use latexsnipper_foundation::{SnipperError, Result};

use crate::plugin::Plugin;
use crate::request::PluginRequest;
use crate::response::PluginResponse;

/// Registry for managing plugins.
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Register a plugin.
    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> Result<()> {
        let name = plugin.name().to_string();
        info!("Registering plugin: {} v{}", name, plugin.version());

        plugin.init()?;

        if self.plugins.contains_key(&name) {
            return Err(SnipperError::Plugin(format!(
                "Plugin '{}' is already registered",
                name
            )));
        }

        self.plugins.insert(name, plugin);
        Ok(())
    }

    /// Unregister a plugin.
    pub fn unregister(&mut self, name: &str) -> Result<()> {
        if let Some(mut plugin) = self.plugins.remove(name) {
            info!("Unregistering plugin: {}", name);
            plugin.cleanup()?;
        }
        Ok(())
    }

    /// Check if a plugin is registered.
    pub fn has(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// Get a list of registered plugin names.
    pub fn list(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// Send a request to a specific plugin.
    pub fn handle(&self, plugin_name: &str, request: &PluginRequest) -> Result<PluginResponse> {
        let plugin = self.plugins.get(plugin_name).ok_or_else(|| {
            SnipperError::Plugin(format!("Plugin '{}' not found", plugin_name))
        })?;

        plugin.handle(request)
    }

    /// Send a request to all registered plugins in sequence.
    /// Each plugin receives the output of the previous plugin.
    pub fn handle_all(&self, request: &PluginRequest) -> Result<PluginResponse> {
        let mut current = request.clone();

        for (name, plugin) in &self.plugins {
            info!("Processing with plugin: {}", name);
            let response = plugin.handle(&current)?;
            current = PluginRequest {
                action: current.action.clone(),
                document: response.document,
                metadata: current.metadata,
            };
        }

        Ok(PluginResponse {
            document: current.document,
            metadata: current.metadata,
        })
    }

    /// Send a request to plugins that match a filter.
    pub fn handle_filtered(
        &self,
        request: &PluginRequest,
        filter: impl Fn(&str) -> bool,
    ) -> Result<PluginResponse> {
        let mut current = request.clone();

        for (name, plugin) in &self.plugins {
            if filter(name) {
                info!("Processing with plugin: {}", name);
                let response = plugin.handle(&current)?;
                current = PluginRequest {
                    action: current.action.clone(),
                    document: response.document,
                    metadata: current.metadata,
                };
            }
        }

        Ok(PluginResponse {
            document: current.document,
            metadata: current.metadata,
        })
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::TransformPlugin;
    use latexsnipper_ast::{Document, Page};

    fn test_plugin() -> TransformPlugin {
        TransformPlugin::new("test-plugin", "0.1.0", |doc| {
            // Add a page if empty
            if doc.pages.is_empty() {
                doc.pages.push(Page {
                    width: 0.0,
                    height: 0.0,
                    blocks: vec![],
                    page_number: None,
                });
            }
            Ok(())
        })
    }

    #[test]
    fn register_and_list() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(test_plugin())).unwrap();
        assert!(registry.has("test-plugin"));
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn unregister() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(test_plugin())).unwrap();
        registry.unregister("test-plugin").unwrap();
        assert!(!registry.has("test-plugin"));
    }

    #[test]
    fn handle_request() {
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(test_plugin())).unwrap();

        let request = PluginRequest::new("test", Document::new());
        let response = registry.handle("test-plugin", &request).unwrap();
        assert!(!response.document.pages.is_empty());
    }

    #[test]
    fn handle_not_found() {
        let registry = PluginRegistry::new();
        let request = PluginRequest::new("test", Document::new());
        assert!(registry.handle("nonexistent", &request).is_err());
    }
}
