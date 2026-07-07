//! Word OOXML table parser — converts <w:tbl> XML into TableBlock AST.
//!
//! Uses regex-based parsing to extract table structure from Word's OOXML.
//! Supports horizontal merge (gridSpan/colspan) and vertical merge (vMerge/rowspan).

use latexsnipper_ast::{Inline, TableBlock, TableCell, TextRun};
use regex::Regex;

/// Intermediate cell data before rowspan resolution.
struct RawCell {
    colspan: u32,
    text: String,
    /// `true` if `<w:vMerge w:val="restart"/>` — starts a vertical merge group.
    vmerge_restart: bool,
    /// `true` if `<w:vMerge w:val="continue"/>` or bare `<w:vMerge/>` — continuation cell.
    vmerge_continue: bool,
}

/// Parse a Word OOXML table from raw XML into a TableBlock.
pub fn parse_word_table_ooxml(xml: &str) -> Option<TableBlock> {
    // Find the table content
    let tbl = extract_between(xml, "<w:tbl", "</w:tbl>")
        .or_else(|| extract_between(xml, "<m:tbl", "</m:tbl>"))?;

    let row_re = Regex::new(r"(?s)<w:tr[^>]*>(.+?)</w:tr>").unwrap();
    let cell_re = Regex::new(r"(?s)<w:tc[^>]*>(.+?)</w:tc>").unwrap();
    let gridspan_re = Regex::new(r#"w:gridSpan[^>]*w:val="(\d+)""#).unwrap();
    let text_re = Regex::new(r"(?s)<w:t[^>]*>(.*?)</w:t>").unwrap();
    // Match vMerge: restart, continue, or bare tag
    let vmerge_restart_re = Regex::new(r#"w:vMerge[^>]*w:val="restart""#).unwrap();
    let vmerge_continue_re = Regex::new(r#"w:vMerge[^>]*w:val="continue""#).unwrap();
    let vmerge_bare_re = Regex::new(r#"<w:vMerge\s*/>"#).unwrap();

    // --- First pass: parse raw cells per row ---
    let mut raw_rows: Vec<Vec<RawCell>> = Vec::new();

    for cap in row_re.captures_iter(&tbl) {
        let row_content = cap.get(1).map_or("", |m| m.as_str());
        let mut current_row = Vec::new();

        for cell_cap in cell_re.captures_iter(row_content) {
            let cell_content = cell_cap.get(1).map_or("", |m| m.as_str());

            let colspan = gridspan_re
                .captures(cell_content)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(1);

            let vmerge_restart = vmerge_restart_re.is_match(cell_content);
            let vmerge_continue =
                vmerge_continue_re.is_match(cell_content) || vmerge_bare_re.is_match(cell_content);

            let mut text = String::new();
            for text_cap in text_re.captures_iter(cell_content) {
                if let Some(t) = text_cap.get(1) {
                    text.push_str(t.as_str());
                }
            }

            current_row.push(RawCell {
                colspan,
                text: text.trim().to_string(),
                vmerge_restart,
                vmerge_continue,
            });
        }

        if !current_row.is_empty() {
            raw_rows.push(current_row);
        }
    }

    if raw_rows.is_empty() {
        return None;
    }

    // --- Second pass: resolve rowspan ---
    // For each vMerge-restart cell, count consecutive continue cells below it
    // in the same visual column to determine rowspan.
    let mut rows = Vec::new();

    for (ri, raw_row) in raw_rows.iter().enumerate() {
        let mut cells = Vec::new();

        for rc in raw_row {
            if rc.vmerge_continue {
                // This cell is a continuation of a merge started above —
                // skip it entirely (it occupies no visual space in output)
                continue;
            }

            // Count rowspan: how many rows below have a continue in the same visual column
            let mut rowspan: u32 = 1;
            if rc.vmerge_restart {
                let mut check_visual = 0;
                // Sum up colspans in row ri to find the right visual offset
                let pos = raw_row
                    .iter()
                    .position(|c| std::ptr::eq(c, rc))
                    .unwrap_or(0);
                for prior in &raw_row[..pos] {
                    check_visual += prior.colspan as usize;
                }

                for below in raw_rows.iter().skip(ri + 1) {
                    let mut below_visual = 0;
                    let mut matched = false;
                    for bc in below {
                        if below_visual == check_visual {
                            if bc.vmerge_continue {
                                rowspan += 1;
                                matched = true;
                            }
                            break;
                        }
                        below_visual += bc.colspan as usize;
                    }
                    if !matched {
                        break;
                    }
                }
            }

            let inlines = if rc.text.is_empty() {
                Vec::new()
            } else {
                vec![Inline::Text(TextRun::new(rc.text.clone()))]
            };

            cells.push(TableCell {
                inlines,
                colspan: rc.colspan,
                rowspan,
                border_style: None,
                border_width: None,
                border_color: None,
                background: None,
                alignment: None,
                geometry: None,
                source: None,
            });
        }

        if !cells.is_empty() {
            rows.push(cells);
        }
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

    #[test]
    fn test_parse_vmerge_restart_and_continue() {
        // Row 0: cell A spans 2 rows vertically, cell B normal
        // Row 1: continuation of A (empty), cell C
        let xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:tr>
  <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
</w:tr>
<w:tr>
  <w:tc><w:tcPr><w:vMerge w:val="continue"/></w:tcPr><w:p></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc>
</w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows.len(), 2);
        // Row 0, cell A: rowspan=2
        assert_eq!(table.rows[0][0].rowspan, 2);
        assert_eq!(table.rows[0][0].colspan, 1);
        // Row 0, cell B: normal
        assert_eq!(table.rows[0][1].rowspan, 1);
        // Row 1: only cell C remains (A's continuation is skipped)
        assert_eq!(table.rows[1].len(), 1);
    }

    #[test]
    fn test_parse_vmerge_bare_tag() {
        // Bare <w:vMerge/> is treated as continue
        let xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:tr>
  <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>X</w:t></w:r></w:p></w:tc>
</w:tr>
<w:tr>
  <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p></w:p></w:tc>
</w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows.len(), 1); // continuation row skipped
        assert_eq!(table.rows[0][0].rowspan, 2);
    }

    #[test]
    fn test_parse_vmerge_three_rows() {
        // A spans 3 rows, B normal in row 0
        let xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:tr>
  <w:tc><w:tcPr><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
</w:tr>
<w:tr>
  <w:tc><w:tcPr><w:vMerge w:val="continue"/></w:tcPr><w:p></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc>
</w:tr>
<w:tr>
  <w:tc><w:tcPr><w:vMerge w:val="continue"/></w:tcPr><w:p></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>D</w:t></w:r></w:p></w:tc>
</w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows.len(), 3); // all 3 rows survive; continuation cells are skipped within each row
        assert_eq!(table.rows[0][0].rowspan, 3);
        // Row 1: only C (A's continuation skipped)
        assert_eq!(table.rows[1].len(), 1);
        assert_eq!(table.rows[1][0].inlines.len(), 1);
        // Row 2: only D (A's continuation skipped)
        assert_eq!(table.rows[2].len(), 1);
    }

    #[test]
    fn test_parse_vmerge_with_colspan() {
        // A spans 2 rows and 2 columns
        let xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:tr>
  <w:tc><w:tcPr><w:gridSpan w:val="2"/><w:vMerge w:val="restart"/></w:tcPr><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
</w:tr>
<w:tr>
  <w:tc><w:tcPr><w:vMerge w:val="continue"/></w:tcPr><w:p></w:p></w:tc>
  <w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc>
</w:tr>
</w:tbl>"#;

        let table = parse_word_table_ooxml(xml).unwrap();
        assert_eq!(table.rows[0][0].colspan, 2);
        assert_eq!(table.rows[0][0].rowspan, 2);
    }
}
