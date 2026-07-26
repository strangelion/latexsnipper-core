use latexsnipper_foundation::{Result, SnipperError};
use log::info;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::{ArtifactEdgeKind, ArtifactId, ArtifactKind, ArtifactRecord};

/// A node entry in the pipeline graph with its dependencies.
struct NodeEntry {
    name: String,
    node: Box<dyn PipelineNode>,
    depends_on: Vec<String>,
    /// Insertion index for deterministic ordering of nodes with equal dependencies.
    index: usize,
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
        let index = self.entries.len();
        self.entries.push(NodeEntry {
            name,
            node,
            depends_on: Vec::new(),
            index,
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
        let index = self.entries.len();
        self.entries.push(NodeEntry {
            name,
            node,
            depends_on,
            index,
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
            ctx.check_control()?;

            let entry = self
                .entries
                .iter()
                .find(|e| &e.name == name)
                .ok_or_else(|| SnipperError::Pipeline(format!("Node '{}' not found", name)))?;

            info!("Pipeline '{}' executing node {}: {}", self.name, i, name);
            if let Some(observer) = ctx.progress_observer() {
                let observer = observer.clone();
                let name = name.clone();
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    observer.node_started(&name, i, order.len());
                }));
            }
            ctx.check_control()?;
            entry.node.process(ctx).await?;
            ctx.check_control()?;
            if let Some(observer) = ctx.progress_observer() {
                let observer = observer.clone();
                let name = name.clone();
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    observer.node_completed(&name, i + 1, order.len());
                }));
            }
            let stage_id = ArtifactId(format!("pipeline:{}:{}", self.name, name));
            ctx.artifacts.artifact_graph.insert(ArtifactRecord {
                id: stage_id.clone(),
                kind: ArtifactKind::PipelineStage,
                stable_id: None,
                content_ref: Some(name.clone()),
                checksum: None,
                provenance: Vec::new(),
            });
            for dependency in &entry.depends_on {
                ctx.artifacts.artifact_graph.link(
                    ArtifactId(format!("pipeline:{}:{}", self.name, dependency)),
                    stage_id.clone(),
                    ArtifactEdgeKind::DerivedFrom,
                );
            }
        }

        let document_id = ArtifactId(format!("document:{}", self.name));
        ctx.artifacts.artifact_graph.insert(ArtifactRecord {
            id: document_id.clone(),
            kind: ArtifactKind::DocumentAst,
            stable_id: None,
            content_ref: Some("Document".to_string()),
            checksum: None,
            provenance: Vec::new(),
        });
        for name in self.terminal_node_names() {
            ctx.artifacts.artifact_graph.link(
                ArtifactId(format!("pipeline:{}:{}", self.name, name)),
                document_id.clone(),
                ArtifactEdgeKind::DerivedFrom,
            );
        }

        info!("Pipeline '{}' completed", self.name);
        Ok(())
    }

    /// Topological sort using Kahn's algorithm.
    /// Nodes with equal in-degree are ordered by their insertion index (FIFO).
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

        // Start with nodes that have no dependencies, ordered by insertion index
        let mut queue: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(name, _)| name.clone())
            .collect();
        // Sort by insertion index to preserve add_node() order
        queue.sort_by_key(|name| {
            self.entries
                .iter()
                .find(|e| &e.name == name)
                .map_or(usize::MAX, |e| e.index)
        });

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
                // Re-sort by insertion index for newly ready nodes
                queue.sort_by_key(|name| {
                    self.entries
                        .iter()
                        .find(|e| &e.name == name)
                        .map_or(usize::MAX, |e| e.index)
                });
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

    fn terminal_node_names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|entry| {
                !self.entries.iter().any(|candidate| {
                    candidate
                        .depends_on
                        .iter()
                        .any(|dependency| dependency == &entry.name)
                })
            })
            .map(|entry| entry.name.as_str())
            .collect()
    }
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PipelineCancellationToken, TransformNode};

    #[tokio::test]
    async fn run_records_stage_and_document_lineage() {
        let mut graph = PipelineGraph::new("lineage");
        graph.add_node(Box::new(TransformNode::new("source", |_| Ok(()))));
        graph.add_node_with_deps(
            Box::new(TransformNode::new("recognize", |_| Ok(()))),
            vec!["source".to_string()],
        );

        let mut context = PipelineContext::new();
        graph.run(&mut context).await.unwrap();

        assert!(context
            .artifacts
            .artifact_graph
            .get(&ArtifactId::from("pipeline:lineage:source"))
            .is_some());
        assert!(context
            .artifacts
            .artifact_graph
            .get(&ArtifactId::from("document:lineage"))
            .is_some());
        assert_eq!(context.artifacts.artifact_graph.edges().len(), 2);
    }

    #[tokio::test]
    async fn cancellation_is_reported_at_a_node_boundary() {
        let mut graph = PipelineGraph::new("cancelled");
        graph.add_node(Box::new(TransformNode::new("never_runs", |_| {
            panic!("cancelled pipeline executed a node")
        })));
        let token = PipelineCancellationToken::new();
        token.cancel();
        let mut context = PipelineContext::new();
        context.set_cancellation_token(token);

        let error = graph.run(&mut context).await.unwrap_err();
        assert!(matches!(error, SnipperError::Cancelled));
    }
}
