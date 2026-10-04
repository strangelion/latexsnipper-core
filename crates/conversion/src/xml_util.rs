#[cfg(feature = "native")]
use quick_xml::events::Event;
use quick_xml::events::{BytesRef, BytesText};

#[cfg(feature = "native")]
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

pub(crate) fn decode_and_unescape_text(event: &BytesText<'_>) -> Option<String> {
    let decoded = event.decode().ok()?;
    quick_xml::escape::unescape(&decoded)
        .ok()
        .map(|text| text.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_predefined_xml_entities() {
        let event = BytesText::from_escaped("x &lt; y &amp;&amp; y &gt; 0");
        assert_eq!(
            decode_and_unescape_text(&event).as_deref(),
            Some("x < y && y > 0")
        );
    }
}
