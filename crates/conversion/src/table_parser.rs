//! Table parsing utilities for converting text to TableBlock AST.
//!
//! This module provides functions to parse tables from various formats:
//! - LaTeX tabular environment
//! - Markdown pipe tables
//! - HTML table tags
//! - Typst table syntax

use latexsnipper_ast::{Formula, Inline, TableBlock, TableCell, TextRun};

/// Parse a LaTeX tabular environment into a TableBlock.
///
/// Example input:
/// ```latex
/// \begin{tabular}{|c|c|}
/// \hline
/// A & B \\
/// \hline
/// C & D \\
/// \hline
/// \end{tabular}
/// ```
pub fn parse_latex_table(latex: &str) -> Option<TableBlock> {
    // Find \begin{tabular} ... \end{tabular}
    let start_marker = "\\begin{tabular}";
    let end_marker = "\\end{tabular}";

    let start = latex.find(start_marker)?;
    let after_start = &latex[start + start_marker.len()..];
    let end = after_start.find(end_marker)?;
    let content = after_start[..end].trim();

    // Skip column spec (e.g., {|c|c|})
    let content = if let Some(brace_end) = content.find('}') {
        content[brace_end + 1..].trim()
    } else {
        content
    };

    let rows = parse_latex_table_rows(content)?;
    if rows.is_empty() {
        return None;
    }

    Some(TableBlock {
        rows,
        geometry: None,
        source: None,
    })
}

/// Parse LaTeX table rows from content between \begin and \end.
fn parse_latex_table_rows(content: &str) -> Option<Vec<Vec<TableCell>>> {
    let mut rows = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip \hline, \cline, empty lines
        if line.is_empty()
            || line.starts_with("\\hline")
            || line.starts_with("\\cline")
            || line.starts_with("\\toprule")
            || line.starts_with("\\midrule")
            || line.starts_with("\\bottomrule")
        {
            continue;
        }

        // Remove trailing \\
        let line = line.trim_end_matches('\\').trim();

        // Split by & for cells
        let cells: Vec<TableCell> = line
            .split('&')
            .map(|cell_text| {
                let cell_text = cell_text.trim();
                TableCell {
                    inlines: parse_cell_content(cell_text),
                    colspan: 1,
                    rowspan: 1,
                    border_style: None,
                    border_width: None,
                    border_color: None,
                    background: None,
                    alignment: None,
                    geometry: None,
                    source: None,
                }
            })
            .collect();

        if !cells.is_empty() {
            rows.push(cells);
        }
    }

    Some(rows)
}

/// Parse cell content (text or formula).
fn parse_cell_content(text: &str) -> Vec<Inline> {
    let mut inlines = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        // Check for inline math $...$
        if let Some(start) = remaining.find('$') {
            // Make sure it's not $$
            if start + 1 < remaining.len() && remaining.as_bytes()[start + 1] == b'$' {
                // Skip $$
                if let Some(end) = remaining[start + 2..].find("$$") {
                    let formula = remaining[start + 2..start + 2 + end].trim().to_string();
                    let before = remaining[..start].trim();
                    if !before.is_empty() {
                        inlines.push(Inline::Text(TextRun::new(before.to_string())));
                    }
                    inlines.push(Inline::Formula(Formula::latex(formula)));
                    remaining = &remaining[start + 2 + end + 2..];
                    continue;
                }
            }

            if let Some(end) = remaining[start + 1..].find('$') {
                let formula = remaining[start + 1..start + 1 + end].trim().to_string();
                let before = remaining[..start].trim();
                if !before.is_empty() {
                    inlines.push(Inline::Text(TextRun::new(before.to_string())));
                }
                let mut f = Formula::latex(formula);
                f.display_mode = false;
                inlines.push(Inline::Formula(f));
                remaining = &remaining[start + 1 + end + 1..];
                continue;
            }
        }

        // No more math, treat rest as text
        let text = remaining.trim().to_string();
        if !text.is_empty() {
            inlines.push(Inline::Text(TextRun::new(text)));
        }
        break;
    }

    inlines
}

/// Parse a Markdown pipe table into a TableBlock.
///
/// Example input:
/// ```markdown
/// | A | B |
/// |---|---|
/// | C | D |
/// ```
pub fn parse_markdown_table(md: &str) -> Option<TableBlock> {
    let lines: Vec<&str> = md.lines().collect();
    if lines.len() < 2 {
        return None;
    }

    // Check if first line is a table row (starts with |)
    if !lines[0].trim_start().starts_with('|') {
        return None;
    }

    let mut rows = Vec::new();
    let mut header_done = false;

    for line in lines.iter() {
        let line = line.trim();

        // Skip separator row (|---|---|)
        if line.contains("---") && !header_done {
            header_done = true;
            continue;
        }

        // Skip empty lines
        if line.is_empty() {
            continue;
        }

        // Parse table row
        if line.starts_with('|') {
            let cells: Vec<TableCell> = line
                .trim_matches('|')
                .split('|')
                .map(|cell_text| {
                    let cell_text = cell_text.trim();
                    TableCell {
                        inlines: parse_cell_content(cell_text),
                        colspan: 1,
                        rowspan: 1,
                        border_style: None,
                        border_width: None,
                        border_color: None,
                        background: None,
                        alignment: None,
                        geometry: None,
                        source: None,
                    }
                })
                .collect();

            if !cells.is_empty() {
                rows.push(cells);
            }
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

/// Parse an HTML table into a TableBlock.
///
/// Example input:
/// ```html
/// <table>
/// <tr><td>A</td><td>B</td></tr>
/// <tr><td>C</td><td>D</td></tr>
/// </table>
/// ```
pub fn parse_html_table(html: &str) -> Option<TableBlock> {
    // Find <table> ... </table>
    let start_marker = "<table";
    let end_marker = "</table>";

    let start = html.find(start_marker)?;
    let after_start = &html[start..];
    let table_start = after_start.find('>')? + 1;
    let table_content = &after_start[table_start..];
    let end = table_content.find(end_marker)?;
    let content = table_content[..end].trim();

    let mut rows = Vec::new();

    let mut remaining = content;
    while !remaining.is_empty() {
        // Find <tr> ... </tr>
        if let Some(tr_start) = remaining.find("<tr") {
            let tr_content_start = remaining[tr_start..].find('>').map(|p| tr_start + p + 1);
            if let Some(tr_content_start) = tr_content_start {
                let tr_end = remaining[tr_content_start..].find("</tr>");
                if let Some(tr_end) = tr_end {
                    let row_content = remaining[tr_content_start..tr_content_start + tr_end].trim();
                    let current_row = parse_html_row(row_content);
                    if !current_row.is_empty() {
                        rows.push(current_row);
                    }
                    remaining = &remaining[tr_content_start + tr_end + 5..];
                    continue;
                }
            }
        }
        break;
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

/// Parse an HTML table row.
fn parse_html_row(row_content: &str) -> Vec<TableCell> {
    let mut cells = Vec::new();
    let mut remaining = row_content;

    while !remaining.is_empty() {
        // Find <td> or <th>
        let td_start = remaining.find("<td").or_else(|| remaining.find("<th"));
        if let Some(td_start) = td_start {
            let is_th = remaining[td_start..].starts_with("<th");

            // Find the closing > of the opening tag
            let tag_content = &remaining[td_start..];
            let tag_end = tag_content.find('>').map(|p| td_start + p + 1);
            if let Some(tag_end) = tag_end {
                // Find the closing tag
                let close_tag = if is_th { "</th>" } else { "</td>" };
                let content_end = remaining[tag_end..].find(close_tag);
                if let Some(content_end) = content_end {
                    let cell_content = remaining[tag_end..tag_end + content_end].trim();
                    let mut colspan_val = 1;
                    let mut rowspan_val = 1;

                    // Parse colspan and rowspan from opening tag
                    let tag_attrs = &remaining[td_start..tag_end];
                    if let Some(cs) = tag_attrs
                        .find("colspan=\"")
                        .and_then(|p| {
                            let start = p + 9;
                            tag_attrs[start..]
                                .find('"')
                                .map(|end| &tag_attrs[start..start + end])
                        })
                        .and_then(|s| s.parse::<u32>().ok())
                    {
                        colspan_val = cs;
                    }

                    if let Some(rs) = tag_attrs
                        .find("rowspan=\"")
                        .and_then(|p| {
                            let start = p + 9;
                            tag_attrs[start..]
                                .find('"')
                                .map(|end| &tag_attrs[start..start + end])
                        })
                        .and_then(|s| s.parse::<u32>().ok())
                    {
                        rowspan_val = rs;
                    }

                    cells.push(TableCell {
                        inlines: parse_cell_content(cell_content),
                        colspan: colspan_val,
                        rowspan: rowspan_val,
                        border_style: None,
                        border_width: None,
                        border_color: None,
                        background: None,
                        alignment: None,
                        geometry: None,
                        source: None,
                    });

                    remaining = &remaining[tag_end + content_end + close_tag.len()..];
                    continue;
                }
            }
        }
        break;
    }

    cells
}

/// Parse a Typst table into a TableBlock.
///
/// Example input:
/// ```typst
/// #table(
///   columns: 2,
///   [A], [B],
///   [C], [D],
/// )
/// ```
pub fn parse_typst_table(typst: &str) -> Option<TableBlock> {
    // Find table( ... )
    let start_marker = "table(";
    let start = typst.find(start_marker)?;
    let after_start = &typst[start + start_marker.len()..];

    // Find matching closing paren
    let mut depth = 1;
    let mut end = 0;
    for (i, c) in after_start.chars().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if end == 0 {
        return None;
    }

    let content = after_start[..end].trim();

    // Parse columns parameter
    let _cols = content
        .find("columns:")
        .and_then(|p| {
            let start = p + 8;
            content[start..]
                .trim()
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|s| s.parse::<usize>().ok())
        })
        .unwrap_or(2);

    // Extract cell content from [ ... ] blocks
    let mut cells = Vec::new();
    let mut remaining = content;

    while let Some(bracket_start) = remaining.find('[') {
        let bracket_end = remaining[bracket_start + 1..].find(']');
        if let Some(bracket_end) = bracket_end {
            let cell_content = remaining[bracket_start + 1..bracket_start + 1 + bracket_end].trim();
            cells.push(TableCell {
                inlines: parse_cell_content(cell_content),
                colspan: 1,
                rowspan: 1,
                border_style: None,
                border_width: None,
                border_color: None,
                background: None,
                alignment: None,
                geometry: None,
                source: None,
            });
            remaining = &remaining[bracket_start + 1 + bracket_end + 1..];
        } else {
            break;
        }
    }

    if cells.is_empty() {
        return None;
    }

    // Arrange cells into rows based on columns parameter
    let cols = _cols;
    let mut rows = Vec::new();
    for chunk in cells.chunks(cols) {
        rows.push(chunk.to_vec());
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
    fn test_parse_latex_table() {
        let latex = r"\begin{tabular}{|c|c|}
\hline
A & B \\
\hline
C & D \\
\hline
\end{tabular}";

        let table = parse_latex_table(latex).unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].len(), 2);
        assert_eq!(table.rows[1].len(), 2);
    }

    #[test]
    fn test_parse_markdown_table() {
        let md = "| A | B |\n|---|---|\n| C | D |";

        let table = parse_markdown_table(md).unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].len(), 2);
        assert_eq!(table.rows[1].len(), 2);
    }

    #[test]
    fn test_parse_html_table() {
        let html = "<table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table>";

        let table = parse_html_table(html).unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].len(), 2);
        assert_eq!(table.rows[1].len(), 2);
    }

    #[test]
    fn test_parse_typst_table() {
        let typst = "#table(columns: 2, [A], [B], [C], [D])";

        let table = parse_typst_table(typst).unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].len(), 2);
        assert_eq!(table.rows[1].len(), 2);
    }
}
