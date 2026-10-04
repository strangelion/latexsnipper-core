//! Bounded experimental AsciiMath grammar and AST mapping.
//!
//! This module is not a registered conversion route. See
//! `docs/formats/asciimath-pilot.md` for the accepted grammar and limitations.

use crate::latex_ast::LatexNode;
use std::fmt;

const MAX_BYTES: usize = 64 * 1024;
const MAX_TOKENS: usize = 4096;
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsciiMathErrorKind {
    UnexpectedToken,
    UnsupportedSyntax,
    RaggedMatrix,
    LimitExceeded,
}

/// UTF-8 byte offset in parser input. Writing errors use 0 or an offset in
/// emitted text; they are not spans in the original LaTeX source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsciiMathError {
    pub kind: AsciiMathErrorKind,
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for AsciiMathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "AsciiMath {:?} at byte {}: {}",
            self.kind, self.offset, self.message
        )
    }
}

impl std::error::Error for AsciiMathError {}

fn error(kind: AsciiMathErrorKind, offset: usize, message: impl Into<String>) -> AsciiMathError {
    AsciiMathError {
        kind,
        offset,
        message: message.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bracket {
    Round,
    Square,
    Curly,
    Invisible,
}

impl Bracket {
    fn pair(self) -> (&'static str, &'static str) {
        match self {
            Self::Round => ("(", ")"),
            Self::Square => ("[", "]"),
            Self::Curly => ("\\{", "\\}"),
            Self::Invisible => ("", ""),
        }
    }
}

#[derive(Debug, Clone)]
enum Kind {
    Atom(LatexNode),
    Unary(&'static str),
    Binary(&'static str),
    Open(Bracket),
    Close(Bracket),
    Comma,
    Slash,
    Sub,
    Sup,
}

#[derive(Debug, Clone)]
struct Token {
    kind: Kind,
    offset: usize,
}

const GREEK: &[&str] = &[
    "alpha",
    "beta",
    "gamma",
    "Gamma",
    "delta",
    "Delta",
    "epsilon",
    "varepsilon",
    "zeta",
    "eta",
    "theta",
    "Theta",
    "vartheta",
    "iota",
    "kappa",
    "lambda",
    "Lambda",
    "mu",
    "nu",
    "xi",
    "Xi",
    "pi",
    "Pi",
    "rho",
    "sigma",
    "Sigma",
    "tau",
    "upsilon",
    "phi",
    "Phi",
    "varphi",
    "chi",
    "psi",
    "Psi",
    "omega",
    "Omega",
];
const OPERATORS: &[&str] = &["sum", "prod", "int", "oint", "lim", "min", "max"];
const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "sec", "csc", "cot", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh",
    "exp", "log", "ln", "det", "dim", "gcd",
];
const CONSTRUCTS: &[&str] = &["sqrt", "root", "frac", "abs", "norm", "floor", "ceil"];
const SETS: &[(&str, &str)] = &[
    ("CC", "C"),
    ("NN", "N"),
    ("QQ", "Q"),
    ("RR", "R"),
    ("ZZ", "Z"),
];
// Recognized but intentionally not implemented; do not silently read these as variables.
const UNSUPPORTED: &[&str] = &[
    "text", "color", "cancel", "stackrel", "overset", "underset", "ubrace", "obrace", "hat", "bar",
    "vec", "tilde", "ddot", "dot", "overarc", "bb", "bbb", "cc", "tt", "fr", "sf", "sech", "csch",
    "coth", "mod", "lcm", "lub", "glb", "and", "or", "if", "quad", "vdots", "ddots", "cdots",
    "aleph", "diamond", "square", "frown", "mlt", "mgt", "prop", "setminus", "ul", "nnn", "uuu",
    "vv", "vvv", "^^", "^^^", "o+", "ox", "o.", "-<", "-<=", ">-", ">-=", "rArr", "lArr", "hArr",
    "<<", ">>", "O/", "emptyset",
];
const SYMBOLS: &[(&str, &str)] = &[
    ("oo", "infty"),
    ("del", "partial"),
    ("grad", "nabla"),
    ("AA", "forall"),
    ("EE", "exists"),
    ("not", "neg"),
    ("xx", "times"),
    ("-:", "div"),
    ("***", "star"),
    ("**", "ast"),
    ("*", "cdot"),
    ("+-", "pm"),
    ("//", "slash"),
    ("!in", "notin"),
    ("in", "in"),
    ("sube", "subseteq"),
    ("supe", "supseteq"),
    ("sub", "subset"),
    ("sup", "supset"),
    ("!=", "neq"),
    ("<=", "leq"),
    (">=", "geq"),
    ("~~", "approx"),
    ("~=", "cong"),
    ("-=", "equiv"),
    ("<=>", "iff"),
    ("=>", "implies"),
    ("|->", "mapsto"),
    ("->", "to"),
    ("rarr", "rightarrow"),
    ("larr", "leftarrow"),
    ("harr", "leftrightarrow"),
    ("uarr", "uparrow"),
    ("darr", "downarrow"),
    ("nn", "cap"),
    ("uu", "cup"),
];

fn lex(input: &str) -> Result<Vec<Token>, AsciiMathError> {
    if input.len() > MAX_BYTES {
        return Err(error(
            AsciiMathErrorKind::LimitExceeded,
            0,
            "input exceeds 64 KiB",
        ));
    }
    let mut tokens = Vec::new();
    let mut offset = 0;
    while offset < input.len() {
        let rest = &input[offset..];
        let ch = rest.chars().next().expect("nonempty UTF-8 suffix");
        if ch.is_ascii_whitespace() {
            offset += 1;
            continue;
        }
        if tokens.len() >= MAX_TOKENS {
            return Err(error(
                AsciiMathErrorKind::LimitExceeded,
                offset,
                "input exceeds 4096 tokens",
            ));
        }
        let brackets = [
            ("{:", Kind::Open(Bracket::Invisible)),
            (":}", Kind::Close(Bracket::Invisible)),
        ];
        if let Some((prefix, kind)) = brackets
            .into_iter()
            .find(|(prefix, _)| rest.starts_with(prefix))
        {
            tokens.push(Token { kind, offset });
            offset += prefix.len();
            continue;
        }
        let keyword = GREEK
            .iter()
            .chain(OPERATORS)
            .chain(FUNCTIONS)
            .chain(CONSTRUCTS)
            .chain(UNSUPPORTED)
            .copied()
            .chain(SYMBOLS.iter().flat_map(|(name, mapped)| [*name, *mapped]))
            .chain(SETS.iter().map(|(name, _)| *name))
            .filter(|name| rest.starts_with(name))
            .max_by_key(|name| name.len());
        if let Some(name) = keyword {
            let kind = if UNSUPPORTED.contains(&name) {
                return Err(error(
                    AsciiMathErrorKind::UnsupportedSyntax,
                    offset,
                    format!("unsupported construct {name}"),
                ));
            } else if let Some((_, letter)) = SETS.iter().find(|(key, _)| *key == name) {
                Kind::Atom(LatexNode::FontModifier {
                    font: "mathbb".into(),
                    content: Box::new(LatexNode::text(*letter)),
                })
            } else if GREEK.contains(&name) {
                Kind::Atom(LatexNode::Greek(name.into()))
            } else if OPERATORS.contains(&name) {
                Kind::Atom(LatexNode::Operator(name.into()))
            } else if FUNCTIONS.contains(&name) {
                Kind::Atom(LatexNode::command(name, vec![]))
            } else if matches!(name, "root" | "frac") {
                Kind::Binary(name)
            } else if CONSTRUCTS.contains(&name) {
                Kind::Unary(name)
            } else {
                let mapped = SYMBOLS
                    .iter()
                    .find(|(key, mapped)| *key == name || *mapped == name)
                    .unwrap()
                    .1;
                Kind::Atom(LatexNode::Symbol(format!("\\{mapped}")))
            };
            tokens.push(Token { kind, offset });
            offset += name.len();
            continue;
        }
        let kind = match ch {
            '(' => Kind::Open(Bracket::Round),
            ')' => Kind::Close(Bracket::Round),
            '[' => Kind::Open(Bracket::Square),
            ']' => Kind::Close(Bracket::Square),
            '{' => Kind::Open(Bracket::Curly),
            '}' => Kind::Close(Bracket::Curly),
            ',' => Kind::Comma,
            '/' => Kind::Slash,
            '_' => Kind::Sub,
            '^' => Kind::Sup,
            '+' | '-' | '=' | '<' | '>' | '!' | '\'' => Kind::Atom(LatexNode::text(ch.to_string())),
            c if c.is_ascii_alphabetic() => Kind::Atom(LatexNode::text(c.to_string())),
            c if c.is_ascii_digit()
                || c == '.' && rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit) =>
            {
                let mut length = rest.bytes().take_while(u8::is_ascii_digit).count();
                if rest.as_bytes().get(length) == Some(&b'.') {
                    length += 1;
                    length += rest[length..]
                        .bytes()
                        .take_while(u8::is_ascii_digit)
                        .count();
                }
                if rest.as_bytes().get(length) == Some(&b'.') {
                    return Err(error(
                        AsciiMathErrorKind::UnexpectedToken,
                        offset + length,
                        "number contains multiple decimal points",
                    ));
                }
                tokens.push(Token {
                    kind: Kind::Atom(LatexNode::text(&rest[..length])),
                    offset,
                });
                offset += length;
                continue;
            }
            _ => {
                return Err(error(
                    AsciiMathErrorKind::UnsupportedSyntax,
                    offset,
                    format!("unsupported character {ch:?}"),
                ))
            }
        };
        tokens.push(Token { kind, offset });
        offset += ch.len_utf8();
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

fn operand(node: LatexNode) -> LatexNode {
    match node {
        LatexNode::Delimited { content, .. } => sequence(content),
        LatexNode::Group(content) => sequence(content),
        other => other,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    end: usize,
}

impl Parser {
    fn offset(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map_or(self.end, |token| token.offset)
    }
    fn unexpected(&self, message: &str) -> AsciiMathError {
        error(AsciiMathErrorKind::UnexpectedToken, self.offset(), message)
    }
    fn simple(&mut self, depth: usize) -> Result<LatexNode, AsciiMathError> {
        if depth >= MAX_DEPTH {
            return Err(error(
                AsciiMathErrorKind::LimitExceeded,
                self.offset(),
                "expression exceeds 64 recursive levels",
            ));
        }
        let token = self
            .tokens
            .get(self.pos)
            .cloned()
            .ok_or_else(|| self.unexpected("missing operand"))?;
        self.pos += 1;
        match token.kind {
            Kind::Atom(node) => Ok(node),
            Kind::Open(bracket) => self.group(bracket, depth + 1, token.offset),
            Kind::Unary(name) => {
                let content = Box::new(operand(self.simple(depth + 1)?));
                if name == "sqrt" {
                    Ok(LatexNode::SquareRoot {
                        index: None,
                        content,
                    })
                } else {
                    let (left, right) = match name {
                        "abs" => ("|", "|"),
                        "norm" => ("\\|", "\\|"),
                        "floor" => ("\\lfloor", "\\rfloor"),
                        "ceil" => ("\\lceil", "\\rceil"),
                        _ => unreachable!(),
                    };
                    Ok(LatexNode::Delimited {
                        left: left.into(),
                        content: vec![*content],
                        right: right.into(),
                    })
                }
            }
            Kind::Binary(name) => {
                let first = Box::new(operand(self.simple(depth + 1)?));
                let second = Box::new(operand(self.simple(depth + 1)?));
                if name == "frac" {
                    Ok(LatexNode::Fraction {
                        num: first,
                        den: second,
                    })
                } else {
                    Ok(LatexNode::SquareRoot {
                        index: Some(first),
                        content: second,
                    })
                }
            }
            _ => Err(error(
                AsciiMathErrorKind::UnexpectedToken,
                token.offset,
                "expected a simple expression",
            )),
        }
    }
    fn intermediate(&mut self, depth: usize) -> Result<LatexNode, AsciiMathError> {
        let mut node = self.simple(depth)?;
        if matches!(
            self.tokens.get(self.pos).map(|token| &token.kind),
            Some(Kind::Sub)
        ) {
            self.pos += 1;
            node = LatexNode::Subscript {
                base: Box::new(node),
                sub: Box::new(operand(self.simple(depth + 1)?)),
            };
        }
        if matches!(
            self.tokens.get(self.pos).map(|token| &token.kind),
            Some(Kind::Sup)
        ) {
            self.pos += 1;
            node = LatexNode::Superscript {
                base: Box::new(node),
                exp: Box::new(operand(self.simple(depth + 1)?)),
            };
        }
        if matches!(
            self.tokens.get(self.pos).map(|token| &token.kind),
            Some(Kind::Sub | Kind::Sup)
        ) {
            return Err(self.unexpected(
                "repeated scripts or subscript after superscript; use explicit grouping",
            ));
        }
        Ok(node)
    }
    fn expression(&mut self, depth: usize) -> Result<Vec<LatexNode>, AsciiMathError> {
        let mut nodes = Vec::new();
        while self.pos < self.tokens.len() {
            if matches!(self.tokens[self.pos].kind, Kind::Close(_) | Kind::Comma) {
                break;
            }
            let mut node = self.intermediate(depth)?;
            if matches!(
                self.tokens.get(self.pos).map(|token| &token.kind),
                Some(Kind::Slash)
            ) {
                self.pos += 1;
                node = LatexNode::Fraction {
                    num: Box::new(operand(node)),
                    den: Box::new(operand(self.intermediate(depth + 1)?)),
                };
                if matches!(
                    self.tokens.get(self.pos).map(|token| &token.kind),
                    Some(Kind::Slash)
                ) {
                    return Err(self
                        .unexpected("chained division is outside the pilot; group each fraction"));
                }
            }
            nodes.push(node);
        }
        Ok(nodes)
    }
    fn group(
        &mut self,
        bracket: Bracket,
        depth: usize,
        offset: usize,
    ) -> Result<LatexNode, AsciiMathError> {
        let mut items = Vec::new();
        loop {
            let nodes = self.expression(depth)?;
            if nodes.is_empty() {
                return Err(self.unexpected("empty group or matrix cell"));
            }
            items.push(sequence(nodes));
            match self.tokens.get(self.pos).map(|token| &token.kind) {
                Some(Kind::Comma) => self.pos += 1,
                Some(Kind::Close(close)) if *close == bracket => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.unexpected("missing or mismatched closing bracket")),
            }
        }
        let (left, right) = bracket.pair();
        // Only uniform round/round or square/square row notation is a matrix.
        if matches!(bracket, Bracket::Round | Bracket::Square) && items.iter().all(|item|
            matches!(item, LatexNode::Delimited { left: row_left, right: row_right, .. } if row_left == left && row_right == right)) {
            let mut rows = Vec::new();
            for item in items {
                let LatexNode::Delimited { content, .. } = item else { unreachable!() };
                let mut cells = vec![Vec::new()];
                for node in content {
                    if matches!(&node, LatexNode::Text(text) if text == ",") { cells.push(Vec::new()); }
                    else { cells.last_mut().unwrap().push(node); }
                }
                rows.push(cells.into_iter().map(sequence).collect::<Vec<_>>());
            }
            if rows.iter().any(|row| row.len() != rows[0].len()) {
                return Err(error(AsciiMathErrorKind::RaggedMatrix, offset, "matrix rows must have equal width"));
            }
            return Ok(LatexNode::Matrix { env: if bracket == Bracket::Round { "pmatrix" } else { "bmatrix" }.into(), rows });
        }
        let mut content = Vec::new();
        for (index, item) in items.into_iter().enumerate() {
            if index > 0 {
                content.push(LatexNode::text(","));
            }
            match item {
                LatexNode::Sequence(nodes) => content.extend(nodes),
                other => content.push(other),
            }
        }
        if bracket == Bracket::Invisible {
            Ok(LatexNode::Group(content))
        } else {
            Ok(LatexNode::Delimited {
                left: left.into(),
                content,
                right: right.into(),
            })
        }
    }
}

/// Parse the documented pilot subset without lossy recovery or a registry claim.
pub fn parse_asciimath(input: &str) -> Result<LatexNode, AsciiMathError> {
    let mut parser = Parser {
        tokens: lex(input)?,
        pos: 0,
        end: input.len(),
    };
    let nodes = parser.expression(0)?;
    if nodes.is_empty() {
        return Err(parser.unexpected("empty expression"));
    }
    if parser.pos != parser.tokens.len() {
        return Err(parser.unexpected("unexpected trailing delimiter"));
    }
    Ok(sequence(nodes))
}

/// Serialize only AST nodes with a supported AsciiMath representation.
/// Unsupported nodes fail instead of falling back to text or dropping content.
pub fn write_asciimath(node: &LatexNode) -> Result<String, AsciiMathError> {
    let mut tokens = MAX_TOKENS;
    let result = write_node(node, 0, &mut tokens)?;
    if result.len() > MAX_BYTES {
        return Err(error(
            AsciiMathErrorKind::LimitExceeded,
            0,
            "serialized output exceeds 64 KiB",
        ));
    }
    // Recheck grammar and resource limits, including ambiguity across node joins.
    let rebuilt = parse_asciimath(&result)?;
    if shape(node) != shape(&rebuilt) {
        return Err(error(
            AsciiMathErrorKind::UnsupportedSyntax,
            0,
            "AST binding or delimiter structure has no supported pilot representation",
        ));
    }
    Ok(result)
}

fn write_node(
    node: &LatexNode,
    depth: usize,
    budget: &mut usize,
) -> Result<String, AsciiMathError> {
    if depth >= MAX_DEPTH || *budget == 0 {
        return Err(error(
            AsciiMathErrorKind::LimitExceeded,
            0,
            "AST exceeds pilot resource limits",
        ));
    }
    *budget -= 1;
    if matches!(node, LatexNode::Text(text) if text.len() > MAX_BYTES) {
        return Err(error(
            AsciiMathErrorKind::LimitExceeded,
            0,
            "AST text exceeds 64 KiB",
        ));
    }
    let unsupported = || {
        error(
            AsciiMathErrorKind::UnsupportedSyntax,
            0,
            "AST node is outside the AsciiMath pilot",
        )
    };
    let remaining_nodes = *budget;
    let mut child = |node: &LatexNode| write_node(node, depth + 1, budget);
    let output = match node {
        LatexNode::Text(text)
            if !text.is_empty()
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.') =>
        {
            if lex(text)?.len() != 1 {
                return Err(unsupported());
            }
            Ok(text.clone())
        }
        LatexNode::Text(text)
            if !text.is_empty()
                && text
                    .chars()
                    .all(|ch| ch.is_ascii_alphabetic() || "+-=<>!'".contains(ch) || ch == ',') =>
        {
            // Separate letters so variable runs cannot become reserved keywords.
            Ok(text
                .chars()
                .map(|ch| ch.to_string())
                .collect::<Vec<_>>()
                .join(" "))
        }
        LatexNode::Sequence(nodes) if !nodes.is_empty() => {
            join_bounded(nodes.iter().map(&mut child), " ")
        }
        LatexNode::Group(nodes) if !nodes.is_empty() => {
            join_bounded(nodes.iter().map(&mut child), " ").map(|inner| format!("{{:{inner}:}}"))
        }
        LatexNode::Fraction { num, den } => Ok(format!("frac({})({})", child(num)?, child(den)?)),
        LatexNode::SquareRoot { index, content } => {
            if let Some(index) = index {
                Ok(format!("root({})({})", child(index)?, child(content)?))
            } else {
                Ok(format!("sqrt({})", child(content)?))
            }
        }
        LatexNode::Subscript { base, sub } => {
            let base_text = child(base)?;
            Ok(format!(
                "{}_({})",
                bind_script_base(base, base_text, false)?,
                child(sub)?
            ))
        }
        LatexNode::Superscript { base, exp } => {
            let base_text = child(base)?;
            Ok(format!(
                "{}^({})",
                bind_script_base(base, base_text, true)?,
                child(exp)?
            ))
        }
        LatexNode::Greek(name) if GREEK.contains(&name.as_str()) => Ok(name.clone()),
        LatexNode::Operator(name) if OPERATORS.contains(&name.as_str()) => Ok(name.clone()),
        LatexNode::FontModifier { font, content } if font == "mathbb" => {
            let LatexNode::Text(letter) = content.as_ref() else {
                return Err(unsupported());
            };
            SETS.iter()
                .find(|(_, mapped)| *mapped == letter)
                .map(|(key, _)| (*key).into())
                .ok_or_else(unsupported)
        }
        LatexNode::Command { name, args }
            if args.is_empty() && FUNCTIONS.contains(&name.as_str()) =>
        {
            Ok(name.clone())
        }
        LatexNode::Symbol(name) | LatexNode::Relation(name) => {
            let name = name.trim_start_matches('\\');
            SYMBOLS
                .iter()
                .find(|(_, mapped)| *mapped == name)
                .map(|(key, _)| (*key).into())
                .ok_or_else(unsupported)
        }
        LatexNode::Delimited {
            left,
            content,
            right,
        } if !content.is_empty() => {
            let inner = join_bounded(content.iter().map(&mut child), " ")?;
            match (left.as_str(), right.as_str()) {
                ("(", ")") | ("[", "]") => Ok(format!("{left}{inner}{right}")),
                ("\\{", "\\}") => Ok(format!("{{{inner}}}")),
                ("|", "|") => Ok(format!("abs({inner})")),
                ("\\|", "\\|") => Ok(format!("norm({inner})")),
                ("\\lfloor", "\\rfloor") => Ok(format!("floor({inner})")),
                ("\\lceil", "\\rceil") => Ok(format!("ceil({inner})")),
                _ => Err(unsupported()),
            }
        }
        LatexNode::Matrix { env, rows }
            if !rows.is_empty() && matches!(env.as_str(), "pmatrix" | "bmatrix") =>
        {
            if rows.len().saturating_mul(rows[0].len().max(1)) > remaining_nodes {
                return Err(error(
                    AsciiMathErrorKind::LimitExceeded,
                    0,
                    "matrix exceeds AST node budget",
                ));
            }
            if rows[0].is_empty() || rows.iter().any(|row| row.len() != rows[0].len()) {
                return Err(unsupported());
            }
            let (left, right) = if env == "pmatrix" {
                ("(", ")")
            } else {
                ("[", "]")
            };
            let inner = join_bounded(
                rows.iter().map(|row| {
                    let cells = join_bounded(row.iter().map(&mut child), ",")?;
                    Ok(format!("{left}{cells}{right}"))
                }),
                ",",
            )?;
            Ok(format!("{left}{inner}{right}"))
        }
        _ => Err(unsupported()),
    }?;
    if output.len() > MAX_BYTES {
        return Err(error(
            AsciiMathErrorKind::LimitExceeded,
            0,
            "serialized output exceeds 64 KiB",
        ));
    }
    Ok(output)
}

fn bind_script_base(
    node: &LatexNode,
    text: String,
    superscript: bool,
) -> Result<String, AsciiMathError> {
    let group = match node {
        LatexNode::Sequence(_) => true,
        LatexNode::Text(_) => lex(&text)?.len() != 1,
        LatexNode::Superscript { .. } => true,
        LatexNode::Subscript { .. } => !superscript,
        _ => false,
    };
    Ok(if group { format!("{{:{text}:}}") } else { text })
}

// Ignore invisible grouping/sequence wrappers, but retain every visible
// delimiter, operand boundary, matrix row/cell, command and script owner.
#[derive(PartialEq, Eq)]
struct Shape {
    name: String,
    children: Vec<Vec<Shape>>,
}

fn shape(node: &LatexNode) -> Vec<Shape> {
    let combined = |nodes: &[LatexNode]| nodes.iter().flat_map(shape).collect::<Vec<_>>();
    let (name, children) = match node {
        LatexNode::Sequence(nodes) | LatexNode::Group(nodes) => return combined(nodes),
        LatexNode::Text(text) => {
            return text
                .chars()
                .map(|ch| Shape {
                    name: format!("text:{ch}"),
                    children: vec![],
                })
                .collect()
        }
        LatexNode::Fraction { num, den } => ("frac".into(), vec![shape(num), shape(den)]),
        LatexNode::SquareRoot { index, content } => (
            if index.is_some() {
                "indexed-root"
            } else {
                "sqrt"
            }
            .into(),
            vec![
                index.as_ref().map_or_else(Vec::new, |index| shape(index)),
                shape(content),
            ],
        ),
        LatexNode::Subscript { base, sub } => ("sub".into(), vec![shape(base), shape(sub)]),
        LatexNode::Superscript { base, exp } => ("sup".into(), vec![shape(base), shape(exp)]),
        LatexNode::Greek(name) => (format!("greek:{name}"), vec![]),
        LatexNode::Operator(name) => (format!("operator:{name}"), vec![]),
        LatexNode::FontModifier { font, content } => (format!("font:{font}"), vec![shape(content)]),
        LatexNode::Command { name, args } => {
            (format!("command:{name}"), args.iter().map(shape).collect())
        }
        LatexNode::Symbol(name) | LatexNode::Relation(name) => {
            (format!("symbol:{}", name.trim_start_matches('\\')), vec![])
        }
        LatexNode::Delimited {
            left,
            content,
            right,
        } => (format!("delimited:{left}:{right}"), vec![combined(content)]),
        LatexNode::Matrix { env, rows } => (
            format!("matrix:{env}"),
            rows.iter()
                .map(|row| {
                    vec![Shape {
                        name: "row".into(),
                        children: row.iter().map(shape).collect(),
                    }]
                })
                .collect(),
        ),
        _ => ("unsupported".into(), vec![]),
    };
    vec![Shape { name, children }]
}

fn join_bounded(
    parts: impl Iterator<Item = Result<String, AsciiMathError>>,
    separator: &str,
) -> Result<String, AsciiMathError> {
    let mut output = String::new();
    for part in parts {
        let part = part?;
        let extra = if output.is_empty() {
            0
        } else {
            separator.len()
        };
        if output
            .len()
            .saturating_add(extra)
            .saturating_add(part.len())
            > MAX_BYTES
        {
            return Err(error(
                AsciiMathErrorKind::LimitExceeded,
                0,
                "serialized output exceeds 64 KiB",
            ));
        }
        if extra > 0 {
            output.push_str(separator);
        }
        output.push_str(&part);
    }
    Ok(output)
}
