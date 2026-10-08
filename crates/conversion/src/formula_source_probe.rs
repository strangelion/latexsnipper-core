//! Experimental inspection of explicitly associated formula source metadata.
//! The caller must establish carrier/object association and retain the input.
//! No container extraction, semantic conversion, rendering or OLE activation.

use std::fmt;
use std::ops::Range;

use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use quick_xml::XmlVersion;

pub const MAX_SOURCE_BYTES: usize = 256 * 1024;
pub const MAX_XML_DEPTH: usize = 64;
pub const MAX_XML_EVENTS: usize = 8192;
pub const MAX_LATEX_ANNOTATIONS: usize = 16;
const MATHML_NS: &[u8] = b"http://www.w3.org/1998/Math/MathML";
const OMML_NS: &[u8] = b"http://schemas.openxmlformats.org/officeDocument/2006/math";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Latex,
    Mathml,
    Omml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeError {
    EmptySource,
    InputLimit,
    DepthLimit,
    EventLimit,
    AnnotationLimit,
    InvalidXml,
    ForbiddenXmlConstruct,
    UnsupportedRoot,
    UnsupportedAnnotationMarkup,
    FormatMismatch,
    InvalidText,
}

impl fmt::Display for ProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Formula source inspection rejected: {self:?}")
    }
}

impl std::error::Error for ProbeError {}

#[derive(Debug, PartialEq, Eq)]
pub struct LatexAnnotation {
    /// Decoded XML text/CDATA, retaining source whitespace and TeX spelling.
    pub source: String,
    /// Entire annotation range in the original XML, including start/end tags.
    pub xml_span: Range<usize>,
}

#[derive(Debug)]
pub struct XmlSourceInspection<'a> {
    /// Exact original XML. Namespace declarations and annotations are retained.
    pub source: &'a str,
    pub format: SourceFormat,
    pub latex_annotations: Vec<LatexAnnotation>,
    pub ignored_annotations: usize,
    /// No annotation is automatically selected when multiple spellings conflict.
    pub conflicting_latex_annotations: bool,
}

#[derive(Debug)]
pub enum TextMetadata<'a> {
    /// A declared source field, not a parsed or semantically validated formula.
    Latex(&'a str),
    Xml(XmlSourceInspection<'a>),
}

/// Inspect only the finite explicit keys `latex`, `mathml`, `omml` (ASCII case
/// insensitive). This is not a heuristic for alternative text or descriptions.
/// Container adapters must establish association/provenance before calling it.
pub fn probe_text_metadata<'a>(
    key: &str,
    value: &'a str,
) -> Result<Option<TextMetadata<'a>>, ProbeError> {
    let expected = if key.eq_ignore_ascii_case("latex") {
        SourceFormat::Latex
    } else if key.eq_ignore_ascii_case("mathml") {
        SourceFormat::Mathml
    } else if key.eq_ignore_ascii_case("omml") {
        SourceFormat::Omml
    } else {
        return Ok(None);
    };
    source_budget(value)?;
    if expected == SourceFormat::Latex {
        if value.contains('\0') {
            return Err(ProbeError::InvalidText);
        }
        return Ok(Some(TextMetadata::Latex(value)));
    }
    let report = inspect_formula_xml(value)?;
    if report.format != expected {
        return Err(ProbeError::FormatMismatch);
    }
    Ok(Some(TextMetadata::Xml(report)))
}

fn source_budget(source: &str) -> Result<(), ProbeError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ProbeError::InputLimit);
    }
    if source.trim().is_empty() {
        return Err(ProbeError::EmptySource);
    }
    Ok(())
}

pub(crate) fn xml_character(ch: char) -> bool {
    matches!(ch, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Node {
    Math,
    Semantics,
    Other,
}

struct ActiveAnnotation {
    start: usize,
    depth: usize,
    source: String,
}

fn namespace_matches(namespace: &ResolveResult<'_>, expected: &[u8]) -> bool {
    matches!(namespace, ResolveResult::Bound(value) if value.as_ref() == expected)
}

pub(crate) fn annotation_encoding(
    element: &BytesStart<'_>,
    reader: &NsReader<&[u8]>,
) -> Result<Option<String>, ProbeError> {
    let mut encoding = None;
    let mut expanded_names = std::collections::HashSet::new();
    for attr in element.attributes() {
        let attr = attr.map_err(|_| ProbeError::InvalidXml)?;
        let (namespace, local) = reader.resolver().resolve_attribute(attr.key);
        let namespace = match namespace {
            ResolveResult::Unknown(_) => return Err(ProbeError::InvalidXml),
            ResolveResult::Bound(ns) => ns.into_inner(),
            ResolveResult::Unbound => b"",
        };
        if !expanded_names.insert((namespace, local.into_inner())) {
            return Err(ProbeError::InvalidXml);
        }
        let value = attr
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
            .map_err(|_| ProbeError::InvalidXml)?;
        if value.chars().any(|ch| !xml_character(ch)) {
            return Err(ProbeError::InvalidXml);
        }
        if attr.key.as_ref() == b"encoding" {
            encoding = Some(value.into_owned());
        }
    }
    Ok(encoding)
}

/// Accept one standalone namespaced MathML `math` or OMML `oMath`/`oMathPara`.
/// Framing success is not proof that the existing converters support its math.
/// UTF-8, bounded XML only; DTD/PI and non-predefined entities are rejected.
/// MathML `math/semantics/annotation` with explicit TeX MIME encodings is decoded
/// separately; original XML is never replaced by an annotation or reconstruction.
pub fn inspect_formula_xml(source: &str) -> Result<XmlSourceInspection<'_>, ProbeError> {
    source_budget(source)?;
    if source.chars().any(|ch| !xml_character(ch)) {
        return Err(ProbeError::InvalidXml);
    }
    let mut reader = NsReader::from_str(source);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut stack = Vec::new();
    let mut format = None;
    let mut annotation: Option<ActiveAnnotation> = None;
    let mut annotations = Vec::<LatexAnnotation>::new();
    let mut ignored_annotations = 0;
    let mut recognized_annotations = 0;
    let mut declarations = 0;
    for _ in 0..MAX_XML_EVENTS {
        let start = reader.buffer_position() as usize;
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|_| ProbeError::InvalidXml)?;
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                if matches!(namespace, ResolveResult::Unknown(_)) {
                    return Err(ProbeError::InvalidXml);
                }
                let empty = matches!(event, Event::Empty(_));
                let local = element.local_name();
                let mathml = namespace_matches(&namespace, MATHML_NS);
                let omml = namespace_matches(&namespace, OMML_NS);
                let encoding = annotation_encoding(element, &reader)?;
                if stack.is_empty() {
                    if format.is_some() {
                        return Err(ProbeError::InvalidXml);
                    }
                    format = Some(if mathml && local.as_ref() == b"math" {
                        SourceFormat::Mathml
                    } else if omml && matches!(local.as_ref(), b"oMath" | b"oMathPara") {
                        SourceFormat::Omml
                    } else {
                        return Err(ProbeError::UnsupportedRoot);
                    });
                }
                if stack.len() + 1 > MAX_XML_DEPTH {
                    return Err(ProbeError::DepthLimit);
                }
                if annotation.is_some() {
                    return Err(ProbeError::UnsupportedAnnotationMarkup);
                }
                if mathml
                    && stack == [Node::Math, Node::Semantics]
                    && local.as_ref() == b"annotation"
                {
                    let supported = encoding.as_deref().is_some_and(|value| {
                        ["application/x-tex", "application/x-latex", "text/x-tex"]
                            .iter()
                            .any(|mime| value.eq_ignore_ascii_case(mime))
                    });
                    if supported {
                        if recognized_annotations >= MAX_LATEX_ANNOTATIONS {
                            return Err(ProbeError::AnnotationLimit);
                        }
                        recognized_annotations += 1;
                        if !empty {
                            annotation = Some(ActiveAnnotation {
                                start,
                                depth: stack.len() + 1,
                                source: String::new(),
                            });
                        }
                    } else {
                        ignored_annotations += 1;
                    }
                }
                if !empty {
                    let node = if stack.is_empty() && mathml {
                        Node::Math
                    } else if stack == [Node::Math] && mathml && local.as_ref() == b"semantics" {
                        Node::Semantics
                    } else {
                        Node::Other
                    };
                    stack.push(node);
                }
            }
            Event::End(_) => {
                if annotation
                    .as_ref()
                    .is_some_and(|active| active.depth == stack.len())
                {
                    let active = annotation.take().unwrap();
                    if !active.source.trim().is_empty() {
                        annotations.push(LatexAnnotation {
                            source: active.source,
                            xml_span: active.start..reader.buffer_position() as usize,
                        });
                    }
                }
                if stack.pop().is_none() {
                    return Err(ProbeError::InvalidXml);
                }
            }
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                let content = crate::xml_util::decode_xml_content(&event)
                    .map_err(|_| ProbeError::InvalidXml)?;
                if stack.is_empty()
                    && (!content
                        .chars()
                        .all(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
                        || !matches!(event, Event::Text(_)))
                {
                    return Err(ProbeError::InvalidXml);
                }
                if let Some(active) = annotation.as_mut() {
                    active.source.push_str(&content);
                }
            }
            Event::DocType(_) | Event::PI(_) => return Err(ProbeError::ForbiddenXmlConstruct),
            Event::Decl(decl) => {
                declarations += 1;
                if format.is_some()
                    || declarations > 1
                    || (start != 0 && &source[..start] != "\u{feff}")
                {
                    return Err(ProbeError::InvalidXml);
                }
                if decl.version().map_err(|_| ProbeError::InvalidXml)?.as_ref() != b"1.0" {
                    return Err(ProbeError::InvalidXml);
                }
                if let Some(encoding) = decl.encoding() {
                    if !encoding
                        .map_err(|_| ProbeError::InvalidXml)?
                        .eq_ignore_ascii_case(b"UTF-8")
                    {
                        return Err(ProbeError::InvalidXml);
                    }
                }
            }
            Event::Eof => {
                if !stack.is_empty() || annotation.is_some() {
                    return Err(ProbeError::InvalidXml);
                }
                let format = format.ok_or(ProbeError::UnsupportedRoot)?;
                let conflicting_latex_annotations = annotations.first().is_some_and(|first| {
                    annotations
                        .iter()
                        .any(|annotation| annotation.source != first.source)
                });
                return Ok(XmlSourceInspection {
                    source,
                    format,
                    latex_annotations: annotations,
                    ignored_annotations,
                    conflicting_latex_annotations,
                });
            }
            _ => {}
        }
    }
    Err(ProbeError::EventLimit)
}
