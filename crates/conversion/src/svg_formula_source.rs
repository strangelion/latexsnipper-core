//! Read-only formula declarations in the top-level SVG metadata element.
//! This module does not render or sanitize SVG, fetch resources or infer text.

use crate::formula_source_probe::{
    annotation_encoding, inspect_formula_xml, probe_text_metadata, xml_character, LatexAnnotation,
    ProbeError, SourceFormat, MAX_SOURCE_BYTES,
};
use quick_xml::{
    events::{BytesStart, Event},
    name::{PrefixDeclaration, ResolveResult},
    reader::NsReader,
    Writer,
};
use std::{fmt, ops::Range};

pub const MAX_SVG_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_EVENTS: usize = 65536;
pub const MAX_DEPTH: usize = 64;
pub const MAX_FIELDS: usize = 16;
pub const MAX_METADATA_BYTES: usize = 1024 * 1024;
pub const SOURCE_NAMESPACE: &str = "urn:latexsnipper:formula-source:v1";
const SVG_NS: &[u8] = b"http://www.w3.org/2000/svg";
const MATHML_NS: &[u8] = b"http://www.w3.org/1998/Math/MathML";
const OMML_NS: &[u8] = b"http://schemas.openxmlformats.org/officeDocument/2006/math";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgSourceError {
    InputLimit,
    InvalidXml,
    UnsupportedRoot,
    ForbiddenXmlConstruct,
    DepthLimit,
    EventLimit,
    MetadataLimit,
    NamespaceLimit,
    InvalidFormula(ProbeError),
}
impl fmt::Display for SvgSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SVG formula metadata inspection rejected: {self:?}")
    }
}
impl std::error::Error for SvgSourceError {}

#[derive(Debug)]
pub struct SvgFormulaSource {
    pub format: SourceFormat,
    /// Decoded declared LaTeX, or XML made standalone by materializing inherited
    /// namespace bindings on its root. No mathematical normalization occurs.
    pub source: String,
    /// Complete original source element in the SVG, before materialization.
    pub xml_span: Range<usize>,
    pub namespace_materialized: bool,
    /// Annotation ranges refer to `source`, not the original SVG.
    pub latex_annotations: Vec<LatexAnnotation>,
}

#[derive(Debug)]
pub struct SvgSourceInspection<'a> {
    pub svg: &'a str,
    pub sources: Vec<SvgFormulaSource>,
    pub ignored_metadata_items: usize,
    pub conflicting_formats: Vec<SourceFormat>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Node {
    Svg,
    Metadata,
    Other,
}
enum Content {
    Latex(String),
    Xml {
        root: String,
        materialized: bool,
        format: SourceFormat,
    },
}
struct Capture {
    start: usize,
    content_start: usize,
    depth: usize,
    content: Content,
}

fn bound(namespace: &ResolveResult<'_>, value: &[u8]) -> bool {
    matches!(namespace, ResolveResult::Bound(ns) if ns.as_ref() == value)
}

/// Recognize only direct children of top-level SVG `metadata`: a namespaced
/// MathML `math`, OMML `oMath`/`oMathPara`, or this adapter's explicit
/// `<source format="latex">` in SOURCE_NAMESPACE. Title/description, RDF/XMP,
/// nested object metadata and arbitrary foreign text are not formula sources.
pub fn inspect_svg_formula_sources(svg: &str) -> Result<SvgSourceInspection<'_>, SvgSourceError> {
    if svg.len() > MAX_SVG_BYTES {
        return Err(SvgSourceError::InputLimit);
    }
    if svg.chars().any(|ch| !xml_character(ch)) {
        return Err(SvgSourceError::InvalidXml);
    }
    let mut reader = NsReader::from_str(svg);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut stack = Vec::new();
    let mut root_seen = false;
    let mut declaration = false;
    let mut capture: Option<Capture> = None;
    let mut sources = Vec::new();
    let mut ignored = 0;
    let mut total = 0;
    for _ in 0..MAX_EVENTS {
        let start = reader.buffer_position() as usize;
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|_| SvgSourceError::InvalidXml)?;
        let unknown = matches!(namespace, ResolveResult::Unknown(_));
        let is_svg = bound(&namespace, SVG_NS);
        let is_mathml = bound(&namespace, MATHML_NS);
        let is_omml = bound(&namespace, OMML_NS);
        let is_source = bound(&namespace, SOURCE_NAMESPACE.as_bytes());
        let end = reader.buffer_position() as usize;
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                if unknown {
                    return Err(SvgSourceError::InvalidXml);
                }
                if element.attributes().count() > 64 {
                    return Err(SvgSourceError::NamespaceLimit);
                }
                annotation_encoding(element, &reader).map_err(SvgSourceError::InvalidFormula)?;
                let empty = matches!(event, Event::Empty(_));
                let local = element.local_name();
                if stack.is_empty() {
                    if root_seen || !is_svg || local.as_ref() != b"svg" {
                        return Err(SvgSourceError::UnsupportedRoot);
                    }
                    root_seen = true;
                }
                if stack.len() + 1 > MAX_DEPTH {
                    return Err(SvgSourceError::DepthLimit);
                }
                if matches!(
                    capture.as_ref().map(|c| &c.content),
                    Some(Content::Latex(_))
                ) {
                    return Err(SvgSourceError::InvalidFormula(
                        ProbeError::UnsupportedAnnotationMarkup,
                    ));
                }
                if capture.is_none() && stack == [Node::Svg, Node::Metadata] {
                    let format = if is_mathml && local.as_ref() == b"math" {
                        Some(SourceFormat::Mathml)
                    } else if is_omml && matches!(local.as_ref(), b"oMath" | b"oMathPara") {
                        Some(SourceFormat::Omml)
                    } else {
                        None
                    };
                    let content = if let Some(format) = format {
                        let (root, materialized) = standalone_root(element, &reader, empty)?;
                        Some(Content::Xml {
                            root,
                            materialized,
                            format,
                        })
                    } else if is_source
                        && local.as_ref() == b"source"
                        && latex_format(element, &reader)?
                    {
                        Some(Content::Latex(String::new()))
                    } else {
                        ignored += 1;
                        None
                    };
                    if let Some(content) = content {
                        if sources.len() >= MAX_FIELDS {
                            return Err(SvgSourceError::MetadataLimit);
                        }
                        let active = Capture {
                            start,
                            content_start: end,
                            depth: stack.len() + 1,
                            content,
                        };
                        if empty {
                            finish(active, end, svg, &mut sources, &mut total)?;
                        } else {
                            capture = Some(active);
                        }
                    }
                }
                if !empty {
                    stack.push(if stack.is_empty() {
                        Node::Svg
                    } else if stack == [Node::Svg] && is_svg && local.as_ref() == b"metadata" {
                        Node::Metadata
                    } else {
                        Node::Other
                    });
                }
            }
            Event::End(_) => {
                if capture.as_ref().is_some_and(|c| c.depth == stack.len()) {
                    finish(capture.take().unwrap(), end, svg, &mut sources, &mut total)?;
                }
                if stack.pop().is_none() {
                    return Err(SvgSourceError::InvalidXml);
                }
            }
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) => {
                let text = crate::xml_util::decode_xml_content(&event)
                    .map_err(|_| SvgSourceError::InvalidXml)?;
                if stack.is_empty()
                    && (!matches!(event, Event::Text(_))
                        || !text.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')))
                {
                    return Err(SvgSourceError::InvalidXml);
                }
                if let Some(Capture {
                    content: Content::Latex(value),
                    ..
                }) = capture.as_mut()
                {
                    if value.len() + text.len() > MAX_SOURCE_BYTES {
                        return Err(SvgSourceError::MetadataLimit);
                    }
                    value.push_str(&text);
                }
            }
            Event::DocType(_) | Event::PI(_) => return Err(SvgSourceError::ForbiddenXmlConstruct),
            Event::Decl(decl) => {
                if root_seen
                    || declaration
                    || (start != 0 && &svg[..start] != "\u{feff}")
                    || decl
                        .version()
                        .map_err(|_| SvgSourceError::InvalidXml)?
                        .as_ref()
                        != b"1.0"
                {
                    return Err(SvgSourceError::InvalidXml);
                }
                if let Some(encoding) = decl.encoding() {
                    if !encoding
                        .map_err(|_| SvgSourceError::InvalidXml)?
                        .eq_ignore_ascii_case(b"UTF-8")
                    {
                        return Err(SvgSourceError::InvalidXml);
                    }
                }
                declaration = true;
            }
            Event::Eof => {
                if !root_seen || !stack.is_empty() || capture.is_some() {
                    return Err(SvgSourceError::InvalidXml);
                }
                let conflicting_formats = conflicts(&sources);
                return Ok(SvgSourceInspection {
                    svg,
                    sources,
                    ignored_metadata_items: ignored,
                    conflicting_formats,
                });
            }
            _ => {}
        }
    }
    Err(SvgSourceError::EventLimit)
}

fn latex_format(
    element: &BytesStart<'_>,
    reader: &NsReader<&[u8]>,
) -> Result<bool, SvgSourceError> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| SvgSourceError::InvalidXml)?;
        if attribute.key.as_ref() == b"format" {
            return Ok(attribute
                .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
                .map_err(|_| SvgSourceError::InvalidXml)?
                .as_ref()
                == "latex");
        }
    }
    Ok(false)
}

fn standalone_root(
    element: &BytesStart<'_>,
    reader: &NsReader<&[u8]>,
    empty: bool,
) -> Result<(String, bool), SvgSourceError> {
    if element.len() > MAX_SOURCE_BYTES {
        return Err(SvgSourceError::MetadataLimit);
    }
    let mut root = element.to_owned();
    let mut materialized = false;
    let existing: Vec<_> = element
        .attributes()
        .map(|a| {
            a.map(|a| a.key.as_ref().to_vec())
                .map_err(|_| SvgSourceError::InvalidXml)
        })
        .collect::<Result<_, _>>()?;
    let mut bytes = element.len();
    for (index, (prefix, ns)) in reader.resolver().bindings().enumerate() {
        if index >= 64 {
            return Err(SvgSourceError::NamespaceLimit);
        }
        let name = match prefix {
            PrefixDeclaration::Default => "xmlns".to_owned(),
            PrefixDeclaration::Named(prefix) => format!(
                "xmlns:{}",
                std::str::from_utf8(prefix).map_err(|_| SvgSourceError::InvalidXml)?
            ),
        };
        if existing.iter().any(|a| a.as_slice() == name.as_bytes()) {
            continue;
        }
        bytes += name.len() + ns.as_ref().len() + 4;
        if bytes > MAX_SOURCE_BYTES {
            return Err(SvgSourceError::MetadataLimit);
        }
        let raw = std::str::from_utf8(ns.as_ref()).map_err(|_| SvgSourceError::InvalidXml)?;
        let value = quick_xml::escape::unescape(raw).map_err(|_| SvgSourceError::InvalidXml)?;
        root.push_attribute((name.as_str(), value.as_ref()));
        materialized = true;
    }
    let mut writer = Writer::new(Vec::new());
    writer
        .write_event(if empty {
            Event::Empty(root)
        } else {
            Event::Start(root)
        })
        .map_err(|_| SvgSourceError::InvalidXml)?;
    Ok((
        String::from_utf8(writer.into_inner()).map_err(|_| SvgSourceError::InvalidXml)?,
        materialized,
    ))
}

fn finish(
    capture: Capture,
    end: usize,
    svg: &str,
    sources: &mut Vec<SvgFormulaSource>,
    total: &mut usize,
) -> Result<(), SvgSourceError> {
    if end - capture.start > MAX_SOURCE_BYTES {
        return Err(SvgSourceError::MetadataLimit);
    }
    let (source, format, materialized, latex_annotations) = match capture.content {
        Content::Latex(value) => {
            probe_text_metadata("latex", &value).map_err(SvgSourceError::InvalidFormula)?;
            (value, SourceFormat::Latex, false, Vec::new())
        }
        Content::Xml {
            mut root,
            materialized,
            format,
        } => {
            if root.len() + end - capture.content_start > MAX_SOURCE_BYTES {
                return Err(SvgSourceError::MetadataLimit);
            }
            root.push_str(&svg[capture.content_start..end]);
            let inspected = inspect_formula_xml(&root).map_err(SvgSourceError::InvalidFormula)?;
            let annotations = inspected.latex_annotations;
            (root, format, materialized, annotations)
        }
    };
    *total += source.len();
    if *total > MAX_METADATA_BYTES {
        return Err(SvgSourceError::MetadataLimit);
    }
    sources.push(SvgFormulaSource {
        source,
        format,
        xml_span: capture.start..end,
        namespace_materialized: materialized,
        latex_annotations,
    });
    Ok(())
}

fn conflicts(sources: &[SvgFormulaSource]) -> Vec<SourceFormat> {
    let mut result = Vec::new();
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
            result.push(format);
        }
    }
    result
}
