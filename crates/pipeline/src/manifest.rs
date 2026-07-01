use latexsnipper_foundation::{Result, SnipperError};
use serde::{Deserialize, Serialize};

/// A declarative pipeline definition loaded from YAML/JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineManifest {
    /// Pipeline name.
    pub name: String,
    /// Pipeline description.
    #[serde(default)]
    pub description: String,
    /// List of node definitions.
    pub nodes: Vec<NodeDef>,
}

/// Definition of a node in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDef {
    /// Unique node name.
    pub name: String,
    /// Node type (e.g., "preprocess", "detect", "recognize", "transform").
    #[serde(rename = "type")]
    pub node_type: String,
    /// Nodes that must complete before this node runs.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Node-specific configuration.
    #[serde(default)]
    pub config: serde_json::Value,
}

impl PipelineManifest {
    /// Parse a manifest from YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml)
            .map_err(|e| SnipperError::Pipeline(format!("Invalid YAML manifest: {}", e)))
    }

    /// Parse a manifest from JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| SnipperError::Pipeline(format!("Invalid JSON manifest: {}", e)))
    }

    /// Validate the manifest (check for missing dependencies).
    pub fn validate(&self) -> Result<()> {
        let names: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.name.as_str()).collect();

        for node in &self.nodes {
            for dep in &node.depends_on {
                if !names.contains(dep.as_str()) {
                    return Err(SnipperError::Pipeline(format!(
                        "Node '{}' depends on unknown node '{}'",
                        node.name, dep
                    )));
                }
            }
        }

        // Check for cycles (simple DFS)
        let mut visited = std::collections::HashSet::new();
        let mut stack = std::collections::HashSet::new();

        for node in &self.nodes {
            if !visited.contains(&node.name) && self.has_cycle(&node.name, &mut visited, &mut stack) {
                return Err(SnipperError::Pipeline(
                    "Circular dependency detected in manifest".into(),
                ));
            }
        }

        Ok(())
    }

    fn has_cycle(
        &self,
        name: &str,
        visited: &mut std::collections::HashSet<String>,
        stack: &mut std::collections::HashSet<String>,
    ) -> bool {
        if stack.contains(name) {
            return true;
        }
        if visited.contains(name) {
            return false;
        }

        visited.insert(name.to_string());
        stack.insert(name.to_string());

        if let Some(node) = self.nodes.iter().find(|n| n.name == name) {
            for dep in &node.depends_on {
                if self.has_cycle(dep, visited, stack) {
                    return true;
                }
            }
        }

        stack.remove(name);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_yaml_manifest() {
        let yaml = r#"
name: test-pipeline
description: A test pipeline
nodes:
  - name: preprocess
    type: preprocess
  - name: detect
    type: detect
    depends_on: [preprocess]
  - name: recognize
    type: recognize
    depends_on: [detect]
"#;
        let manifest = PipelineManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "test-pipeline");
        assert_eq!(manifest.nodes.len(), 3);
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn parse_json_manifest() {
        let json = r#"{
            "name": "test-pipeline",
            "nodes": [
                {"name": "a", "type": "preprocess"},
                {"name": "b", "type": "detect", "depends_on": ["a"]}
            ]
        }"#;
        let manifest = PipelineManifest::from_json(json).unwrap();
        assert_eq!(manifest.nodes.len(), 2);
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn detect_missing_dependency() {
        let yaml = r#"
name: broken
nodes:
  - name: a
    type: detect
    depends_on: [nonexistent]
"#;
        let manifest = PipelineManifest::from_yaml(yaml).unwrap();
        assert!(manifest.validate().is_err());
    }
}
