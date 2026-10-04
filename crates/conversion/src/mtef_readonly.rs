//! Experimental inspection of raw MTEF v5 bytes, not an equation converter.
//!
//! The public Wiris MTEF v5 record specification is the format reference.
//! Records are flat preorder entries; `depth` and byte spans expose nesting.
//! Even a stopped inspection borrows the entire unchanged input. `complete`
//! only means framing succeeded, not that references, MTCode, template slots,
//! typography or third-party application compatibility have been validated.
//! No container extraction, OLE activation, renderer or writer is provided.

use std::ops::Range;

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_RECORDS: usize = 4096;
pub const MAX_DEPTH: usize = 64;
pub const MAX_STRING_BYTES: usize = 4096;
pub const MAX_ARRAY_ITEMS: usize = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub platform: u8,
    pub product: u8,
    pub product_version: u8,
    pub product_subversion: u8,
    /// Raw, possibly non-UTF-8 application key, excluding its terminator.
    pub application_key: Range<usize>,
    pub inline: bool,
    pub span: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    UnsupportedVersion,
    UnsupportedRecord,
    UnsupportedOptions,
    Truncated,
    Malformed,
    LimitExceeded,
    FutureRecord,
    TrailingBytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub offset: usize,
    pub message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    pub units: u8,
    /// Decimal source spelling, not a rounded floating-point value.
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    pub font_index: u16,
    pub character_style: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabStop {
    pub kind: u8,
    pub offset: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Unsigned(u16),
    Signed(i32),
    /// Range in the unchanged source; names and opaque payloads are not decoded.
    Bytes(Range<usize>),
    Dimensions(Vec<Dimension>),
    Styles(Vec<Style>),
    Tabs(Vec<TabStop>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub value: FieldValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub record_type: u8,
    pub depth: usize,
    /// Includes nested records and their END. For incomplete records, this is
    /// only the consumed prefix, not a guess at the true record boundary.
    pub span: Range<usize>,
    pub complete: bool,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection<'a> {
    pub source: &'a [u8],
    pub header: Option<Header>,
    pub records: Vec<Record>,
    pub diagnostics: Vec<Diagnostic>,
    /// Framing only. Never use this as a semantic conversion/validity verdict.
    pub complete: bool,
    pub consumed: usize,
}

/// Inspect one raw v5 stream with fixed budgets. On unknown unframed records,
/// stop without attempting resynchronization. OLE/WMF/clipboard envelopes must
/// first be extracted by a separately validated host/container layer.
pub fn inspect_mtef_v5(source: &[u8]) -> Inspection<'_> {
    let mut parser = Parser {
        source,
        pos: 0,
        records: Vec::new(),
        diagnostics: Vec::new(),
        array_items: 0,
    };
    let mut header = None;
    let result = (|| {
        if source.len() > MAX_INPUT_BYTES {
            return Err(problem(
                DiagnosticKind::LimitExceeded,
                0,
                "input byte budget exceeded",
            ));
        }
        header = Some(parser.header()?);
        parser.list(0)?;
        if parser.pos != source.len() {
            return Err(problem(
                DiagnosticKind::TrailingBytes,
                parser.pos,
                "bytes remain after equation END",
            ));
        }
        Ok(())
    })();
    let complete = result.is_ok();
    if let Err(error) = result {
        parser.diagnostics.push(error);
    }
    Inspection {
        source,
        header,
        records: parser.records,
        diagnostics: parser.diagnostics,
        complete,
        consumed: parser.pos,
    }
}

fn problem(kind: DiagnosticKind, offset: usize, message: &'static str) -> Diagnostic {
    Diagnostic {
        kind,
        offset,
        message,
    }
}

type ReadResult<T> = Result<T, Diagnostic>;

struct Parser<'a> {
    source: &'a [u8],
    pos: usize,
    records: Vec<Record>,
    diagnostics: Vec<Diagnostic>,
    array_items: usize,
}

impl Parser<'_> {
    fn take(&mut self, count: usize) -> ReadResult<Range<usize>> {
        let start = self.pos;
        let end = start
            .checked_add(count)
            .filter(|end| *end <= self.source.len())
            .ok_or_else(|| {
                problem(
                    DiagnosticKind::Truncated,
                    start,
                    "field exceeds remaining input",
                )
            })?;
        self.pos = end;
        Ok(start..end)
    }

    fn byte(&mut self) -> ReadResult<u8> {
        let span = self.take(1)?;
        Ok(self.source[span.start])
    }

    fn u16(&mut self) -> ReadResult<u16> {
        let span = self.take(2)?;
        Ok(u16::from_le_bytes([
            self.source[span.start],
            self.source[span.start + 1],
        ]))
    }

    fn unsigned(&mut self) -> ReadResult<u16> {
        match self.byte()? {
            255 => self.u16(),
            first => Ok(u16::from(first)),
        }
    }

    fn signed(&mut self) -> ReadResult<i32> {
        match self.byte()? {
            255 => Ok(i32::from(self.u16()?) - 32768),
            first => Ok(i32::from(first) - 128),
        }
    }

    fn string(&mut self) -> ReadResult<Range<usize>> {
        let start = self.pos;
        loop {
            let offset = self.pos;
            let byte = self.byte()?;
            if byte == 0 {
                return Ok(start..offset);
            }
            if self.pos - start > MAX_STRING_BYTES {
                return Err(problem(
                    DiagnosticKind::LimitExceeded,
                    offset,
                    "string byte budget exceeded",
                ));
            }
        }
    }

    fn header(&mut self) -> ReadResult<Header> {
        if self.byte()? != 5 {
            return Err(problem(
                DiagnosticKind::UnsupportedVersion,
                0,
                "only raw MTEF version 5 is inspected",
            ));
        }
        let platform = self.byte()?;
        let product = self.byte()?;
        let product_version = self.byte()?;
        let product_subversion = self.byte()?;
        let application_key = self.string()?;
        let offset = self.pos;
        let options = self.byte()?;
        if options & !1 != 0 {
            return Err(problem(
                DiagnosticKind::UnsupportedOptions,
                offset,
                "unknown equation option bits",
            ));
        }
        Ok(Header {
            platform,
            product,
            product_version,
            product_subversion,
            application_key,
            inline: options & 1 != 0,
            span: 0..self.pos,
        })
    }

    fn items(&mut self, count: usize) -> ReadResult<()> {
        if count > MAX_ARRAY_ITEMS - self.array_items {
            return Err(problem(
                DiagnosticKind::LimitExceeded,
                self.pos,
                "aggregate array item budget exceeded",
            ));
        }
        self.array_items += count;
        Ok(())
    }

    fn field(&mut self, index: usize, name: &'static str, value: FieldValue) {
        self.records[index].fields.push(Field { name, value });
    }

    fn byte_field(&mut self, index: usize, name: &'static str) -> ReadResult<u8> {
        let value = self.byte()?;
        self.field(index, name, FieldValue::Unsigned(u16::from(value)));
        Ok(value)
    }

    fn unsigned_field(&mut self, index: usize, name: &'static str) -> ReadResult<()> {
        let value = self.unsigned()?;
        self.field(index, name, FieldValue::Unsigned(value));
        Ok(())
    }

    fn string_field(&mut self, index: usize, name: &'static str) -> ReadResult<()> {
        let span = self.string()?;
        self.field(index, name, FieldValue::Bytes(span));
        Ok(())
    }

    fn options(&mut self, index: usize, allowed: u8) -> ReadResult<u8> {
        let offset = self.pos;
        let value = self.byte_field(index, "options")?;
        if value & !allowed != 0 {
            return Err(problem(
                DiagnosticKind::UnsupportedOptions,
                offset,
                "unknown record option bits",
            ));
        }
        Ok(value)
    }

    fn nudge(&mut self, index: usize, options: u8) -> ReadResult<()> {
        if options & 8 == 0 {
            return Ok(());
        }
        let x = self.byte()?;
        let y = self.byte()?;
        let (x, y) = if x == 128 && y == 128 {
            (i32::from(self.u16()? as i16), i32::from(self.u16()? as i16))
        } else {
            (i32::from(x) - 128, i32::from(y) - 128)
        };
        self.field(index, "nudge_x", FieldValue::Signed(x));
        self.field(index, "nudge_y", FieldValue::Signed(y));
        Ok(())
    }

    fn ruler(&mut self, depth: usize) -> ReadResult<()> {
        if self.source.get(self.pos) != Some(&7) {
            return Err(problem(
                DiagnosticKind::Malformed,
                self.pos,
                "ruler option requires a RULER record",
            ));
        }
        self.record(depth)?;
        Ok(())
    }

    fn list(&mut self, depth: usize) -> ReadResult<()> {
        loop {
            if self.record(depth)? == 0 {
                return Ok(());
            }
        }
    }

    fn record(&mut self, depth: usize) -> ReadResult<u8> {
        let start = self.pos;
        if depth >= MAX_DEPTH || self.records.len() >= MAX_RECORDS {
            return Err(problem(
                DiagnosticKind::LimitExceeded,
                start,
                "record count or nesting budget exceeded",
            ));
        }
        let tag = self.byte()?;
        let index = self.records.len();
        self.records.push(Record {
            record_type: tag,
            depth,
            span: start..self.pos,
            complete: false,
            fields: Vec::new(),
        });
        let result = self.record_body(index, tag, depth);
        self.records[index].span.end = self.pos;
        self.records[index].complete = result.is_ok();
        result.map(|()| tag)
    }

    fn record_body(&mut self, index: usize, tag: u8, depth: usize) -> ReadResult<()> {
        match tag {
            0 => {}
            1 => {
                let options = self.options(index, 0x0f)?;
                self.nudge(index, options)?;
                if options & 4 != 0 {
                    let value = self.u16()?;
                    self.field(index, "line_spacing", FieldValue::Unsigned(value));
                }
                if options & 2 != 0 {
                    self.ruler(depth + 1)?;
                }
                if options & 1 == 0 {
                    self.list(depth + 1)?;
                }
            }
            2 => {
                let options = self.options(index, 0x3f)?;
                if options & 0x14 == 0x14 {
                    return Err(problem(
                        DiagnosticKind::Malformed,
                        self.pos - 1,
                        "8-bit and 16-bit font positions are exclusive",
                    ));
                }
                self.nudge(index, options)?;
                let value = self.signed()?;
                self.field(index, "typeface", FieldValue::Signed(value));
                if options & 0x20 == 0 {
                    let value = self.u16()?;
                    self.field(index, "mtcode", FieldValue::Unsigned(value));
                }
                if options & 4 != 0 {
                    self.byte_field(index, "font_position")?;
                }
                if options & 0x10 != 0 {
                    let value = self.u16()?;
                    self.field(index, "font_position", FieldValue::Unsigned(value));
                }
                if options & 1 != 0 {
                    self.list(depth + 1)?;
                }
            }
            3 => {
                let options = self.options(index, 8)?;
                self.nudge(index, options)?;
                self.byte_field(index, "selector")?;
                let first = self.byte()?;
                let variation = if first & 0x80 != 0 {
                    u16::from(first & 0x7f) | (u16::from(self.byte()?) << 8)
                } else {
                    u16::from(first)
                };
                self.field(index, "variation", FieldValue::Unsigned(variation));
                self.byte_field(index, "template_options")?;
                self.list(depth + 1)?;
            }
            4 => {
                let options = self.options(index, 0x0a)?;
                self.nudge(index, options)?;
                self.byte_field(index, "horizontal_alignment")?;
                self.byte_field(index, "vertical_alignment")?;
                if options & 2 != 0 {
                    self.ruler(depth + 1)?;
                }
                self.list(depth + 1)?;
            }
            5 => {
                let options = self.options(index, 8)?;
                self.nudge(index, options)?;
                self.byte_field(index, "vertical_alignment")?;
                self.byte_field(index, "column_horizontal_alignment")?;
                self.byte_field(index, "column_vertical_alignment")?;
                let rows = self.byte_field(index, "rows")?;
                let cols = self.byte_field(index, "columns")?;
                for (name, count) in [("row_partitions", rows), ("column_partitions", cols)] {
                    let span = self.take((usize::from(count) + 1).div_ceil(4))?;
                    self.field(index, name, FieldValue::Bytes(span));
                }
                self.list(depth + 1)?;
            }
            6 => {
                let options = self.options(index, 8)?;
                self.nudge(index, options)?;
                self.byte_field(index, "embellishment")?;
            }
            7 => {
                let count = usize::from(self.byte()?);
                self.items(count)?;
                let mut tabs = Vec::with_capacity(count);
                for _ in 0..count {
                    tabs.push(TabStop {
                        kind: self.byte()?,
                        offset: self.u16()?,
                    });
                }
                self.field(index, "tab_stops", FieldValue::Tabs(tabs));
            }
            8 => {
                self.unsigned_field(index, "font_index")?;
                self.byte_field(index, "character_style")?;
            }
            9 => match self.byte()? {
                101 => {
                    let value = i32::from(self.u16()? as i16);
                    self.field(index, "negative_point_size", FieldValue::Signed(value));
                }
                100 => {
                    self.byte_field(index, "logical_size")?;
                    let value = i32::from(self.u16()? as i16);
                    self.field(index, "size_delta", FieldValue::Signed(value));
                }
                logical_size => {
                    self.field(
                        index,
                        "logical_size",
                        FieldValue::Unsigned(u16::from(logical_size)),
                    );
                    let value = i32::from(self.byte()?) - 128;
                    self.field(index, "size_delta", FieldValue::Signed(value));
                }
            },
            10..=14 => {
                self.field(
                    index,
                    "logical_size",
                    FieldValue::Unsigned(u16::from(tag - 10)),
                );
            }
            15 => {
                self.unsigned_field(index, "color_index")?;
            }
            16 => {
                let options = self.options(index, 7)?;
                let names: &[&str] = if options & 1 == 0 {
                    &["red", "green", "blue"]
                } else {
                    &["cyan", "magenta", "yellow", "black"]
                };
                for name in names {
                    let value = self.u16()?;
                    if value > 1000 {
                        return Err(problem(
                            DiagnosticKind::Malformed,
                            self.pos - 2,
                            "color component is outside 0..1000",
                        ));
                    }
                    self.field(index, name, FieldValue::Unsigned(value));
                }
                if options & 4 != 0 {
                    self.string_field(index, "color_name")?;
                }
            }
            17 => {
                self.unsigned_field(index, "encoding_index")?;
                self.string_field(index, "font_name")?;
            }
            18 => {
                self.options(index, 0)?;
                for name in ["sizes", "spacing"] {
                    let values = self.dimensions()?;
                    self.field(index, name, FieldValue::Dimensions(values));
                }
                let count = usize::from(self.byte()?);
                self.items(count)?;
                let mut styles = Vec::with_capacity(count);
                for _ in 0..count {
                    let font_index = self.unsigned()?;
                    let character_style = if font_index == 0 {
                        None
                    } else {
                        Some(self.byte()?)
                    };
                    styles.push(Style {
                        font_index,
                        character_style,
                    });
                }
                self.field(index, "styles", FieldValue::Styles(styles));
            }
            19 => {
                self.string_field(index, "encoding_name")?;
            }
            100..=255 => {
                let count = usize::from(self.unsigned()?);
                let span = self.take(count)?;
                self.field(index, "opaque_payload", FieldValue::Bytes(span));
                self.diagnostics.push(problem(
                    DiagnosticKind::FutureRecord,
                    self.records[index].span.start,
                    "length-framed future record retained, semantics unknown",
                ));
            }
            _ => {
                return Err(problem(
                    DiagnosticKind::UnsupportedRecord,
                    self.records[index].span.start,
                    "unknown unframed record; no resynchronization attempted",
                ))
            }
        }
        Ok(())
    }

    fn dimensions(&mut self) -> ReadResult<Vec<Dimension>> {
        let count = usize::from(self.byte()?);
        self.items(count)?;
        let start = self.pos;
        let mut nibble_index = 0;
        let mut dimensions = Vec::with_capacity(count);
        for _ in 0..count {
            let offset = start + nibble_index / 2;
            let units = self.nibble(start, &mut nibble_index)?;
            if units > 4 {
                return Err(problem(
                    DiagnosticKind::Malformed,
                    offset,
                    "invalid dimension units nibble",
                ));
            }
            let mut value = String::new();
            loop {
                let offset = start + nibble_index / 2;
                let nibble = self.nibble(start, &mut nibble_index)?;
                match nibble {
                    0..=9 => value.push(char::from(b'0' + nibble)),
                    10 => value.push('.'),
                    11 => value.push('-'),
                    15 => break,
                    _ => {
                        return Err(problem(
                            DiagnosticKind::Malformed,
                            offset,
                            "invalid dimension value nibble",
                        ))
                    }
                }
                if value.len() > MAX_STRING_BYTES {
                    return Err(problem(
                        DiagnosticKind::LimitExceeded,
                        offset,
                        "dimension spelling budget exceeded",
                    ));
                }
            }
            dimensions.push(Dimension { units, value });
        }
        if !nibble_index.is_multiple_of(2) {
            let offset = start + nibble_index / 2;
            if self.nibble(start, &mut nibble_index)? != 0 {
                return Err(problem(
                    DiagnosticKind::Malformed,
                    offset,
                    "nonzero dimension array padding nibble",
                ));
            }
        }
        Ok(dimensions)
    }

    fn nibble(&mut self, start: usize, index: &mut usize) -> ReadResult<u8> {
        let byte_index = start + *index / 2;
        let byte = *self.source.get(byte_index).ok_or_else(|| {
            problem(
                DiagnosticKind::Truncated,
                byte_index,
                "unterminated dimension array",
            )
        })?;
        self.pos = byte_index + 1;
        let result = if (*index).is_multiple_of(2) {
            byte >> 4
        } else {
            byte & 15
        };
        *index += 1;
        Ok(result)
    }
}
