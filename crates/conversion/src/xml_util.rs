use quick_xml::events::BytesText;

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
