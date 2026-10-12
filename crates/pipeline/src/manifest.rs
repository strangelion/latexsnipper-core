use latexsnipper_foundation::{Result, SnipperError};
use serde::{Deserialize, Serialize};

const MAX_MANIFEST_YAML_BYTES: usize = 4 * 1024 * 1024;

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
        if yaml.len() > MAX_MANIFEST_YAML_BYTES {
            return Err(SnipperError::Pipeline(
                "YAML manifest exceeds the 4 MiB input budget".into(),
            ));
        }
        let options = serde_saphyr::options! {
            emit_comments: false,
            merge_keys: serde_saphyr::MergeKeyPolicy::AsOrdinary,
            reject_unsupported_tags: true,
            with_snippet: false,
            budget: serde_saphyr::budget! {
                max_depth: 64,
                max_documents: 1,
                max_events: 100_000,
                max_nodes: 100_000,
                max_aliases: 10_000,
                max_anchors: 1_024,
                max_recorded_anchor_events: 100_000,
                max_recorded_anchor_bytes: MAX_MANIFEST_YAML_BYTES,
                max_total_scalar_bytes: MAX_MANIFEST_YAML_BYTES,
                max_total_comment_bytes: MAX_MANIFEST_YAML_BYTES,
            },
            alias_limits: serde_saphyr::alias_limits! {
                max_total_replayed_events: 100_000,
                max_replay_stack_depth: 64,
                max_alias_expansions_per_anchor: 10_000,
            },
        };
        serde_saphyr::from_str_with_options(yaml, options)
            .map_err(|e| SnipperError::Pipeline(format!("Invalid YAML manifest: {}", e)))
    }

    /// Parse a manifest from JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| SnipperError::Pipeline(format!("Invalid JSON manifest: {}", e)))
    }

    /// Build a PipelineManifest from a list of StageSpecs.
    pub fn from_stage_specs(name: &str, specs: &[latexsnipper_ast::StageSpec]) -> Self {
        let nodes: Vec<NodeDef> = specs
            .iter()
            .map(|spec| NodeDef {
                name: spec.stage_id.clone(),
                node_type: format!("{:?}", spec.kind).to_lowercase(),
                depends_on: Vec::new(),
                config: spec.options.clone(),
            })
            .collect();
        Self {
            name: name.to_string(),
            description: format!("Generated from {} stage specs", specs.len()),
            nodes,
        }
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
            if !visited.contains(&node.name) && self.has_cycle(&node.name, &mut visited, &mut stack)
            {
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
    fn yaml_preserves_unicode_scalars_aliases_and_literal_merge_keys() {
        let yaml = r#"
name: "\u516c\u5f0f-pipeline"
description: |
  first line
  second line
defaults: &defaults {threshold: 0.75, enabled: true}
nodes:
  - name: a
    type: detect
    config:
      defaults: *defaults
      <<: *defaults
      enabled: false
      empty: null
      offsets: [1, -2, 0.5]
      nested: {formula: '\frac{a}{b}', label: "\u6761\u4ef6"}
"#;
        let manifest = PipelineManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "\u{516c}\u{5f0f}-pipeline");
        assert_eq!(manifest.description, "first line\nsecond line\n");
        assert_eq!(
            manifest.nodes[0].config,
            serde_json::json!({
                "defaults": {"threshold": 0.75, "enabled": true},
                "<<": {"threshold": 0.75, "enabled": true},
                "enabled": false,
                "empty": null,
                "offsets": [1, -2, 0.5],
                "nested": {"formula": r"\frac{a}{b}", "label": "\u{6761}\u{4ef6}"},
            })
        );
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn yaml_rejects_malformed_and_multiple_documents() {
        for yaml in [
            "name: broken\nnodes: [",
            "name: missing-nodes",
            "name: first\nnodes: []\n---\nname: second\nnodes: []",
            "name: duplicate\nname: second\nnodes: []",
        ] {
            assert!(PipelineManifest::from_yaml(yaml).is_err(), "{yaml}");
        }
    }

    #[test]
    fn yaml_rejects_external_tags_duplicate_config_keys_and_resource_exhaustion() {
        for yaml in [
            "name: tags\nnodes: [{name: a, type: detect, config: !include fixture.yaml}]",
            "name: tags\nnodes: [{name: a, type: detect, config: !custom {value: x}}]",
            "name: duplicates\nnodes: [{name: a, type: detect, config: {value: 1, value: 2}}]",
        ] {
            assert!(PipelineManifest::from_yaml(yaml).is_err(), "{yaml}");
        }
        let nested = format!(
            "name: depth\nnodes: [{{name: a, type: detect, config: {}0{}}}]",
            "[".repeat(128),
            "]".repeat(128)
        );
        assert!(PipelineManifest::from_yaml(&nested).is_err());
        let aliases = format!(
            "name: aliases\nvalue: &v x\nnodes: [{{name: a, type: detect, config: [{}]}}]",
            vec!["*v"; 10_001].join(",")
        );
        assert!(PipelineManifest::from_yaml(&aliases).is_err());
        let expanded = format!(
            "name: expanded\nvalue: &v '{}'\nnodes: [{{name: a, type: detect, config: [{}]}}]",
            "x".repeat(512 * 1024),
            vec!["*v"; 40].join(",")
        );
        let expansion_error = PipelineManifest::from_yaml(&expanded)
            .unwrap_err()
            .to_string();
        assert!(
            expansion_error.to_ascii_lowercase().contains("budget"),
            "{}",
            expansion_error.chars().take(300).collect::<String>()
        );
        let bounded = format!(
            "name: bounded\nvalue: &v '{}'\nnodes: [{{name: a, type: detect, config: [*v]}}]",
            "x".repeat(512 * 1024),
        );
        assert_eq!(
            PipelineManifest::from_yaml(&bounded).unwrap().nodes[0].config[0]
                .as_str()
                .unwrap()
                .len(),
            512 * 1024
        );
        let oversized = " ".repeat(MAX_MANIFEST_YAML_BYTES + 1);
        assert!(PipelineManifest::from_yaml(&oversized)
            .unwrap_err()
            .to_string()
            .contains("4 MiB input budget"));
    }

    #[test]
    fn yaml_keeps_environment_like_text_literal_and_does_not_echo_source_snippets() {
        let manifest = PipelineManifest::from_yaml(
            "name: literal\nnodes: [{name: a, type: detect, config: {token: '${TOKEN}'}}]",
        )
        .unwrap();
        assert_eq!(manifest.nodes[0].config["token"], "${TOKEN}");
        let error =
            PipelineManifest::from_yaml("# synthetic-private-context\nname: broken\nnodes: [")
                .unwrap_err()
                .to_string();
        assert!(!error.contains("synthetic-private-context"), "{error}");
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
