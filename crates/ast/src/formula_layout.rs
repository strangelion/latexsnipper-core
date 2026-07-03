use serde::{Deserialize, Serialize};

use crate::Rect;

/// Formula layout tree — structured representation of a formula.
///
/// This provides a hierarchical view of a LaTeX formula,
/// enabling symbol-level analysis and confidence scoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormulaLayout {
    /// Root node of the formula tree.
    pub root: FormulaNode,
    /// Total number of symbols in the formula.
    pub symbol_count: usize,
}

/// A node in the formula tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FormulaNode {
    /// A single symbol (number, letter, operator, etc.).
    Symbol(SymbolInfo),
    /// A LaTeX command (e.g., \frac, \sqrt, \sum).
    Command(CommandInfo),
    /// A group of nodes (e.g., {abc}).
    Group(Vec<FormulaNode>),
    /// An environment (e.g., \begin{matrix}...\end{matrix}).
    Environment(EnvInfo),
    /// Superscript: base^exp.
    Superscript {
        /// The base expression.
        base: Box<FormulaNode>,
        /// The exponent.
        exp: Box<FormulaNode>,
    },
    /// Subscript: base_sub.
    Subscript {
        /// The base expression.
        base: Box<FormulaNode>,
        /// The subscript.
        sub: Box<FormulaNode>,
    },
    /// Fraction: \frac{num}{den}.
    Fraction {
        /// Numerator.
        num: Box<FormulaNode>,
        /// Denominator.
        den: Box<FormulaNode>,
    },
    /// Square root: \sqrt{content}.
    SquareRoot {
        /// Content under the radical.
        content: Box<FormulaNode>,
    },
    /// Plain text (for debugging or unrecognized content).
    Text(String),
}

/// Information about a symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    /// The LaTeX representation of the symbol.
    pub latex: String,
    /// Category of the symbol.
    pub category: SymbolCategory,
    /// Bounding box in the original image (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rect: Option<Rect>,
    /// Confidence score for this symbol.
    pub confidence: f32,
}

/// Category of a mathematical symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolCategory {
    /// Digits: 0-9.
    Number,
    /// Letters: a-z, A-Z.
    Letter,
    /// Operators: +, -, *, /, etc.
    Operator,
    /// Relations: =, <, >, etc.
    Relation,
    /// Arrows: \rightarrow, \leftarrow, etc.
    Arrow,
    /// Greek letters: \alpha, \beta, etc.
    Greek,
    /// Accents: \hat, \bar, etc.
    Accent,
    /// Delimiters: (, ), [, ], etc.
    Delimiter,
    /// Functions: \sin, \cos, etc.
    Function,
    /// Constants: \infty, \pi, e, i, etc.
    Constant,
    /// Unknown or uncategorized.
    Unknown,
}

/// Information about a LaTeX command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    /// The command name (e.g., "frac", "sqrt", "sum").
    pub name: String,
    /// Arguments to the command.
    pub args: Vec<FormulaNode>,
}

/// Information about an environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvInfo {
    /// The environment name (e.g., "matrix", "aligned").
    pub name: String,
    /// Content inside the environment.
    pub content: Vec<Vec<FormulaNode>>,
}

impl FormulaLayout {
    /// Create a new empty layout.
    pub fn empty() -> Self {
        Self {
            root: FormulaNode::Text(String::new()),
            symbol_count: 0,
        }
    }

    /// Create a layout from a single text node.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            root: FormulaNode::Text(text.into()),
            symbol_count: 0,
        }
    }
}

impl SymbolInfo {
    /// Create a new symbol.
    pub fn new(latex: impl Into<String>, category: SymbolCategory) -> Self {
        Self {
            latex: latex.into(),
            category,
            rect: None,
            confidence: 1.0,
        }
    }

    /// Set the bounding box.
    pub fn with_rect(mut self, rect: Rect) -> Self {
        self.rect = Some(rect);
        self
    }

    /// Set the confidence.
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
}

impl CommandInfo {
    /// Create a new command.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            args: Vec::new(),
        }
    }

    /// Add an argument.
    pub fn with_arg(mut self, arg: FormulaNode) -> Self {
        self.args.push(arg);
        self
    }
}

impl EnvInfo {
    /// Create a new environment.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            content: Vec::new(),
        }
    }

    /// Add a row to the environment.
    pub fn with_row(mut self, row: Vec<FormulaNode>) -> Self {
        self.content.push(row);
        self
    }
}

/// Categorize a LaTeX symbol string.
pub fn categorize_symbol(latex: &str) -> SymbolCategory {
    match latex {
        "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => SymbolCategory::Number,
        "+" | "-" | "*" | "/" | "\\times" | "\\cdot" | "\\pm" | "\\mp" => {
            SymbolCategory::Operator
        }
        "=" | "<" | ">" | "\\leq" | "\\geq" | "\\neq" | "\\approx" | "\\equiv" => {
            SymbolCategory::Relation
        }
        "\\rightarrow" | "\\leftarrow" | "\\leftrightarrow" | "\\Rightarrow"
        | "\\Leftarrow" | "\\Leftrightarrow" => SymbolCategory::Arrow,
        "\\alpha" | "\\beta" | "\\gamma" | "\\delta" | "\\epsilon" | "\\zeta" | "\\eta"
        | "\\theta" | "\\iota" | "\\kappa" | "\\lambda" | "\\mu" | "\\nu" | "\\xi"
        | "\\pi" | "\\rho" | "\\sigma" | "\\tau" | "\\upsilon" | "\\phi" | "\\chi"
        | "\\psi" | "\\omega" | "\\Alpha" | "\\Beta" | "\\Gamma" | "\\Delta"
        | "\\Theta" | "\\Lambda" | "\\Xi" | "\\Pi" | "\\Sigma" | "\\Phi" | "\\Psi"
        | "\\Omega" => SymbolCategory::Greek,
        "\\hat" | "\\bar" | "\\tilde" | "\\vec" | "\\dot" | "\\ddot" | "\\widehat"
        | "\\widetilde" | "\\overline" => SymbolCategory::Accent,
        "(" | ")" | "[" | "]" | "\\{" | "\\}" | "\\left(" | "\\right)" | "\\left["
        | "\\right]" | "\\left\\{" | "\\right\\}" | "\\langle" | "\\rangle" => {
            SymbolCategory::Delimiter
        }
        "\\sin" | "\\cos" | "\\tan" | "\\log" | "\\ln" | "\\exp" | "\\lim"
        | "\\max" | "\\min" | "\\sup" | "\\inf" | "\\det" | "\\gcd" => {
            SymbolCategory::Function
        }
        _ => {
            // Check if it's a single letter
            if latex.len() == 1 && latex.chars().next().unwrap().is_alphabetic() {
                SymbolCategory::Letter
            } else {
                SymbolCategory::Unknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_symbol() {
        assert_eq!(categorize_symbol("0"), SymbolCategory::Number);
        assert_eq!(categorize_symbol("x"), SymbolCategory::Letter);
        assert_eq!(categorize_symbol("+"), SymbolCategory::Operator);
        assert_eq!(categorize_symbol("="), SymbolCategory::Relation);
        assert_eq!(categorize_symbol("\\alpha"), SymbolCategory::Greek);
        assert_eq!(categorize_symbol("\\frac"), SymbolCategory::Unknown);
    }

    #[test]
    fn test_formula_layout_empty() {
        let layout = FormulaLayout::empty();
        assert_eq!(layout.symbol_count, 0);
    }

    #[test]
    fn test_symbol_info() {
        let sym = SymbolInfo::new("x", SymbolCategory::Letter)
            .with_confidence(0.95);
        assert_eq!(sym.latex, "x");
        assert_eq!(sym.category, SymbolCategory::Letter);
        assert_eq!(sym.confidence, 0.95);
    }
}
