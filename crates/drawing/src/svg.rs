use std::io::Cursor;

use quick_xml::events::Event;
use quick_xml::{Reader, Writer, XmlVersion};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{DrawingSecurityError, DrawingSecurityPolicy};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvgSanitizerReport {
    pub canonical_svg: String,
    pub canonical_sha256: String,
    pub view_box: String,
    pub element_count: usize,
    pub path_command_count: usize,
    pub external_references: usize,
    pub scripts: usize,
    pub event_attributes: usize,
    pub foreign_objects: usize,
}

pub fn sanitize_svg(
    source: &str,
    policy: &DrawingSecurityPolicy,
) -> Result<SvgSanitizerReport, DrawingSecurityError> {
    if source.len() as u64 > policy.max_source_bytes {
        return Err(DrawingSecurityError::SvgComplexityLimit(format!(
            "SVG source has {} bytes",
            source.len()
        )));
    }
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut elements = 0usize;
    let mut path_commands = 0usize;
    let mut view_box = None;
    loop {
        let event = reader.read_event().map_err(|error| {
            DrawingSecurityError::SvgExternalReferenceForbidden(error.to_string())
        })?;
        match &event {
            Event::Start(start) | Event::Empty(start) => {
                elements = elements.saturating_add(1);
                if elements > policy.max_svg_elements {
                    return Err(DrawingSecurityError::SvgComplexityLimit(format!(
                        "SVG contains more than {} elements",
                        policy.max_svg_elements
                    )));
                }
                let name = String::from_utf8_lossy(start.name().as_ref()).to_ascii_lowercase();
                if name.ends_with("script") || name == "script" {
                    return Err(DrawingSecurityError::SvgScriptForbidden(
                        "script elements are forbidden".to_owned(),
                    ));
                }
                if name.ends_with("foreignobject") || name == "foreignobject" {
                    return Err(DrawingSecurityError::SvgExternalReferenceForbidden(
                        "foreignObject is forbidden by the strict SVG policy".to_owned(),
                    ));
                }
                for attribute in start.attributes().with_checks(true) {
                    let attribute = attribute.map_err(|error| {
                        DrawingSecurityError::SvgExternalReferenceForbidden(error.to_string())
                    })?;
                    let key = String::from_utf8_lossy(attribute.key.as_ref()).to_ascii_lowercase();
                    let value = attribute
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|error| {
                            DrawingSecurityError::SvgExternalReferenceForbidden(error.to_string())
                        })?
                        .into_owned();
                    if key.starts_with("on") {
                        return Err(DrawingSecurityError::SvgScriptForbidden(format!(
                            "event attribute '{key}' is forbidden"
                        )));
                    }
                    if is_reference_attribute(&key) && !safe_reference(&value) {
                        return Err(DrawingSecurityError::SvgExternalReferenceForbidden(
                            format!("attribute '{key}' references '{value}'"),
                        ));
                    }
                    let lower_value = value.to_ascii_lowercase();
                    if (lower_value.contains("url(") && !only_local_fragment_urls(&lower_value))
                        || lower_value.contains("@import")
                        || lower_value.contains("javascript:")
                    {
                        return Err(DrawingSecurityError::SvgExternalReferenceForbidden(
                            format!("attribute '{key}' contains an unsafe URL"),
                        ));
                    }
                    if name == "path" && key == "d" {
                        path_commands = path_commands.saturating_add(
                            value
                                .bytes()
                                .filter(|byte| {
                                    matches!(
                                        byte.to_ascii_lowercase(),
                                        b'm' | b'l'
                                            | b'h'
                                            | b'v'
                                            | b'c'
                                            | b's'
                                            | b'q'
                                            | b't'
                                            | b'a'
                                            | b'z'
                                    )
                                })
                                .count(),
                        );
                        if path_commands > policy.max_svg_path_commands {
                            return Err(DrawingSecurityError::SvgComplexityLimit(format!(
                                "SVG path commands exceed {}",
                                policy.max_svg_path_commands
                            )));
                        }
                    }
                    if name == "svg" && key == "viewbox" {
                        validate_view_box(&value)?;
                        view_box = Some(value);
                    }
                }
            }
            Event::DocType(_) | Event::PI(_) => {
                return Err(DrawingSecurityError::SvgExternalReferenceForbidden(
                    "DOCTYPE and processing instructions are forbidden".to_owned(),
                ));
            }
            Event::Eof => break,
            _ => {}
        }
        writer.write_event(event.into_owned()).map_err(|error| {
            DrawingSecurityError::SvgExternalReferenceForbidden(error.to_string())
        })?;
    }
    let view_box = view_box.ok_or_else(|| {
        DrawingSecurityError::SvgComplexityLimit("root SVG must declare a valid viewBox".to_owned())
    })?;
    let bytes = writer.into_inner().into_inner();
    if bytes.len() as u64 > policy.max_output_bytes {
        return Err(DrawingSecurityError::SvgComplexityLimit(format!(
            "canonical SVG exceeds {} bytes",
            policy.max_output_bytes
        )));
    }
    let canonical_svg = String::from_utf8(bytes)
        .map_err(|error| DrawingSecurityError::SvgExternalReferenceForbidden(error.to_string()))?;
    Ok(SvgSanitizerReport {
        canonical_sha256: format!("{:x}", Sha256::digest(canonical_svg.as_bytes())),
        canonical_svg,
        view_box,
        element_count: elements,
        path_command_count: path_commands,
        external_references: 0,
        scripts: 0,
        event_attributes: 0,
        foreign_objects: 0,
    })
}

fn is_reference_attribute(key: &str) -> bool {
    matches!(key, "href" | "xlink:href" | "src")
}

fn safe_reference(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.starts_with('#') || value.starts_with("data:image/png;base64,")
}

fn only_local_fragment_urls(value: &str) -> bool {
    let mut remaining = value;
    while let Some(index) = remaining.find("url(") {
        remaining = &remaining[index + 4..];
        let Some(end) = remaining.find(')') else {
            return false;
        };
        let target = remaining[..end].trim_matches([' ', '\'', '"']);
        if !target.starts_with('#') {
            return false;
        }
        remaining = &remaining[end + 1..];
    }
    true
}

fn validate_view_box(value: &str) -> Result<(), DrawingSecurityError> {
    let numbers = value
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| {
            DrawingSecurityError::SvgComplexityLimit("viewBox is not numeric".to_owned())
        })?;
    if numbers.len() != 4
        || numbers.iter().any(|number| !number.is_finite())
        || numbers[2] <= 0.0
        || numbers[3] <= 0.0
    {
        return Err(DrawingSecurityError::SvgComplexityLimit(
            "viewBox must contain four finite values with positive width and height".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_svg_accepts_local_fragments_and_has_stable_hash() {
        let source = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><defs><clipPath id="c"><rect width="10" height="10"/></clipPath></defs><path clip-path="url(#c)" d="M0 0L10 10"/></svg>"#;
        let first = sanitize_svg(source, &DrawingSecurityPolicy::default()).unwrap();
        let second = sanitize_svg(source, &DrawingSecurityPolicy::default()).unwrap();
        assert_eq!(first.canonical_sha256, second.canonical_sha256);
        assert_eq!(first.view_box, "0 0 10 10");
    }

    #[test]
    fn hostile_svg_features_fail_closed() {
        let fixtures = [
            (
                r#"<svg viewBox="0 0 1 1"><script>alert(1)</script></svg>"#,
                "DRAWING_SVG_SCRIPT_FORBIDDEN",
            ),
            (
                r#"<svg viewBox="0 0 1 1"><image href="https://example.invalid/a.png"/></svg>"#,
                "DRAWING_SVG_EXTERNAL_REFERENCE_FORBIDDEN",
            ),
            (
                r#"<svg viewBox="0 0 1 1"><rect onclick="x()"/></svg>"#,
                "DRAWING_SVG_SCRIPT_FORBIDDEN",
            ),
            (
                r#"<svg viewBox="0 0 1 1"><foreignObject/></svg>"#,
                "DRAWING_SVG_EXTERNAL_REFERENCE_FORBIDDEN",
            ),
            (
                r#"<!DOCTYPE svg SYSTEM "file:///etc/passwd"><svg viewBox="0 0 1 1"/>"#,
                "DRAWING_SVG_EXTERNAL_REFERENCE_FORBIDDEN",
            ),
        ];
        for (source, code) in fixtures {
            assert_eq!(
                sanitize_svg(source, &DrawingSecurityPolicy::default())
                    .unwrap_err()
                    .code(),
                code
            );
        }
    }

    #[test]
    fn path_bomb_and_invalid_bounds_are_rejected() {
        let policy = DrawingSecurityPolicy {
            max_svg_path_commands: 2,
            ..DrawingSecurityPolicy::default()
        };
        assert_eq!(
            sanitize_svg(
                r#"<svg viewBox="0 0 1 1"><path d="M0 0L1 1L0 1Z"/></svg>"#,
                &policy,
            )
            .unwrap_err()
            .code(),
            "DRAWING_SVG_COMPLEXITY_LIMIT"
        );
        assert!(sanitize_svg(
            r#"<svg viewBox="0 0 -1 1"/>"#,
            &DrawingSecurityPolicy::default(),
        )
        .is_err());
    }
}
