//! Simple LaTeX parser that builds a structured AST.

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
                // Handle superscript/subscript by modifying the last node
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
                // The base should already be in the nodes list
                // For now, return just the superscript
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
            _ => {
                // Regular text
                self.parse_text()
            }
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
                // Unwrap single-element groups
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
                // Read a single token (letter, digit, or symbol)
                let start = self.pos;
                while self.pos < self.chars.len() {
                    match self.chars[self.pos] {
                        '\\' | '{' | '}' | '$' | '^' | '_' | ' ' | '(' | ')' | '[' | ']' => break,
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
                '\\' | '{' | '}' | '$' | '^' | '_' => break,
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

        // Read command name
        let start = self.pos;
        while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_alphabetic() {
            self.pos += 1;
        }

        if self.pos == start {
            // Not an alphabetic command, handle escaped character
            let ch = self.chars[self.pos];
            self.pos += 1;
            return Some(LatexNode::Text(ch.to_string()));
        }

        let cmd: String = self.chars[start..self.pos].iter().collect();

        match cmd.as_str() {
            // Greek letters
            "alpha" | "beta" | "gamma" | "delta" | "epsilon" | "varepsilon" | "zeta" | "eta"
            | "theta" | "vartheta" | "iota" | "kappa" | "varkappa" | "lambda" | "mu" | "nu"
            | "xi" | "pi" | "varpi" | "rho" | "varrho" | "sigma" | "varsigma" | "tau"
            | "upsilon" | "phi" | "varphi" | "chi" | "psi" | "omega" | "Gamma" | "Delta"
            | "Theta" | "Lambda" | "Xi" | "Pi" | "Sigma" | "Upsilon" | "Phi" | "Psi" | "Omega" => {
                Some(LatexNode::Greek(cmd))
            }
            // Operators
            "int" | "iint" | "iiint" | "oint" | "sum" | "prod" | "coprod" | "lim" | "limsup"
            | "liminf" | "max" | "min" | "sup" | "inf" => Some(LatexNode::Operator(cmd)),
            // Relations
            "leq" | "le" | "geq" | "ge" | "neq" | "ne" | "approx" | "equiv" | "sim" | "propto"
            | "ll" | "gg" | "prec" | "succ" => Some(LatexNode::Relation(cmd)),
            // Symbols
            "infty" | "partial" | "nabla" | "forall" | "exists" | "neg" | "land" | "lor" | "in"
            | "notin" | "subset" | "supset" | "cup" | "cap" | "emptyset" | "pm" | "mp"
            | "times" | "div" | "cdot" | "ast" | "star" | "circ" | "bullet" | "diamond"
            | "oplus" | "otimes" | "odot" | "lfloor" | "rfloor" | "lceil" | "rceil" | "langle"
            | "rangle" | "lvert" | "rvert" | "lVert" | "rVert" | "quad" | "qquad" | "ldots"
            | "cdots" | "vdots" | "ddots" | "hbar" | "ell" => Some(LatexNode::Symbol(cmd)),
            // Commands with arguments
            "frac" => {
                let num = self.parse_single();
                let den = self.parse_single();
                Some(LatexNode::Fraction {
                    num: Box::new(num),
                    den: Box::new(den),
                })
            }
            "sqrt" => {
                // Check for optional [n] argument
                let mut index = None;
                if self.pos < self.chars.len() && self.chars[self.pos] == '[' {
                    self.pos += 1;
                    // Read content until ']'
                    let start = self.pos;
                    while self.pos < self.chars.len() && self.chars[self.pos] != ']' {
                        self.pos += 1;
                    }
                    let idx_text: String = self.chars[start..self.pos].iter().collect();
                    if !idx_text.is_empty() {
                        index = Some(Box::new(LatexNode::Text(idx_text)));
                    }
                    if self.pos < self.chars.len() {
                        self.pos += 1; // Skip ']'
                    }
                }
                let content = self.parse_single();
                Some(LatexNode::SquareRoot {
                    index,
                    content: Box::new(content),
                })
            }
            "binom" => {
                let n = self.parse_single();
                let k = self.parse_single();
                Some(LatexNode::Command {
                    name: "binom".to_string(),
                    args: vec![n, k],
                })
            }
            // Font modifiers
            "mathbb" | "mathbf" | "mathit" | "mathsf" | "mathtt" | "mathcal" | "mathfrak"
            | "mathrm" | "mathnormal" => {
                let content = self.parse_single();
                Some(LatexNode::FontModifier {
                    font: cmd,
                    content: Box::new(content),
                })
            }
            // Text command
            "text" | "textbf" | "textit" => {
                let content = self.parse_single();
                Some(LatexNode::Command {
                    name: cmd,
                    args: vec![content],
                })
            }
            // Unknown command
            _ => Some(LatexNode::Command {
                name: cmd,
                args: Vec::new(),
            }),
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
                        return nodes;
                    }
                    depth -= 1;
                    self.pos += 1;
                }
                c if c == delimiter && depth == 0 => {
                    self.pos += 1;
                    return nodes;
                }
                _ => {
                    if let Some(node) = self.parse_element() {
                        nodes.push(node);
                    }
                }
            }
        }

        nodes
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
}
