use latexsnipper_ast::{
    categorize_symbol, CommandInfo, EnvInfo, FormulaLayout, FormulaNode, SymbolCategory, SymbolInfo,
};
use latexsnipper_foundation::{Result, SnipperError};

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

    Ok(FormulaLayout {
        root,
        symbol_count,
        semantic_annotations: Vec::new(),
    })
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
        self.parse_expression_until(false)
    }

    fn parse_expression_until(&mut self, environment_delimiter: bool) -> Result<FormulaNode> {
        let mut nodes = Vec::new();

        while self.pos < self.input.len() {
            if environment_delimiter && self.at_environment_delimiter() {
                break;
            }

            let ch = self.input[self.pos];

            match ch {
                '%' => self.skip_comment(),
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
        self.skip_whitespace();
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

        // TeX control words are alphabetic; control symbols consume exactly one
        // non-alphabetic character (for example `\,` and `\!`).
        let cmd = if self.input[self.pos].is_alphabetic() {
            let start = self.pos;
            while self.pos < self.input.len() && self.input[self.pos].is_alphabetic() {
                self.pos += 1;
            }
            self.input[start..self.pos].iter().collect()
        } else {
            let command_symbol = self.input[self.pos];
            self.pos += 1;
            command_symbol.to_string()
        };

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
                let (opt_arg, content) = self.parse_command_with_braces()?;
                let index = opt_arg.map(Box::new);
                Ok(FormulaNode::SquareRoot {
                    index,
                    content: Box::new(content),
                })
            }
            "begin" => {
                let env_name = self.parse_group()?;
                let env_name_str = extract_text(&env_name);
                if matches!(
                    env_name_str.as_str(),
                    "verbatim" | "verbatim*" | "lstlisting" | "minted"
                ) {
                    return Err(SnipperError::Inference("Literal code environments cannot be treated as mathematical layout; retain raw source".into()));
                }
                let column_spec = if env_name_str == "array" {
                    Some(self.parse_array_column_spec()?)
                } else {
                    None
                };
                let rows = self.parse_environment_content(&env_name_str)?;
                let mut env = EnvInfo::new(env_name_str);
                env.column_spec = column_spec;
                let env = rows.into_iter().fold(env, |e, row| e.with_row(row));
                Ok(FormulaNode::Environment(env))
            }
            "end" => {
                let _env_name = self.parse_group()?;
                Ok(FormulaNode::Text(String::new()))
            }

            // Functions with optional limits
            "sum" | "prod" | "int" | "iint" | "iiint" | "oint" => {
                let node = self.parse_command_with_subsup(FormulaNode::Symbol(SymbolInfo::new(
                    format!("\\{}", cmd),
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

            // Style commands (displaystyle, textstyle, etc.)
            "displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle" => {
                let content = self.parse_atom()?;
                Ok(FormulaNode::Command(
                    CommandInfo::new(&cmd).with_arg(content),
                ))
            }

            // Phantom commands
            "phantom" | "vphantom" | "hphantom" => {
                let content = self.parse_atom()?;
                Ok(FormulaNode::Command(
                    CommandInfo::new(&cmd).with_arg(content),
                ))
            }

            // Boxed content
            "boxed" => {
                let content = self.parse_atom()?;
                Ok(FormulaNode::Command(
                    CommandInfo::new(&cmd).with_arg(content),
                ))
            }

            // Equation tag
            "tag" => {
                let tag = self.parse_group()?;
                Ok(FormulaNode::Command(CommandInfo::new(&cmd).with_arg(tag)))
            }

            // Common single-argument math macros.
            "abs" | "norm" | "ceil" | "floor" => {
                let content = self.parse_atom()?;
                Ok(FormulaNode::Command(
                    CommandInfo::new(&cmd).with_arg(content),
                ))
            }

            // Named delimiters are symbols and must not consume the next atom.
            "lvert" | "rvert" | "lVert" | "rVert" | "lceil" | "rceil" | "lfloor" | "rfloor" => {
                Ok(FormulaNode::Symbol(SymbolInfo::new(
                    format!("\\{}", cmd),
                    SymbolCategory::Delimiter,
                )))
            }

            "operatorname" => {
                let name = self.parse_group()?;
                Ok(FormulaNode::Command(CommandInfo::new(&cmd).with_arg(name)))
            }

            // Spacing commands
            "quad" | "qquad" | "," | ";" | ":" | "!" => Ok(FormulaNode::Symbol(SymbolInfo::new(
                format!("\\{}", cmd),
                SymbolCategory::Unknown,
            ))),

            // Underbrace/Overbrace (standalone)
            "underbrace" => {
                let content = self.parse_atom()?;
                let mut node = FormulaNode::Command(CommandInfo::new(&cmd).with_arg(content));
                // Check for _{label}
                node = self.parse_command_with_subsup(node);
                Ok(node)
            }
            "overbrace" => {
                let content = self.parse_atom()?;
                let mut node = FormulaNode::Command(CommandInfo::new(&cmd).with_arg(content));
                // Check for ^{label}
                node = self.parse_command_with_subsup(node);
                Ok(node)
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
                '%' => self.skip_comment(),
                '{' => {
                    self.pos += 1;
                    if let Ok(group) = self.parse_group() {
                        nodes.push(group);
                    }
                }
                '\\' => {
                    let cmd = self.parse_command();
                    if let Ok(n) = cmd {
                        nodes.push(n);
                    }
                }
                _ => {
                    let latex = ch.to_string();
                    nodes.push(FormulaNode::Symbol(SymbolInfo::new(
                        &latex,
                        categorize_symbol(&latex),
                    )));
                    self.pos += 1;
                }
            }
        }
        match nodes.len() {
            0 => FormulaNode::Text(String::new()),
            1 => nodes.pop().unwrap(),
            _ => FormulaNode::Group(nodes),
        }
    }

    /// Parse a command with optional braces and subscript/superscript.
    /// Returns (optional_arg, required_arg).
    fn parse_command_with_braces(&mut self) -> Result<(Option<FormulaNode>, FormulaNode)> {
        self.skip_whitespace();
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

    fn parse_environment_content(&mut self, expected_name: &str) -> Result<Vec<Vec<FormulaNode>>> {
        let mut rows = Vec::new();
        let mut current_row = Vec::new();

        while self.pos < self.input.len() {
            if self.at_command("end") {
                self.pos += "\\end".chars().count();
                let actual_name = extract_text(&self.parse_group()?);
                if actual_name != expected_name {
                    return Err(SnipperError::Inference(format!(
                        "mismatched LaTeX environment: expected \\end{{{expected_name}}}, found \\end{{{actual_name}}}"
                    )));
                }
                break;
            }

            if self.input[self.pos] == '\\' && self.peek_at(self.pos + 1) == Some(&'\\') {
                // Row separator \\
                self.pos += 2;
                rows.push(std::mem::take(&mut current_row));
                continue;
            }

            if self.input[self.pos] == '&' {
                // FormulaLayout stores rows rather than cells, so retain the
                // column separator explicitly for canonical LaTeX projection.
                self.pos += 1;
                current_row.push(FormulaNode::Text("&".to_string()));
                continue;
            }

            let node = self.parse_expression_until(true)?;
            if !matches!(&node, FormulaNode::Text(text) if text.is_empty()) {
                current_row.push(node);
            }
        }

        if !current_row.is_empty() || rows.is_empty() {
            rows.push(current_row);
        }

        Ok(rows)
    }

    fn parse_array_column_spec(&mut self) -> Result<String> {
        self.skip_whitespace();
        if self.peek_at(self.pos) != Some(&'{') {
            return Err(SnipperError::Inference(
                "array requires a braced column specification".into(),
            ));
        }
        self.pos += 1;
        let start = self.pos;
        let mut depth = 0;
        while self.pos < self.input.len() {
            if self.pos - start > 4096 {
                return Err(SnipperError::Inference(
                    "array column specification exceeds the layout limit".into(),
                ));
            }
            match self.input[self.pos] {
                '\\' if self.pos + 1 < self.input.len() => {
                    self.pos += 2;
                    continue;
                }
                '%' => {
                    self.skip_comment();
                    continue;
                }
                '{' => depth += 1,
                '}' if depth == 0 => {
                    let result = self.input[start..self.pos].iter().collect();
                    self.pos += 1;
                    return Ok(result);
                }
                '}' => depth -= 1,
                _ => {}
            }
            self.pos += 1;
        }
        Err(SnipperError::Inference(
            "unclosed array column specification".into(),
        ))
    }

    fn at_environment_delimiter(&self) -> bool {
        self.input.get(self.pos) == Some(&'&')
            || (self.input.get(self.pos) == Some(&'\\')
                && (self.input.get(self.pos + 1) == Some(&'\\') || self.at_command("end")))
    }

    fn at_command(&self, name: &str) -> bool {
        if self.input.get(self.pos) != Some(&'\\') {
            return false;
        }

        let name: Vec<char> = name.chars().collect();
        let start = self.pos + 1;
        let end = start + name.len();
        self.input.get(start..end) == Some(name.as_slice())
            && self.input.get(end).is_none_or(|ch| !ch.is_alphabetic())
    }

    fn peek_at(&self, pos: usize) -> Option<&char> {
        self.input.get(pos)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            if self.input[self.pos].is_whitespace() {
                self.pos += 1;
            } else if self.input[self.pos] == '%' {
                self.skip_comment();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) {
        while self.pos < self.input.len() && !matches!(self.input[self.pos], '\n' | '\r') {
            self.pos += 1;
        }
        if self.input.get(self.pos) == Some(&'\r') {
            self.pos += 1;
        }
        if self.input.get(self.pos) == Some(&'\n') {
            self.pos += 1;
        }
        while self
            .input
            .get(self.pos)
            .is_some_and(|ch| matches!(ch, ' ' | '\t'))
        {
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
        FormulaNode::SquareRoot { index, content } => {
            index.as_deref().map(count_symbols).unwrap_or_default() + count_symbols(content)
        }
        FormulaNode::Text(_) => 0,
        // Custom glyphs count as one symbol.
        FormulaNode::CustomGlyph(_) => 1,
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
            FormulaNode::SquareRoot { content, .. } => match content.as_ref() {
                FormulaNode::Symbol(s) => assert_eq!(s.latex, "x"),
                _ => panic!("Expected Symbol"),
            },
            _ => panic!("Expected SquareRoot"),
        }
    }

    #[test]
    fn test_parse_square_root_with_index() {
        let layout = parse_formula_latex("\\sqrt[3]{x}").unwrap();
        assert_eq!(layout.symbol_count, 2);
        assert_eq!(layout.canonical_latex(), "\\sqrt[3]{x}");
        match &layout.root {
            FormulaNode::SquareRoot { index, content } => {
                assert!(index.is_some(), "Expected index for sqrt[3]{{x}}");
                match content.as_ref() {
                    FormulaNode::Symbol(s) => assert_eq!(s.latex, "x"),
                    _ => panic!("Expected Symbol"),
                }
            }
            _ => panic!("Expected SquareRoot"),
        }
    }

    #[test]
    fn test_parse_greek() {
        let layout = parse_formula_latex("\\alpha + \\beta").unwrap();
        assert_eq!(layout.symbol_count, 3);
    }

    #[test]
    fn test_parse_matrix_rows_and_columns() {
        let layout = parse_formula_latex("\\begin{matrix}a&b\\\\c&d\\end{matrix}").unwrap();

        assert_eq!(
            layout.canonical_latex(),
            "\\begin{matrix}a&b\\\\c&d\\end{matrix}"
        );
        match &layout.root {
            FormulaNode::Environment(environment) => {
                assert_eq!(environment.name, "matrix");
                assert_eq!(environment.content.len(), 2);
            }
            _ => panic!("Expected Environment"),
        }
    }

    #[test]
    fn array_layout_keeps_column_spec_outside_mathematical_content() {
        for columns in ["lc", "*{2}{lr}", r">{\bfseries}p{2cm}", "l% fake }\nc"] {
            let source = format!("\\begin{{array}}{{{columns}}}a&b\\\\c&d\\end{{array}}");
            let layout = parse_formula_latex(&source).unwrap();
            assert_eq!(layout.symbol_count, 4);
            assert_eq!(layout.canonical_latex(), source);
            let FormulaNode::Environment(environment) = &layout.root else {
                panic!("Expected Environment");
            };
            assert_eq!(environment.column_spec.as_deref(), Some(columns));
            assert_eq!(environment.content.len(), 2);
        }
    }

    #[test]
    fn array_layout_rejects_missing_unclosed_or_oversized_column_spec() {
        for source in [
            r"\begin{array}x\end{array}".to_string(),
            r"\begin{array}{l".to_string(),
            format!("\\begin{{array}}{{{}}}x\\end{{array}}", "l".repeat(4098)),
        ] {
            assert!(parse_formula_latex(&source).is_err(), "{source}");
        }
    }

    #[test]
    fn test_rejects_mismatched_environment_end() {
        let error = parse_formula_latex("\\begin{matrix}x\\end{cases}").unwrap_err();
        assert!(error.to_string().contains("mismatched LaTeX environment"));
    }

    #[test]
    fn test_named_delimiters_do_not_consume_following_atom() {
        let layout = parse_formula_latex("\\lvert x\\rvert").unwrap();
        assert_eq!(layout.canonical_latex(), "\\lvert x\\rvert");
    }

    #[test]
    fn test_control_symbol_spacing_is_preserved() {
        let layout = parse_formula_latex("a\\,b").unwrap();
        assert_eq!(layout.canonical_latex(), "a\\,b");
    }

    #[test]
    fn comments_do_not_add_symbols_or_change_groups_scripts_and_rows() {
        for ending in ["\n", "\r\n", "\r"] {
            for (source, expected, count) in [
                (format!("\\frac% FAKE {{}}{ending}{{a% FAKE }}{ending}+b}}{{c}}"), r"\frac{a+b}{c}", 4),
                (format!("x^% FAKE }}{ending}2"), r"x^{2}", 2),
                (format!("\\sqrt[% FAKE ]{ending}3]{{x}}"), r"\sqrt[3]{x}", 2),
                (format!("\\alpha% FAKE \\beta{ending}x"), r"\alpha x", 2),
                (format!("\\begin{{matrix}}a&b% FAKE & \\\\ \\end{{matrix}}{ending}\\\\c&d\\end{{matrix}}"), r"\begin{matrix}a&b\\c&d\end{matrix}", 4),
            ] {
                let layout = parse_formula_latex(&source).unwrap();
                assert_eq!(layout.canonical_latex(), expected, "{source:?}");
                assert_eq!(layout.symbol_count, count, "{source:?}");
            }
        }
        let layout = parse_formula_latex(r"50\%x").unwrap();
        assert_eq!(layout.canonical_latex(), r"50\%x");
        assert!(parse_formula_latex("\\begin{verbatim}a%literal\n\\end{verbatim}").is_err());
    }
}
