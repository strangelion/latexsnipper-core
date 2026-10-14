use quick_xml::events::{BytesRef, Event};

#[derive(Clone, Copy)]
pub(crate) enum FormulaXmlKind {
    Mathml,
    Omml,
}

// Formula APIs must not extract the first equation from a larger XML document.
pub(crate) fn guard_single_math_root(xml: &str, kind: FormulaXmlKind) -> Result<(), String> {
    crate::document_converter::guard_formula_input(xml, true).map_err(|e| e.to_string())?;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut depth = 0usize;
    let mut roots = 0usize;
    let mut equations = 0usize;
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            event @ (Event::Start(_) | Event::Empty(_)) => {
                let (element, empty) = match &event {
                    Event::Start(e) => (e, false),
                    Event::Empty(e) => (e, true),
                    _ => unreachable!(),
                };
                let name = element.name();
                let local = name
                    .as_ref()
                    .rsplit(|byte| *byte == b':')
                    .next()
                    .unwrap_or_default();
                if depth == 0 {
                    roots += 1;
                    let valid = match kind {
                        FormulaXmlKind::Mathml => local == b"math",
                        FormulaXmlKind::Omml => matches!(local, b"oMath" | b"oMathPara"),
                    };
                    if !valid || roots != 1 {
                        return Err("Formula XML requires one standalone math root; retain the complete original XML".into());
                    }
                }
                if matches!(
                    (kind, local),
                    (FormulaXmlKind::Mathml, b"math") | (FormulaXmlKind::Omml, b"oMath")
                ) {
                    equations += 1;
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth
                    .checked_sub(1)
                    .ok_or("Unexpected formula XML closing element")?;
            }
            Event::Text(text) if depth == 0 => {
                if !text.decode().map_err(|e| e.to_string())?.trim().is_empty() {
                    return Err(
                        "Text outside formula XML root; retain the complete original XML".into(),
                    );
                }
            }
            Event::GeneralRef(_) | Event::CData(_) if depth == 0 => {
                return Err(
                    "Content outside formula XML root; retain the complete original XML".into(),
                );
            }
            Event::DocType(_) | Event::PI(_) => {
                return Err("Formula XML DTD/processing instructions are not supported".into())
            }
            Event::Decl(_) if roots != 0 => return Err("Misplaced formula XML declaration".into()),
            Event::Eof => break,
            _ => {}
        }
    }
    if depth != 0 || roots != 1 || equations != 1 {
        return Err(
            "Formula XML must contain exactly one equation; retain the complete original XML"
                .into(),
        );
    }
    Ok(())
}

pub(crate) fn decode_xml_content(event: &Event<'_>) -> Result<String, String> {
    match event {
        Event::Text(text) => text
            .decode()
            .map(|text| text.into_owned())
            .map_err(|error| error.to_string()),
        Event::CData(text) => text
            .decode()
            .map(|text| text.into_owned())
            .map_err(|error| error.to_string()),
        Event::GeneralRef(reference) => decode_xml_reference(reference),
        _ => Err("Expected XML text content".to_string()),
    }
}

pub(crate) fn decode_xml_reference(event: &BytesRef<'_>) -> Result<String, String> {
    if let Some(character) = event
        .resolve_char_ref()
        .map_err(|error| error.to_string())?
    {
        // Reject character references outside the XML 1.0 legal ranges.
        if !matches!(character, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
        {
            return Err("Invalid XML character reference".to_string());
        }
        return Ok(character.to_string());
    }
    let name = event.decode().map_err(|error| error.to_string())?;
    quick_xml::escape::resolve_predefined_entity(&name)
        .map(str::to_string)
        .ok_or_else(|| format!("Unsupported XML entity reference: {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_predefined_xml_entities() {
        let mut reader = quick_xml::Reader::from_str("x &lt; y &amp;&amp; y &gt; 0");
        let mut text = String::new();
        loop {
            let event = reader.read_event().unwrap();
            if matches!(event, Event::Eof) {
                break;
            }
            text.push_str(&decode_xml_content(&event).unwrap());
        }
        assert_eq!(text, "x < y && y > 0");
    }
}
