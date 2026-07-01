use latexsnipper_foundation::{Result, SnipperError};
use log::info;

use crate::context::PipelineContext;
use crate::node::PipelineNode;

/// A node entry in the pipeline graph with its dependencies.
struct NodeEntry {
    name: String,
    node: Box<dyn PipelineNode>,
    depends_on: Vec<String>,
}

/// A pipeline graph that executes nodes respecting dependency order (DAG).
pub struct PipelineGraph {
    name: String,
    entries: Vec<NodeEntry>,
}

impl PipelineGraph {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            entries: Vec::new(),
        }
    }

    /// Add a node with no dependencies.
    pub fn add_node(&mut self, node: Box<dyn PipelineNode>) -> &mut Self {
        let name = node.name().to_string();
        self.entries.push(NodeEntry {
            name,
            node,
            depends_on: Vec::new(),
        });
        self
    }

    /// Add a node that depends on other nodes.
    pub fn add_node_with_deps(
        &mut self,
        node: Box<dyn PipelineNode>,
        depends_on: Vec<String>,
    ) -> &mut Self {
        let name = node.name().to_string();
        self.entries.push(NodeEntry {
            name,
            node,
            depends_on,
        });
        self
    }

    /// Execute all nodes in topological order (respects dependencies).
    pub async fn run(&self, ctx: &mut PipelineContext) -> Result<()> {
        let order = self.topological_sort()?;
        info!(
            "Pipeline '{}' starting with {} nodes",
            self.name,
            order.len()
        );

        for (i, name) in order.iter().enumerate() {
            if ctx.cancelled {
                info!("Pipeline '{}' cancelled at node {}", self.name, name);
                break;
            }

            let entry = self
                .entries
                .iter()
                .find(|e| &e.name == name)
                .ok_or_else(|| SnipperError::Pipeline(format!("Node '{}' not found", name)))?;

            info!("Pipeline '{}' executing node {}: {}", self.name, i, name);
            entry.node.process(ctx).await?;
        }

        info!("Pipeline '{}' completed", self.name);
        Ok(())
    }

    /// Topological sort using Kahn's algorithm.
    fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize all nodes
        for entry in &self.entries {
            in_degree.entry(entry.name.clone()).or_insert(0);
            dependents.entry(entry.name.clone()).or_default();
        }

        // Count incoming edges
        for entry in &self.entries {
            for dep in &entry.depends_on {
                if !self.entries.iter().any(|e| &e.name == dep) {
                    return Err(SnipperError::Pipeline(format!(
                        "Node '{}' depends on unknown node '{}'",
                        entry.name, dep
                    )));
                }
                *in_degree.entry(entry.name.clone()).or_insert(0) += 1;
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .push(entry.name.clone());
            }
        }

        // Start with nodes that have no dependencies
        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(name, _)| name.clone())
            .collect();
        queue.sort(); // Deterministic order

        let mut result = Vec::new();

        while !queue.is_empty() {
            let name = queue.remove(0);
            result.push(name.clone());

            if let Some(deps) = dependents.get(&name) {
                for dep_name in deps {
                    let deg = in_degree.get_mut(dep_name).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(dep_name.clone());
                    }
                }
                queue.sort(); // Keep deterministic
            }
        }

        if result.len() != self.entries.len() {
            return Err(SnipperError::Pipeline(
                "Circular dependency detected in pipeline graph".into(),
            ));
        }

        Ok(result)
    }

    /// Get the number of nodes.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the pipeline is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the pipeline name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

use std::collections::HashMap;
