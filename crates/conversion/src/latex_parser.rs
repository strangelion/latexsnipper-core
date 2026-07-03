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
}

impl LatexParser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
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
            _ => self.parse_text(),
        }
    }

    fn parse_single(&mut self) -> LatexNode {
        if self.pos >= self.chars.len() {
            return LatexNode::Text(String::new());
        }

        match self.chars[self.pos] {
            '{' => {
                self.pos += 1;
                let content = self.parse_until('}');
                if content.len() == 1 {
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
                        '\\' | '{' | '}' | '$' | '^' | '_' | ' ' | '(' | ')' | '[' | ']' | ':'
                        | ',' | ';' => break,
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

    fn parse_text(&mut self) -> Option<LatexNode> {
        let start = self.pos;
        while self.pos < self.chars.len() {
            match self.chars[self.pos] {
                '\\' | '{' | '}' | '$' | '^' | '_' | ':' | ',' | ';' => break,
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
                // \, \; \: are thin/medium spaces, not punctuation
                ',' | ';' | ':' => Some(LatexNode::Text(" ".to_string())),
                ' ' | '!' => Some(LatexNode::Text(ch.to_string())),
                '(' | ')' | '[' | ']' => Some(LatexNode::Text(ch.to_string())),
                _ => Some(LatexNode::Text(ch.to_string())),
            };
        }

        let start = self.pos;
        while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_alphabetic() {
            self.pos += 1;
        }

        let cmd: String = self.chars[start..self.pos].iter().collect();

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
            "int" | "iint" | "iiint" | "oint" | "sum" | "prod" | "coprod" | "lim" | "limsup"
            | "liminf" | "max" | "min" | "sup" | "inf" => Some(LatexNode::Operator(cmd)),
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
            | "Leftarrow" | "Leftrightarrow" | "mapsto" | "uparrow" | "downarrow" | "nearrow"
            | "searrow" | "swarrow" | "nwarrow" => Some(LatexNode::Symbol(cmd)),
            // Fraction
            "frac" => {
                let num = self.parse_single();
                let den = self.parse_single();
                Some(LatexNode::Fraction {
                    num: Box::new(num),
                    den: Box::new(den),
                })
            }
            // Square root
            "sqrt" => {
                let mut index = None;
                if self.pos < self.chars.len() && self.chars[self.pos] == '[' {
                    self.pos += 1;
                    let start = self.pos;
                    while self.pos < self.chars.len() && self.chars[self.pos] != ']' {
                        self.pos += 1;
                    }
                    let idx_text: String = self.chars[start..self.pos].iter().collect();
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
            "vec" => Some(LatexNode::Accent {
                chr: "\u{20D7}".to_string(),
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
            // Font modifiers
            "mathbb" | "mathbf" | "mathit" | "mathsf" | "mathtt" | "mathcal" | "mathfrak"
            | "mathrm" | "mathnormal" | "boldsymbol" | "bm" => {
                let font = if cmd == "bm" {
                    "boldsymbol".to_string()
                } else {
                    cmd
                };
                let content = self.parse_single();
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
            "text" | "textbf" | "textit" | "textrm" | "textsf" | "texttt" => {
                let content = self.parse_single();
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
            "begin" => self.parse_environment(),
            // \left ... \right
            "left" => self.parse_delimited(),
            // Unknown command — store as Command node
            _ => Some(LatexNode::Command {
                name: cmd,
                args: Vec::new(),
            }),
        }
    }

    fn parse_environment(&mut self) -> Option<LatexNode> {
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

        let content = self.parse_until_begin_end(&env_name);

        match env_name.as_str() {
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
            "aligned" | "align" | "gather" => {
                let rows = Self::parse_matrix_content(&content);
                Some(LatexNode::Matrix {
                    env: env_name,
                    rows,
                })
            }
            "array" => {
                let rows = Self::parse_matrix_content(&content);
                Some(LatexNode::Matrix {
                    env: env_name,
                    rows,
                })
            }
            _ => {
                // Unknown environment — treat content as a group
                let mut parser = LatexParser::new(&content);
                let nodes = parser.parse();
                Some(LatexNode::Command {
                    name: format!("begin{{{}}}", env_name),
                    args: vec![nodes],
                })
            }
        }
    }

    fn parse_delimited(&mut self) -> Option<LatexNode> {
        self.skip_whitespace();
        if self.pos >= self.chars.len() {
            return None;
        }

        let left_ch = self.chars[self.pos];
        self.pos += 1;

        let left = match left_ch {
            '(' => "(".to_string(),
            ')' => ")".to_string(),
            '[' => "[".to_string(),
            ']' => "]".to_string(),
            '|' => "|".to_string(),
            '{' => ".".to_string(), // \left{ → invisible
            _ => left_ch.to_string(),
        };

        let right = match left_ch {
            '(' => ")",
            ')' => "(",
            '[' => "]",
            ']' => "[",
            '|' => "|",
            '{' => "}",
            _ => ")",
        };

        // Parse content until \right{right_char}
        let mut content_str = String::new();
        while self.pos < self.chars.len() {
            if self.pos + 5 < self.chars.len() {
                let remaining: String = self.chars[self.pos..].iter().take(6).collect();
                if remaining.starts_with("\\right") {
                    let after_right = &self.chars[self.pos + 6..];
                    if !after_right.is_empty()
                        && after_right[0] == right.chars().next().unwrap_or(')')
                    {
                        self.pos += 7; // skip \rightX
                        break;
                    }
                }
            }
            content_str.push(self.chars[self.pos]);
            self.pos += 1;
        }

        let mut parser = LatexParser::new(&content_str);
        let content_nodes = parser.parse();

        Some(LatexNode::Delimited {
            left,
            content: if let LatexNode::Sequence(nodes) = content_nodes {
                nodes
            } else {
                vec![content_nodes]
            },
            right: right.to_string(),
        })
    }

    fn parse_until_begin_end(&mut self, env_name: &str) -> String {
        let mut result = String::new();
        let mut depth = 0i32;
        let end_tag = format!("\\end{{{}}}", env_name);

        while self.pos < self.chars.len() {
            let remaining: String = self.chars[self.pos..].iter().take(end_tag.len()).collect();
            if remaining == end_tag {
                self.pos += end_tag.len();
                break;
            }
            // Track nested \begin{}...\end{}
            if self.pos + 5 < self.chars.len() {
                let sub: String = self.chars[self.pos..].iter().take(6).collect();
                if sub.starts_with("\\begin") {
                    depth += 1;
                } else if sub.starts_with("\\end{") {
                    // Only decrement if it matches our environment
                    let after_end: String = self.chars[self.pos + 5..]
                        .iter()
                        .take(env_name.len() + 1)
                        .collect();
                    if after_end.starts_with(env_name) && after_end.ends_with('}') && depth > 0 {
                        depth -= 1;
                    }
                }
            }
            result.push(self.chars[self.pos]);
            self.pos += 1;
        }

        result
    }

    fn parse_matrix_content(content: &str) -> Vec<Vec<LatexNode>> {
        let mut rows = Vec::new();
        for row_str in content.split('\\') {
            let row_str = row_str.trim();
            if row_str.is_empty() || row_str == "\\" {
                continue;
            }
            let cells: Vec<LatexNode> = row_str
                .split('&')
                .map(|cell| {
                    let mut parser = LatexParser::new(cell.trim());
                    parser.parse()
                })
                .collect();
            if !cells.is_empty() {
                rows.push(cells);
            }
        }
        rows
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos] == ' ' {
            self.pos += 1;
        }
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
        match node {
            LatexNode::Sequence(nodes) => {
                assert!(!nodes.is_empty());
            }
            _ => {}
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
        match node {
            LatexNode::Sequence(nodes) => {
                assert!(nodes.len() >= 3);
            }
            _ => {}
        }
    }
}
