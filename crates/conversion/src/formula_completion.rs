//! Source-preserving editing structure for bounded, incomplete LaTeX formulas.
//!
//! This tree describes editing ownership, not mathematical validity or macro
//! expansion. Offsets are UTF-8 bytes, including cursor positions between chars.
//!
//! ```
//! use latexsnipper_conversion::formula_completion::{complete_formula, apply_formula_completion};
//! let source = r"\fr";
//! let result = complete_formula(source, source.len(), 42).unwrap();
//! let applied = apply_formula_completion(source, source.len(), 42, &result, "builtin:frac").unwrap();
//! assert_eq!(applied.source, r"\frac{}{}");
//! assert_eq!(applied.cursor, 6);
//! assert_eq!(applied.tab_stops.len(), 2);
//! ```
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[path = "formula_completion_candidates.rs"]
mod candidates;
#[path = "formula_completion_commands.rs"]
mod commands;
#[path = "formula_completion_delimiters.rs"]
mod delimiters;
pub use candidates::{
    apply_formula_candidate, apply_formula_completion, complete_formula, AppliedFormulaCompletion,
    FormulaCompletionCandidate, FormulaCompletionResult,
};
#[path = "formula_completion_fragments.rs"]
pub mod fragments;
#[path = "formula_completion_preferences.rs"]
pub mod preferences;
#[path = "formula_completion_scoped.rs"]
pub mod scoped;
#[path = "formula_completion_wire.rs"]
mod wire;
pub use wire::{
    FormulaCompletionApplyRequest, FormulaCompletionInputFormat, FormulaCompletionReply,
    FormulaCompletionRequest, FormulaCompletionSnapshot,
};

pub const FORMULA_COMPLETION_VERSION: u16 = 1;
pub const FORMULA_COMPLETION_MAX_BYTES: usize = 65_536;
pub const FORMULA_COMPLETION_MAX_NODES: usize = 4096;
/// Recursion depth must remain below this value.
pub const FORMULA_COMPLETION_MAX_DEPTH: usize = 64;
pub const FORMULA_COMPLETION_MAX_CANDIDATES: usize = 128;
const MAX_BYTES: usize = FORMULA_COMPLETION_MAX_BYTES;
const MAX_NODES: usize = FORMULA_COMPLETION_MAX_NODES;
const MAX_DEPTH: usize = FORMULA_COMPLETION_MAX_DEPTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaSourceRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormulaSlot {
    Formula,
    Group,
    FractionNumerator,
    FractionDenominator,
    FractionAlignment,
    BinomialUpper,
    BinomialLower,
    RadicalIndex,
    RadicalBody,
    Superscript,
    Subscript,
    MathArgument,
    Text,
    EnvironmentName,
    ArrayColumnSpec,
    MatrixCell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FormulaEditKind {
    Root,
    Text,
    Comment,
    Command { name: String },
    Group,
    Argument,
    Script { base: Option<usize> },
    Environment { name: String },
    Cell { row: usize, column: usize },
    Separator,
}

/// Flat tree with parent indices; source ranges retain every source byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaEditNode {
    pub parent: Option<usize>,
    pub kind: FormulaEditKind,
    pub range: FormulaSourceRange,
    pub content: FormulaSourceRange,
    pub slot: Option<FormulaSlot>,
    /// Whether the finite structure has its expected closing token/arguments.
    /// This is not a complete LaTeX validity check.
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaEditDiagnostic {
    pub code: String,
    pub range: FormulaSourceRange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaCommandPrefix {
    /// The full command token, so completion in its middle can replace it once.
    pub replace: FormulaSourceRange,
    pub prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaCellContext {
    pub environment: String,
    /// Zero-based indices, local to the nearest cell-bearing environment.
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaCompletionContext {
    pub schema_version: u16,
    pub source_sha256: String,
    #[serde(with = "wire::revision_decimal")]
    pub revision: u64,
    pub cursor: usize,
    pub slot: FormulaSlot,
    /// Root-to-slot node indices; clients need not infer parents from source text.
    pub owner_path: Vec<usize>,
    pub active_environment: Option<String>,
    pub cell: Option<FormulaCellContext>,
    pub in_comment: bool,
    pub in_text: bool,
    pub command_prefix: Option<FormulaCommandPrefix>,
    pub nodes: Vec<FormulaEditNode>,
    pub diagnostics: Vec<FormulaEditDiagnostic>,
}

impl FormulaCompletionContext {
    /// A concise stale-response token, not an authorization credential.
    pub fn snapshot(&self) -> FormulaCompletionSnapshot {
        FormulaCompletionSnapshot {
            schema_version: self.schema_version,
            source_sha256: self.source_sha256.clone(),
            revision: self.revision,
            cursor: self.cursor,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FormulaCompletionError {
    InvalidCursor,
    LimitExceeded,
    StaleContext,
    UnknownCandidate,
}

impl std::fmt::Display for FormulaCompletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Formula completion {self:?}")
    }
}
impl std::error::Error for FormulaCompletionError {}
type EditResult<T> = Result<T, FormulaCompletionError>;

struct EditingParser<'a> {
    source: &'a str,
    signatures: &'a [CommandSignature],
    pos: usize,
    nodes: Vec<FormulaEditNode>,
    diagnostics: Vec<FormulaEditDiagnostic>,
}

struct CommandSignature {
    command: String,
    optional: Option<FormulaSlot>,
    slots: Vec<FormulaSlot>,
}

impl EditingParser<'_> {
    fn ch(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn add(
        &mut self,
        parent: Option<usize>,
        kind: FormulaEditKind,
        slot: Option<FormulaSlot>,
        start: usize,
    ) -> EditResult<usize> {
        if self.nodes.len() >= MAX_NODES {
            return Err(FormulaCompletionError::LimitExceeded);
        }
        let id = self.nodes.len();
        self.nodes.push(FormulaEditNode {
            parent,
            kind,
            slot,
            range: FormulaSourceRange { start, end: start },
            content: FormulaSourceRange { start, end: start },
            closed: true,
        });
        Ok(id)
    }

    fn finish(&mut self, id: usize, end: usize, closed: bool) {
        self.nodes[id].range.end = self.pos;
        self.nodes[id].content.end = end;
        self.nodes[id].closed = closed;
    }

    fn diagnostic(&mut self, code: &str, range: FormulaSourceRange) {
        self.diagnostics.push(FormulaEditDiagnostic {
            code: code.into(),
            range,
        });
    }

    fn trivia(&mut self, parent: usize) -> EditResult<()> {
        while self.ch().is_some_and(char::is_whitespace) || self.ch() == Some('%') {
            self.leaf(parent)?;
        }
        Ok(())
    }

    fn leaf(&mut self, parent: usize) -> EditResult<()> {
        let start = self.pos;
        let comment = self.ch() == Some('%');
        if comment {
            while self.ch().is_some_and(|ch| !matches!(ch, '\r' | '\n')) {
                self.pos += self.ch().expect("remaining character").len_utf8();
            }
        } else {
            // A single source atom is required for unbraced command/script args.
            self.pos += self.ch().expect("remaining character").len_utf8();
        }
        let kind = if comment {
            FormulaEditKind::Comment
        } else {
            FormulaEditKind::Text
        };
        let id = self.add(Some(parent), kind, None, start)?;
        self.finish(id, self.pos, true);
        Ok(())
    }

    fn sequence(
        &mut self,
        parent: usize,
        closing: Option<char>,
        depth: usize,
        text: bool,
    ) -> EditResult<()> {
        if depth >= MAX_DEPTH {
            return Err(FormulaCompletionError::LimitExceeded);
        }
        while let Some(ch) = self.ch() {
            if closing == Some(ch) {
                break;
            }
            self.atom(parent, depth, text)?;
        }
        Ok(())
    }

    fn group(
        &mut self,
        parent: usize,
        slot: FormulaSlot,
        closing: char,
        depth: usize,
        text: bool,
    ) -> EditResult<usize> {
        let start = self.pos;
        self.pos += 1;
        let id = self.add(Some(parent), FormulaEditKind::Group, Some(slot), start)?;
        self.nodes[id].content.start = self.pos;
        self.sequence(id, Some(closing), depth + 1, text)?;
        let end = self.pos;
        let closed = self.ch() == Some(closing);
        if closed {
            self.pos += 1;
        }
        self.finish(id, end, closed);
        if !closed {
            self.diagnostic("UNCLOSED_GROUP", self.nodes[id].range);
        }
        Ok(id)
    }

    fn argument(
        &mut self,
        parent: usize,
        slot: FormulaSlot,
        depth: usize,
        text: bool,
    ) -> EditResult<usize> {
        self.trivia(parent)?;
        if self.ch() == Some('{') {
            return self.group(parent, slot, '}', depth, text);
        }
        let start = self.pos;
        let id = self.add(Some(parent), FormulaEditKind::Argument, Some(slot), start)?;
        let missing = self.ch().is_none_or(|ch| matches!(ch, '}' | '&'))
            || self.source[self.pos..].starts_with("\\\\")
            || self.end_environment().is_some();
        if missing {
            self.finish(id, start, false);
            self.diagnostic("MISSING_ARGUMENT", self.nodes[id].range);
        } else {
            // Presentation fences do not turn an unbraced TeX token argument
            // into a braced argument or consume its following source tokens.
            self.atom_token(id, depth + 1, text, false)?;
            self.finish(id, self.pos, true);
        }
        Ok(id)
    }

    fn optional_argument(
        &mut self,
        parent: usize,
        slot: FormulaSlot,
        depth: usize,
        text: bool,
    ) -> EditResult<usize> {
        let start = self.pos;
        self.pos += 1;
        let id = self.add(Some(parent), FormulaEditKind::Group, Some(slot), start)?;
        self.nodes[id].content.start = self.pos;
        let (end, closed) = commands::optional_end(self.source, self.pos);
        // A TeX bracket-delimited argument ends at the first unbraced ].
        // Bound nested parsing so a presentation fence or command cannot eat it.
        let source = self.source;
        self.source = &source[..end];
        let parsed = self.sequence(id, None, depth + 1, text);
        self.source = source;
        parsed?;
        if closed {
            self.pos += 1;
        }
        self.finish(id, end, closed);
        if !closed {
            self.diagnostic("UNCLOSED_GROUP", self.nodes[id].range);
        }
        Ok(id)
    }

    fn atom(&mut self, parent: usize, depth: usize, text: bool) -> EditResult<()> {
        self.atom_token(parent, depth, text, true)
    }

    fn atom_token(
        &mut self,
        parent: usize,
        depth: usize,
        text: bool,
        fences: bool,
    ) -> EditResult<()> {
        if depth >= MAX_DEPTH {
            return Err(FormulaCompletionError::LimitExceeded);
        }
        if !text && fences && self.fence(parent, depth)? {
            return Ok(());
        }
        match self.ch().expect("remaining source") {
            '{' => {
                self.group(
                    parent,
                    if text {
                        FormulaSlot::Text
                    } else {
                        FormulaSlot::Group
                    },
                    '}',
                    depth,
                    text,
                )?;
            }
            '\\' => self.command(parent, depth, text, false)?,
            '^' | '_' if !text => {
                let start = self.pos;
                let base = self.nodes.iter().enumerate().rev().find_map(|(id, node)| {
                    (node.parent == Some(parent)
                        && node.kind != FormulaEditKind::Comment
                        && !self.source[node.range.start..node.range.end]
                            .chars()
                            .all(char::is_whitespace))
                    .then_some(id)
                });
                let slot = if self.ch() == Some('^') {
                    FormulaSlot::Superscript
                } else {
                    FormulaSlot::Subscript
                };
                self.pos += 1;
                let id = self.add(
                    Some(parent),
                    FormulaEditKind::Script { base },
                    Some(slot),
                    start,
                )?;
                self.nodes[id].content.start = self.pos;
                if base.is_none() {
                    self.diagnostic(
                        "MISSING_SCRIPT_BASE",
                        FormulaSourceRange {
                            start,
                            end: self.pos,
                        },
                    );
                }
                let arg = self.argument(id, slot, depth, false)?;
                self.finish(id, self.pos, self.nodes[arg].closed);
            }
            '}' if !text => {
                let range = FormulaSourceRange {
                    start: self.pos,
                    end: self.pos + 1,
                };
                self.diagnostic("UNMATCHED_GROUP", range);
                self.leaf(parent)?;
            }
            _ => self.leaf(parent)?,
        }
        Ok(())
    }

    fn command_token(&mut self, parent: usize) -> EditResult<(usize, String, usize, usize)> {
        let start = self.pos;
        self.pos += 1;
        let name_start = self.pos;
        if self.ch().is_some_and(|ch| ch.is_ascii_alphabetic()) {
            while self.ch().is_some_and(|ch| ch.is_ascii_alphabetic()) {
                self.pos += 1;
            }
        } else if let Some(ch) = self.ch() {
            self.pos += ch.len_utf8();
        }
        let name = self.source[name_start..self.pos].to_string();
        let name_end = self.pos;
        let id = self.add(
            Some(parent),
            FormulaEditKind::Command { name: name.clone() },
            None,
            start,
        )?;
        self.finish(id, name_end, true);
        Ok((id, name, start, name_end))
    }

    fn command(
        &mut self,
        parent: usize,
        depth: usize,
        text: bool,
        matched_end: bool,
    ) -> EditResult<()> {
        let (id, name, start, name_end) = self.command_token(parent)?;
        if text {
            self.finish(id, name_end, true);
            return Ok(());
        }
        let builtin_slots = commands::slots(&name);
        // Copy the bounded signature so recursive parsing can mutably borrow self.
        let signature = self.signatures.iter().find(|entry| entry.command == name);
        let declared_slots = signature.map_or(builtin_slots, |entry| entry.slots.as_slice());
        let optional = signature.and_then(|entry| entry.optional);
        let count = declared_slots.len();
        let mut slots =
            [FormulaSlot::MathArgument; scoped::FORMULA_COMPLETION_MAX_DEFINITION_ARGUMENTS];
        slots[..count].copy_from_slice(declared_slots);
        let mut closed = true;
        if commands::starred(&name) {
            self.trivia(id)?;
            if self.ch() == Some('*') {
                self.pos += 1;
            }
        }
        if delimiters::single_marker(&name) {
            closed &= self.sized_marker(id, depth)?;
        }
        if let Some(slot) = optional {
            self.trivia(id)?;
            if self.ch() == Some('[') {
                let argument =
                    self.optional_argument(id, slot, depth, slot == FormulaSlot::Text)?;
                closed &= self.nodes[argument].closed;
            }
        }
        if name == "cfrac" {
            self.trivia(id)?;
            if self.ch() == Some('[') {
                let alignment =
                    self.optional_argument(id, FormulaSlot::FractionAlignment, depth, true)?;
                if !self.nodes[alignment].closed {
                    self.finish(id, name_end, false);
                    return Ok(());
                }
            }
        }
        if name == "sqrt" {
            self.trivia(id)?;
            if self.ch() == Some('[') {
                let index = self.optional_argument(id, FormulaSlot::RadicalIndex, depth, false)?;
                closed &= self.nodes[index].closed;
            }
            if closed {
                let body = self.argument(id, FormulaSlot::RadicalBody, depth, false)?;
                closed &= self.nodes[body].closed;
            }
        }
        if name == "verb" {
            if self.ch() == Some('*') {
                self.pos += 1;
            }
            let start = self.pos;
            let verb = self.add(
                Some(id),
                FormulaEditKind::Argument,
                Some(FormulaSlot::Text),
                start,
            )?;
            if let Some(delimiter) = self.ch().filter(|ch| !ch.is_whitespace()) {
                self.pos += delimiter.len_utf8();
                self.nodes[verb].content.start = self.pos;
                while self
                    .ch()
                    .is_some_and(|ch| ch != delimiter && !matches!(ch, '\r' | '\n'))
                {
                    self.pos += self.ch().expect("remaining verbatim character").len_utf8();
                }
                let end = self.pos;
                closed = self.ch() == Some(delimiter);
                if closed {
                    self.pos += delimiter.len_utf8();
                }
                self.finish(verb, end, closed);
            } else {
                self.finish(verb, self.pos, false);
                closed = false;
            }
            if !closed {
                self.diagnostic("UNCLOSED_VERBATIM", self.nodes[verb].range);
            }
        }
        let mut environment = None;
        for slot in &slots[..count] {
            if !closed {
                break;
            }
            let arg = self.argument(
                id,
                *slot,
                depth,
                matches!(slot, FormulaSlot::Text | FormulaSlot::EnvironmentName),
            )?;
            closed &= self.nodes[arg].closed;
            if name == "begin"
                && self.nodes[arg].closed
                && self.nodes[arg].kind == FormulaEditKind::Group
            {
                let content = self.nodes[arg].content;
                let value =
                    crate::latex_utils::without_comments(&self.source[content.start..content.end]);
                if !value.is_empty()
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphabetic() || byte == b'*')
                {
                    environment = Some(value.to_string());
                }
            }
            if !self.nodes[arg].closed {
                break;
            }
        }
        if name == "end" && !matched_end && closed {
            self.diagnostic(
                "UNMATCHED_ENVIRONMENT_END",
                FormulaSourceRange {
                    start,
                    end: self.pos,
                },
            );
        }
        if let Some(environment) = environment {
            closed &= self.environment(id, &environment, depth)?;
        }
        self.finish(id, name_end, closed);
        Ok(())
    }

    fn end_environment(&self) -> Option<(String, usize)> {
        self.source.get(self.pos..)?.strip_prefix("\\end")?;
        let bytes = self.source.as_bytes();
        let mut pos = self.pos + 4;
        loop {
            match bytes.get(pos) {
                Some(byte) if byte.is_ascii_whitespace() => pos += 1,
                Some(b'%') => {
                    while bytes
                        .get(pos)
                        .is_some_and(|byte| !matches!(byte, b'\r' | b'\n'))
                    {
                        pos += 1;
                    }
                }
                _ => break,
            }
        }
        if bytes.get(pos) != Some(&b'{') {
            return None;
        }
        pos += 1;
        let mut name = String::new();
        loop {
            match bytes.get(pos) {
                Some(byte) if byte.is_ascii_alphabetic() || *byte == b'*' => {
                    name.push(*byte as char);
                    pos += 1;
                }
                Some(b'%') => {
                    while bytes
                        .get(pos)
                        .is_some_and(|byte| !matches!(byte, b'\r' | b'\n'))
                    {
                        pos += 1;
                    }
                    if bytes.get(pos) == Some(&b'\r') {
                        pos += 1;
                    }
                    if bytes.get(pos) == Some(&b'\n') {
                        pos += 1;
                    }
                    while bytes
                        .get(pos)
                        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                    {
                        pos += 1;
                    }
                }
                Some(b'}') if !name.is_empty() => return Some((name, pos + 1)),
                _ => return None,
            }
        }
    }

    fn environment(&mut self, parent: usize, name: &str, depth: usize) -> EditResult<bool> {
        if depth >= MAX_DEPTH {
            return Err(FormulaCompletionError::LimitExceeded);
        }
        let id = self.add(
            Some(parent),
            FormulaEditKind::Environment { name: name.into() },
            None,
            self.pos,
        )?;
        if name == "array" {
            let columns = self.argument(id, FormulaSlot::ArrayColumnSpec, depth, true)?;
            if !self.nodes[columns].closed {
                self.finish(id, self.pos, false);
                return Ok(false);
            }
        }
        let cells = matches!(
            name,
            "matrix"
                | "pmatrix"
                | "bmatrix"
                | "Bmatrix"
                | "vmatrix"
                | "Vmatrix"
                | "smallmatrix"
                | "cases"
                | "aligned"
                | "align"
                | "align*"
                | "gathered"
                | "array"
        );
        let (mut row, mut column) = (0, 0);
        let mut cell = if cells {
            Some(self.add(
                Some(id),
                FormulaEditKind::Cell { row, column },
                Some(FormulaSlot::MatrixCell),
                self.pos,
            )?)
        } else {
            None
        };
        let closed = loop {
            if let Some((ending, next)) = self.end_environment() {
                if let Some(cell) = cell {
                    self.finish(cell, self.pos, true);
                }
                let end = self.pos;
                if ending == name {
                    self.command(id, depth + 1, false, true)?;
                    debug_assert_eq!(self.pos, next);
                    self.finish(id, end, true);
                    break true;
                }
                self.diagnostic(
                    "ENVIRONMENT_MISMATCH",
                    FormulaSourceRange {
                        start: self.pos,
                        end: next,
                    },
                );
                self.finish(id, end, false);
                break false;
            }
            if self.ch().is_none() {
                if let Some(cell) = cell {
                    self.finish(cell, self.pos, true);
                }
                self.finish(id, self.pos, false);
                self.diagnostic("UNCLOSED_ENVIRONMENT", self.nodes[id].range);
                break false;
            }
            if cells && (self.ch() == Some('&') || self.source[self.pos..].starts_with("\\\\")) {
                let previous = cell.expect("cell environment");
                self.finish(previous, self.pos, true);
                let start = self.pos;
                if self.ch() == Some('&') {
                    self.pos += 1;
                    column += 1;
                } else {
                    self.pos += 2;
                    row += 1;
                    column = 0;
                }
                let separator = self.add(Some(id), FormulaEditKind::Separator, None, start)?;
                self.finish(separator, self.pos, true);
                cell = Some(self.add(
                    Some(id),
                    FormulaEditKind::Cell { row, column },
                    Some(FormulaSlot::MatrixCell),
                    self.pos,
                )?);
            } else {
                self.atom(cell.unwrap_or(id), depth + 1, false)?;
            }
        };
        Ok(closed)
    }
}

/// Analyze a bare LaTeX formula, including unfinished groups/arguments.
///
/// No candidate is applied, no input is normalized, and no macros are executed.
/// Known finite owners are distinguished from arbitrary command/group source.
pub fn analyze_formula_context(
    source: &str,
    cursor: usize,
    revision: u64,
) -> EditResult<FormulaCompletionContext> {
    analyze_with_signatures(source, cursor, revision, &[])
}

fn analyze_with_signatures(
    source: &str,
    cursor: usize,
    revision: u64,
    signatures: &[CommandSignature],
) -> EditResult<FormulaCompletionContext> {
    if source.len() > MAX_BYTES {
        return Err(FormulaCompletionError::LimitExceeded);
    }
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return Err(FormulaCompletionError::InvalidCursor);
    }
    let mut parser = EditingParser {
        source,
        signatures,
        pos: 0,
        nodes: Vec::new(),
        diagnostics: Vec::new(),
    };
    let root = parser.add(None, FormulaEditKind::Root, Some(FormulaSlot::Formula), 0)?;
    parser.sequence(root, None, 0, false)?;
    parser.finish(root, source.len(), parser.diagnostics.is_empty());
    let mut selected = 0;
    let mut depth = 0;
    for (id, node) in parser.nodes.iter().enumerate() {
        let in_content = node.content.start <= cursor && cursor <= node.content.end;
        if node.slot.is_none()
            || (!in_content && !delimiters::owns_closing_header(source, node, cursor))
        {
            continue;
        }
        let mut node_depth = 0;
        let mut parent = node.parent;
        while let Some(id) = parent {
            node_depth += 1;
            parent = parser.nodes[id].parent;
        }
        // At one missing-argument boundary, choose the first still-empty slot.
        let old = &parser.nodes[selected];
        let old_missing = !old.closed && old.range.start == old.range.end;
        if node_depth > depth || (node_depth == depth && !old_missing) {
            selected = id;
            depth = node_depth;
        }
    }
    let mut owner_path = Vec::new();
    let mut owner = Some(selected);
    while let Some(id) = owner {
        owner_path.push(id);
        owner = parser.nodes[id].parent;
    }
    owner_path.reverse();
    let active_environment = owner_path
        .iter()
        .rev()
        .find_map(|id| match &parser.nodes[*id].kind {
            FormulaEditKind::Environment { name } => Some(name.clone()),
            _ => None,
        });
    let cell = owner_path
        .iter()
        .rev()
        .find_map(|id| match parser.nodes[*id].kind {
            FormulaEditKind::Cell { row, column } => Some(FormulaCellContext {
                environment: active_environment.clone()?,
                row,
                column,
            }),
            _ => None,
        });
    let in_text = owner_path
        .iter()
        .any(|id| parser.nodes[*id].slot == Some(FormulaSlot::Text));
    let in_comment = parser.nodes.iter().any(|node| {
        node.kind == FormulaEditKind::Comment
            && node.range.start <= cursor
            && (cursor < node.range.end || cursor == source.len() && cursor == node.range.end)
    });
    let command_prefix = if in_text || in_comment {
        None
    } else {
        parser.nodes.iter().rev().find_map(|node| match &node.kind {
            FormulaEditKind::Command { name }
                if (name.is_empty() || name.bytes().all(|byte| byte.is_ascii_alphabetic()))
                    && node.range.start < cursor
                    && cursor <= node.content.end =>
            {
                Some(FormulaCommandPrefix {
                    replace: FormulaSourceRange {
                        start: node.range.start,
                        end: node.content.end,
                    },
                    prefix: source[node.range.start + 1..cursor].into(),
                })
            }
            _ => None,
        })
    };
    Ok(FormulaCompletionContext {
        schema_version: FORMULA_COMPLETION_VERSION,
        source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
        revision,
        cursor,
        slot: parser.nodes[selected].slot.expect("root has a slot"),
        owner_path,
        active_environment,
        cell,
        in_comment,
        in_text,
        command_prefix,
        nodes: parser.nodes,
        diagnostics: parser.diagnostics,
    })
}
