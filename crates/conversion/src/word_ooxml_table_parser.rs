//! Word OOXML table parser — converts <w:tbl> XML into TableBlock AST.
//!
//! Uses regex-based parsing to extract table structure from Word's OOXML.

use latexsnipper_ast::{Inline, TableBlock, TableCell, TextRun};
use regex::Regex;

/// Parse a Word OOXML table from raw XML into a TableBlock.
pub fn parse_word_table_ooxml(xml: &str) -> Option<TableBlock> {
    // Find the table content
    let tbl = extract_between(xml, "<w:tbl", "</w:tbl>")
        .or_else(|| extract_between(xml, "<m:tbl", "</m:tbl>"))?;

    let mut rows = Vec::new();

    // Split into rows
    let row_re = Regex::new(r"(?s)<w:tr[^>]*>(.+?)</w:tr>").unwrap();
    let cell_re = Regex::new(r"(?s)<w:tc[^>]*>(.+?)</w:tc>").unwrap();
    let gridspan_re = Regex::new(r#"w:gridSpan[^>]*w:val="(\d+)""#).unwrap();
    let text_re = Regex::new(r"(?s)<w:t[^>]*>(.*?)</w:t>").unwrap();

    for cap in row_re.captures_iter(&tbl) {
        let row_content = cap.get(1).map_or("", |m| m.as_str());
        let mut current_row = Vec::new();

        for cell_cap in cell_re.captures_iter(row_content) {
            let cell_content = cell_cap.get(1).map_or("", |m| m.as_str());

            // Extract colspan
            let colspan = gridspan_re
                .captures(cell_content)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(1);

            // Extract text
            let mut text = String::new();
            for text_cap in text_re.captures_iter(cell_content) {
                if let Some(t) = text_cap.get(1) {
                    text.push_str(t.as_str());
                }
            }
            let text = text.trim().to_string();

            let inlines = if text.is_empty() {
                Vec::new()
            } else {
                vec![Inline::Text(TextRun::new(text))]
            };

            current_row.push(TableCell {
                inlines,
                colspan,
                rowspan: 1,
                border_style: None,
                border_width: None,
                border_color: None,
                background: None,
                alignment: None,
                geometry: None,
                source: None,
            });
        }

        if !current_row.is_empty() {
            rows.push(current_row);
        }
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

fn extract_between(text: &str, start: &str, end: &str) -> Option<String> {
    let s = text.find(start)?;
    let after = &text[s..];
    let e = after.find(end)?;
    Some(after[..e + end.len()].to_string())
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
<w:tr>
<w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Span 2</w:t></w:r></w:p></w:tc>
<w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
</w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].len(), 2);
        assert_eq!(table.rows[0][0].colspan, 2);
    }

    #[test]
    fn test_parse_empty_table() {
        let xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:tr><w:tc><w:p></w:p></w:tc></w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows.len(), 1);
    }
}
