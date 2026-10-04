//! Independent, bounded UnicodeMath pilot. Not a registered conversion route.
//! See docs/formats/unicodemath-pilot.md for the finite grammar and gates.
use crate::latex_ast::LatexNode;
use std::fmt;
const MAX_BYTES: usize = 64 * 1024;
const MAX_TOKENS: usize = 4096;
const MAX_NODES: usize = 8192;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnicodeMathErrorKind {
    UnexpectedToken,
    UnsupportedSyntax,
    LimitExceeded,
}
/// Parser offsets are UTF-8 bytes; writer offsets are emitted-text offsets or 0.
/// They are not original LaTeX spans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnicodeMathError {
    pub kind: UnicodeMathErrorKind,
    pub offset: usize,
    pub message: String,
}
impl fmt::Display for UnicodeMathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UnicodeMath {:?} at byte {}: {}",
            self.kind, self.offset, self.message
        )
    }
}
impl std::error::Error for UnicodeMathError {}
fn error(kind: UnicodeMathErrorKind, offset: usize, message: &str) -> UnicodeMathError {
    UnicodeMathError {
        kind,
        offset,
        message: message.into(),
    }
}

// Explicit glyph inventories, not a claim to implement all Unicode properties.
const GREEK: &[(char, &str)] = &[
    ('α', "alpha"),
    ('β', "beta"),
    ('γ', "gamma"),
    ('Γ', "Gamma"),
    ('δ', "delta"),
    ('Δ', "Delta"),
    ('ϵ', "epsilon"),
    ('ε', "varepsilon"),
    ('ζ', "zeta"),
    ('η', "eta"),
    ('θ', "theta"),
    ('Θ', "Theta"),
    ('ϑ', "vartheta"),
    ('ι', "iota"),
    ('κ', "kappa"),
    ('λ', "lambda"),
    ('Λ', "Lambda"),
    ('μ', "mu"),
    ('ν', "nu"),
    ('ξ', "xi"),
    ('Ξ', "Xi"),
    ('π', "pi"),
    ('Π', "Pi"),
    ('ρ', "rho"),
    ('σ', "sigma"),
    ('Σ', "Sigma"),
    ('τ', "tau"),
    ('υ', "upsilon"),
    ('φ', "varphi"),
    ('ϕ', "phi"),
    ('Φ', "Phi"),
    ('χ', "chi"),
    ('ψ', "psi"),
    ('Ψ', "Psi"),
    ('ω', "omega"),
    ('Ω', "Omega"),
];
const SYMBOLS: &[(char, &str)] = &[
    ('∞', "infty"),
    ('∂', "partial"),
    ('∇', "nabla"),
    ('±', "pm"),
    ('×', "times"),
    ('÷', "div"),
    ('⋅', "cdot"),
    ('∈', "in"),
    ('∉', "notin"),
    ('≤', "leq"),
    ('≥', "geq"),
    ('≠', "neq"),
    ('≈', "approx"),
    ('≡', "equiv"),
    ('→', "rightarrow"),
    ('←', "leftarrow"),
    ('↔', "leftrightarrow"),
    ('∪', "cup"),
    ('∩', "cap"),
];
const MATRICES: &[(char, &str)] = &[
    ('■', "matrix"),
    ('⒨', "pmatrix"),
    ('ⓢ', "bmatrix"),
    ('Ⓢ', "Bmatrix"),
    ('⒱', "vmatrix"),
    ('⒩', "Vmatrix"),
];
const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "sec", "csc", "cot", "sinh", "cosh", "tanh", "arcsin", "arccos", "arctan",
    "log", "ln", "exp", "lim", "min", "max", "det", "dim", "gcd",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Operand(char),
    Other(char),
    Open(char),
    Close(char),
    Root(char),
    Matrix(char),
    Space,
    Slash,
    Sub,
    Sup,
    Cell,
    Row,
}
#[derive(Debug, Clone, Copy)]
struct Token {
    kind: Kind,
    offset: usize,
}
fn lex(source: &str) -> Result<Vec<Token>, UnicodeMathError> {
    if source.len() > MAX_BYTES {
        return Err(error(
            UnicodeMathErrorKind::LimitExceeded,
            0,
            "input exceeds 64 KiB",
        ));
    }
    let mut tokens = Vec::new();
    for (offset, ch) in source.char_indices() {
        if tokens.len() >= MAX_TOKENS {
            return Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                offset,
                "input exceeds 4096 tokens",
            ));
        }
        let kind = match ch {
            ' ' => Kind::Space,
            '/' => Kind::Slash,
            '_' => Kind::Sub,
            '^' => Kind::Sup,
            '&' => Kind::Cell,
            '@' => Kind::Row,
            '(' | '[' | '{' | '〖' => Kind::Open(ch),
            ')' | ']' | '}' | '〗' => Kind::Close(ch),
            '√' | '∛' | '∜' => Kind::Root(ch),
            ch if MATRICES.iter().any(|(glyph, _)| *glyph == ch) => Kind::Matrix(ch),
            ch if ch.is_ascii_alphanumeric()
                || GREEK.iter().any(|(glyph, _)| *glyph == ch)
                || ch == '∞' =>
            {
                Kind::Operand(ch)
            }
            '+' | '-' | '=' | '<' | '>' | ',' | '.' => Kind::Other(ch),
            ch if SYMBOLS.iter().any(|(glyph, _)| *glyph == ch) => Kind::Other(ch),
            _ => {
                return Err(error(
                    UnicodeMathErrorKind::UnsupportedSyntax,
                    offset,
                    "glyph/control word is outside the UnicodeMath pilot",
                ))
            }
        };
        tokens.push(Token { kind, offset });
    }
    Ok(tokens)
}
fn sequence(mut nodes: Vec<LatexNode>) -> LatexNode {
    if nodes.len() == 1 {
        nodes.remove(0)
    } else {
        LatexNode::Sequence(nodes)
    }
}
fn glyph_node(ch: char) -> LatexNode {
    if let Some((_, name)) = GREEK.iter().find(|(glyph, _)| *glyph == ch) {
        LatexNode::Greek((*name).into())
    } else if let Some((_, name)) = SYMBOLS.iter().find(|(glyph, _)| *glyph == ch) {
        LatexNode::Symbol((*name).into())
    } else {
        LatexNode::Text(ch.to_string())
    }
}
fn consume_round(node: LatexNode) -> LatexNode {
    match node {
        LatexNode::Delimited {
            left,
            content,
            right,
        } if left == "(" && right == ")" => sequence(content),
        node => node,
    }
}
fn pair(open: char) -> (char, &'static str, &'static str) {
    match open {
        '(' => (')', "(", ")"),
        '[' => (']', "[", "]"),
        '{' => ('}', "\\{", "\\}"),
        _ => ('〗', "", ""),
    }
}
struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    end: usize,
}
impl Parser {
    fn kind(&self) -> Option<Kind> {
        self.tokens.get(self.pos).map(|token| token.kind)
    }
    fn offset(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map_or(self.end, |token| token.offset)
    }
    fn fail(&self, message: &str) -> UnicodeMathError {
        error(
            UnicodeMathErrorKind::UnexpectedToken,
            self.offset(),
            message,
        )
    }
    fn depth(&self, depth: usize) -> Result<(), UnicodeMathError> {
        if depth >= MAX_DEPTH {
            Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                self.offset(),
                "nesting exceeds pilot budget",
            ))
        } else {
            Ok(())
        }
    }
    fn spaces(&mut self) {
        while self.kind() == Some(Kind::Space) {
            self.pos += 1;
        }
    }
    fn starts_factor(&self) -> bool {
        matches!(
            self.kind(),
            Some(Kind::Operand(_) | Kind::Open(_) | Kind::Root(_) | Kind::Matrix(_))
        )
    }
    fn expression(&mut self, depth: usize) -> Result<Vec<LatexNode>, UnicodeMathError> {
        self.depth(depth)?;
        let mut nodes = Vec::new();
        loop {
            self.spaces();
            if matches!(
                self.kind(),
                None | Some(Kind::Close(_) | Kind::Cell | Kind::Row)
            ) {
                break;
            }
            if let Some(Kind::Other(ch)) = self.kind() {
                self.pos += 1;
                nodes.push(glyph_node(ch));
                continue;
            }
            let mut value = self.operand(depth)?;
            // Left-associative fractions. Spaces terminate operand runs rather
            // than disappearing in the lexer.
            let mut chain = 0;
            loop {
                let saved = self.pos;
                self.spaces();
                if self.kind() != Some(Kind::Slash) {
                    self.pos = saved;
                    break;
                }
                chain += 1;
                if chain >= MAX_DEPTH {
                    return Err(error(
                        UnicodeMathErrorKind::LimitExceeded,
                        self.offset(),
                        "fraction chain exceeds pilot budget",
                    ));
                }
                self.pos += 1;
                self.spaces();
                let den = self.operand(depth + 1)?;
                value = LatexNode::Fraction {
                    num: Box::new(consume_round(value)),
                    den: Box::new(consume_round(den)),
                };
            }
            nodes.push(value);
        }
        Ok(nodes)
    }
    fn operand(&mut self, depth: usize) -> Result<LatexNode, UnicodeMathError> {
        self.depth(depth)?;
        if !self.starts_factor() {
            return Err(self.fail("expected a nonempty operand"));
        }
        let mut nodes = Vec::new();
        while self.starts_factor() {
            nodes.push(self.factor(depth + 1)?);
        }
        Ok(sequence(nodes))
    }
    fn run(&mut self) -> Result<Vec<LatexNode>, UnicodeMathError> {
        let start = self.pos;
        let mut nodes = Vec::new();
        let mut word = String::new();
        while let Some(Kind::Operand(ch)) = self.kind() {
            if ch.is_ascii_alphabetic() {
                word.push(ch);
            } else {
                if FUNCTIONS.contains(&word.as_str()) {
                    return Err(error(
                        UnicodeMathErrorKind::UnsupportedSyntax,
                        self.tokens[start].offset,
                        "named functions require a separate application grammar",
                    ));
                }
                word.clear();
            }
            if ch.is_ascii_digit() {
                let mut number = String::new();
                while let Some(Kind::Operand(digit)) = self.kind() {
                    if !digit.is_ascii_digit() {
                        break;
                    }
                    number.push(digit);
                    self.pos += 1;
                }
                if matches!(self.kind(), Some(Kind::Other('.' | ',')))
                    && matches!(self.tokens.get(self.pos + 1).map(|t| t.kind), Some(Kind::Operand(ch)) if ch.is_ascii_digit())
                {
                    let Some(Kind::Other(decimal)) = self.kind() else {
                        unreachable!()
                    };
                    number.push(decimal);
                    self.pos += 1;
                    while let Some(Kind::Operand(digit)) = self.kind() {
                        if !digit.is_ascii_digit() {
                            break;
                        }
                        number.push(digit);
                        self.pos += 1;
                    }
                    if matches!(self.kind(), Some(Kind::Other('.' | ',')))
                        && matches!(self.tokens.get(self.pos + 1).map(|t| t.kind), Some(Kind::Operand(ch)) if ch.is_ascii_digit())
                    {
                        return Err(self.fail("multiple decimal separators in one number"));
                    }
                }
                nodes.push(LatexNode::Text(number));
            } else {
                nodes.push(glyph_node(ch));
                self.pos += 1;
            }
        }
        if FUNCTIONS.contains(&word.as_str()) {
            return Err(error(
                UnicodeMathErrorKind::UnsupportedSyntax,
                self.tokens[start].offset,
                "named functions require a separate application grammar",
            ));
        }
        Ok(nodes)
    }
    fn atom(&mut self, depth: usize) -> Result<LatexNode, UnicodeMathError> {
        self.depth(depth)?;
        match self.kind() {
            Some(Kind::Operand(_)) => Ok(sequence(self.run()?)),
            Some(Kind::Open(open)) => {
                self.pos += 1;
                let content = self.expression(depth + 1)?;
                let (close, left, right) = pair(open);
                if self.kind() != Some(Kind::Close(close)) {
                    return Err(self.fail("unmatched delimiter"));
                }
                if content.is_empty() {
                    return Err(self.fail("empty grouping is outside the pilot"));
                }
                self.pos += 1;
                if open == '〖' {
                    Ok(LatexNode::Group(content))
                } else {
                    Ok(LatexNode::Delimited {
                        left: left.into(),
                        content,
                        right: right.into(),
                    })
                }
            }
            Some(Kind::Root(glyph)) => {
                self.pos += 1;
                self.spaces();
                let (index, content) = if self.kind() == Some(Kind::Open('(')) {
                    self.pos += 1;
                    let first = self.expression(depth + 1)?;
                    if first.is_empty() {
                        return Err(self.fail("empty root operand"));
                    }
                    let (index, content) = if self.kind() == Some(Kind::Cell) && glyph == '√' {
                        self.pos += 1;
                        let second = self.expression(depth + 1)?;
                        if second.is_empty() {
                            return Err(self.fail("empty radicand"));
                        }
                        (Some(Box::new(sequence(first))), sequence(second))
                    } else {
                        (None, sequence(first))
                    };
                    if self.kind() != Some(Kind::Close(')')) {
                        return Err(self.fail("expected root closing parenthesis"));
                    }
                    self.pos += 1;
                    (index, content)
                } else {
                    (None, self.operand(depth + 1)?)
                };
                let index = match glyph {
                    '∛' => Some(Box::new(LatexNode::Text("3".into()))),
                    '∜' => Some(Box::new(LatexNode::Text("4".into()))),
                    _ => index,
                };
                Ok(LatexNode::SquareRoot {
                    index,
                    content: Box::new(content),
                })
            }
            Some(Kind::Matrix(glyph)) => self.matrix(glyph, depth + 1),
            _ => Err(self.fail("expected an operand or grouping")),
        }
    }
    fn factor(&mut self, depth: usize) -> Result<LatexNode, UnicodeMathError> {
        self.depth(depth)?;
        let is_run = matches!(self.kind(), Some(Kind::Operand(_)));
        let mut base = self.atom(depth)?;
        if !matches!(self.kind(), Some(Kind::Sub | Kind::Sup)) {
            return Ok(base);
        }
        let mut prefix = Vec::new();
        // Ordinary letter runs attach scripts to the final character. Numeric
        // runs remain atomic in this explicitly limited pilot.
        if is_run {
            if let LatexNode::Sequence(mut nodes) = base {
                base = nodes.pop().ok_or_else(|| self.fail("empty script base"))?;
                prefix = nodes;
            }
        }
        let mut sub = None;
        let mut sup = None;
        while let Some(mark @ (Kind::Sub | Kind::Sup)) = self.kind() {
            self.pos += 1;
            let value = self.script_operand(mark, depth + 1)?;
            let slot = if mark == Kind::Sub {
                &mut sub
            } else {
                &mut sup
            };
            if slot.is_some() {
                return Err(self.fail("mixed repeated script chain requires explicit grouping"));
            }
            *slot = Some(value);
        }
        // Both mixed orders bind to one base, with one canonical AST order.
        if let Some(sub) = sub {
            base = LatexNode::Subscript {
                base: Box::new(base),
                sub: Box::new(sub),
            };
        }
        if let Some(exp) = sup {
            base = LatexNode::Superscript {
                base: Box::new(base),
                exp: Box::new(exp),
            };
        }
        prefix.push(base);
        Ok(sequence(prefix))
    }
    fn script_operand(&mut self, mark: Kind, depth: usize) -> Result<LatexNode, UnicodeMathError> {
        self.depth(depth)?;
        if self.kind() == Some(Kind::Space) {
            return Err(self.fail("space cannot start a script operand"));
        }
        let sign = if matches!(self.kind(), Some(Kind::Other('+' | '-'))) {
            let Some(Kind::Other(ch)) = self.kind() else {
                unreachable!()
            };
            self.pos += 1;
            Some(glyph_node(ch))
        } else {
            None
        };
        let is_run = matches!(self.kind(), Some(Kind::Operand(_)));
        let mut value = consume_round(self.atom(depth + 1)?);
        if self.kind() == Some(mark) {
            self.pos += 1;
            let rhs = self.script_operand(mark, depth + 1)?;
            let mut prefix = Vec::new();
            if is_run {
                if let LatexNode::Sequence(mut nodes) = value {
                    value = nodes
                        .pop()
                        .ok_or_else(|| self.fail("empty script operand"))?;
                    prefix = nodes;
                }
            }
            value = if mark == Kind::Sub {
                LatexNode::Subscript {
                    base: Box::new(value),
                    sub: Box::new(rhs),
                }
            } else {
                LatexNode::Superscript {
                    base: Box::new(value),
                    exp: Box::new(rhs),
                }
            };
            prefix.push(value);
            value = sequence(prefix);
        }
        if self.starts_factor() {
            return Err(error(
                UnicodeMathErrorKind::UnsupportedSyntax,
                self.offset(),
                "compound ungrouped script operands require explicit grouping",
            ));
        }
        Ok(if let Some(sign) = sign {
            sequence(vec![sign, value])
        } else {
            value
        })
    }
    fn matrix(&mut self, glyph: char, depth: usize) -> Result<LatexNode, UnicodeMathError> {
        self.depth(depth)?;
        self.pos += 1;
        self.spaces();
        if self.kind() != Some(Kind::Open('(')) {
            return Err(self.fail("matrix requires a parenthesized row list"));
        }
        self.pos += 1;
        let mut rows = Vec::new();
        let mut row = Vec::new();
        loop {
            row.push(sequence(self.expression(depth + 1)?));
            match self.kind() {
                Some(Kind::Cell) => self.pos += 1,
                Some(Kind::Row) => {
                    self.pos += 1;
                    rows.push(row);
                    row = Vec::new();
                }
                Some(Kind::Close(')')) => {
                    self.pos += 1;
                    rows.push(row);
                    break;
                }
                _ => return Err(self.fail("expected matrix cell, row or closing separator")),
            }
        }
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        if rows.len().saturating_mul(width) > MAX_TOKENS {
            return Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                self.offset(),
                "padded matrix exceeds 4096 cells",
            ));
        }
        // UnicodeMath pads short rows, unlike the AsciiMath pilot.
        for row in &mut rows {
            row.resize_with(width, || LatexNode::Sequence(vec![]));
        }
        let env = MATRICES
            .iter()
            .find(|(ch, _)| *ch == glyph)
            .expect("lexed matrix")
            .1;
        Ok(LatexNode::Matrix {
            env: env.into(),
            rows,
        })
    }
}
/// Parse a finite UnicodeMath subset, not Word autocorrect or rich-text buildup.
pub fn parse_unicodemath(source: &str) -> Result<LatexNode, UnicodeMathError> {
    let mut parser = Parser {
        tokens: lex(source)?,
        pos: 0,
        end: source.len(),
    };
    let nodes = parser.expression(0)?;
    if parser.pos != parser.tokens.len() {
        return Err(parser.fail("unexpected trailing delimiter or separator"));
    }
    if nodes.is_empty() {
        return Err(parser.fail("empty formula"));
    }
    let node = sequence(nodes);
    check_ast_budget(&node)?;
    Ok(node)
}
fn check_ast_budget(node: &LatexNode) -> Result<(), UnicodeMathError> {
    let mut stack = vec![(node, 0)];
    let mut count = 0;
    while let Some((node, depth)) = stack.pop() {
        count += 1;
        if count > MAX_NODES || depth >= MAX_DEPTH {
            return Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                0,
                "AST exceeds node/depth budget",
            ));
        }
        let mut push = |node| {
            if count + stack.len() >= MAX_NODES {
                return Err(error(
                    UnicodeMathErrorKind::LimitExceeded,
                    0,
                    "AST exceeds node budget",
                ));
            }
            stack.push((node, depth + 1));
            Ok(())
        };
        match node {
            LatexNode::Sequence(nodes)
            | LatexNode::Group(nodes)
            | LatexNode::Delimited { content: nodes, .. } => {
                if nodes.len() > MAX_NODES - count {
                    return Err(error(
                        UnicodeMathErrorKind::LimitExceeded,
                        0,
                        "AST exceeds node budget",
                    ));
                }
                for node in nodes {
                    push(node)?;
                }
            }
            LatexNode::Fraction { num, den } => {
                push(num)?;
                push(den)?;
            }
            LatexNode::Subscript { base, sub } => {
                push(base)?;
                push(sub)?;
            }
            LatexNode::Superscript { base, exp } => {
                push(base)?;
                push(exp)?;
            }
            LatexNode::SquareRoot { index, content } => {
                if let Some(index) = index {
                    push(index)?;
                }
                push(content)?;
            }
            LatexNode::Matrix { rows, .. } => {
                if rows.len() > MAX_TOKENS || rows.iter().map(Vec::len).sum::<usize>() > MAX_TOKENS
                {
                    return Err(error(
                        UnicodeMathErrorKind::LimitExceeded,
                        0,
                        "matrix exceeds cell budget",
                    ));
                }
                for row in rows {
                    for node in row {
                        push(node)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}
/// Serialize supported AST nodes and reparse to guard ownership and delimiters.
pub fn write_unicodemath(node: &LatexNode) -> Result<String, UnicodeMathError> {
    check_ast_budget(node)?;
    let output = write_node(node, 0)?;
    let rebuilt = parse_unicodemath(&output)?;
    if crate::pilot_ast::shape(node) != crate::pilot_ast::shape(&rebuilt) {
        return Err(error(
            UnicodeMathErrorKind::UnsupportedSyntax,
            0,
            "AST ownership/delimiters have no supported representation",
        ));
    }
    Ok(output)
}
fn join(
    parts: impl Iterator<Item = Result<String, UnicodeMathError>>,
    separator: &str,
) -> Result<String, UnicodeMathError> {
    let mut output = String::new();
    for (index, part) in parts.enumerate() {
        let part = part?;
        let extra = if index == 0 { 0 } else { separator.len() };
        if output
            .len()
            .saturating_add(extra)
            .saturating_add(part.len())
            > MAX_BYTES
        {
            return Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                0,
                "output exceeds 64 KiB",
            ));
        }
        if index != 0 {
            output.push_str(separator);
        }
        output.push_str(&part);
    }
    Ok(output)
}
fn write_node(node: &LatexNode, depth: usize) -> Result<String, UnicodeMathError> {
    if depth >= MAX_DEPTH {
        return Err(error(
            UnicodeMathErrorKind::LimitExceeded,
            0,
            "AST exceeds depth budget",
        ));
    }
    let unsupported = || {
        error(
            UnicodeMathErrorKind::UnsupportedSyntax,
            0,
            "AST node is outside the UnicodeMath pilot",
        )
    };
    let child = |node: &LatexNode| write_node(node, depth + 1);
    let output = match node {
        LatexNode::Text(text) => {
            if text.len() > MAX_BYTES {
                return Err(error(
                    UnicodeMathErrorKind::LimitExceeded,
                    0,
                    "AST text exceeds 64 KiB",
                ));
            }
            if text.is_empty()
                || !text
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || "+-=<>.,".contains(ch))
            {
                return Err(unsupported());
            }
            let parsed = parse_unicodemath(text)?;
            if matches!(&parsed, LatexNode::Text(value) if value == text) {
                text.clone()
            } else {
                // Canonicalize external multi-character Text nodes into the
                // same token boundaries used by this grammar's parser.
                child(&parsed)?
            }
        }
        LatexNode::Sequence(nodes) => join(nodes.iter().map(child), " ")?,
        LatexNode::Group(nodes) if !nodes.is_empty() => join(nodes.iter().map(child), " ")?,
        LatexNode::Fraction { num, den } => format!("({})/({})", child(num)?, child(den)?),
        LatexNode::SquareRoot { index, content } => {
            if let Some(index) = index {
                format!("√({}&{})", child(index)?, child(content)?)
            } else {
                format!("√({})", child(content)?)
            }
        }
        LatexNode::Subscript { base, sub } => format!("〖{}〗_({})", child(base)?, child(sub)?),
        LatexNode::Superscript { base, exp } => format!("〖{}〗^({})", child(base)?, child(exp)?),
        LatexNode::Greek(name) => GREEK
            .iter()
            .find(|(_, mapped)| *mapped == name)
            .map(|(glyph, _)| glyph.to_string())
            .ok_or_else(unsupported)?,
        LatexNode::Symbol(name) | LatexNode::Relation(name) => SYMBOLS
            .iter()
            .find(|(_, mapped)| *mapped == name.trim_start_matches('\\'))
            .map(|(glyph, _)| glyph.to_string())
            .ok_or_else(unsupported)?,
        LatexNode::Delimited {
            left,
            content,
            right,
        } if !content.is_empty() => {
            let (left, right) = match (left.as_str(), right.as_str()) {
                ("(", ")") => ("(", ")"),
                ("[", "]") => ("[", "]"),
                ("\\{", "\\}") => ("{", "}"),
                _ => return Err(unsupported()),
            };
            format!("{left}{}{right}", join(content.iter().map(child), " ")?)
        }
        LatexNode::Matrix { env, rows } if !rows.is_empty() && !rows[0].is_empty() => {
            if rows.iter().any(|row| row.len() != rows[0].len()) {
                return Err(unsupported());
            }
            let glyph = MATRICES
                .iter()
                .find(|(_, mapped)| *mapped == env)
                .map(|(glyph, _)| *glyph)
                .ok_or_else(unsupported)?;
            format!(
                "{glyph}({})",
                join(rows.iter().map(|row| join(row.iter().map(child), "&")), "@")?
            )
        }
        _ => return Err(unsupported()),
    };
    if output.len() > MAX_BYTES {
        return Err(error(
            UnicodeMathErrorKind::LimitExceeded,
            0,
            "output exceeds 64 KiB",
        ));
    }
    Ok(output)
}
