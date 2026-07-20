//! Runtime-only artifact lineage.
//!
//! The graph complements `Document.assets`, `SourceInfo`, and `Provenance`
//! without changing the Document 1.0.0 wire schema.

use std::collections::{HashMap, HashSet};

use latexsnipper_ast::Provenance;
use serde::{Deserialize, Serialize};

pub const ARTIFACT_TRACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactId(pub String);

impl From<String> for ArtifactId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ArtifactId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    SourceImage,
    SourcePdfPage,
    SourceFormula,
    DetectedRegion,
    CroppedRegion,
    RecognizedText,
    RecognizedFormula,
    FusedRegion,
    DocumentAst,
    SemanticFragment,
    RenderSvg,
    RenderPng,
    RenderFragment,
    Export,
    PipelineStage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRecord {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    #[serde(default)]
    pub provenance: Vec<Provenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactEdgeKind {
    DerivedFrom,
    CroppedFrom,
    RecognizedFrom,
    ConvertedFrom,
    RenderedFrom,
    ExportedFrom,
    ReplacedBy,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEdge {
    pub from: ArtifactId,
    pub to: ArtifactId,
    pub kind: ArtifactEdgeKind,
}

/// Stable, serializable lineage view for debugging and evaluation evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactTrace {
    pub schema_version: u32,
    pub artifacts: Vec<ArtifactRecord>,
    pub edges: Vec<ArtifactEdge>,
}

#[derive(Debug, Clone, Default)]
pub struct ArtifactGraph {
    artifacts: HashMap<ArtifactId, ArtifactRecord>,
    edges: Vec<ArtifactEdge>,
}

impl ArtifactGraph {
    pub fn insert(&mut self, record: ArtifactRecord) {
        self.artifacts.insert(record.id.clone(), record);
    }

    pub fn link(
        &mut self,
        from: impl Into<ArtifactId>,
        to: impl Into<ArtifactId>,
        kind: ArtifactEdgeKind,
    ) {
        self.edges.push(ArtifactEdge {
            from: from.into(),
            to: to.into(),
            kind,
        });
    }

    pub fn get(&self, id: &ArtifactId) -> Option<&ArtifactRecord> {
        self.artifacts.get(id)
    }

    pub fn artifacts(&self) -> impl Iterator<Item = &ArtifactRecord> {
        self.artifacts.values()
    }

    /// Alias for session-oriented callers that treat graph entries as records.
    pub fn records(&self) -> impl Iterator<Item = &ArtifactRecord> {
        self.artifacts()
    }

    pub fn edges(&self) -> &[ArtifactEdge] {
        &self.edges
    }

    /// Produce a deterministic trace suitable for JSON diagnostics or evidence.
    pub fn trace(&self) -> ArtifactTrace {
        let mut artifacts: Vec<ArtifactRecord> = self.artifacts.values().cloned().collect();
        artifacts.sort_by(|left, right| left.id.cmp(&right.id));
        let mut edges = self.edges.clone();
        edges.sort_by(|left, right| {
            (&left.from, &left.to, left.kind as u8).cmp(&(&right.from, &right.to, right.kind as u8))
        });
        ArtifactTrace {
            schema_version: ARTIFACT_TRACE_SCHEMA_VERSION,
            artifacts,
            edges,
        }
    }

    /// Return descendants whose cached outputs must be invalidated when `id`
    /// is replaced or edited.
    pub fn descendants_of(&self, id: &ArtifactId) -> HashSet<ArtifactId> {
        let mut descendants = HashSet::new();
        let mut pending = vec![id.clone()];
        while let Some(current) = pending.pop() {
            for edge in self.edges.iter().filter(|edge| edge.from == current) {
                if descendants.insert(edge.to.clone()) {
                    pending.push(edge.to.clone());
                }
            }
        }
        descendants
    }

    /// Compact a runtime-only lineage view while retaining a valid graph.
    /// Callers own the retention policy; historical exports can use `trace()`
    /// before compaction when a complete audit trail is required.
    pub fn retain_artifacts(&mut self, mut retain: impl FnMut(&ArtifactRecord) -> bool) {
        self.artifacts.retain(|_, record| retain(record));
        self.edges.retain(|edge| {
            self.artifacts.contains_key(&edge.from) && self.artifacts.contains_key(&edge.to)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendants_follow_lineage_edges() {
        let mut graph = ArtifactGraph::default();
        graph.link("source", "semantic", ArtifactEdgeKind::ConvertedFrom);
        graph.link("semantic", "svg", ArtifactEdgeKind::RenderedFrom);
        let descendants = graph.descendants_of(&ArtifactId::from("source"));
        assert!(descendants.contains(&ArtifactId::from("semantic")));
        assert!(descendants.contains(&ArtifactId::from("svg")));
    }

    #[test]
    fn trace_is_sorted_and_serializable() {
        let mut graph = ArtifactGraph::default();
        graph.insert(ArtifactRecord {
            id: ArtifactId::from("z"),
            kind: ArtifactKind::Export,
            stable_id: None,
            content_ref: None,
            checksum: None,
            provenance: Vec::new(),
        });
        graph.insert(ArtifactRecord {
            id: ArtifactId::from("a"),
            kind: ArtifactKind::DocumentAst,
            stable_id: None,
            content_ref: None,
            checksum: None,
            provenance: Vec::new(),
        });
        let trace = graph.trace();
        assert_eq!(trace.artifacts[0].id, ArtifactId::from("a"));
        assert_eq!(
            serde_json::to_value(trace).unwrap()["schemaVersion"],
            ARTIFACT_TRACE_SCHEMA_VERSION
        );
    }
}
