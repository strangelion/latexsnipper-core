use latexsnipper_ast::Inline;
use latexsnipper_ast::TableBlock;
#[cfg(test)]
use latexsnipper_ast::{Block, ParagraphBlock, TableCell, TableRow};

/// Exports a TableBlock to various output formats.
pub struct TableExporter;

impl TableExporter {
    /// Export table to CSV format (opens directly in Excel/Google Sheets).
    pub fn to_csv(table: &TableBlock) -> String {
        let mut lines = Vec::new();
        for row in &table.rows {
            let cells: Vec<String> = row
                .cells
                .iter()
                .map(|cell| {
                    let inlines = cell.collect_inlines();
                    let text = extract_cell_text(&inlines);
                    csv_escape(&text)
                })
                .collect();
            lines.push(cells.join(","));
        }
        lines.join("\n")
    }

    /// Export table to TSV format (tab-separated, alternative for Excel).
    pub fn to_tsv(table: &TableBlock) -> String {
        let mut lines = Vec::new();
        for row in &table.rows {
            let cells: Vec<String> = row
                .cells
                .iter()
                .map(|cell| {
                    let inlines = cell.collect_inlines();
                    extract_cell_text(&inlines)
                })
                .collect();
            lines.push(cells.join("\t"));
        }
        lines.join("\n")
    }

    /// Export table to Word-compatible HTML (mso- namespace styles for Word rendering).
    pub fn to_word_html(table: &TableBlock) -> String {
        let mut parts = vec![
            r#"<html xmlns:o="urn:schemas-microsoft-com:office:office"
xmlns:w="urn:schemas-microsoft-com:office:word"
xmlns="http://www.w3.org/TR/REC-html40">"#
                .to_string(),
            "<head>".to_string(),
            "<style>".to_string(),
            "table { border-collapse: collapse; }".to_string(),
            "th, td { border: 1pt solid black; padding: 4pt; }".to_string(),
            "th { font-weight: bold; background-color: #f0f0f0; }".to_string(),
            "</style>".to_string(),
            "</head>".to_string(),
            "<body>".to_string(),
        ];

        parts.push(
            r#"<table style="border-collapse: collapse; mso-displayed-decimal-separator: \.; mso-displayed-thousand-separator: \,;">"#.to_string(),
        );

        for (i, row) in table.rows.iter().enumerate() {
            parts.push("  <tr>".to_string());
            for cell in &row.cells {
                let cell_inlines = cell.collect_inlines();
                let content = render_cell_html(&cell_inlines);
                let tag = if i == 0 { "th" } else { "td" };
                let mut attrs = String::new();
                if cell.colspan > 1 {
                    attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
                }
                if cell.rowspan > 1 {
                    attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
                }
                parts.push(format!(
                    "    <{}{} style=\"border: 1pt solid black; padding: 4pt;\">{}</{}>",
                    tag, attrs, content, tag
                ));
            }
            parts.push("  </tr>".to_string());
        }

        parts.push("</table>".to_string());
        parts.push("</body>".to_string());
        parts.push("</html>".to_string());
        parts.join("\n")
    }
}

fn extract_cell_text(inlines: &[Inline]) -> String {
    let mut parts = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => parts.push(t.text.clone()),
            Inline::Formula(f) => {
                // For CSV/TSV, render formula as LaTeX source
                parts.push(format!("${}$", f.as_latex()));
            }
            Inline::Image(_) => parts.push("[image]".to_string()),
            Inline::Footnote { content } => {
                let inner = render_cell_html(&[*content.clone()]);
                parts.push(format!("[^{}]", inner));
            }
            Inline::Label { .. } => {}
            Inline::Reference { key, .. } => {
                parts.push(format!("({})", key));
            }
            Inline::Citation { key, .. } => {
                parts.push(format!("[{}]", key));
            }
            _ => {}
        }
    }
    parts.join(" ")
}

fn render_cell_html(inlines: &[Inline]) -> String {
    let mut parts = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let escaped = xml_escape(&t.text);
                if t.bold == Some(true) {
                    parts.push(format!("<strong>{}</strong>", escaped));
                } else if t.italic == Some(true) {
                    parts.push(format!("<em>{}</em>", escaped));
                } else {
                    parts.push(escaped);
                }
            }
            Inline::Formula(f) => {
                let latex = f.as_latex();
                parts.push(format!("<i>{}</i>", xml_escape(latex)));
            }
            Inline::Image(_) => parts.push("[image]".to_string()),
            Inline::Footnote { content } => {
                let inner = render_cell_html(&[*content.clone()]);
                parts.push(format!("<sup>{}</sup>", inner));
            }
            Inline::Label { .. } => {}
            Inline::Reference { key, .. } => {
                parts.push(format!("<a href=\"#{}\">{}</a>", key, key));
            }
            Inline::Citation { key, .. } => {
                parts.push(format!("<cite>{}</cite>", key));
            }
            _ => {}
        }
    }
    parts.join(" ")
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_ast::{Rect, SourceInfo, TableCell, TextRun};

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

    fn sample_table() -> TableBlock {
        fn cell_with_geo(text: &str, geo: Rect, src: SourceInfo) -> TableCell {
            let mut c = make_cell(text);
            c.geometry = Some(geo);
            c.source = Some(src);
            c
        }

        TableBlock {
            rows: vec![
                TableRow {
                    cells: vec![
                        cell_with_geo(
                            "Name",
                            Rect::new(0.0, 0.0, 100.0, 20.0),
                            SourceInfo::new(),
                        ),
                        cell_with_geo(
                            "Score",
                            Rect::new(100.0, 0.0, 100.0, 20.0),
                            SourceInfo::new(),
                        ),
                    ],
                    height: None,
                    is_header: false,
                },
                TableRow {
                    cells: vec![
                        cell_with_geo(
                            "Alice",
                            Rect::new(0.0, 20.0, 100.0, 20.0),
                            SourceInfo::new(),
                        ),
                        cell_with_geo(
                            "95",
                            Rect::new(100.0, 20.0, 100.0, 20.0),
                            SourceInfo::new(),
                        ),
                    ],
                    height: None,
                    is_header: false,
                },
            ],
            columns: vec![],
            caption: None,
            style: None,
            geometry: Some(Rect::new(0.0, 0.0, 200.0, 40.0)),
            source: Some(SourceInfo::new()),
        }
    }

    #[test]
    fn csv_export() {
        let table = sample_table();
        let csv = TableExporter::to_csv(&table);
        assert!(csv.contains("Name,Score"));
        assert!(csv.contains("Alice,95"));
    }

    #[test]
    fn tsv_export() {
        let table = sample_table();
        let tsv = TableExporter::to_tsv(&table);
        assert!(tsv.contains("Name\tScore"));
        assert!(tsv.contains("Alice\t95"));
    }

    #[test]
    fn word_html_export() {
        let table = sample_table();
        let html = TableExporter::to_word_html(&table);
        assert!(html.contains("mso-displayed-decimal-separator"));
        assert!(html.contains("<th"));
        assert!(html.contains("Name"));
        assert!(html.contains("Alice"));
    }

    #[test]
    fn csv_with_commas() {
        let table = TableBlock {
            rows: vec![TableRow {
                cells: vec![TableCell {
                    content: vec![Block::Paragraph(ParagraphBlock {
                        inlines: vec![Inline::Text(TextRun::new("hello, world"))],
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
        let csv = TableExporter::to_csv(&table);
        assert_eq!(csv, "\"hello, world\"");
    }
}
