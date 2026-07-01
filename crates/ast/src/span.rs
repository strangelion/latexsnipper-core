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

/// Source location information for an AST node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Unique node identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<NodeId>,
    /// Byte span in source text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// Line/column position.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
}

impl SourceInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_node_id(mut self, id: NodeId) -> Self {
        self.node_id = Some(id);
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
