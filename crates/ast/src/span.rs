use serde::{Deserialize, Serialize};

/// Unique identifier for an AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u64);

impl NodeId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Generate the next ID from a counter.
    pub fn next(counter: &mut u64) -> Self {
        let id = *counter;
        *counter += 1;
        Self(id)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// Byte offset range in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Create a span from a single position.
    pub fn single(pos: usize) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Length of the span.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Check if the span is empty.
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }

    /// Check if this span contains another span.
    pub fn contains(&self, other: &Span) -> bool {
        self.start <= other.start && self.end >= other.end
    }

    /// Merge two spans (union).
    pub fn merge(&self, other: &Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Line/column position in source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

impl Position {
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }

    pub fn zero() -> Self {
        Self { line: 1, column: 1 }
    }
}

impl std::fmt::Display for Position {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// The coordinate space in which geometry values are expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinateSpace {
    /// Raw image pixel coordinates.
    ImagePixels,
    /// PDF points (1 pt = 1/72 inch).
    PdfPoints,
    /// Office English Metric Units (EMU).
    OfficeEmu,
    /// Normalized [0,1] coordinate space.
    Normalized01,
    /// Logical page coordinates (e.g., CSS px).
    PageLogical,
}

/// PDF-specific source information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfSourceInfo {
    pub page_index: usize,
    pub object_id: Option<String>,
    pub xobject_name: Option<String>,
    pub text_span_id: Option<String>,
}

/// Source location information for an AST node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Unique node identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<NodeId>,
    /// Stable identifier that survives re-processing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stable_id: Option<String>,
    /// Byte span in source text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Line/column position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    /// Page index (0-based) for multi-page input (PDF).
    ///
    /// `None` means page-agnostic (single-page input or unknown).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
    /// Bounding rectangle in the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<crate::Rect>,
    /// Precise quadrilateral (for rotated content).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quad: Option<crate::Quad>,
    /// Coordinate space that region/quad values use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinate_space: Option<CoordinateSpace>,
    /// Confidence score from recognition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Producer/tool that created this node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    /// Provider ID if this node was produced by a remote API/VLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    /// Artifact entry ID if this node's source is tracked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    /// Media asset ID if this node was derived from an asset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<crate::AssetId>,
    /// Office application source information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub office: Option<crate::OfficeSourceInfo>,
    /// PDF-specific source information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf: Option<PdfSourceInfo>,
}

impl SourceInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_node_id(mut self, id: NodeId) -> Self {
        self.node_id = Some(id);
        self
    }

    pub fn with_stable_id(mut self, id: impl Into<String>) -> Self {
        self.stable_id = Some(id.into());
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_position(mut self, pos: Position) -> Self {
        self.position = Some(pos);
        self
    }

    pub fn with_region(mut self, rect: crate::Rect) -> Self {
        self.region = Some(rect);
        self
    }

    pub fn with_quad(mut self, quad: crate::Quad) -> Self {
        self.quad = Some(quad);
        self
    }

    pub fn with_coordinate_space(mut self, cs: CoordinateSpace) -> Self {
        self.coordinate_space = Some(cs);
        self
    }

    pub fn with_confidence(mut self, conf: f32) -> Self {
        self.confidence = Some(conf);
        self
    }

    pub fn with_producer(mut self, producer: impl Into<String>) -> Self {
        self.producer = Some(producer.into());
        self
    }

    /// Tag this source info with a page index (for multi-page PDF input).
    pub fn with_page(mut self, page: usize) -> Self {
        self.page = Some(page);
        self
    }
}

/// Generates unique NodeIds for AST nodes.
#[derive(Debug)]
pub struct NodeIdGenerator {
    counter: u64,
}

impl NodeIdGenerator {
    pub fn new() -> Self {
        Self { counter: 0 }
    }

    pub fn generate(&mut self) -> NodeId {
        NodeId::next(&mut self.counter)
    }
}

impl Default for NodeIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Provenance — tracks how a node was produced or transformed
// ---------------------------------------------------------------------------

/// Records the provenance of an AST node or artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub node_id: Option<NodeId>,
    pub artifact_id: Option<String>,
    pub stage_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub operation: ProvenanceOperation,
    pub confidence: Option<f32>,
    pub timestamp: Option<String>,
}

/// The type of operation that produced or changed a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceOperation {
    Decoded,
    Detected,
    Recognized,
    Normalized,
    Converted,
    Exported,
    EnhancedByApi,
    Degraded,
    ManuallyEdited,
}

// ---------------------------------------------------------------------------
// BlockPolicy — per-block processing policy
// ---------------------------------------------------------------------------

/// Controls what operations should (or should not) be applied to a block.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlockPolicy {
    pub recognize: Option<bool>,
    pub convert: Option<bool>,
    pub export: Option<bool>,
    pub editable: Option<bool>,
    pub preserve_layout: Option<bool>,
    pub preserve_asset: Option<bool>,
    pub allow_remote_api: Option<bool>,
    pub translate: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_display() {
        assert_eq!(NodeId(42).to_string(), "#42");
    }

    #[test]
    fn span_merge() {
        let s1 = Span::new(10, 20);
        let s2 = Span::new(15, 30);
        assert_eq!(s1.merge(&s2), Span::new(10, 30));
    }

    #[test]
    fn span_contains() {
        let outer = Span::new(0, 100);
        let inner = Span::new(10, 50);
        assert!(outer.contains(&inner));
        assert!(!inner.contains(&outer));
    }

    #[test]
    fn position_display() {
        assert_eq!(Position::new(5, 12).to_string(), "5:12");
    }

    #[test]
    fn node_id_generator() {
        let mut gen = NodeIdGenerator::new();
        let a = gen.generate();
        let b = gen.generate();
        let c = gen.generate();
        assert_eq!(a, NodeId(0));
        assert_eq!(b, NodeId(1));
        assert_eq!(c, NodeId(2));
    }
}
