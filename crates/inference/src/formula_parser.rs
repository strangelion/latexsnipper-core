use latexsnipper_ast::{
    categorize_symbol, CommandInfo, EnvInfo, FormulaLayout, FormulaNode, SymbolCategory, SymbolInfo,
};
use latexsnipper_foundation::Result;

/// Parse a LaTeX formula string into a structured FormulaLayout.
///
/// This parser handles:
/// - Symbols (numbers, letters, operators)
/// - Commands (\frac, \sqrt, \sum, etc.)
/// - Groups ({})
/// - Superscripts (^) and subscripts (_)
/// - Environments (\begin{...}...\end{...})
pub fn parse_formula_latex(latex: &str) -> Result<FormulaLayout> {
    let mut parser = FormulaParser::new(latex);
    let root = parser.parse()?;
    let symbol_count = count_symbols(&root);

    Ok(FormulaLayout { root, symbol_count })
}

/// Internal parser state.
struct FormulaParser {
    input: Vec<char>,
    pos: usize,
}

impl FormulaParser {
    fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn parse(&mut self) -> Result<FormulaNode> {
        self.parse_expression()
    }

    fn parse_expression(&mut self) -> Result<FormulaNode> {
        let mut nodes = Vec::new();

        while self.pos < self.input.len() {
            let ch = self.input[self.pos];

            match ch {
                '{' => {
                    self.pos += 1;
                    let group = self.parse_group()?;
                    nodes.push(group);
                }
                '}' => {
                    // End of group
                    break;
                }
                '^' => {
                    self.pos += 1;
                    if let Some(last) = nodes.last_mut() {
                        let exp = self.parse_atom()?;
                        let base = std::mem::replace(last, FormulaNode::Text(String::new()));
                        *last = FormulaNode::Superscript {
                            base: Box::new(base),
                            exp: Box::new(exp),
                        };
                    }
                }
                '_' => {
                    self.pos += 1;
                    if let Some(last) = nodes.last_mut() {
                        let sub = self.parse_atom()?;
                        let base = std::mem::replace(last, FormulaNode::Text(String::new()));
                        *last = FormulaNode::Subscript {
                            base: Box::new(base),
                            sub: Box::new(sub),
                        };
                    }
                }
                '\\' => {
                    let cmd = self.parse_command()?;
                    nodes.push(cmd);
                }
                ' ' | '\t' | '\n' | '\r' => {
                    self.pos += 1;
                }
                _ => {
                    let symbol = self.parse_symbol()?;
                    nodes.push(symbol);
                }
            }
        }

        if nodes.len() == 1 {
            Ok(nodes.pop().unwrap())
        } else if nodes.is_empty() {
            Ok(FormulaNode::Text(String::new()))
        } else {
            Ok(FormulaNode::Group(nodes))
        }
    }

    fn parse_group(&mut self) -> Result<FormulaNode> {
        // Consume opening brace if present.
        // Callers: parse_expression/@'{' and parse_atom/@'{' already consume '{',
        // but parse_command calls parse_group directly (for \frac, \sqrt, etc.)
        // where '{' has NOT been consumed yet.
        if self.pos < self.input.len() && self.input[self.pos] == '{' {
            self.pos += 1;
        }

        let mut nodes = Vec::new();

        while self.pos < self.input.len() {
            let ch = self.input[self.pos];

            if ch == '}' {
                self.pos += 1;
                break;
            }

            let node = self.parse_expression()?;
            nodes.push(node);
        }

        if nodes.len() == 1 {
            Ok(nodes.pop().unwrap())
        } else if nodes.is_empty() {
            Ok(FormulaNode::Text(String::new()))
        } else {
            Ok(FormulaNode::Group(nodes))
        }
    }

    fn parse_command(&mut self) -> Result<FormulaNode> {
        // Skip backslash
        self.pos += 1;

        if self.pos >= self.input.len() {
            return Ok(FormulaNode::Text("\\".to_string()));
        }

        // Read command name
        let start = self.pos;
        while self.pos < self.input.len() && self.input[self.pos].is_alphabetic() {
            self.pos += 1;
        }
        let cmd: String = self.input[start..self.pos].iter().collect();

        if cmd.is_empty() {
            return Ok(FormulaNode::Text("\\".to_string()));
        }

        // Match known commands
        match cmd.as_str() {
            // Structural commands
            "frac" => {
                let num = self.parse_group()?;
                let den = self.parse_group()?;
                Ok(FormulaNode::Fraction {
                    num: Box::new(num),
                    den: Box::new(den),
                })
            }
            "sqrt" => {
                // Optional argument: \sqrt[n]{x}
                let (_opt_arg, content) = self.parse_command_with_braces()?;
                Ok(FormulaNode::SquareRoot {
                    content: Box::new(content),
                })
            }
            "begin" => {
                let env_name = self.parse_group()?;
                let env_name_str = extract_text(&env_name);
                let content = self.parse_environment_content()?;
                Ok(FormulaNode::Environment(
                    EnvInfo::new(env_name_str).with_row(content),
                ))
            }
            "end" => {
                let _env_name = self.parse_group()?;
                Ok(FormulaNode::Text(String::new()))
            }

            // Functions with optional limits
            "sum" | "prod" | "int" | "iint" | "iiint" | "oint" => {
                let node = self.parse_command_with_subsup(FormulaNode::Symbol(SymbolInfo::new(
                    &format!("\\{}", cmd),
                    SymbolCategory::Operator,
                )));
                Ok(node)
            }

            // Functions
            "sin" | "cos" | "tan" | "lim" | "log" | "ln" | "exp" | "det" | "gcd" | "min"
            | "max" | "sup" | "inf" => {
                let cmd_node = FormulaNode::Command(CommandInfo::new(&cmd));
                let cmd_node = self.parse_command_with_subsup(cmd_node);
                Ok(cmd_node)
            }

            // Delimiters
            "left" | "right" => {
                let delimiter = self.parse_atom()?;
                Ok(FormulaNode::Command(
                    CommandInfo::new(&cmd).with_arg(delimiter),
                ))
            }

            // Accents
            "bar" | "hat" | "vec" | "dot" | "ddot" | "tilde" | "widehat" | "widetilde"
            | "overline" => {
                let content = self.parse_atom()?;
                Ok(FormulaNode::Command(
                    CommandInfo::new(&cmd).with_arg(content),
                ))
            }

            // Greek letters
            "alpha" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\alpha",
                SymbolCategory::Greek,
            ))),
            "beta" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\beta",
                SymbolCategory::Greek,
            ))),
            "gamma" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\gamma",
                SymbolCategory::Greek,
            ))),
            "delta" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\delta",
                SymbolCategory::Greek,
            ))),
            "epsilon" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\epsilon",
                SymbolCategory::Greek,
            ))),
            "zeta" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\zeta",
                SymbolCategory::Greek,
            ))),
            "eta" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\eta",
                SymbolCategory::Greek,
            ))),
            "theta" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\theta",
                SymbolCategory::Greek,
            ))),
            "iota" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\iota",
                SymbolCategory::Greek,
            ))),
            "kappa" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\kappa",
                SymbolCategory::Greek,
            ))),
            "lambda" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\lambda",
                SymbolCategory::Greek,
            ))),
            "mu" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\mu",
                SymbolCategory::Greek,
            ))),
            "nu" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\nu",
                SymbolCategory::Greek,
            ))),
            "xi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\xi",
                SymbolCategory::Greek,
            ))),
            "pi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\pi",
                SymbolCategory::Greek,
            ))),
            "rho" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\rho",
                SymbolCategory::Greek,
            ))),
            "sigma" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\sigma",
                SymbolCategory::Greek,
            ))),
            "tau" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\tau",
                SymbolCategory::Greek,
            ))),
            "upsilon" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\upsilon",
                SymbolCategory::Greek,
            ))),
            "phi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\phi",
                SymbolCategory::Greek,
            ))),
            "chi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\chi",
                SymbolCategory::Greek,
            ))),
            "psi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\psi",
                SymbolCategory::Greek,
            ))),
            "omega" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\omega",
                SymbolCategory::Greek,
            ))),
            "Alpha" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Alpha",
                SymbolCategory::Greek,
            ))),
            "Beta" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Beta",
                SymbolCategory::Greek,
            ))),
            "Gamma" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Gamma",
                SymbolCategory::Greek,
            ))),
            "Delta" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Delta",
                SymbolCategory::Greek,
            ))),
            "Theta" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Theta",
                SymbolCategory::Greek,
            ))),
            "Lambda" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Lambda",
                SymbolCategory::Greek,
            ))),
            "Xi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Xi",
                SymbolCategory::Greek,
            ))),
            "Pi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Pi",
                SymbolCategory::Greek,
            ))),
            "Sigma" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Sigma",
                SymbolCategory::Greek,
            ))),
            "Phi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Phi",
                SymbolCategory::Greek,
            ))),
            "Psi" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Psi",
                SymbolCategory::Greek,
            ))),
            "Omega" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Omega",
                SymbolCategory::Greek,
            ))),

            "infty" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\infty",
                SymbolCategory::Constant,
            ))),
            "neq" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\neq",
                SymbolCategory::Relation,
            ))),
            "leq" | "le" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\leq",
                SymbolCategory::Relation,
            ))),
            "geq" | "ge" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\geq",
                SymbolCategory::Relation,
            ))),
            "approx" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\approx",
                SymbolCategory::Relation,
            ))),
            "equiv" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\equiv",
                SymbolCategory::Relation,
            ))),
            "times" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\times",
                SymbolCategory::Operator,
            ))),
            "cdot" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\cdot",
                SymbolCategory::Operator,
            ))),
            "pm" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\pm",
                SymbolCategory::Operator,
            ))),
            "mp" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\mp",
                SymbolCategory::Operator,
            ))),

            // Arrows
            "rightarrow" | "to" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\rightarrow",
                SymbolCategory::Arrow,
            ))),
            "leftarrow" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\leftarrow",
                SymbolCategory::Arrow,
            ))),
            "leftrightarrow" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\leftrightarrow",
                SymbolCategory::Arrow,
            ))),
            "Rightarrow" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Rightarrow",
                SymbolCategory::Arrow,
            ))),
            "Leftarrow" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Leftarrow",
                SymbolCategory::Arrow,
            ))),
            "Leftrightarrow" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                "\\Leftrightarrow",
                SymbolCategory::Arrow,
            ))),

            // Unknown command - try to parse as simple command with optional args
            _ => {
                // Try optional subscript/superscript
                let cmd_node = FormulaNode::Command(CommandInfo::new(&cmd));
                Ok(self.parse_command_with_subsup(cmd_node))
            }
        }
    }

    fn parse_symbol(&mut self) -> Result<FormulaNode> {
        let ch = self.input[self.pos];
        self.pos += 1;

        let latex = ch.to_string();
        let category = categorize_symbol(&latex);

        Ok(FormulaNode::Symbol(SymbolInfo::new(latex, category)))
    }

    fn parse_atom(&mut self) -> Result<FormulaNode> {
        self.skip_whitespace();

        if self.pos >= self.input.len() {
            return Ok(FormulaNode::Text(String::new()));
        }

        let ch = self.input[self.pos];

        match ch {
            '{' => {
                self.pos += 1;
                self.parse_group()
            }
            '[' => {
                self.pos += 1;
                let node = self.parse_optional_arg();
                self.skip_whitespace();
                Ok(node)
            }
            '\\' => self.parse_command(),
            _ => self.parse_symbol(),
        }
    }

    /// Parse an optional argument [...] in LaTeX.
    /// The opening '[' has already been consumed.
    fn parse_optional_arg(&mut self) -> FormulaNode {
        let mut nodes = Vec::new();
        while self.pos < self.input.len() {
            let ch = self.input[self.pos];
            if ch == ']' {
                self.pos += 1;
                break;
            }
            match ch {
                '{' => {
                    self.pos += 1;
                    let _ = self.parse_group(); // skip nested
                }
                '\\' => {
                    let cmd = self.parse_command();
                    if let Ok(n) = cmd {
                        nodes.push(n);
                    }
                }
                _ => {
                    nodes.push(FormulaNode::Text(ch.to_string()));
                    self.pos += 1;
                }
            }
        }
        if nodes.is_empty() {
            FormulaNode::Text(String::new())
        } else {
            FormulaNode::Group(nodes)
        }
    }

    /// Parse a command with optional braces and subscript/superscript.
    /// Returns (optional_arg, required_arg).
    fn parse_command_with_braces(&mut self) -> Result<(Option<FormulaNode>, FormulaNode)> {
        // Check for optional argument [...]
        let opt = if self.pos < self.input.len() && self.input[self.pos] == '[' {
            self.pos += 1;
            let node = self.parse_optional_arg();
            Some(node)
        } else {
            None
        };
        let content = self.parse_group()?;
        Ok((opt, content))
    }

    /// Parse subscript/superscript after a command node.
    fn parse_command_with_subsup(&mut self, mut node: FormulaNode) -> FormulaNode {
        loop {
            self.skip_whitespace();
            if self.pos >= self.input.len() {
                break;
            }
            match self.input[self.pos] {
                '^' => {
                    self.pos += 1;
                    if let Ok(exp) = self.parse_atom() {
                        let base = std::mem::replace(&mut node, FormulaNode::Text(String::new()));
                        node = FormulaNode::Superscript {
                            base: Box::new(base),
                            exp: Box::new(exp),
                        };
                    }
                }
                '_' => {
                    self.pos += 1;
                    if let Ok(sub) = self.parse_atom() {
                        let base = std::mem::replace(&mut node, FormulaNode::Text(String::new()));
                        node = FormulaNode::Subscript {
                            base: Box::new(base),
                            sub: Box::new(sub),
                        };
                    }
                }
                _ => break,
            }
        }
        node
    }

    fn parse_environment_content(&mut self) -> Result<Vec<FormulaNode>> {
        let mut nodes = Vec::new();

        while self.pos < self.input.len() {
            let ch = self.input[self.pos];

            if ch == '\\' && self.peek_str(3) == "end" {
                break;
            }

            if ch == '\\' && self.peek_at(self.pos + 1) == Some(&'\\') {
                // Row separator \\
                self.pos += 2;
                continue;
            }

            let node = self.parse_expression()?;
            nodes.push(node);
        }

        Ok(nodes)
    }

    fn peek_str(&self, len: usize) -> String {
        let start = self.pos;
        let end = (start + len).min(self.input.len());
        self.input[start..end].iter().collect()
    }

    fn peek_at(&self, pos: usize) -> Option<&char> {
        self.input.get(pos)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }
}

/// Count total symbols in a formula tree.
fn count_symbols(node: &FormulaNode) -> usize {
    match node {
        FormulaNode::Symbol(_) => 1,
        FormulaNode::Command(cmd) => 1 + cmd.args.iter().map(count_symbols).sum::<usize>(),
        FormulaNode::Group(nodes) => nodes.iter().map(count_symbols).sum(),
        FormulaNode::Environment(env) => env
            .content
            .iter()
            .flat_map(|row| row.iter())
            .map(count_symbols)
            .sum(),
        FormulaNode::Superscript { base, exp } => count_symbols(base) + count_symbols(exp),
        FormulaNode::Subscript { base, sub } => count_symbols(base) + count_symbols(sub),
        FormulaNode::Fraction { num, den } => count_symbols(num) + count_symbols(den),
        FormulaNode::SquareRoot { content } => count_symbols(content),
        FormulaNode::Text(_) => 0,
    }
}

/// Extract text content from a formula node.
fn extract_text(node: &FormulaNode) -> String {
    match node {
        FormulaNode::Text(s) => s.clone(),
        FormulaNode::Symbol(s) => s.latex.clone(),
        FormulaNode::Group(nodes) => nodes.iter().map(extract_text).collect(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_symbol() {
        let layout = parse_formula_latex("x").unwrap();
        assert_eq!(layout.symbol_count, 1);
        match &layout.root {
            FormulaNode::Symbol(s) => assert_eq!(s.latex, "x"),
            _ => panic!("Expected Symbol"),
        }
    }

    #[test]
    fn test_parse_number() {
        let layout = parse_formula_latex("42").unwrap();
        assert_eq!(layout.symbol_count, 2);
    }

    #[test]
    fn test_parse_operator() {
        let layout = parse_formula_latex("a+b").unwrap();
        assert_eq!(layout.symbol_count, 3);
    }

    #[test]
    fn test_parse_superscript() {
        let layout = parse_formula_latex("x^2").unwrap();
        assert_eq!(layout.symbol_count, 2);
        match &layout.root {
            FormulaNode::Superscript { base, exp } => {
                match base.as_ref() {
                    FormulaNode::Symbol(s) => assert_eq!(s.latex, "x"),
                    _ => panic!("Expected Symbol for base"),
                }
                match exp.as_ref() {
                    FormulaNode::Symbol(s) => assert_eq!(s.latex, "2"),
                    _ => panic!("Expected Symbol for exp"),
                }
            }
            _ => panic!("Expected Superscript"),
        }
    }

    #[test]
    fn test_parse_fraction() {
        let layout = parse_formula_latex("\\frac{a}{b}").unwrap();
        assert_eq!(layout.symbol_count, 2);
        match &layout.root {
            FormulaNode::Fraction { num, den } => {
                match num.as_ref() {
                    FormulaNode::Symbol(s) => assert_eq!(s.latex, "a"),
                    _ => panic!("Expected Symbol for num"),
                }
                match den.as_ref() {
                    FormulaNode::Symbol(s) => assert_eq!(s.latex, "b"),
                    _ => panic!("Expected Symbol for den"),
                }
            }
            _ => panic!("Expected Fraction"),
        }
    }

    #[test]
    fn test_parse_square_root() {
        let layout = parse_formula_latex("\\sqrt{x}").unwrap();
        assert_eq!(layout.symbol_count, 1);
        match &layout.root {
            FormulaNode::SquareRoot { content } => match content.as_ref() {
                FormulaNode::Symbol(s) => assert_eq!(s.latex, "x"),
                _ => panic!("Expected Symbol"),
            },
            _ => panic!("Expected SquareRoot"),
        }
    }

    #[test]
    fn test_parse_greek() {
        let layout = parse_formula_latex("\\alpha + \\beta").unwrap();
        assert_eq!(layout.symbol_count, 3);
    }
}
