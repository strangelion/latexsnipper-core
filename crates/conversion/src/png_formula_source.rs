//! Bounded, read-only PNG formula metadata extraction. Pixel data is never
//! inflated. A source field is a declaration, not proof of pixel equivalence.

use std::{fmt, ops::Range};

use flate2::{Decompress, FlushDecompress, Status};

use crate::formula_source_probe::{
    probe_text_metadata, LatexAnnotation, ProbeError, SourceFormat, TextMetadata, MAX_SOURCE_BYTES,
};

pub const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CHUNKS: usize = 4096;
pub const MAX_TEXT_CHUNKS: usize = 64;
pub const MAX_FORMULA_FIELDS: usize = 16;
pub const MAX_METADATA_BYTES: usize = 1024 * 1024;
const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PngSourceError {
    InputLimit,
    InvalidFraming,
    InvalidChecksum,
    UnsupportedCriticalChunk,
    ChunkLimit,
    MetadataLimit,
    InvalidText,
    InvalidCompression,
    InvalidFormula(ProbeError),
}

impl fmt::Display for PngSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PNG formula metadata inspection rejected: {self:?}")
    }
}
impl std::error::Error for PngSourceError {}

#[derive(Debug)]
pub struct PngFormulaSource {
    /// Exact keyword spelling. Matching follows our explicit field convention.
    pub keyword: String,
    /// Latin-1 transcoded to UTF-8 or decoded iTXt UTF-8, without normalization.
    pub source: String,
    pub format: SourceFormat,
    /// Entire chunk including length, type and CRC in the caller's PNG bytes.
    pub chunk_span: Range<usize>,
    pub chunk_type: [u8; 4],
    pub compressed: bool,
    pub language: String,
    pub translated_keyword: String,
    /// XML spans refer to `source`, not the compressed PNG bytes.
    pub latex_annotations: Vec<LatexAnnotation>,
}

#[derive(Debug, Default)]
pub struct PngSourceInspection {
    pub sources: Vec<PngFormulaSource>,
    pub ignored_text_chunks: usize,
    /// Different exact spellings within the same format, including MathML TeX
    /// annotations versus explicit LaTeX fields. No source is selected for use.
    pub conflicting_formats: Vec<SourceFormat>,
}

/// Extract only `latex`, `mathml` and `omml` keywords (ASCII case insensitive,
/// an application convention, not PNG-standard formula keywords). CRCs and
/// basic critical chunk framing are checked; pixels/ancillary semantics are
/// not validated. Unknown text fields are counted but never decompressed.
/// Any malformed recognized field rejects the report; retain the original PNG.
pub fn inspect_png_formula_sources(png: &[u8]) -> Result<PngSourceInspection, PngSourceError> {
    if png.len() > MAX_PNG_BYTES {
        return Err(PngSourceError::InputLimit);
    }
    if !png.starts_with(SIGNATURE) {
        return Err(PngSourceError::InvalidFraming);
    }
    let mut report = PngSourceInspection::default();
    let mut offset = 8;
    let mut chunks = 0;
    let mut text_chunks = 0;
    let mut metadata_bytes = 0;
    let mut decoded_bytes = 0;
    let mut image_data = false;
    let mut image_data_ended = false;
    let mut palette = false;
    let mut color = 0;
    let mut depth = 0;
    while offset < png.len() {
        chunks += 1;
        if chunks > MAX_CHUNKS {
            return Err(PngSourceError::ChunkLimit);
        }
        let header = png
            .get(offset..offset + 8)
            .ok_or(PngSourceError::InvalidFraming)?;
        let length = be_u32(&header[..4]) as usize;
        let end = offset
            .checked_add(12)
            .and_then(|n| n.checked_add(length))
            .filter(|&n| n <= png.len())
            .ok_or(PngSourceError::InvalidFraming)?;
        let kind: [u8; 4] = header[4..8].try_into().unwrap();
        if !kind.iter().all(u8::is_ascii_alphabetic) || !kind[2].is_ascii_uppercase() {
            return Err(PngSourceError::InvalidFraming);
        }
        let data = &png[offset + 8..end - 4];
        if crc32fast::hash(&png[offset + 4..end - 4]) != be_u32(&png[end - 4..end]) {
            return Err(PngSourceError::InvalidChecksum);
        }
        if chunks == 1 && &kind != b"IHDR" {
            return Err(PngSourceError::InvalidFraming);
        }
        if image_data && &kind != b"IDAT" {
            image_data_ended = true;
        }
        match &kind {
            b"IHDR" => {
                if chunks != 1 || !valid_header(data) {
                    return Err(PngSourceError::InvalidFraming);
                }
                depth = data[8];
                color = data[9];
            }
            b"PLTE" => {
                if palette
                    || image_data
                    || matches!(color, 0 | 4)
                    || data.is_empty()
                    || !data.len().is_multiple_of(3)
                    || data.len() > 768
                    || (color == 3 && data.len() / 3 > (1usize << depth))
                {
                    return Err(PngSourceError::InvalidFraming);
                }
                palette = true;
            }
            b"IDAT" => {
                if image_data_ended || (color == 3 && !palette) {
                    return Err(PngSourceError::InvalidFraming);
                }
                image_data = true;
            }
            b"IEND" => {
                if !image_data || !data.is_empty() || end != png.len() {
                    return Err(PngSourceError::InvalidFraming);
                }
                report.conflicting_formats = conflicts(&report.sources);
                return Ok(report);
            }
            b"tEXt" | b"zTXt" | b"iTXt" => {
                text_chunks += 1;
                metadata_bytes += data.len();
                if text_chunks > MAX_TEXT_CHUNKS || metadata_bytes > MAX_METADATA_BYTES {
                    return Err(PngSourceError::MetadataLimit);
                }
                if let Some(source) = read_text(data, kind, offset..end)? {
                    decoded_bytes += source.source.len();
                    if report.sources.len() >= MAX_FORMULA_FIELDS
                        || decoded_bytes > MAX_METADATA_BYTES
                    {
                        return Err(PngSourceError::MetadataLimit);
                    }
                    report.sources.push(source);
                } else {
                    report.ignored_text_chunks += 1;
                }
            }
            _ if kind[0].is_ascii_uppercase() => {
                return Err(PngSourceError::UnsupportedCriticalChunk)
            }
            _ => {}
        }
        offset = end;
    }
    Err(PngSourceError::InvalidFraming)
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}

fn valid_header(data: &[u8]) -> bool {
    if data.len() != 13 {
        return false;
    }
    let valid_dimensions = [be_u32(&data[..4]), be_u32(&data[4..8])]
        .iter()
        .all(|&n| n > 0 && n <= 0x7fff_ffff);
    valid_dimensions
        && data[10] == 0
        && data[11] == 0
        && data[12] <= 1
        && matches!(
            (data[9], data[8]),
            (0, 1 | 2 | 4 | 8 | 16) | (2 | 4 | 6, 8 | 16) | (3, 1 | 2 | 4 | 8)
        )
}

fn split_zero(data: &[u8]) -> Result<(&[u8], &[u8]), PngSourceError> {
    let zero = data
        .iter()
        .position(|&b| b == 0)
        .ok_or(PngSourceError::InvalidText)?;
    Ok((&data[..zero], &data[zero + 1..]))
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, PngSourceError> {
    let mut decoder = Decompress::new(true);
    let mut output = vec![0; MAX_SOURCE_BYTES + 1];
    let status = decoder
        .decompress(data, &mut output, FlushDecompress::Finish)
        .map_err(|_| PngSourceError::InvalidCompression)?;
    if decoder.total_out() > MAX_SOURCE_BYTES as u64 {
        return Err(PngSourceError::MetadataLimit);
    }
    if status != Status::StreamEnd || decoder.total_in() != data.len() as u64 {
        return Err(PngSourceError::InvalidCompression);
    }
    output.truncate(decoder.total_out() as usize);
    Ok(output)
}

fn read_text(
    data: &[u8],
    kind: [u8; 4],
    span: Range<usize>,
) -> Result<Option<PngFormulaSource>, PngSourceError> {
    let (key, mut body) = split_zero(data)?;
    if key.is_empty()
        || key.len() > 79
        || key.first() == Some(&b' ')
        || key.last() == Some(&b' ')
        || key.windows(2).any(|w| w == b"  ")
        || !key.iter().all(|&b| matches!(b, 32..=126 | 161..=255))
    {
        return Err(PngSourceError::InvalidText);
    }
    if ![b"latex".as_slice(), b"mathml", b"omml"]
        .iter()
        .any(|name| key.eq_ignore_ascii_case(name))
    {
        return Ok(None);
    }
    let mut compressed = false;
    let mut language = String::new();
    let mut translated_keyword = String::new();
    if &kind == b"zTXt" {
        if body.first() != Some(&0) {
            return Err(PngSourceError::InvalidCompression);
        }
        compressed = true;
        body = &body[1..];
    } else if &kind == b"iTXt" {
        if body.len() < 2 || body[0] > 1 || (body[0] == 1 && body[1] != 0) {
            return Err(PngSourceError::InvalidCompression);
        }
        compressed = body[0] == 1;
        let (lang, rest) = split_zero(&body[2..])?;
        if !lang.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'-') {
            return Err(PngSourceError::InvalidText);
        }
        language = latin1(lang);
        let (translated, rest) = split_zero(rest)?;
        translated_keyword = std::str::from_utf8(translated)
            .map_err(|_| PngSourceError::InvalidText)?
            .to_owned();
        body = rest;
    }
    let decoded;
    if compressed {
        decoded = inflate(body)?;
        body = &decoded;
    }
    if body.len() > MAX_SOURCE_BYTES {
        return Err(PngSourceError::MetadataLimit);
    }
    if body.contains(&0) {
        return Err(PngSourceError::InvalidText);
    }
    let source = if &kind == b"iTXt" {
        std::str::from_utf8(body)
            .map_err(|_| PngSourceError::InvalidText)?
            .to_owned()
    } else {
        latin1(body)
    };
    let keyword = latin1(key);
    let metadata =
        probe_text_metadata(&keyword, &source).map_err(PngSourceError::InvalidFormula)?;
    let (format, latex_annotations) = match metadata {
        Some(TextMetadata::Latex(_)) => (SourceFormat::Latex, Vec::new()),
        Some(TextMetadata::Xml(xml)) => (xml.format, xml.latex_annotations),
        None => unreachable!("keyword checked above"),
    };
    Ok(Some(PngFormulaSource {
        keyword,
        source,
        format,
        chunk_span: span,
        chunk_type: kind,
        compressed,
        language,
        translated_keyword,
        latex_annotations,
    }))
}

fn conflicts(sources: &[PngFormulaSource]) -> Vec<SourceFormat> {
    let mut conflicts = Vec::new();
    for format in [
        SourceFormat::Latex,
        SourceFormat::Mathml,
        SourceFormat::Omml,
    ] {
        let mut values: Vec<&str> = sources
            .iter()
            .filter(|s| s.format == format)
            .map(|s| s.source.as_str())
            .collect();
        if format == SourceFormat::Latex {
            values.extend(
                sources
                    .iter()
                    .flat_map(|s| s.latex_annotations.iter().map(|a| a.source.as_str())),
            );
        }
        if values
            .first()
            .is_some_and(|first| values.iter().any(|value| value != first))
        {
            conflicts.push(format);
        }
    }
    conflicts
}
