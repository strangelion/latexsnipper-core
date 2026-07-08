//! Word OOXML table writer — converts TableBlock AST into <w:tbl> XML.
//!
//! Writes Core's TableCell structure back to Word's OOXML format,
//! preserving colspan, rowspan, and cell content.

use latexsnipper_ast::{Inline, TableBlock};
#[cfg(test)]
use latexsnipper_ast::{Block, ParagraphBlock, TableCell, TableRow};

/// Write a TableBlock to Word OOXML table XML.
///
/// The output can be used with Word's Range.InsertXML via Flat OPC wrapper.
pub fn write_word_table_ooxml(table: &TableBlock) -> String {
    let mut parts = Vec::new();

    // Table properties
    parts.push(r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">"#.to_string());
    parts.push(r#"<w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders></w:tblPr>"#.to_string());
    parts.push(
        r#"<w:tblGrid><w:gridCol w:w="2500"/><w:gridCol w:w="2500"/></w:tblGrid>"#.to_string(),
    );

    for row in &table.rows {
        parts.push("<w:tr>".to_string());

        for cell in &row.cells {
            parts.push("<w:tc>".to_string());

            // Cell properties
            parts.push("<w:tcPr>".to_string());
            if cell.colspan > 1 {
                parts.push(format!(r#"<w:gridSpan w:val="{}"/>"#, cell.colspan));
            }
            if cell.rowspan > 0 {
                parts.push(r#"<w:vMerge w:val="restart"/>"#.to_string());
            }
            parts.push("</w:tcPr>".to_string());

            // Cell content
            parts.push("<w:p>".to_string());
            let cell_inlines = cell.collect_inlines();
            for inline in &cell_inlines {
                match inline {
                    Inline::Text(t) => {
                        parts.push(format!(
                            r#"<w:r><w:rPr></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                            xml_escape(&t.text)
                        ));
                    }
                    Inline::Formula(f) => {
                        // Output formula as LaTeX source text
                        let latex = f.as_latex();
                        parts.push(format!(
                            r#"<w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
                            xml_escape(latex)
                        ));
                    }
                    _ => {}
                }
            }
            parts.push("</w:p>".to_string());
            parts.push("</w:tc>".to_string());
        }

        parts.push("</w:tr>".to_string());
    }

    parts.push("</w:tbl>".to_string());
    parts.join("\n")
}

/// Escape XML special characters.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::{TableCell, TextRun};

    fn make_cell(text: &str) -> TableCell {
        TableCell {
            content: vec![Block::Paragraph(ParagraphBlock {
                inlines: vec![Inline::Text(TextRun::new(text.to_string()))],
                geometry: None,
                source: None,
                style: None,
            })],
            colspan: 1,
            rowspan: 1,
            data_type: None,
            formula: None,
            style: None,
            border_style: None,
            border_width: None,
            border_color: None,
            background: None,
            alignment: None,
            geometry: None,
            source: None,
        }
    }

    #[test]
    fn test_write_simple_table() {
        let table = TableBlock {
            rows: vec![TableRow {
                cells: vec![make_cell("A"), make_cell("B")],
                height: None,
                is_header: false,
            }],
            columns: vec![],
            caption: None,
            style: None,
            geometry: None,
            source: None,
        };

        let xml = write_word_table_ooxml(&table);
        assert!(xml.contains("<w:tbl"));
        assert!(xml.contains("<w:tr>"));
        assert!(xml.contains("A"));
        assert!(xml.contains("B"));
    }

    #[test]
    fn test_write_with_colspan() {
        let table = TableBlock {
            rows: vec![TableRow {
                cells: vec![TableCell {
                    content: vec![Block::Paragraph(ParagraphBlock {
                        inlines: vec![Inline::Text(TextRun::new("Span"))],
                        geometry: None,
                        source: None,
                        style: None,
                    })],
                    colspan: 2,
                    rowspan: 1,
                    data_type: None,
                    formula: None,
                    style: None,
                    border_style: None,
                    border_width: None,
                    border_color: None,
                    background: None,
                    alignment: None,
                    geometry: None,
                    source: None,
                }],
                height: None,
                is_header: false,
            }],
            columns: vec![],
            caption: None,
            style: None,
            geometry: None,
            source: None,
        };

        let xml = write_word_table_ooxml(&table);
        assert!(xml.contains(r#"<w:gridSpan w:val="2"/>"#));
    }
}
