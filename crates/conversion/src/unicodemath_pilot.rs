//! Independent, bounded UnicodeMath grammar; pilot module name retained.
//! See docs/formats/unicodemath-pilot.md for the finite grammar and gates.
use crate::latex_ast::LatexNode;
use std::fmt;
const MAX_BYTES: usize = 64 * 1024;
const MAX_TOKENS: usize = 4096;
const MAX_NODES: usize = 8192;
const MAX_DEPTH: usize = 64;
const NARY_PLACEMENT_BITS: u16 = 7;
const NARY_GROWTH_BITS: u16 = 64 | 128;
const NARY_ALLOWED_BITS: u16 = NARY_PLACEMENT_BITS | 8 | 16 | NARY_GROWTH_BITS;

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
const NARY: &[(char, &str)] = &[
    ('∫', "int"),
    ('∬', "iint"),
    ('∭', "iiint"),
    ('∮', "oint"),
    ('∑', "sum"),
    ('∏', "prod"),
    ('∐', "coprod"),
    ('⋃', "bigcup"),
    ('⋂', "bigcap"),
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Quoted(usize),
    Operand(char),
    Other(char),
    Open(char),
    Close(char),
    Root(char),
    Matrix(char),
    Nary(char),
    Space,
    Slash,
    Sub,
    Sup,
    Apply,
    Naryand,
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
    let mut chars = source.char_indices().peekable();
    let mut work = 0;
    while let Some((offset, ch)) = chars.next() {
        if work >= MAX_TOKENS {
            return Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                offset,
                "input exceeds 4096 tokens",
            ));
        }
        work += 1;
        if ch == '"' {
            let mut end = None;
            while let Some((at, inner)) = chars.next() {
                if work >= MAX_TOKENS {
                    return Err(error(
                        UnicodeMathErrorKind::LimitExceeded,
                        at,
                        "quoted text exceeds lexical budget",
                    ));
                }
                work += 1;
                if inner.is_control() || matches!(inner, '\u{2028}' | '\u{2029}') {
                    return Err(error(
                        UnicodeMathErrorKind::UnsupportedSyntax,
                        at,
                        "control characters and line breaks are outside quoted text",
                    ));
                }
                if inner == '\\'
                    && chars
                        .peek()
                        .is_some_and(|(_, value)| matches!(value, '"' | '\\'))
                {
                    let (escaped_at, _) = chars.next().expect("peeked escape");
                    if work >= MAX_TOKENS {
                        return Err(error(
                            UnicodeMathErrorKind::LimitExceeded,
                            escaped_at,
                            "quoted text exceeds lexical budget",
                        ));
                    }
                    work += 1;
                } else if inner == '"' {
                    end = Some(at);
                    break;
                }
            }
            let end = end.ok_or_else(|| {
                error(
                    UnicodeMathErrorKind::UnexpectedToken,
                    offset,
                    "unterminated quoted text",
                )
            })?;
            tokens.push(Token {
                kind: Kind::Quoted(end),
                offset,
            });
            continue;
        }
        let kind = match ch {
            ' ' => Kind::Space,
            '/' => Kind::Slash,
            '_' => Kind::Sub,
            '^' => Kind::Sup,
            '&' => Kind::Cell,
            '@' => Kind::Row,
            '\u{2061}' => Kind::Apply,
            '▒' => Kind::Naryand,
            '(' | '[' | '{' | '〖' => Kind::Open(ch),
            ')' | ']' | '}' | '〗' => Kind::Close(ch),
            '√' | '∛' | '∜' => Kind::Root(ch),
            ch if MATRICES.iter().any(|(glyph, _)| *glyph == ch) => Kind::Matrix(ch),
            ch if NARY.iter().any(|(glyph, _)| *glyph == ch) => Kind::Nary(ch),
            ch if ch.is_ascii_alphanumeric()
                || GREEK.iter().any(|(glyph, _)| *glyph == ch)
                || matches!(ch, '∞' | 'ⅆ') =>
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
struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    end: usize,
}
impl Parser<'_> {
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
            Some(
                Kind::Operand(_)
                    | Kind::Open(_)
                    | Kind::Root(_)
                    | Kind::Matrix(_)
                    | Kind::Quoted(_)
                    | Kind::Nary(_)
            )
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
    fn run(&mut self, allow_identifier: bool) -> Result<Vec<LatexNode>, UnicodeMathError> {
        let start = self.pos;
        let mut nodes = Vec::new();
        let mut word = String::new();
        while let Some(Kind::Operand(ch)) = self.kind() {
            if ch.is_ascii_alphabetic() {
                word.push(ch);
            } else {
                if !allow_identifier && FUNCTIONS.contains(&word.as_str()) {
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
        if !allow_identifier && FUNCTIONS.contains(&word.as_str()) {
            return Err(error(
                UnicodeMathErrorKind::UnsupportedSyntax,
                self.tokens[start].offset,
                "named functions require a separate application grammar",
            ));
        }
        Ok(nodes)
    }
    fn atom(
        &mut self,
        depth: usize,
        allow_identifier: bool,
    ) -> Result<LatexNode, UnicodeMathError> {
        self.depth(depth)?;
        // Recognize complete names only, never substrings of variable runs.
        let mut end = self.pos;
        let mut name = String::new();
        while let Some(Token {
            kind: Kind::Operand(ch),
            ..
        }) = self.tokens.get(end)
        {
            if !ch.is_ascii_alphabetic() {
                break;
            }
            name.push(*ch);
            end += 1;
        }
        if FUNCTIONS.contains(&name.as_str()) {
            if matches!(
                self.tokens.get(end).map(|token| token.kind),
                Some(Kind::Operand(_))
            ) {
                if !allow_identifier {
                    return Err(self.fail("function name needs an application or operand boundary"));
                }
            } else {
                self.pos = end;
                return Ok(LatexNode::Operator(name));
            }
        }
        match self.kind() {
            Some(Kind::Nary(glyph)) => {
                self.pos += 1;
                let mut head = LatexNode::Operator(
                    NARY.iter()
                        .find(|(value, _)| *value == glyph)
                        .expect("lexed n-ary")
                        .1
                        .into(),
                );
                if matches!(self.kind(), Some(Kind::Operand(ch)) if ch.is_ascii_digit()) {
                    let offset = self.offset();
                    let mut flag = 0_u16;
                    let mut digits = 0;
                    while let Some(Kind::Operand(ch)) = self
                        .kind()
                        .filter(|kind| matches!(kind, Kind::Operand(ch) if ch.is_ascii_digit()))
                    {
                        digits += 1;
                        if digits > 3 {
                            return Err(error(
                                UnicodeMathErrorKind::UnsupportedSyntax,
                                offset,
                                "n-ary flag exceeds the finite mask range",
                            ));
                        }
                        flag = flag * 10 + ch.to_digit(10).expect("ASCII digit") as u16;
                        self.pos += 1;
                    }
                    if flag & !NARY_ALLOWED_BITS != 0
                        || flag & NARY_PLACEMENT_BITS > 2
                        || flag & NARY_GROWTH_BITS == NARY_GROWTH_BITS
                    {
                        return Err(error(
                            UnicodeMathErrorKind::UnsupportedSyntax,
                            offset,
                            "n-ary flags support finite placement, empty slots and one growth setting only",
                        ));
                    }
                    if flag & 8 != 0 {
                        head = LatexNode::Subscript {
                            base: Box::new(head),
                            sub: Box::new(LatexNode::Sequence(vec![])),
                        };
                    }
                    if flag & 16 != 0 {
                        head = LatexNode::Superscript {
                            base: Box::new(head),
                            exp: Box::new(LatexNode::Sequence(vec![])),
                        };
                    }
                    let placement = flag & NARY_PLACEMENT_BITS;
                    if placement != 0 {
                        head = LatexNode::Command {
                            name: if placement == 1 { "limits" } else { "nolimits" }.into(),
                            args: vec![head],
                        };
                    }
                    if flag & NARY_GROWTH_BITS != 0 {
                        head = LatexNode::Command {
                            name: if flag & 64 != 0 {
                                "nary-no-grow"
                            } else {
                                "nary-grow"
                            }
                            .into(),
                            args: vec![head],
                        };
                    }
                }
                Ok(head)
            }
            Some(Kind::Quoted(end)) => {
                let start = self.offset();
                let mut chars = self.source[start + 1..end].chars().peekable();
                let mut text = String::new();
                while let Some(ch) = chars.next() {
                    if ch == '\\' && chars.peek().is_some_and(|ch| matches!(ch, '"' | '\\')) {
                        text.push(chars.next().expect("peeked escape"));
                    } else {
                        text.push(ch);
                    }
                }
                let encoded = encode_text(&text, start)?;
                self.pos += 1;
                Ok(LatexNode::Command {
                    name: "text".into(),
                    args: vec![LatexNode::Text(encoded)],
                })
            }
            Some(Kind::Operand(_)) => Ok(sequence(self.run(allow_identifier)?)),
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
    fn ambiguous_builtin_run(&self) -> bool {
        let mut word = String::new();
        let mut index = self.pos;
        while let Some(token) = self.tokens.get(index) {
            match token.kind {
                Kind::Operand(ch) if ch.is_ascii_alphabetic() => word.push(ch),
                Kind::Operand(_) => {
                    if FUNCTIONS.contains(&word.as_str()) {
                        return true;
                    }
                    word.clear();
                }
                Kind::Other('.' | ',')
                    if matches!(index.checked_sub(1).and_then(|i| self.tokens.get(i)).map(|t| t.kind),
                        Some(Kind::Operand(ch)) if ch.is_ascii_digit())
                        && matches!(self.tokens.get(index + 1).map(|t| t.kind),
                        Some(Kind::Operand(ch)) if ch.is_ascii_digit()) =>
                {
                    word.clear();
                }
                _ => break,
            }
            index += 1;
        }
        FUNCTIONS.contains(&word.as_str())
    }

    fn factor(&mut self, depth: usize) -> Result<LatexNode, UnicodeMathError> {
        self.depth(depth)?;
        let is_run = matches!(self.kind(), Some(Kind::Operand(_)));
        let ambiguous_builtin = self.ambiguous_builtin_run();
        let base = self.atom(depth, true)?;
        let mut base = self.scripts(base, is_run, depth)?;
        let nary = nary_name(&base).is_some();
        let before_spaces = self.pos;
        self.spaces();
        if is_run
            && !nary
            && !is_function_head(&base)
            && matches!(self.kind(), Some(Kind::Apply | Kind::Naryand))
        {
            if let Some(promoted) = promote_function_head(&base) {
                base = promoted;
            }
        }
        self.pos = before_spaces;
        if nary && nary_limits(&base).is_none() {
            return Err(error(
                UnicodeMathErrorKind::UnsupportedSyntax,
                self.offset(),
                "duplicate n-ary limit slots are outside the finite grammar",
            ));
        }
        if !is_function_head(&base) && !nary {
            if ambiguous_builtin {
                return Err(error(
                    UnicodeMathErrorKind::UnsupportedSyntax,
                    self.offset(),
                    "named functions require a separate application grammar",
                ));
            }
            if matches!(self.kind(), Some(Kind::Apply | Kind::Naryand)) {
                return Err(error(
                    UnicodeMathErrorKind::UnsupportedSyntax,
                    self.offset(),
                    "application requires a supported function or n-ary head",
                ));
            }
            return Ok(base);
        }
        let saved = self.pos;
        self.spaces();
        let marker = if nary { Kind::Naryand } else { Kind::Apply };
        let explicit = self.kind() == Some(marker) || (!nary && self.kind() == Some(Kind::Naryand));
        if nary && !explicit && self.pos == saved {
            if self.starts_factor() {
                return Err(self.fail("n-ary operands require a blank or explicit operand marker"));
            }
            return Ok(base);
        }
        if explicit {
            self.pos += 1;
            // UTN 28 moves scripts immediately after Apply onto the name.
            if !nary && matches!(self.kind(), Some(Kind::Sub | Kind::Sup)) {
                if !matches!(
                    base,
                    LatexNode::Operator(_) | LatexNode::OperatorName { .. }
                ) {
                    return Err(self.fail("repeated function-head scripts require grouping"));
                }
                base = self.scripts(base, false, depth + 1)?;
            }
            self.spaces();
        }
        if !self.starts_factor() {
            if explicit {
                return Err(self.fail("application needs a nonempty argument"));
            }
            self.pos = saved;
            return Ok(base);
        }
        // A bracketed argument ends at its closing bracket. Adjacent factors
        // after it belong to the enclosing expression, not to this function.
        let bracketed = matches!(self.kind(), Some(Kind::Open(_)));
        let mut argument = if bracketed {
            self.factor(depth + 1)?
        } else {
            self.operand(depth + 1)?
        };
        if !bracketed {
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
                        "function argument fraction chain exceeds budget",
                    ));
                }
                self.pos += 1;
                self.spaces();
                argument = LatexNode::Fraction {
                    num: Box::new(consume_round(argument)),
                    den: Box::new(consume_round(self.operand(depth + 1)?)),
                };
            }
        }
        // Keep the owned argument nested through the existing LaTeX bridge.
        Ok(LatexNode::Group(vec![
            base,
            LatexNode::Group(vec![argument]),
        ]))
    }
    fn scripts(
        &mut self,
        mut base: LatexNode,
        is_run: bool,
        depth: usize,
    ) -> Result<LatexNode, UnicodeMathError> {
        self.depth(depth)?;
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
            let value = if nary_name(&base).is_some()
                && self.kind() == Some(Kind::Open('('))
                && self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|token| token.kind == Kind::Close(')'))
            {
                self.pos += 2;
                if self.starts_factor() {
                    return Err(self.fail("an empty n-ary slot needs an operand boundary"));
                }
                LatexNode::Sequence(vec![])
            } else {
                self.script_operand(mark, depth + 1)?
            };
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
        let mut value = consume_round(self.atom(depth + 1, false)?);
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
        source,
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
    validate_nary_placement(&node)?;
    Ok(node)
}

// Validate complete heads, not their temporary unscripted bases inside groups.
fn validate_nary_placement(node: &LatexNode) -> Result<(), UnicodeMathError> {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if let Some(head) = nary_limits(node) {
            if head.limits.is_some() && head.lower.is_none() && head.upper.is_none() {
                return Err(error(
                    UnicodeMathErrorKind::UnsupportedSyntax,
                    0,
                    "explicit n-ary placement currently requires a lower or upper limit",
                ));
            }
            stack.extend(head.lower.into_iter().chain(head.upper));
            continue;
        }
        match node {
            LatexNode::Sequence(nodes)
            | LatexNode::Group(nodes)
            | LatexNode::Delimited { content: nodes, .. } => stack.extend(nodes),
            LatexNode::Command { args, .. } => stack.extend(args),
            LatexNode::Subscript { base, sub } => stack.extend([base.as_ref(), sub.as_ref()]),
            LatexNode::Superscript { base, exp } => stack.extend([base.as_ref(), exp.as_ref()]),
            LatexNode::Fraction { num, den } => stack.extend([num.as_ref(), den.as_ref()]),
            LatexNode::SquareRoot { index, content } => {
                stack.push(content);
                stack.extend(index.iter().map(Box::as_ref));
            }
            LatexNode::Matrix { rows, .. } => stack.extend(rows.iter().flatten()),
            _ => {}
        }
    }
    Ok(())
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
            LatexNode::Command { args, .. } | LatexNode::OperatorName { args, .. } => {
                if args.len() > MAX_NODES - count {
                    return Err(error(
                        UnicodeMathErrorKind::LimitExceeded,
                        0,
                        "AST command exceeds node budget",
                    ));
                }
                for node in args {
                    push(node)?;
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
    if crate::pilot_ast::unicode_shape(node) != crate::pilot_ast::unicode_shape(&rebuilt) {
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
    if nary_name(node).is_some() && nary_limits(node).is_none() {
        return Err(error(
            UnicodeMathErrorKind::UnsupportedSyntax,
            0,
            "duplicate n-ary limit slots cannot be serialized",
        ));
    }
    if let Some(head) = nary_limits(node) {
        let mut output = head.glyph.to_string();
        let flag = head
            .limits
            .map_or(0, |vertical| if vertical { 1 } else { 2 })
            + head.grow.map_or(0, |grow| if grow { 128 } else { 64 });
        let flag =
            flag + if head.lower.is_some_and(LatexNode::is_empty) {
                8
            } else {
                0
            } + if head.upper.is_some_and(LatexNode::is_empty) {
                16
            } else {
                0
            };
        if flag != 0 {
            output.push_str(&flag.to_string());
        }
        if let Some(lower) = head.lower.filter(|node| !node.is_empty()) {
            output = format!("〖{output}〗_({})", write_node(lower, depth + 1)?);
        }
        if let Some(upper) = head.upper.filter(|node| !node.is_empty()) {
            output = format!("〖{output}〗^({})", write_node(upper, depth + 1)?);
        }
        if output.len() > MAX_BYTES {
            return Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                0,
                "n-ary head output exceeds 64 KiB",
            ));
        }
        return Ok(output);
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
        LatexNode::Command { name, args } if name == "text" && args.len() == 1 => {
            let LatexNode::Text(encoded) = &args[0] else {
                return Err(unsupported());
            };
            if encoded.len() > MAX_BYTES {
                return Err(error(
                    UnicodeMathErrorKind::LimitExceeded,
                    0,
                    "AST text exceeds 64 KiB",
                ));
            }
            let text = crate::latex_utils::decode_text_symbols(encoded);
            if text
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '\u{2028}' | '\u{2029}'))
            {
                return Err(unsupported());
            }
            let mut quoted = String::from("\"");
            for ch in text.chars() {
                if matches!(ch, '\\' | '"') {
                    quoted.push('\\');
                }
                quoted.push(ch);
                if quoted.len() >= MAX_BYTES {
                    return Err(error(
                        UnicodeMathErrorKind::LimitExceeded,
                        0,
                        "quoted output exceeds 64 KiB",
                    ));
                }
            }
            quoted.push('"');
            quoted
        }
        LatexNode::Text(text) => {
            if text.len() > MAX_BYTES {
                return Err(error(
                    UnicodeMathErrorKind::LimitExceeded,
                    0,
                    "AST text exceeds 64 KiB",
                ));
            }
            if !text.is_empty() && text.chars().all(|ch| ch == ' ') {
                return Ok(String::new());
            }
            if text.is_empty()
                || !text
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || "+-=<>., ⅆ".contains(ch))
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
        LatexNode::Sequence(nodes) => write_sequence(nodes, depth + 1)?,
        LatexNode::Group(nodes) if nodes.len() == 2 && nary_name(&nodes[0]).is_some() => {
            if nodes[1].is_empty() {
                child(&nodes[0])?
            } else {
                format!("{}▒〖{}〗", child(&nodes[0])?, child(&nodes[1])?)
            }
        }
        LatexNode::Group(nodes) if nodes.len() == 2 && is_function_head(&nodes[0]) => {
            write_function_application(&nodes[0], &nodes[1], depth + 1)?
        }
        LatexNode::Group(nodes) if !nodes.is_empty() => write_sequence(nodes, depth + 1)?,
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
        LatexNode::Operator(name) if FUNCTIONS.contains(&name.as_str()) => name.clone(),
        LatexNode::OperatorName { .. } if custom_function_name(node).is_some() => {
            custom_function_name(node)
                .expect("checked identifier")
                .into()
        }
        LatexNode::Operator(name) => NARY
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

fn write_sequence(nodes: &[LatexNode], depth: usize) -> Result<String, UnicodeMathError> {
    let mut index = 0;
    join(
        std::iter::from_fn(|| {
            while matches!(nodes.get(index), Some(LatexNode::Text(text))
                if !text.is_empty() && text.chars().all(|ch| ch == ' '))
            {
                index += 1;
            }
            let node = nodes.get(index)?;
            index += 1;
            while matches!(nodes.get(index), Some(LatexNode::Text(text))
                if !text.is_empty() && text.chars().all(|ch| ch == ' '))
            {
                index += 1;
            }
            let argument = nodes.get(index).filter(|argument| match argument {
                LatexNode::Relation(_) => false,
                LatexNode::Text(text) => {
                    !matches!(text.trim(), "+" | "-" | "=" | "<" | ">" | "." | ",")
                }
                _ => true,
            });
            Some(
                if let Some(argument) =
                    argument.filter(|_| is_function_head(node) || nary_name(node).is_some())
                {
                    index += 1;
                    let marker = if nary_name(node).is_some() {
                        "▒"
                    } else {
                        "\u{2061}"
                    };
                    write_node(node, depth).and_then(|head| {
                        if is_custom_function_head(node) {
                            write_function_application(node, argument, depth)
                        } else {
                            write_node(argument, depth)
                                .map(|body| format!("{head}{marker}〖{body}〗"))
                        }
                    })
                } else {
                    write_node(node, depth)
                },
            )
        }),
        " ",
    )
}

pub(crate) fn is_function_head(node: &LatexNode) -> bool {
    function_name(node).is_some()
}

pub(crate) fn function_name(node: &LatexNode) -> Option<&str> {
    // Callers validate depth/node budgets before external AST traversal.
    match node {
        LatexNode::Operator(name) if is_function_name(name) => Some(name),
        LatexNode::OperatorName { .. } => custom_function_name(node),
        LatexNode::Subscript { base, .. } | LatexNode::Superscript { base, .. } => {
            function_name(base)
        }
        LatexNode::Group(nodes) if nodes.len() == 1 => function_name(&nodes[0]),
        _ => None,
    }
}

pub(crate) fn is_identifier_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && !name.starts_with("LatexSnipperInternal")
}

pub(crate) fn custom_function_name(node: &LatexNode) -> Option<&str> {
    fn literal(node: &LatexNode) -> Option<&str> {
        match node {
            LatexNode::Text(name) if is_identifier_name(name) => Some(name),
            LatexNode::Group(nodes) if nodes.len() == 1 => literal(&nodes[0]),
            _ => None,
        }
    }
    match node {
        LatexNode::OperatorName { name, args } if name == "operatorname" && args.len() == 1 => {
            literal(&args[0])
        }
        LatexNode::Subscript { base, .. } | LatexNode::Superscript { base, .. } => {
            custom_function_name(base)
        }
        LatexNode::Group(nodes) if nodes.len() == 1 => custom_function_name(&nodes[0]),
        _ => None,
    }
}

pub(crate) fn is_custom_function_head(node: &LatexNode) -> bool {
    custom_function_name(node).is_some()
}

// Promotion happens only after an explicit Apply, with no speculative parsing.
fn promote_function_head(node: &LatexNode) -> Option<LatexNode> {
    fn plain(node: &LatexNode, name: &mut String) -> Option<()> {
        match node {
            LatexNode::Text(text) if text.bytes().all(|byte| byte.is_ascii_alphanumeric()) => {
                if name.len() + text.len() > 64 {
                    return None;
                }
                name.push_str(text);
            }
            LatexNode::Group(nodes) | LatexNode::Sequence(nodes) => {
                for node in nodes {
                    plain(node, name)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
    fn replace_head(node: &LatexNode, prefix: &str) -> Option<LatexNode> {
        match node {
            LatexNode::Subscript { base, sub } => Some(LatexNode::Subscript {
                base: Box::new(replace_head(base, prefix)?),
                sub: sub.clone(),
            }),
            LatexNode::Superscript { base, exp } => Some(LatexNode::Superscript {
                base: Box::new(replace_head(base, prefix)?),
                exp: exp.clone(),
            }),
            LatexNode::Group(nodes) if nodes.len() == 1 => replace_head(&nodes[0], prefix),
            LatexNode::Group(nodes) | LatexNode::Sequence(nodes) if !nodes.is_empty() => {
                let mut name = prefix.to_string();
                for node in &nodes[..nodes.len() - 1] {
                    plain(node, &mut name)?;
                }
                replace_head(nodes.last()?, &name)
            }
            _ => {
                let mut name = prefix.to_string();
                plain(node, &mut name)?;
                if !is_identifier_name(&name) {
                    return None;
                }
                Some(LatexNode::OperatorName {
                    name: "operatorname".into(),
                    args: vec![LatexNode::Text(name)],
                })
            }
        }
    }
    replace_head(node, "")
}

fn write_function_application(
    head: &LatexNode,
    argument: &LatexNode,
    depth: usize,
) -> Result<String, UnicodeMathError> {
    fn custom_head(node: &LatexNode, depth: usize) -> Result<String, UnicodeMathError> {
        if depth > MAX_DEPTH {
            return Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                0,
                "function head exceeds depth budget",
            ));
        }
        match node {
            LatexNode::Subscript { base, sub } => Ok(format!(
                "{}_({})",
                custom_head(base, depth + 1)?,
                write_node(sub, depth + 1)?
            )),
            LatexNode::Superscript { base, exp } => Ok(format!(
                "{}^({})",
                custom_head(base, depth + 1)?,
                write_node(exp, depth + 1)?
            )),
            LatexNode::Group(nodes) if nodes.len() == 1 => custom_head(&nodes[0], depth + 1),
            _ => custom_function_name(node)
                .map(|name| format!("{name}\u{2061}"))
                .ok_or_else(|| {
                    error(
                        UnicodeMathErrorKind::UnsupportedSyntax,
                        0,
                        "unsupported custom function head",
                    )
                }),
        }
    }
    let custom = is_custom_function_head(head);
    let head = if custom {
        custom_head(head, depth)?
    } else {
        format!("{}\u{2061}", write_node(head, depth)?)
    };
    let separator = if custom && !head.ends_with('\u{2061}') {
        " "
    } else {
        ""
    };
    Ok(format!(
        "{head}{separator}〖{}〗",
        write_node(argument, depth)?
    ))
}

pub(crate) fn is_function_name(name: &str) -> bool {
    FUNCTIONS.contains(&name)
}

pub(crate) fn nary_name(node: &LatexNode) -> Option<&str> {
    match node {
        LatexNode::Operator(name) if NARY.iter().any(|(_, mapped)| *mapped == name) => Some(name),
        LatexNode::Superscript { base, .. } | LatexNode::Subscript { base, .. } => nary_name(base),
        LatexNode::Group(nodes) if nodes.len() == 1 => nary_name(&nodes[0]),
        LatexNode::Command { name, args }
            if matches!(
                name.as_str(),
                "limits" | "nolimits" | "nary-grow" | "nary-no-grow"
            ) && args.len() == 1 =>
        {
            nary_name(&args[0])
        }
        _ => None,
    }
}

pub(crate) struct NaryLimits<'a> {
    pub name: &'a str,
    pub glyph: char,
    pub lower: Option<&'a LatexNode>,
    pub upper: Option<&'a LatexNode>,
    pub limits: Option<bool>,
    pub grow: Option<bool>,
}

pub(crate) fn nary_limits(node: &LatexNode) -> Option<NaryLimits<'_>> {
    match node {
        LatexNode::Operator(name) => Some(NaryLimits {
            name,
            glyph: NARY.iter().find(|(_, mapped)| *mapped == name)?.0,
            lower: None,
            upper: None,
            limits: None,
            grow: None,
        }),
        LatexNode::Subscript { base, sub } => {
            let mut head = nary_limits(base)?;
            if head.lower.is_some_and(|lower| !lower.is_empty()) && !sub.is_empty() {
                return None;
            }
            if !sub.is_empty() || head.lower.is_none() {
                head.lower = Some(sub);
            }
            Some(head)
        }
        LatexNode::Superscript { base, exp } => {
            let mut head = nary_limits(base)?;
            if head.upper.is_some_and(|upper| !upper.is_empty()) && !exp.is_empty() {
                return None;
            }
            if !exp.is_empty() || head.upper.is_none() {
                head.upper = Some(exp);
            }
            Some(head)
        }
        LatexNode::Group(nodes) if nodes.len() == 1 => nary_limits(&nodes[0]),
        LatexNode::Command { name, args }
            if matches!(name.as_str(), "limits" | "nolimits") && args.len() == 1 =>
        {
            let mut head = nary_limits(&args[0])?;
            if head.limits.is_some() {
                return None;
            }
            head.limits = Some(name == "limits");
            Some(head)
        }
        LatexNode::Command { name, args }
            if matches!(name.as_str(), "nary-grow" | "nary-no-grow") && args.len() == 1 =>
        {
            let mut head = nary_limits(&args[0])?;
            if head.grow.is_some() {
                return None;
            }
            head.grow = Some(name == "nary-grow");
            Some(head)
        }
        _ => None,
    }
}

pub(crate) fn contains_growth(node: &LatexNode) -> bool {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        match node {
            LatexNode::Command { name, args } => {
                if matches!(name.as_str(), "nary-grow" | "nary-no-grow") {
                    return true;
                }
                stack.extend(args);
            }
            LatexNode::Sequence(nodes)
            | LatexNode::Group(nodes)
            | LatexNode::Delimited { content: nodes, .. } => stack.extend(nodes),
            LatexNode::Subscript { base, sub } => stack.extend([base.as_ref(), sub.as_ref()]),
            LatexNode::Superscript { base, exp } => stack.extend([base.as_ref(), exp.as_ref()]),
            LatexNode::Fraction { num, den } => stack.extend([num.as_ref(), den.as_ref()]),
            LatexNode::SquareRoot { index, content } => {
                stack.push(content);
                stack.extend(index.iter().map(Box::as_ref));
            }
            LatexNode::Matrix { rows, .. } => stack.extend(rows.iter().flatten()),
            _ => {}
        }
    }
    false
}

fn encode_text(text: &str, offset: usize) -> Result<String, UnicodeMathError> {
    let mut length = 0_usize;
    for ch in text.chars() {
        length = length.saturating_add(match ch {
            '\\' => "\\backslash{}".len(),
            '^' => "\\textasciicircum{}".len(),
            '~' => "\\textasciitilde{}".len(),
            '%' | '&' | '#' | '_' | '$' | '{' | '}' => 2,
            _ => ch.len_utf8(),
        });
        if length > MAX_BYTES {
            return Err(error(
                UnicodeMathErrorKind::LimitExceeded,
                offset,
                "escaped text exceeds 64 KiB",
            ));
        }
    }
    Ok(crate::latex_utils::escape_text_symbols(text))
}
