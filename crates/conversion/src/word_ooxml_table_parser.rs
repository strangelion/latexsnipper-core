//! Word OOXML table parser — converts <w:tbl> XML into TableBlock AST.
//!
//! Maps Word's complex table model to Core's simpler TableCell structure.
//! Preserves merged cells, basic formatting, and cell content.

use latexsnipper_ast::{Inline, TableBlock, TableCell, TextRun};
use quick_xml::events::Event;
use quick_xml::Reader;

/// Parse a Word OOXML table from raw XML into a TableBlock.
///
/// The input should be the raw XML string containing a <w:tbl> element.
pub fn parse_word_table_ooxml(xml: &str) -> Option<TableBlock> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut rows = Vec::new();

    // Current cell properties
    let mut current_colspan: u32 = 1;
    let mut current_rowspan: u32 = 1;
    let mut current_text = String::new();
    let mut current_row: Vec<TableCell> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match tag_name.as_str() {
                    "w:tbl" | "m:tbl" => {},
                    "w:tr" | "m:tr" => {
                        current_row.clear();
                    }
                    "w:tc" | "m:tc" => {
                        current_text.clear();
                        current_colspan = 1;
                        current_rowspan = 1;
                    }
                    // Parse cell properties for colspan/rowspan
                    "w:gridSpan" | "m:gridSpan" => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref());
                            if key.ends_with(":val") || key == "val" {
                                if let Ok(val) = String::from_utf8_lossy(&attr.value).parse::<u32>() {
                                    current_colspan = val;
                                }
                            }
                        }
                    }
                    "w:vMerge" | "m:vMerge" => {
                        for attr in e.attributes().flatten() {
                            let key = String::from_utf8_lossy(attr.key.as_ref());
                            if key == "w:val" || key == "m:val" {
                                let val = String::from_utf8_lossy(&attr.value);
                                if val == "restart" {
                                    current_rowspan = 1; // mark as merge start
                                } else if val == "continue" {
                                    current_rowspan = 0; // merge continuation — skip this cell
                                }
                            }
                        }
                    }
                    // Text content
                    "w:t" | "m:t" => {
                        let text_content = reader.read_text(e.name()).unwrap_or_default();
                        current_text.push_str(&text_content);
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

                match tag_name.as_str() {
                    "w:tbl" | "m:tbl" => {
                        break; // done
                    }
                    "w:tr" | "m:tr" => {
                        if !current_row.is_empty() {
                            rows.push(std::mem::take(&mut current_row));
                        }
                    }
                    "w:tc" | "m:tc" => {
                        if current_rowspan > 0 {
                            // Normal cell or merge start
                            let inlines = if current_text.trim().is_empty() {
                                Vec::new()
                            } else {
                                vec![Inline::Text(TextRun::new(current_text.clone()))]
                            };

                            current_row.push(TableCell {
                                inlines,
                                colspan: current_colspan,
                                rowspan: current_rowspan,
                                border_style: None,
                                border_width: None,
                                border_color: None,
                                background: None,
                                alignment: None,
                                geometry: None,
                                source: None,
                            });
                        }
                        // merge continuation (rowspan == 0) — cell is not added
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    if rows.is_empty() {
        return None;
    }

    Some(TableBlock {
        rows,
        geometry: None,
        source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_word_table() {
        let xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr>
<w:tr><w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>D</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].len(), 2);
        assert_eq!(table.rows[1].len(), 2);
    }

    #[test]
    fn test_parse_gridspan() {
        let xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Span 2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0][0].colspan, 2);
    }

    #[test]
    fn test_parse_empty_table() {
        let xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:tr><w:tc><w:p></w:p></w:tc></w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0][0].inlines.is_empty(), true);
    }
}
