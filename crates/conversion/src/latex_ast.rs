//! LaTeX AST types for structured parsing and conversion.

/// A LaTeX AST node.
#[derive(Debug, Clone)]
pub enum LatexNode {
    /// Plain text
    Text(String),
    /// Command with arguments: \cmd{arg1}{arg2}
    Command {
        name: String,
        args: Vec<LatexNode>,
    },
    /// Superscript: a^{b}
    Superscript {
        base: Box<LatexNode>,
        exp: Box<LatexNode>,
    },
    /// Subscript: a_{b}
    Subscript {
        base: Box<LatexNode>,
        sub: Box<LatexNode>,
    },
    /// Group: {content}
    Group(Vec<LatexNode>),
    /// Math environment: $...$ or $$...$$
    Math {
        content: Vec<LatexNode>,
        display: bool,
    },
    /// Delimiters: \left( ... \right)
    Delimited {
        left: String,
        content: Vec<LatexNode>,
        right: String,
    },
    /// Fraction: \frac{num}{den}
    Fraction {
        num: Box<LatexNode>,
        den: Box<LatexNode>,
    },
    /// Square root: \sqrt{x} or \sqrt[n]{x}
    SquareRoot {
        index: Option<Box<LatexNode>>,
        content: Box<LatexNode>,
    },
    /// Operator: \sum, \int, etc.
    Operator(String),
    /// Relation: \leq, \geq, etc.
    Relation(String),
    /// Greek letter: \alpha, \beta, etc.
    Greek(String),
    /// Symbol: \infty, \partial, etc.
    Symbol(String),
    /// Font modifier: \mathbb{R}, \mathbf{x}
    FontModifier {
        font: String,
        content: Box<LatexNode>,
    },
    /// Matrix: \begin{pmatrix} ... \end{pmatrix}
    Matrix {
        env: String,
        rows: Vec<Vec<LatexNode>>,
    },
    /// Cases: \begin{cases} ... \end{cases}
    Cases(Vec<Vec<LatexNode>>),
    /// List of nodes
    Sequence(Vec<LatexNode>),
}

impl LatexNode {
    /// Create a text node
    pub fn text(s: impl Into<String>) -> Self {
        LatexNode::Text(s.into())
    }

    /// Create a command node
    pub fn command(name: impl Into<String>, args: Vec<LatexNode>) -> Self {
        LatexNode::Command {
            name: name.into(),
            args,
        }
    }

    /// Create a group node
    pub fn group(nodes: Vec<LatexNode>) -> Self {
        LatexNode::Group(nodes)
    }

    /// Check if this is an empty node
    pub fn is_empty(&self) -> bool {
        match self {
            LatexNode::Text(s) => s.is_empty(),
            LatexNode::Sequence(nodes) => nodes.is_empty(),
            LatexNode::Group(nodes) => nodes.is_empty(),
            _ => false,
        }
    }
}
