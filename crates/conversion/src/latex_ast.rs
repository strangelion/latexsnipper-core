//! LaTeX AST types for structured parsing and conversion.

/// A LaTeX AST node.
#[derive(Debug, Clone)]
pub enum LatexNode {
    /// Plain text
    Text(String),
    /// Command with arguments: \cmd{arg1}{arg2}
    Command { name: String, args: Vec<LatexNode> },
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
    /// Square root: `\sqrt{x}` or `\sqrt[n]{x}`.
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
    /// Description list item: `\item[label] content`.
    DescriptionItem {
        label: Option<Box<LatexNode>>,
        content: Vec<LatexNode>,
    },
    /// Description list: \begin{description} ... \end{description}
    Description(Vec<LatexNode>),
    /// Accent: \hat{x}, \vec{v}, \bar{x}, etc.
    Accent {
        chr: String,
        content: Box<LatexNode>,
    },
    /// Operator name: \operatorname{Spec}
    OperatorName { name: String, args: Vec<LatexNode> },
    /// Overbrace: \overbrace{content}^{label}
    Overbrace {
        content: Box<LatexNode>,
        label: Option<Box<LatexNode>>,
    },
    /// Underbrace: \underbrace{content}_{label}
    Underbrace {
        content: Box<LatexNode>,
        label: Option<Box<LatexNode>>,
    },
    /// Overset: \overset{top}{base} — stack top above base
    Overset {
        top: Box<LatexNode>,
        base: Box<LatexNode>,
    },
    /// Underset: \underset{bottom}{base} — stack bottom below base
    Underset {
        bottom: Box<LatexNode>,
        base: Box<LatexNode>,
    },
    /// Arrow with text above and/or below: \xrightarrow{text}, \xleftarrow{text}
    XArrow {
        direction: String, // "rightarrow" or "leftarrow"
        above: Option<Box<LatexNode>>,
        below: Option<Box<LatexNode>>,
    },
    /// Footnote: \footnote{content}
    Footnote { content: Box<LatexNode> },
    /// Label: \label{key}
    Label { key: String },
    /// Reference: \ref{key} or \eqref{key}
    Reference { key: String, eq_ref: bool },
    /// Citation: \cite{key}, \citep{key}, \citet{key}
    Citation { key: String, style: String },
    /// Bibliography: \bibliography{file}
    Bibliography { file: String },
    /// Table of contents: \tableofcontents
    TableOfContents,
    /// Theorem-like environment: \begin{theorem}...\end{theorem}
    Theorem {
        name: String,
        content: Box<LatexNode>,
    },
    /// Proof environment: \begin{proof}...\end{proof}
    Proof { content: Box<LatexNode> },
    /// Minipage: \begin{minipage}{width}...\end{minipage}
    Minipage {
        width: String,
        content: Box<LatexNode>,
    },
    /// Float: \begin{figure}...\end{figure} or \begin{table}...\end{table}
    Float {
        env: String,
        caption: Option<String>,
        content: Box<LatexNode>,
    },
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

impl std::fmt::Display for LatexNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LatexNode::Text(s) => write!(f, "{}", s),
            LatexNode::Sequence(nodes) => {
                let mut result = String::new();
                for (i, n) in nodes.iter().enumerate() {
                    if i > 0 {
                        // Only add space if not adjacent to a previous symbol/operator
                        // that would make the spacing wrong (e.g. E=mc^2 should not become E = m c ^ 2)
                        let prev_str = format!("{}", n);
                        let prev_text = format!("{}", nodes[i - 1]);
                        // Don't add space:
                        // - before superscript/subscript
                        // - between single chars that form a contiguous token
                        // - between a symbol and its following operator token
                        let skip = prev_str.starts_with('^')
                            || prev_str.starts_with('_')
                            || prev_str == ")"
                            || prev_str == "]"
                            || prev_str == ","
                            || prev_text.len() == 1 && prev_str.len() == 1;
                        if !skip {
                            result.push(' ');
                        }
                    }
                    write!(f, "{}", n)?;
                }
                Ok(())
            }
            LatexNode::Group(nodes) => {
                write!(f, "{{")?;
                for (i, n) in nodes.iter().enumerate() {
                    if i > 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", n)?;
                }
                write!(f, "}}")
            }
            LatexNode::Superscript { base, exp } if base.is_empty() => {
                write!(f, "^{{{}}}", exp)
            }
            LatexNode::Superscript { base, exp } => {
                write!(f, "{{{}}}^{{{}}}", base, exp)
            }
            LatexNode::Subscript { base, sub } if base.is_empty() => {
                write!(f, "_{{{}}}", sub)
            }
            LatexNode::Subscript { base, sub } => {
                write!(f, "{{{}}}_{{{}}}", base, sub)
            }
            LatexNode::Fraction { num, den } => write!(f, "\\frac{{{}}}{{{}}}", num, den),
            LatexNode::SquareRoot {
                index: Some(idx),
                content,
            } => write!(f, "\\sqrt[{}]{{{}}}", idx, content),
            LatexNode::SquareRoot {
                index: None,
                content,
            } => write!(f, "\\sqrt{{{}}}", content),
            LatexNode::Operator(op) => write!(f, "\\{}", op),
            LatexNode::Relation(rel) => write!(f, "\\{}", rel),
            LatexNode::Greek(g) => write!(f, "\\{}", g),
            LatexNode::Symbol(s) => write!(f, "{}", s),
            LatexNode::Command { name, args } => {
                write!(f, "\\{}", name)?;
                for arg in args {
                    write!(f, "{{{}}}", arg)?;
                }
                Ok(())
            }
            LatexNode::Math {
                content,
                display: false,
            } => {
                let s: String = content
                    .iter()
                    .map(|n| format!("{}", n))
                    .collect::<Vec<_>>()
                    .join(" ");
                write!(f, "${}$", s)
            }
            LatexNode::Math {
                content,
                display: true,
            } => {
                let s: String = content
                    .iter()
                    .map(|n| format!("{}", n))
                    .collect::<Vec<_>>()
                    .join(" ");
                write!(f, "$${}$$", s)
            }
            LatexNode::Delimited {
                left,
                content,
                right,
            } => {
                let s: String = content
                    .iter()
                    .map(|n| format!("{}", n))
                    .collect::<Vec<_>>()
                    .join(" ");
                write!(f, "\\left{}{}\\right{}", left, s, right)
            }
            LatexNode::FontModifier { font, content } => write!(f, "\\{}{{{}}}", font, content),
            LatexNode::Matrix { env, .. } => write!(f, "\\begin{{{}}}...\\end{{{}}}", env, env),
            LatexNode::Cases(..) => write!(f, "\\begin{{cases}}...\\end{{cases}}"),
            LatexNode::Accent { chr, content } => {
                let name = match chr.as_str() {
                    "\u{0302}" => "hat",
                    "\u{0304}" | "\u{0305}" => "bar",
                    "\u{0307}" => "dot",
                    "\u{0308}" => "ddot",
                    "\u{0303}" => "tilde",
                    "\u{030C}" => "check",
                    "\u{20D7}" => "vec",
                    "\u{0306}" => "breve",
                    _ => chr,
                };
                write!(f, "\\{}{{{}}}", name, content)
            }
            LatexNode::OperatorName { name: _, args } => {
                let s: String = args.iter().map(|n| format!("{}", n)).collect();
                write!(f, "\\operatorname{{{}}}", s)
            }
            LatexNode::Overbrace {
                content,
                label: Some(l),
            } => {
                write!(f, "\\overbrace{{{}}}^{{{}}}", content, l)
            }
            LatexNode::Overbrace {
                content,
                label: None,
            } => {
                write!(f, "\\overbrace{{{}}}", content)
            }
            LatexNode::Underbrace {
                content,
                label: Some(l),
            } => {
                write!(f, "\\underbrace{{{}}}_{{{}}}", content, l)
            }
            LatexNode::Underbrace {
                content,
                label: None,
            } => {
                write!(f, "\\underbrace{{{}}}", content)
            }
            LatexNode::Overset { top, base } => {
                write!(f, "\\overset{{{}}}{{{}}}", top, base)
            }
            LatexNode::Underset { bottom, base } => {
                write!(f, "\\underset{{{}}}{{{}}}", bottom, base)
            }
            LatexNode::XArrow {
                direction,
                above,
                below,
            } => {
                let cmd = if direction == "rightarrow" {
                    "xrightarrow"
                } else {
                    "xleftarrow"
                };
                match (above, below) {
                    (Some(a), Some(b)) => write!(f, "\\{}[{}]{{{}}}", cmd, b, a),
                    (Some(a), None) => write!(f, "\\{}{{{}}}", cmd, a),
                    (None, Some(b)) => write!(f, "\\{}[{}]{{}}", cmd, b),
                    (None, None) => write!(f, "\\{}{{}}", cmd),
                }
            }
            LatexNode::DescriptionItem { label, content } => {
                write!(f, "\\item")?;
                if let Some(l) = label {
                    write!(f, "[{}]", l)?;
                }
                for node in content {
                    write!(f, "{}", node)?;
                }
                Ok(())
            }
            LatexNode::Description(items) => {
                writeln!(f, "\\begin{{description}}")?;
                for item in items {
                    writeln!(f, "{}", item)?;
                }
                write!(f, "\\end{{description}}")
            }
            LatexNode::Footnote { content } => {
                write!(f, "\\footnote{{{}}}", content)
            }
            LatexNode::Label { key } => {
                write!(f, "\\label{{{}}}", key)
            }
            LatexNode::Reference { key, eq_ref } => {
                if *eq_ref {
                    write!(f, "\\eqref{{{}}}", key)
                } else {
                    write!(f, "\\ref{{{}}}", key)
                }
            }
            LatexNode::Citation { key, style } => {
                let cmd = match style.as_str() {
                    "author" => "citet",
                    "parenthetical" => "citep",
                    _ => "cite",
                };
                write!(f, "\\{}{{{}}}", cmd, key)
            }
            LatexNode::Bibliography { file } => {
                write!(f, "\\bibliography{{{}}}", file)
            }
            LatexNode::TableOfContents => {
                write!(f, "\\tableofcontents")
            }
            LatexNode::Theorem { name, content } => {
                writeln!(f, "\\begin{{{}}}", name)?;
                write!(f, "{}", content)?;
                write!(f, "\\end{{{}}}", name)
            }
            LatexNode::Proof { content } => {
                writeln!(f, "\\begin{{proof}}")?;
                write!(f, "{}", content)?;
                write!(f, "\\end{{proof}}")
            }
            LatexNode::Minipage { width, content } => {
                writeln!(f, "\\begin{{minipage}}{{{}}}", width)?;
                write!(f, "{}", content)?;
                write!(f, "\\end{{minipage}}")
            }
            LatexNode::Float {
                env,
                caption,
                content,
            } => {
                writeln!(f, "\\begin{{{}}}", env)?;
                write!(f, "{}", content)?;
                if let Some(cap) = caption {
                    writeln!(f, "\\caption{{{}}}", cap)?;
                }
                write!(f, "\\end{{{}}}", env)
            }
        }
    }
}
