//! LaTeX parser that builds a structured AST.

use crate::latex_ast::LatexNode;

/// Parse a LaTeX string into an AST.
pub fn parse_latex(latex: &str) -> LatexNode {
    let mut parser = LatexParser::new(latex);
    parser.parse()
}

struct LatexParser {
    chars: Vec<char>,
    pos: usize,
    delimiter_depth: usize,
}

impl LatexParser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            delimiter_depth: 0,
        }
    }

    fn parse(&mut self) -> LatexNode {
        let mut nodes = Vec::new();
        while self.pos < self.chars.len() {
            if let Some(node) = self.parse_element() {
                match &node {
                    LatexNode::Superscript { base, .. } if base.is_empty() => {
                        if let Some(last) = nodes.pop() {
                            nodes.push(LatexNode::Superscript {
                                base: Box::new(last),
                                exp: Box::new(match node {
                                    LatexNode::Superscript { exp, .. } => *exp,
                                    _ => LatexNode::Text(String::new()),
                                }),
                            });
                        } else {
                            nodes.push(node);
                        }
                    }
                    LatexNode::Subscript { base, .. } if base.is_empty() => {
                        if let Some(last) = nodes.pop() {
                            nodes.push(LatexNode::Subscript {
                                base: Box::new(last),
                                sub: Box::new(match node {
                                    LatexNode::Subscript { sub, .. } => *sub,
                                    _ => LatexNode::Text(String::new()),
                                }),
                            });
                        } else {
                            nodes.push(node);
                        }
                    }
                    _ => {
                        nodes.push(node);
                    }
                }
            }
        }
        if nodes.len() == 1 {
            nodes.remove(0)
        } else {
            LatexNode::Sequence(nodes)
        }
    }

    fn parse_element(&mut self) -> Option<LatexNode> {
        if self.pos >= self.chars.len() {
            return None;
        }

        match self.chars[self.pos] {
            '%' => {
                self.skip_comment();
                None
            }
            '\\' => {
                self.pos += 1;
                self.parse_command()
            }
            '{' => {
                self.pos += 1;
                let content = self.parse_until('}');
                Some(LatexNode::Group(content))
            }
            '$' => {
                self.pos += 1;
                let content = self.parse_until('$');
                Some(LatexNode::Math {
                    content,
                    display: false,
                })
            }
            '^' => {
                self.pos += 1;
                let exp = self.parse_single();
                Some(LatexNode::Superscript {
                    base: Box::new(LatexNode::Text(String::new())),
                    exp: Box::new(exp),
                })
            }
            '_' => {
                self.pos += 1;
                let sub = self.parse_single();
                Some(LatexNode::Subscript {
                    base: Box::new(LatexNode::Text(String::new())),
                    sub: Box::new(sub),
                })
            }
            ',' | ';' | ':' | '+' | '-' | '=' | '<' | '>' | '/' | '*' | '|' => {
                let ch = self.chars[self.pos];
                self.pos += 1;
                Some(LatexNode::Text(ch.to_string()))
            }
            _ => self.parse_text(),
        }
    }

    fn parse_single(&mut self) -> LatexNode {
        // A space after a TeX control word terminates the command name; it is
        // not the command argument. Commands such as `\vec v` therefore own
        // `v`, rather than producing an empty accent followed by a sibling.
        self.skip_whitespace();
        if self.pos >= self.chars.len() {
            return LatexNode::Text(String::new());
        }

        match self.chars[self.pos] {
            '{' => {
                self.pos += 1;
                let content = self.parse_until('}');
                if content.len() == 1
                    && !matches!(
                        content.first(),
                        Some(LatexNode::Subscript { .. } | LatexNode::Superscript { .. })
                    )
                {
                    content
                        .into_iter()
                        .next()
                        .unwrap_or(LatexNode::Text(String::new()))
                } else {
                    LatexNode::Group(content)
                }
            }
            '\\' => {
                self.pos += 1;
                self.parse_command()
                    .unwrap_or(LatexNode::Text(String::new()))
            }
            _ => {
                let start = self.pos;
                while self.pos < self.chars.len() {
                    match self.chars[self.pos] {
                        '\\' | '{' | '}' | '$' | '^' | '_' | '%' | ' ' | '(' | ')' | '[' | ']'
                        | ':' | ',' | ';' | '+' | '-' | '=' | '<' | '>' | '/' | '*' | '|' => break,
                        _ => self.pos += 1,
                    }
                }
                if self.pos > start {
                    let text: String = self.chars[start..self.pos].iter().collect();
                    LatexNode::Text(text)
                } else {
                    LatexNode::Text(String::new())
                }
            }
        }
    }

    fn parse_group_text(&mut self) -> String {
        self.skip_whitespace();
        if self.pos >= self.chars.len() || self.chars[self.pos] != '{' {
            return String::new();
        }
        self.pos += 1;
        let start = self.pos;
        let mut depth = 0i32;
        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                '\\' if self.pos + 1 < self.chars.len() => {
                    self.pos += 2;
                    continue;
                }
                '%' => {
                    self.skip_comment();
                    continue;
                }
                '{' => depth += 1,
                '}' if depth == 0 => break,
                '}' => depth -= 1,
                _ => {}
            }
            self.pos += 1;
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        if self.pos < self.chars.len() {
            self.pos += 1; // consume '}'
        }
        text
    }

    fn parse_text(&mut self) -> Option<LatexNode> {
        let start = self.pos;
        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                '\\' | '{' | '}' | '$' | '^' | '_' | '%' | ':' | ',' | ';' | '+' | '-' | '='
                | '<' | '>' | '/' | '*' | '|' => break,
                _ => self.pos += 1,
            }
        }
        if self.pos > start {
            let text: String = self.chars[start..self.pos].iter().collect();
            Some(LatexNode::Text(text))
        } else {
            self.pos += 1;
            None
        }
    }

    fn parse_command(&mut self) -> Option<LatexNode> {
        if self.pos >= self.chars.len() {
            return None;
        }

        // Handle non-alphabetic commands like \, \; \! \: \( \) \[ \]
        if !self.chars[self.pos].is_ascii_alphabetic() {
            let ch = self.chars[self.pos];
            self.pos += 1;
            return match ch {
                // \, \; \: are thin/medium/thick math spaces — NOT punctuation.
                // Emit as Symbol so each output format (OMML spacing,
                // MathML mspace, HTML &thinsp;, plain-text space) can
                // choose the correct representation.
                ',' => Some(LatexNode::Symbol(",".to_string())),
                ';' => Some(LatexNode::Symbol(";".to_string())),
                ':' => Some(LatexNode::Symbol(":".to_string())),
                // \! is a negative thin space, not an exclamation mark
                '!' => Some(LatexNode::Symbol("!".to_string())),
                '%' | '&' | '#' | '_' | '$' | '{' | '}' => Some(LatexNode::Command {
                    name: ch.to_string(),
                    args: Vec::new(),
                }),
                '(' | ')' | '[' | ']' => Some(LatexNode::Text(ch.to_string())),
                _ => Some(LatexNode::Text(ch.to_string())),
            };
        }

        let start = self.pos;
        while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_alphabetic() {
            self.pos += 1;
        }

        let cmd: String = self.chars[start..self.pos].iter().collect();
        // A TeX control word consumes its following space/comment separator.
        self.skip_whitespace();

        if crate::latex_utils::literal_command_symbol(&cmd).is_some() {
            if self.chars.get(self.pos..self.pos + 2) == Some(&['{', '}']) {
                self.pos += 2;
            }
            return Some(LatexNode::Command {
                name: cmd,
                args: Vec::new(),
            });
        }
        match cmd.as_str() {
            // Greek letters
            "alpha" | "beta" | "gamma" | "delta" | "epsilon" | "varepsilon" | "zeta" | "eta"
            | "theta" | "vartheta" | "iota" | "kappa" | "varkappa" | "lambda" | "mu" | "nu"
            | "xi" | "pi" | "varpi" | "rho" | "varrho" | "sigma" | "varsigma" | "tau"
            | "upsilon" | "phi" | "varphi" | "chi" | "psi" | "omega" | "digamma" | "omicron"
            | "sampi" | "Sampi" | "backepsilon" | "varDelta" | "varGamma" | "varLambda"
            | "varPi" | "varTheta" | "Gamma" | "Delta" | "Theta" | "Lambda" | "Xi" | "Pi"
            | "Sigma" | "Upsilon" | "Phi" | "Psi" | "Omega" => Some(LatexNode::Greek(cmd)),
            // Operators
            "int" | "iint" | "iiint" | "oint" | "sum" | "prod" | "coprod" | "bigcup" | "bigcap"
            | "lim" | "limsup" | "liminf" | "max" | "min" | "sup" | "inf" | "log" | "ln"
            | "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "arcsin" | "arccos" | "arctan"
            | "sinh" | "cosh" | "tanh" | "det" | "gcd" => Some(LatexNode::Operator(cmd)),
            // Relations
            "leq" | "le" | "geq" | "ge" | "neq" | "ne" | "approx" | "equiv" | "sim" | "propto"
            | "ll" | "gg" | "prec" | "succ" | "cong" => Some(LatexNode::Relation(cmd)),
            // Symbols
            "infty" | "partial" | "nabla" | "forall" | "exists" | "neg" | "land" | "lor" | "in"
            | "notin" | "subset" | "supset" | "cup" | "cap" | "emptyset" | "pm" | "mp"
            | "times" | "div" | "cdot" | "ast" | "star" | "circ" | "bullet" | "diamond"
            | "oplus" | "otimes" | "odot" | "lfloor" | "rfloor" | "lceil" | "rceil" | "langle"
            | "rangle" | "lvert" | "rvert" | "lVert" | "rVert" | "quad" | "qquad" | "ldots"
            | "cdots" | "vdots" | "ddots" | "hbar" | "ell" | "prime" | "perp" | "parallel"
            | "mid" | "therefore" | "because" | "wp" | "Re" | "Im" | "aleph" | "beth" | "gimel"
            | "daleth" | "to" | "rightarrow" | "leftarrow" | "leftrightarrow" | "Rightarrow"
            | "Leftarrow" | "Leftrightarrow" | "implies" | "mapsto" | "uparrow" | "downarrow"
            | "nearrow" | "searrow" | "swarrow" | "nwarrow" => Some(LatexNode::Symbol(cmd)),
            // Fraction
            "frac" => {
                let num = self.parse_single();
                let den = self.parse_single();
                Some(LatexNode::Fraction {
                    num: Box::new(num),
                    den: Box::new(den),
                })
            }
            "substack" => {
                while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
                    self.pos += 1;
                }
                let content = self.parse_group_text();
                let args = crate::latex_utils::split_stack_rows(&content)
                    .into_iter()
                    .map(parse_latex)
                    .collect();
                Some(LatexNode::Command { name: cmd, args })
            }
            // Continued fractions retain operands and optional source alignment.
            "cfrac" => {
                while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
                    self.pos += 1;
                }
                let alignment = if self.chars.get(self.pos) == Some(&'[') {
                    let start = self.pos + 1;
                    if let Some(offset) = self.chars[start..].iter().position(|ch| *ch == ']') {
                        self.pos = start + offset + 1;
                        Some(LatexNode::Text(
                            self.chars[start..start + offset].iter().collect(),
                        ))
                    } else {
                        // Keep malformed optional syntax as source siblings, not operands.
                        return Some(LatexNode::Command {
                            name: cmd,
                            args: Vec::new(),
                        });
                    }
                } else {
                    None
                };
                while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
                    self.pos += 1;
                }
                let num = self.parse_single();
                while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
                    self.pos += 1;
                }
                let den = self.parse_single();
                // The optional third item retains source alignment; it is not a math operand.
                let mut args = vec![num, den];
                args.extend(alignment);
                Some(LatexNode::Command { name: cmd, args })
            }
            // Styled fractions retain their command and exactly two operands.
            "dfrac" | "tfrac" => {
                let num = self.parse_single();
                let den = self.parse_single();
                Some(LatexNode::Command {
                    name: cmd,
                    args: vec![num, den],
                })
            }
            // Square root
            "sqrt" => {
                self.skip_whitespace();
                let mut index = None;
                if self.pos < self.chars.len() && self.chars[self.pos] == '[' {
                    self.pos += 1;
                    let start = self.pos;
                    let mut groups = 0usize;
                    while self.pos < self.chars.len() {
                        match self.chars[self.pos] {
                            '%' => {
                                self.skip_comment();
                                continue;
                            }
                            '\\' if self
                                .chars
                                .get(self.pos + 1)
                                .is_some_and(|ch| !ch.is_ascii_alphabetic()) =>
                            {
                                self.pos += 2;
                                continue;
                            }
                            '{' => groups += 1,
                            '}' => groups = groups.saturating_sub(1),
                            ']' if groups == 0 => break,
                            _ => {}
                        }
                        self.pos += 1;
                    }
                    let raw: String = self.chars[start..self.pos].iter().collect();
                    let idx_text = crate::latex_utils::without_comments(&raw).into_owned();
                    if !idx_text.is_empty() {
                        index = Some(Box::new(LatexNode::Text(idx_text)));
                    }
                    if self.pos < self.chars.len() {
                        self.pos += 1;
                    }
                }
                let content = self.parse_single();
                Some(LatexNode::SquareRoot {
                    index,
                    content: Box::new(content),
                })
            }
            // Binomial
            "binom" => {
                let n = self.parse_single();
                let k = self.parse_single();
                Some(LatexNode::Command {
                    name: "binom".to_string(),
                    args: vec![n, k],
                })
            }
            // Accent commands
            "hat" | "widehat" => Some(LatexNode::Accent {
                chr: "\u{0302}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            "vec" | "overrightarrow" => Some(LatexNode::Accent {
                chr: "\u{20D7}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            "overleftarrow" => Some(LatexNode::Accent {
                chr: "\u{20D6}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            "bar" | "overline" => Some(LatexNode::Accent {
                chr: "\u{0305}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            "dot" => Some(LatexNode::Accent {
                chr: "\u{0307}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            "ddot" => Some(LatexNode::Accent {
                chr: "\u{0308}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            "tilde" | "widetilde" => Some(LatexNode::Accent {
                chr: "\u{0303}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            "check" => Some(LatexNode::Accent {
                chr: "\u{030C}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            "breve" => Some(LatexNode::Accent {
                chr: "\u{0306}".to_string(),
                content: Box::new(self.parse_single()),
            }),
            // Overset / Underset
            "overset" => {
                let top = self.parse_single();
                let base = self.parse_single();
                Some(LatexNode::Overset {
                    top: Box::new(top),
                    base: Box::new(base),
                })
            }
            "underset" => {
                let bottom = self.parse_single();
                let base = self.parse_single();
                Some(LatexNode::Underset {
                    bottom: Box::new(bottom),
                    base: Box::new(base),
                })
            }
            // Arrow with text: \xrightarrow{text} or \xrightarrow[below]{above}
            "xrightarrow" | "xleftarrow" => {
                let dir = if cmd == "xrightarrow" {
                    "rightarrow"
                } else {
                    "leftarrow"
                };
                // Check for optional argument [below]
                let (below, above) = if self.pos < self.chars.len() && self.chars[self.pos] == '[' {
                    self.pos += 1;
                    let below_node = self.parse_until(']');
                    let below_content = if below_node.len() == 1 {
                        below_node
                            .into_iter()
                            .next()
                            .unwrap_or(LatexNode::Text(String::new()))
                    } else {
                        LatexNode::Group(below_node)
                    };
                    // Required argument {above}
                    let above_node = self.parse_single();
                    (Some(Box::new(below_content)), above_node)
                } else {
                    // Required argument {above}
                    let above_node = self.parse_single();
                    let above_content = if above_node.is_empty() {
                        LatexNode::Text(String::new())
                    } else {
                        above_node
                    };
                    (None, above_content)
                };
                Some(LatexNode::XArrow {
                    direction: dir.to_string(),
                    above: Some(Box::new(above)),
                    below,
                })
            }
            // Font modifiers
            "mathbb" | "mathbf" | "mathit" | "mathsf" | "mathtt" | "mathcal" | "mathfrak"
            | "mathrm" | "mathnormal" | "boldsymbol" | "bm" => {
                let font = if cmd == "bm" {
                    "boldsymbol".to_string()
                } else {
                    cmd
                };
                let content = self.parse_font_argument();
                Some(LatexNode::FontModifier {
                    font,
                    content: Box::new(content),
                })
            }
            // Operator name
            "operatorname" => {
                let name = self.parse_single();
                let args = vec![name];
                Some(LatexNode::OperatorName {
                    name: "operatorname".to_string(),
                    args,
                })
            }
            // Text commands
            "text" | "textbf" | "textit" | "textrm" | "textsf" | "texttt" | "underline" => {
                let raw = self.parse_group_text();
                let content_str = crate::latex_utils::without_comments(&raw).into_owned();
                let content = LatexNode::Text(content_str);
                Some(LatexNode::Command {
                    name: cmd,
                    args: vec![content],
                })
            }
            "tiny" | "scriptsize" | "footnotesize" | "small" | "normalsize" | "large" | "Large"
            | "LARGE" | "huge" | "Huge" => {
                let args = if self.pos < self.chars.len() && self.chars[self.pos] == '{' {
                    vec![self.parse_single()]
                } else {
                    Vec::new()
                };
                Some(LatexNode::Command { name: cmd, args })
            }
            // Single-argument layout and convenience commands.
            "phantom" | "vphantom" | "hphantom" | "boxed" | "tag" | "abs" | "norm" | "floor"
            | "ceil" | "displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle" => {
                Some(LatexNode::Command {
                    name: cmd,
                    args: vec![self.parse_single()],
                })
            }
            // Footnote
            "footnote" => {
                let content = self.parse_single();
                Some(LatexNode::Footnote {
                    content: Box::new(content),
                })
            }
            // Label and references
            "label" => {
                let key = self.parse_group_text();
                Some(LatexNode::Label { key })
            }
            "ref" => {
                let key = self.parse_group_text();
                Some(LatexNode::Reference { key, eq_ref: false })
            }
            "eqref" => {
                let key = self.parse_group_text();
                Some(LatexNode::Reference { key, eq_ref: true })
            }
            // Citations
            "cite" | "citep" | "citet" | "citealp" | "citealt" => {
                let key = self.parse_group_text();
                let style = match cmd.as_str() {
                    "citet" => "author",
                    "citep" => "parenthetical",
                    _ => "plain",
                };
                Some(LatexNode::Citation {
                    key,
                    style: style.to_string(),
                })
            }
            "bibliography" => {
                let file = self.parse_group_text();
                Some(LatexNode::Bibliography { file })
            }
            // Two-argument commands
            "textcolor" | "colorbox" | "fcolorbox" | "color" => {
                let arg1 = self.parse_single();
                let arg2 = self.parse_single();
                Some(LatexNode::Command {
                    name: cmd,
                    args: vec![arg1, arg2],
                })
            }
            // Overbrace / Underbrace
            "overbrace" => {
                let content = self.parse_single();
                // Check for ^{label}
                let label = if self.pos < self.chars.len() && self.chars[self.pos] == '^' {
                    self.pos += 1;
                    Some(Box::new(self.parse_single()))
                } else {
                    None
                };
                Some(LatexNode::Overbrace {
                    content: Box::new(content),
                    label,
                })
            }
            "underbrace" => {
                let content = self.parse_single();
                let label = if self.pos < self.chars.len() && self.chars[self.pos] == '_' {
                    self.pos += 1;
                    Some(Box::new(self.parse_single()))
                } else {
                    None
                };
                Some(LatexNode::Underbrace {
                    content: Box::new(content),
                    label,
                })
            }
            // Matrix environments
            "begin" => self.parse_environment(start - 1),
            // \left ... \right
            "left" => self.parse_delimited(start - 1),
            // Standalone commands
            "tableofcontents" => Some(LatexNode::TableOfContents),
            // Unknown command — store as Command node
            _ => Some(LatexNode::Command {
                name: cmd,
                args: Vec::new(),
            }),
        }
    }

    fn parse_environment(&mut self, raw_start: usize) -> Option<LatexNode> {
        // We already consumed \begin, now read {envname}
        self.skip_whitespace();
        if self.pos >= self.chars.len() || self.chars[self.pos] != '{' {
            return None;
        }
        self.pos += 1;
        let env_name: String = self
            .parse_until('}')
            .iter()
            .map(|n| {
                if let LatexNode::Text(s) = n {
                    s.clone()
                } else {
                    String::new()
                }
            })
            .collect();

        let column_spec = if env_name == "array" {
            Some(self.parse_group_text())
        } else {
            None
        };
        let content = self.parse_until_begin_end(&env_name);

        match env_name.as_str() {
            "document" => Some(LatexParser::new(&content).parse()),
            "matrix" | "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix"
            | "smallmatrix" => {
                let rows = Self::parse_matrix_content(&content);
                Some(LatexNode::Matrix {
                    env: env_name,
                    rows,
                })
            }
            "cases" => {
                let rows = Self::parse_matrix_content(&content);
                Some(LatexNode::Cases(rows))
            }
            "aligned" | "align" | "align*" | "gather" | "gather*" => {
                let rows = Self::parse_matrix_content(&content);
                Some(LatexNode::Matrix {
                    env: env_name,
                    rows,
                })
            }
            "array" => {
                let rows = Self::parse_matrix_content(&content);
                Some(LatexNode::Array {
                    column_spec: column_spec.unwrap_or_default(),
                    rows,
                })
            }
            "description" => {
                let items = Self::parse_description_content(&content);
                Some(LatexNode::Description(items))
            }
            "tableofcontents" => Some(LatexNode::TableOfContents),
            "theorem" | "lemma" | "corollary" | "proposition" | "definition" | "example"
            | "remark" => {
                let mut parser = LatexParser::new(&content);
                let nodes = parser.parse();
                Some(LatexNode::Theorem {
                    name: env_name,
                    content: Box::new(nodes),
                })
            }
            "proof" => {
                let mut parser = LatexParser::new(&content);
                let nodes = parser.parse();
                Some(LatexNode::Proof {
                    content: Box::new(nodes),
                })
            }
            "minipage" => {
                let width = self.parse_group_text();
                let mut parser = LatexParser::new(&content);
                let nodes = parser.parse();
                Some(LatexNode::Minipage {
                    width,
                    content: Box::new(nodes),
                })
            }
            "figure" | "table" => {
                let mut parser = LatexParser::new(&content);
                let nodes = parser.parse();
                Some(LatexNode::Float {
                    env: env_name,
                    caption: None,
                    content: Box::new(nodes),
                })
            }
            _ => {
                // Unknown environments are opaque source, not successfully converted math.
                Some(LatexNode::Command {
                    name: format!("begin{{{}}}", env_name),
                    args: vec![LatexNode::Text(
                        self.chars[raw_start..self.pos].iter().collect(),
                    )],
                })
            }
        }
    }

    fn parse_description_content(content: &str) -> Vec<LatexNode> {
        let mut items = Vec::new();
        let mut current_item = String::new();
        let mut chars = content.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                let cmd: String = chars.by_ref().take_while(|&c| c.is_alphabetic()).collect();
                if cmd == "item" {
                    // Save previous item if any
                    if !current_item.is_empty() {
                        let mut parser = LatexParser::new(&current_item);
                        let node = parser.parse();
                        items.push(LatexNode::DescriptionItem {
                            label: None,
                            content: vec![node],
                        });
                        current_item.clear();
                    }
                    // Check for optional [label]
                    let remaining: String = chars.clone().collect();
                    let remaining = remaining.trim_start().to_string();
                    if remaining.starts_with('[') {
                        // Find closing bracket
                        let mut depth = 0i32;
                        let mut label_content = String::new();
                        chars.next(); // consume '['
                        for c in chars.by_ref() {
                            match c {
                                '[' => depth += 1,
                                ']' if depth == 0 => break,
                                ']' => depth -= 1,
                                _ => label_content.push(c),
                            }
                        }
                        // Store label for next item
                        current_item.push_str(&format!("[{}]", label_content));
                    }
                } else {
                    current_item.push('\\');
                    current_item.push_str(&cmd);
                }
            } else {
                current_item.push(ch);
            }
        }

        // Add last item
        if !current_item.is_empty() {
            let mut parser = LatexParser::new(&current_item);
            let node = parser.parse();
            items.push(LatexNode::DescriptionItem {
                label: None,
                content: vec![node],
            });
        }

        items
    }

    fn delimiter_token(&mut self) -> Option<String> {
        self.skip_whitespace();
        let start = self.pos;
        let ch = *self.chars.get(self.pos)?;
        self.pos += 1;
        if ch == '\\' {
            let first = *self.chars.get(self.pos)?;
            self.pos += 1;
            if first.is_ascii_alphabetic() {
                while self
                    .chars
                    .get(self.pos)
                    .is_some_and(char::is_ascii_alphabetic)
                {
                    self.pos += 1;
                }
                let token = self.chars[start..self.pos].iter().collect();
                self.skip_whitespace();
                return Some(token);
            }
        }
        Some(self.chars[start..self.pos].iter().collect())
    }

    fn parse_delimited(&mut self, raw_start: usize) -> Option<LatexNode> {
        let left = self.delimiter_token().unwrap_or_default();
        let content_start = self.pos;
        let mut content_end = self.pos;
        let mut nesting = 0usize;
        let mut braces = 0usize;
        let mut valid = latexsnipper_syntax::latex::scalable_delimiter_glyph(&left).is_some();
        let mut right = None;
        while self.pos < self.chars.len() {
            if self.chars[self.pos] == '%' {
                self.skip_comment();
                continue;
            }
            if self.chars[self.pos] == '\\' {
                let start = self.pos;
                let command = self.delimiter_token().unwrap_or_default();
                if command == r"\left" {
                    nesting += 1;
                    valid &= nesting + self.delimiter_depth < 128;
                    let token = self.delimiter_token().unwrap_or_default();
                    valid &= latexsnipper_syntax::latex::scalable_delimiter_glyph(&token).is_some();
                } else if command == r"\right" {
                    let token = self.delimiter_token().unwrap_or_default();
                    valid &= latexsnipper_syntax::latex::scalable_delimiter_glyph(&token).is_some();
                    if nesting == 0 {
                        valid &= braces == 0;
                        content_end = start;
                        right = Some(token);
                        break;
                    }
                    nesting -= 1;
                }
                continue;
            }
            match self.chars[self.pos] {
                '{' => braces += 1,
                '}' => braces = braces.saturating_sub(1),
                _ => {}
            }
            self.pos += 1;
        }
        if !valid || right.is_none() || self.delimiter_depth >= 128 {
            return Some(LatexNode::Command {
                name: "invalid-scalable-delimiter".into(),
                args: vec![LatexNode::Text(
                    self.chars[raw_start..self.pos].iter().collect(),
                )],
            });
        }
        let content_str: String = self.chars[content_start..content_end].iter().collect();
        let mut parser = LatexParser::new(&content_str);
        parser.delimiter_depth = self.delimiter_depth + 1;
        let content_nodes = parser.parse();

        Some(LatexNode::Delimited {
            left,
            content: if let LatexNode::Sequence(nodes) = content_nodes {
                nodes
            } else {
                vec![content_nodes]
            },
            right: right.expect("checked right delimiter"),
        })
    }

    fn parse_until_begin_end(&mut self, env_name: &str) -> String {
        let mut result = String::new();
        let mut environments: Vec<String> = Vec::new();

        while self.pos < self.chars.len() {
            if self.chars[self.pos] == '%' {
                let start = self.pos;
                self.skip_comment();
                result.extend(self.chars[start..self.pos].iter());
                continue;
            }
            if self.chars[self.pos] == '\\'
                && self
                    .chars
                    .get(self.pos + 1)
                    .is_some_and(|ch| !ch.is_ascii_alphabetic())
            {
                result.extend(self.chars[self.pos..self.pos + 2].iter());
                self.pos += 2;
                continue;
            }
            if let Some((begin, name, length)) = self.environment_tag_at() {
                if !begin && environments.is_empty() && name == env_name {
                    self.pos += length;
                    break;
                }
                if begin {
                    environments.push(name);
                } else if environments.last() == Some(&name) {
                    environments.pop();
                }
                result.extend(self.chars[self.pos..self.pos + length].iter());
                self.pos += length;
                continue;
            }
            result.push(self.chars[self.pos]);
            self.pos += 1;
        }

        result
    }

    fn environment_tag_at(&self) -> Option<(bool, String, usize)> {
        let remaining = &self.chars[self.pos..];
        let (begin, command_len) = if remaining.starts_with(&['\\', 'b', 'e', 'g', 'i', 'n']) {
            (true, 6)
        } else if remaining.starts_with(&['\\', 'e', 'n', 'd']) {
            (false, 4)
        } else {
            return None;
        };
        let mut pos = command_len;
        loop {
            if remaining.get(pos).is_some_and(|ch| ch.is_whitespace()) {
                pos += 1;
            } else if remaining.get(pos) == Some(&'%') {
                while remaining
                    .get(pos)
                    .is_some_and(|ch| !matches!(ch, '\n' | '\r'))
                {
                    pos += 1;
                }
            } else {
                break;
            }
        }
        if remaining.get(pos) != Some(&'{') {
            return None;
        }
        pos += 1;
        let mut name = String::new();
        while let Some(ch) = remaining.get(pos) {
            match ch {
                '%' => {
                    while remaining
                        .get(pos)
                        .is_some_and(|ch| !matches!(ch, '\n' | '\r'))
                    {
                        pos += 1;
                    }
                    if remaining.get(pos) == Some(&'\r') {
                        pos += 1;
                    }
                    if remaining.get(pos) == Some(&'\n') {
                        pos += 1;
                    }
                    while remaining
                        .get(pos)
                        .is_some_and(|ch| matches!(ch, ' ' | '\t'))
                    {
                        pos += 1;
                    }
                }
                '}' => return Some((begin, name, pos + 1)),
                '\\' | '{' => return None,
                _ => {
                    name.push(*ch);
                    pos += 1;
                }
            }
        }
        None
    }

    fn parse_matrix_content(content: &str) -> Vec<Vec<LatexNode>> {
        crate::latex_utils::split_matrix_rows(content)
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|cell| LatexParser::new(cell).parse())
                    .collect()
            })
            .collect()
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() {
            if self.chars[self.pos].is_whitespace() {
                self.pos += 1;
            } else if self.chars[self.pos] == '%' {
                self.skip_comment();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) {
        while self.pos < self.chars.len() && !matches!(self.chars[self.pos], '\n' | '\r') {
            self.pos += 1;
        }
        if self.chars.get(self.pos) == Some(&'\r') {
            self.pos += 1;
        }
        if self.chars.get(self.pos) == Some(&'\n') {
            self.pos += 1;
        }
        while self
            .chars
            .get(self.pos)
            .is_some_and(|ch| matches!(ch, ' ' | '\t'))
        {
            self.pos += 1;
        }
    }

    fn parse_font_argument(&mut self) -> LatexNode {
        self.skip_whitespace();
        self.parse_single()
    }

    fn parse_until(&mut self, delimiter: char) -> Vec<LatexNode> {
        let mut nodes = Vec::new();
        let mut depth = 0i32;

        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                '{' => {
                    depth += 1;
                    self.pos += 1;
                }
                '}' => {
                    if depth == 0 && delimiter == '}' {
                        self.pos += 1;
                        return Self::merge_sub_sup(nodes);
                    }
                    depth -= 1;
                    self.pos += 1;
                }
                c if c == delimiter && depth == 0 => {
                    self.pos += 1;
                    return Self::merge_sub_sup(nodes);
                }
                _ => {
                    if let Some(node) = self.parse_element() {
                        nodes.push(node);
                    }
                }
            }
        }

        Self::merge_sub_sup(nodes)
    }

    /// Merge empty-base Subscript/Superscript with the preceding node.
    /// E.g. [Text("x"), Subscript{empty,"i"}] → [Subscript{Text("x"),"i"}]
    fn merge_sub_sup(nodes: Vec<LatexNode>) -> Vec<LatexNode> {
        let mut result = Vec::with_capacity(nodes.len());
        for node in nodes {
            match &node {
                LatexNode::Superscript { base, .. } if base.is_empty() => {
                    if let Some(last) = result.pop() {
                        result.push(LatexNode::Superscript {
                            base: Box::new(last),
                            exp: Box::new(match node {
                                LatexNode::Superscript { exp, .. } => *exp,
                                _ => LatexNode::Text(String::new()),
                            }),
                        });
                    } else {
                        result.push(node);
                    }
                }
                LatexNode::Subscript { base, .. } if base.is_empty() => {
                    if let Some(last) = result.pop() {
                        result.push(LatexNode::Subscript {
                            base: Box::new(last),
                            sub: Box::new(match node {
                                LatexNode::Subscript { sub, .. } => *sub,
                                _ => LatexNode::Text(String::new()),
                            }),
                        });
                    } else {
                        result.push(node);
                    }
                }
                _ => result.push(node),
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_text() {
        let node = parse_latex("hello");
        match node {
            LatexNode::Text(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected Text"),
        }
    }

    #[test]
    fn test_fraction() {
        let node = parse_latex("\\frac{a}{b}");
        match node {
            LatexNode::Fraction { num, den } => {
                match *num {
                    LatexNode::Text(s) => assert_eq!(s, "a"),
                    _ => panic!("Expected Text in numerator"),
                }
                match *den {
                    LatexNode::Text(s) => assert_eq!(s, "b"),
                    _ => panic!("Expected Text in denominator"),
                }
            }
            _ => panic!("Expected Fraction"),
        }
    }

    #[test]
    fn test_superscript() {
        let node = parse_latex("x^{2}");
        match node {
            LatexNode::Superscript { base, exp } => {
                match *base {
                    LatexNode::Text(s) => assert_eq!(s, "x"),
                    _ => panic!("Expected Text base"),
                }
                match *exp {
                    LatexNode::Text(s) => assert_eq!(s, "2"),
                    _ => panic!("Expected Text exponent"),
                }
            }
            _ => panic!("Expected Superscript"),
        }
    }

    #[test]
    fn test_greek() {
        let node = parse_latex("\\alpha");
        match node {
            LatexNode::Greek(s) => assert_eq!(s, "alpha"),
            _ => panic!("Expected Greek"),
        }
    }

    #[test]
    fn test_complex() {
        let node = parse_latex("\\frac{a}{b} + \\sqrt{c}");
        if let LatexNode::Sequence(nodes) = node {
            assert!(!nodes.is_empty());
        }
    }

    #[test]
    fn test_binom() {
        let node = parse_latex("\\binom{n}{k}");
        match node {
            LatexNode::Command { name, args } => {
                assert_eq!(name, "binom");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_operatorname() {
        let node = parse_latex("\\operatorname{Spec}");
        match node {
            LatexNode::OperatorName { name, args } => {
                assert_eq!(name, "operatorname");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected OperatorName"),
        }
    }

    #[test]
    fn test_accent() {
        let node = parse_latex("\\hat{x}");
        match node {
            LatexNode::Accent { chr, content } => {
                assert_eq!(chr, "\u{0302}");
                match *content {
                    LatexNode::Text(s) => assert_eq!(s, "x"),
                    _ => panic!("Expected Text"),
                }
            }
            _ => panic!("Expected Accent"),
        }
    }

    #[test]
    fn test_complex_expression() {
        // E=mc^2\operatorname{Spec}(4{})
        let node = parse_latex("E=mc^2\\operatorname{Spec}(4{})");
        if let LatexNode::Sequence(nodes) = node {
            assert!(nodes.len() >= 3);
        }
    }

    #[test]
    fn test_lim_with_subscript() {
        let node = parse_latex("\\lim_{x \\to 0} f(x)");
        match node {
            LatexNode::Sequence(nodes) => {
                assert!(!nodes.is_empty());
                // First node should be an Operator (lim) with a subscript attached
                if let LatexNode::Subscript { base, .. } = &nodes[0] {
                    match base.as_ref() {
                        LatexNode::Operator(name) => assert_eq!(name, "lim"),
                        _ => panic!("Expected Operator base in Subscript, got: {:?}", base),
                    }
                } else {
                    // The subscript merge happens at parse() level — check it worked
                    // \lim is emitted as Operator("lim"), then _ is parsed as
                    // Subscript{empty, ...}, then parse() merges them
                    let result = format!("{:?}", nodes[0]);
                    assert!(result.contains("lim"), "lim missing: {}", result);
                }
            }
            _ => panic!("Expected Sequence"),
        }
    }

    #[test]
    fn matrix_rows_do_not_split_relation_commands() {
        let node = parse_latex("\\begin{cases}x^2&x\\geq0\\\\-x&x<0\\end{cases}");
        let LatexNode::Cases(rows) = node else {
            panic!("Expected cases AST");
        };
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(rows[0].len(), 2, "{rows:?}");
        assert!(format!("{:?}", rows[0][1]).contains("geq"), "{rows:?}");
    }

    #[test]
    fn test_text_with_spaces() {
        let node = parse_latex("\\text{Hello World}");
        let rendered = format!("{}", node);
        assert!(rendered.contains("Hello World"), "space lost: {}", rendered);
    }
}
