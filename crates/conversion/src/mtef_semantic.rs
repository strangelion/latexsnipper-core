//! Experimental finite MTEF v5 structure reader, not registered MathType import.
//! Source bytes always survive failure. Layout/font losses are explicit; no
//! third-party activation, private-use guessing or writer is provided.

use crate::latex_ast::LatexNode;
use crate::mtef_diagnostics::{diagnose_inspection, IssueKind};
use crate::mtef_readonly::{inspect_mtef_v5, FieldValue, Inspection, Record};

pub const PROFILE_VERSION: u8 = 1;
pub const MAX_AST_NODES: usize = 8192;
pub const MAX_AST_DEPTH: usize = 64;
pub const MAX_MATRIX_CELLS: usize = 1024;
pub const MAX_LATEX_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Framing,
    UnsupportedHeader,
    Reference,
    InvalidRoot,
    InvalidSlots,
    UnsupportedRecord,
    UnsupportedCharacter,
    UnsupportedTemplate,
    LimitExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub kind: ErrorKind,
    pub offset: usize,
    pub message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LossKind {
    Typography,
    Geometry,
    Color,
    Spacing,
    MatrixLayout,
    TemplateLayout,
    EncodedPosition,
    CharacterNormalization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loss {
    pub kind: LossKind,
    pub offset: usize,
}

#[derive(Debug)]
pub struct SemanticRead<'a> {
    pub inspection: Inspection<'a>,
    /// All-or-nothing finite structure, never a partial AST after an error.
    pub ast: Option<LatexNode>,
    /// Canonical finite output with explicit command boundaries. Not original TeX.
    pub latex: Option<String>,
    pub losses: Vec<Loss>,
    pub error: Option<ReadError>,
}

/// Read repository-reviewed finite glyph/template structures. Unavailable
/// presentation is reported, not reconstructed from font names or pixels.
pub fn read_mtef_v5(source: &[u8]) -> SemanticRead<'_> {
    let inspection = inspect_mtef_v5(source);
    let mut reader = Reader {
        inspection: &inspection,
        children: vec![Vec::new(); inspection.records.len()],
        losses: Vec::new(),
        nodes: 0,
    };
    let result = reader.read();
    let losses = reader.losses;
    let (ast, latex, error) = match result {
        Ok(node) => {
            let mut latex = String::new();
            if write_latex(&node.node, &mut latex).is_err() {
                (
                    None,
                    None,
                    Some(ReadError {
                        kind: ErrorKind::LimitExceeded,
                        offset: 0,
                        message: "canonical output exceeds its byte budget",
                    }),
                )
            } else {
                (Some(node.node), Some(latex), None)
            }
        }
        Err(error) => (None, None, Some(error)),
    };
    SemanticRead {
        inspection,
        ast,
        latex,
        losses,
        error,
    }
}

type ReadResult<T> = Result<T, ReadError>;

struct Built {
    node: LatexNode,
    depth: usize,
    number: bool,
}

struct Reader<'read, 'source> {
    inspection: &'read Inspection<'source>,
    children: Vec<Vec<usize>>,
    losses: Vec<Loss>,
    nodes: usize,
}

impl Reader<'_, '_> {
    fn error(&self, index: usize, kind: ErrorKind, message: &'static str) -> ReadError {
        ReadError {
            kind,
            offset: self
                .inspection
                .records
                .get(index)
                .map_or(self.inspection.consumed, |record| record.span.start),
            message,
        }
    }

    fn loss(&mut self, index: usize, kind: LossKind) {
        self.losses.push(Loss {
            kind,
            offset: self.inspection.records[index].span.start,
        });
    }

    fn make(
        &mut self,
        index: usize,
        node: LatexNode,
        depth: usize,
        number: bool,
    ) -> ReadResult<Built> {
        self.nodes += 1;
        if self.nodes > MAX_AST_NODES || depth > MAX_AST_DEPTH {
            return Err(self.error(
                index,
                ErrorKind::LimitExceeded,
                "AST work or nesting budget exceeded",
            ));
        }
        Ok(Built {
            node,
            depth,
            number,
        })
    }

    fn read(&mut self) -> ReadResult<Built> {
        if !self.inspection.complete {
            return Err(self.error(
                usize::MAX,
                ErrorKind::Framing,
                "raw v5 framing is incomplete",
            ));
        }
        if self.inspection.header.as_ref().is_none_or(|header| {
            header.platform > 1 || header.product > 1 || header.product_version < 4
        }) {
            return Err(ReadError {
                kind: ErrorKind::UnsupportedHeader,
                offset: 0,
                message: "unsupported platform/product/version header combination",
            });
        }
        if let Some(issue) = diagnose_inspection(self.inspection)
            .into_iter()
            .find(|issue| issue.kind == IssueKind::UnresolvedReference)
        {
            return Err(ReadError {
                kind: ErrorKind::Reference,
                offset: issue.offset,
                message: "definition must precede its reference",
            });
        }
        let mut parents: Vec<usize> = Vec::new();
        let mut roots = Vec::new();
        for (index, record) in self.inspection.records.iter().enumerate() {
            parents.truncate(record.depth);
            if let Some(&parent) = parents.last() {
                self.children[parent].push(index);
            } else {
                roots.push(index);
            }
            parents.push(index);
        }
        // Even default mathematical styles require renderer/font decisions.
        self.losses.push(Loss {
            kind: LossKind::Typography,
            offset: self
                .inspection
                .header
                .as_ref()
                .map_or(0, |header| header.span.end),
        });
        let mut structures = Vec::new();
        for index in roots {
            if !self.metadata(index) {
                structures.push(index);
            }
        }
        if structures.len() != 1
            || !matches!(self.inspection.records[structures[0]].record_type, 1 | 4)
        {
            return Err(self.error(
                structures.first().copied().unwrap_or(usize::MAX),
                ErrorKind::InvalidRoot,
                "one top-level LINE or PILE is required",
            ));
        }
        let result = self.node(structures[0])?;
        if !has_content(&result.node) {
            return Err(self.error(
                structures[0],
                ErrorKind::InvalidRoot,
                "equation has no displayed content",
            ));
        }
        Ok(result)
    }

    fn metadata(&mut self, index: usize) -> bool {
        let kind = match self.inspection.records[index].record_type {
            0 => return true,
            7 => LossKind::Spacing,
            8..=14 | 17..=19 => LossKind::Typography,
            15..=16 => LossKind::Color,
            _ => return false,
        };
        self.loss(index, kind);
        true
    }

    fn options(&mut self, index: usize) {
        let options = unsigned(&self.inspection.records[index], "options").unwrap_or(0);
        if options & 8 != 0 {
            self.loss(index, LossKind::Geometry);
        }
        if self.inspection.records[index].record_type == 1 && options & 4 != 0 {
            self.loss(index, LossKind::Spacing);
        }
    }

    fn sequence(&mut self, index: usize) -> ReadResult<Built> {
        let mut nodes: Vec<Built> = Vec::new();
        for child in self.children[index].clone() {
            if self.metadata(child) {
                continue;
            }
            let record = &self.inspection.records[child];
            if record.record_type == 3 && matches!(unsigned(record, "selector"), Some(27..=29)) {
                let base = nodes.pop().ok_or_else(|| {
                    self.error(
                        child,
                        ErrorKind::InvalidSlots,
                        "postscript has no preceding base",
                    )
                })?;
                nodes.push(self.script(child, base)?);
            } else {
                if record.record_type == 1 {
                    return Err(self.error(
                        child,
                        ErrorKind::InvalidSlots,
                        "nested LINE is not a line-content object",
                    ));
                }
                let built = self.node(child)?;
                if built.number && nodes.last().is_some_and(|previous| previous.number) {
                    if let (LatexNode::Text(previous), LatexNode::Text(next)) =
                        (&mut nodes.last_mut().unwrap().node, &built.node)
                    {
                        previous.push_str(next);
                        if previous.matches('.').count() > 1 {
                            return Err(self.error(
                                child,
                                ErrorKind::UnsupportedCharacter,
                                "ambiguous decimal run",
                            ));
                        }
                        continue;
                    }
                }
                nodes.push(built);
            }
        }
        if nodes.len() == 1 {
            return Ok(nodes.pop().unwrap());
        }
        let depth = nodes.iter().map(|node| node.depth).max().unwrap_or(0) + 1;
        self.make(
            index,
            LatexNode::Sequence(nodes.into_iter().map(|node| node.node).collect()),
            depth,
            false,
        )
    }

    fn node(&mut self, index: usize) -> ReadResult<Built> {
        self.options(index);
        match self.inspection.records[index].record_type {
            1 => self.sequence(index),
            2 => self.character(index),
            3 => self.template(index),
            4 | 5 => self.matrix(index),
            _ => Err(self.error(
                index,
                ErrorKind::UnsupportedRecord,
                "record has no finite semantic mapping",
            )),
        }
    }

    fn character(&mut self, index: usize) -> ReadResult<Built> {
        let record = &self.inspection.records[index];
        if unsigned(record, "options").unwrap_or(0) & 3 != 0 {
            return Err(self.error(
                index,
                ErrorKind::UnsupportedCharacter,
                "function starts and embellishments require a separate mapping",
            ));
        }
        let typeface = match field(record, "typeface") {
            Some(FieldValue::Signed(value)) => *value,
            _ => 0,
        };
        if !matches!(typeface, 3..=6 | 8) {
            return Err(self.error(
                index,
                ErrorKind::UnsupportedCharacter,
                "typeface is outside the finite mathematical profile",
            ));
        }
        let code = unsigned(record, "mtcode").ok_or_else(|| {
            self.error(
                index,
                ErrorKind::UnsupportedCharacter,
                "encoded-only character has no mapped MTCode",
            )
        })?;
        let node = glyph(code).ok_or_else(|| {
            self.error(
                index,
                ErrorKind::UnsupportedCharacter,
                "MTCode is outside the explicit glyph inventory",
            )
        })?;
        let number = typeface == 8
            && matches!(&node, LatexNode::Text(text) if text.chars().all(|ch| ch.is_ascii_digit() || ch == '.'));
        if field(record, "font_position").is_some() {
            self.loss(index, LossKind::EncodedPosition);
        }
        if code == 0x2212 {
            self.loss(index, LossKind::CharacterNormalization);
        }
        if code == 0x20 {
            self.loss(index, LossKind::Spacing);
        }
        self.make(index, node, 1, number)
    }

    fn slots(&mut self, index: usize) -> ReadResult<Vec<usize>> {
        let mut slots = Vec::new();
        for child in self.children[index].clone() {
            if self.metadata(child) {
                continue;
            }
            if self.inspection.records[child].record_type != 1 {
                return Err(self.error(
                    child,
                    ErrorKind::InvalidSlots,
                    "container requires direct LINE slots",
                ));
            }
            slots.push(child);
        }
        Ok(slots)
    }

    fn null(&self, index: usize) -> bool {
        unsigned(&self.inspection.records[index], "options").unwrap_or(0) & 1 != 0
    }

    fn required(&mut self, index: usize) -> ReadResult<Built> {
        if self.null(index) {
            return Err(self.error(index, ErrorKind::InvalidSlots, "required slot is NULL"));
        }
        let result = self.node(index)?;
        if !has_content(&result.node) {
            return Err(self.error(index, ErrorKind::InvalidSlots, "required slot is empty"));
        }
        Ok(result)
    }

    fn template(&mut self, index: usize) -> ReadResult<Built> {
        let record = &self.inspection.records[index];
        let selector = unsigned(record, "selector").unwrap_or(255);
        let variation = unsigned(record, "variation").unwrap_or(255);
        if unsigned(record, "template_options") != Some(0)
            || !matches!((selector, variation), (10, 0..=1) | (11, 0..=7))
        {
            return Err(self.error(
                index,
                ErrorKind::UnsupportedTemplate,
                "unsupported template selector, variation or option",
            ));
        }
        let slots = self.slots(index)?;
        if slots.len() != 2 {
            return Err(self.error(
                index,
                ErrorKind::InvalidSlots,
                "root/fraction requires two slots",
            ));
        }
        let first = self.required(slots[0])?;
        if selector == 11 {
            let second = self.required(slots[1])?;
            if variation != 0 {
                self.loss(index, LossKind::TemplateLayout);
            }
            let depth = first.depth.max(second.depth) + 1;
            self.make(
                index,
                LatexNode::Fraction {
                    num: Box::new(first.node),
                    den: Box::new(second.node),
                },
                depth,
                false,
            )
        } else {
            let degree = if variation == 0 {
                if !self.null(slots[1]) {
                    return Err(self.error(
                        slots[1],
                        ErrorKind::InvalidSlots,
                        "square-root degree must be NULL",
                    ));
                }
                self.options(slots[1]);
                None
            } else {
                Some(self.required(slots[1])?)
            };
            let depth = first
                .depth
                .max(degree.as_ref().map_or(0, |degree| degree.depth))
                + 1;
            self.make(
                index,
                LatexNode::SquareRoot {
                    index: degree.map(|degree| Box::new(degree.node)),
                    content: Box::new(first.node),
                },
                depth,
                false,
            )
        }
    }

    fn script(&mut self, index: usize, mut base: Built) -> ReadResult<Built> {
        self.options(index);
        let record = &self.inspection.records[index];
        let selector = unsigned(record, "selector").unwrap_or(255);
        if unsigned(record, "variation") != Some(0)
            || unsigned(record, "template_options") != Some(0)
        {
            return Err(self.error(
                index,
                ErrorKind::UnsupportedTemplate,
                "prescripts or unknown script options are unsupported",
            ));
        }
        let slots = self.slots(index)?;
        if slots.len() != 2
            || self.null(slots[0]) != (selector == 28)
            || self.null(slots[1]) != (selector == 27)
        {
            return Err(self.error(
                index,
                ErrorKind::InvalidSlots,
                "script slots do not match selector",
            ));
        }
        for (slot, superscript) in [(slots[0], false), (slots[1], true)] {
            if self.null(slot) {
                self.options(slot);
                continue;
            }
            if has_script(&base.node, superscript) {
                return Err(self.error(
                    index,
                    ErrorKind::InvalidSlots,
                    "duplicate script on the same base",
                ));
            }
            let value = self.required(slot)?;
            let depth = base.depth.max(value.depth) + 1;
            let node = if superscript {
                LatexNode::Superscript {
                    base: Box::new(base.node),
                    exp: Box::new(value.node),
                }
            } else {
                LatexNode::Subscript {
                    base: Box::new(base.node),
                    sub: Box::new(value.node),
                }
            };
            base = self.make(index, node, depth, false)?;
        }
        Ok(base)
    }

    fn matrix(&mut self, index: usize) -> ReadResult<Built> {
        let record = &self.inspection.records[index];
        let pile = record.record_type == 4;
        let declared = if pile {
            None
        } else {
            Some((
                usize::from(unsigned(record, "rows").unwrap_or(0)),
                usize::from(unsigned(record, "columns").unwrap_or(0)),
            ))
        };
        if declared.is_some_and(|(rows, columns)| {
            rows == 0 || columns == 0 || rows * columns > MAX_MATRIX_CELLS
        }) {
            return Err(self.error(
                index,
                ErrorKind::LimitExceeded,
                "matrix dimensions are empty or exceed the cell budget",
            ));
        }
        let slots = self.slots(index)?;
        let (rows, columns) = declared.unwrap_or((slots.len(), 1));
        if rows == 0 || slots.len() != rows * columns {
            return Err(self.error(
                index,
                ErrorKind::InvalidSlots,
                "matrix/pile cell count does not match dimensions",
            ));
        }
        self.loss(index, LossKind::MatrixLayout);
        let mut cells = Vec::new();
        let mut depth = 0;
        for slot in slots {
            let node = self.node(slot)?;
            depth = depth.max(node.depth);
            cells.push(node.node);
        }
        let mut cells = cells.into_iter();
        let rows = (0..rows)
            .map(|_| cells.by_ref().take(columns).collect())
            .collect();
        self.make(
            index,
            LatexNode::Matrix {
                env: if pile { "aligned" } else { "matrix" }.into(),
                rows,
            },
            depth + 1,
            false,
        )
    }
}

fn field<'a>(record: &'a Record, name: &str) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| &field.value)
}
fn unsigned(record: &Record, name: &str) -> Option<u16> {
    match field(record, name) {
        Some(FieldValue::Unsigned(value)) => Some(*value),
        _ => None,
    }
}
fn has_script(node: &LatexNode, superscript: bool) -> bool {
    match node {
        LatexNode::Superscript { base, .. } => superscript || has_script(base, superscript),
        LatexNode::Subscript { base, .. } => !superscript || has_script(base, superscript),
        _ => false,
    }
}
fn has_content(node: &LatexNode) -> bool {
    match node {
        LatexNode::Text(text) => !text.trim().is_empty(),
        LatexNode::Sequence(nodes) => nodes.iter().any(has_content),
        _ => true,
    }
}

// Only called with the bounded, internally built finite AST. In particular,
// input cannot inject arbitrary command names or unescaped TeX control syntax.
fn write_latex(node: &LatexNode, output: &mut String) -> Result<(), ()> {
    let bracket = |node: &LatexNode, output: &mut String| -> Result<(), ()> {
        output.push('{');
        write_latex(node, output)?;
        output.push('}');
        Ok(())
    };
    match node {
        LatexNode::Text(text) => output.push_str(text),
        LatexNode::Sequence(nodes) => {
            for node in nodes {
                write_latex(node, output)?;
            }
        }
        LatexNode::Greek(name) | LatexNode::Command { name, .. } => {
            output.push('\\');
            output.push_str(name);
            output.push(' ');
        }
        LatexNode::Fraction { num, den } => {
            output.push_str("\\frac");
            bracket(num, output)?;
            bracket(den, output)?;
        }
        LatexNode::SquareRoot { index, content } => {
            output.push_str("\\sqrt");
            if let Some(index) = index {
                output.push('[');
                bracket(index, output)?;
                output.push(']');
            }
            bracket(content, output)?;
        }
        LatexNode::Subscript { base, sub } => {
            bracket(base, output)?;
            output.push('_');
            bracket(sub, output)?;
        }
        LatexNode::Superscript { base, exp } => {
            bracket(base, output)?;
            output.push('^');
            bracket(exp, output)?;
        }
        LatexNode::Matrix { env, rows } => {
            output.push_str("\\begin{");
            output.push_str(env);
            output.push('}');
            for (row_index, row) in rows.iter().enumerate() {
                if row_index != 0 {
                    output.push_str(" \\\\ ");
                }
                for (column, cell) in row.iter().enumerate() {
                    if column != 0 {
                        output.push_str(" & ");
                    }
                    write_latex(cell, output)?;
                }
            }
            output.push_str("\\end{");
            output.push_str(env);
            output.push('}');
        }
        _ => return Err(()),
    }
    if output.len() > MAX_LATEX_BYTES {
        return Err(());
    }
    Ok(())
}

// Explicit Unicode subset of MTCode; never decode PUA/font positions by cast.
fn glyph(code: u16) -> Option<LatexNode> {
    if code < 128 {
        let ch = char::from_u32(u32::from(code))?;
        if ch.is_ascii_alphanumeric() || " +-=/(),.[]<>!|:;".contains(ch) {
            return Some(LatexNode::Text(ch.to_string()));
        }
    }
    let greek = match code {
        0x03b1 => "alpha",
        0x03b2 => "beta",
        0x03b3 => "gamma",
        0x0393 => "Gamma",
        0x03b4 => "delta",
        0x0394 => "Delta",
        0x03b8 => "theta",
        0x0398 => "Theta",
        0x03bb => "lambda",
        0x039b => "Lambda",
        0x03bc => "mu",
        0x03c0 => "pi",
        0x03c1 => "rho",
        0x03c3 => "sigma",
        0x03c6 => "varphi",
        0x03c9 => "omega",
        0x03a9 => "Omega",
        _ => "",
    };
    if !greek.is_empty() {
        return Some(LatexNode::Greek(greek.into()));
    }
    let command = match code {
        0x00b1 => "pm",
        0x00d7 => "times",
        0x00f7 => "div",
        0x221e => "infty",
        0x2202 => "partial",
        0x2207 => "nabla",
        0x2264 => "leq",
        0x2265 => "geq",
        0x2260 => "neq",
        0x2248 => "approx",
        0x2208 => "in",
        0x2209 => "notin",
        0x2212 => return Some(LatexNode::Text("-".into())),
        _ => return None,
    };
    Some(LatexNode::Command {
        name: command.into(),
        args: Vec::new(),
    })
}
