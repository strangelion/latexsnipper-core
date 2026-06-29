use latexsnipper_ast::{Document, Block, Inline, Formula, FormulaSource};
use latexsnipper_foundation::Result;

use crate::converter::Converter;

/// Converts Document AST to HTML format with MathJax rendering.
pub struct HtmlConverter;

impl Converter for HtmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();

        parts.push(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>LaTeXSnipper Export</title>
<script>
MathJax = {
  tex: {
    inlineMath: [['$', '$']],
    displayMath: [['$$', '$$']]
  }
};
</script>
<script src="https://cdn.jsdelivr.net/npm/mathjax@3/es5/tex-mml-chtml.js"></script>
</head>
<body>
"#.to_string());

        for page in &doc.pages {
            for block in &page.blocks {
                match block {
                    Block::Formula(f) => {
                        let content = convert_formula_to_html(&f.formula);
                        if f.formula.display_mode {
                            parts.push(format!("$$\n{}\n$$", content));
                        } else {
                            parts.push(format!("${}$", content));
                        }
                    }
                    Block::Paragraph(p) => {
                        let text = render_paragraph(p);
                        if !text.is_empty() {
                            parts.push(format!("<p>{}</p>", text));
                        }
                    }
                    Block::Table(t) => {
                        parts.push(render_table(t));
                    }
                    Block::Figure(f) => {
                        if let Some(caption) = &f.caption {
                            if let Some(data) = &f.image_data {
                                parts.push(format!("<figure><img src=\"data:image/png;base64,{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>", data, caption, caption));
                            } else {
                                parts.push(format!("<figure><img src=\"image.png\" alt=\"{}\"><figcaption>{}</figcaption></figure>", caption, caption));
                            }
                        }
                    }
                }
            }
        }

        parts.push("</body>\n</html>".to_string());

        Ok(parts.join("\n"))
    }

    fn name(&self) -> &str { "html" }
    fn extension(&self) -> &str { "html" }
    fn mime_type(&self) -> &str { "text/html" }
}

fn convert_formula_to_html(f: &Formula) -> String {
    match &f.source {
        FormulaSource::Latex(s) => s.clone(),
        FormulaSource::Typst(s) => typst_to_latex(s),
        FormulaSource::Omml(s) => s.clone(),
        FormulaSource::MathML(s) => s.clone(),
    }
}

fn typst_to_latex(typst: &str) -> String {
    let mut result = typst.to_string();
    let mappings = [
        ("sqrt(", "\\sqrt{"),
        ("integral", "\\int"),
        ("sum", "\\sum"),
        ("product", "\\prod"),
        ("infinity", "\\infty"),
        ("pi", "\\pi"),
        ("alpha", "\\alpha"),
        ("beta", "\\beta"),
        ("gamma", "\\gamma"),
        ("delta", "\\delta"),
        ("theta", "\\theta"),
        ("lambda", "\\lambda"),
        ("sigma", "\\sigma"),
        ("omega", "\\omega"),
        ("plus.minus", "\\pm"),
        ("times", "\\times"),
        ("div", "\\div"),
        ("dot", "\\cdot"),
        ("lt.eq", "\\leq"),
        ("gt.eq", "\\geq"),
        ("neq", "\\neq"),
        ("approx", "\\approx"),
        ("rightarrow", "\\rightarrow"),
        ("leftarrow", "\\leftarrow"),
        ("in", "\\in"),
        ("notin", "\\notin"),
        ("subset", "\\subset"),
        ("cup", "\\cup"),
        ("cap", "\\cap"),
    ];

    for (from, to) in &mappings {
        result = result.replace(from, to);
    }

    result
}

fn render_paragraph(p: &latexsnipper_ast::ParagraphBlock) -> String {
    let mut parts = Vec::new();
    for inline in &p.inlines {
        match inline {
            Inline::Text(t) => {
                let mut text = xml_escape(&t.text);
                if t.bold == Some(true) {
                    text = format!("<strong>{}</strong>", text);
                }
                if t.italic == Some(true) {
                    text = format!("<em>{}</em>", text);
                }
                parts.push(text);
            }
            Inline::Formula(f) => {
                let content = convert_formula_to_html(f);
                if f.display_mode {
                    parts.push(format!("$$\n{}\n$$", content));
                } else {
                    parts.push(format!("${}$", content));
                }
            }
            Inline::Image(_) => {
                parts.push("<img src=\"image.png\" alt=\"image\">".to_string());
            }
        }
    }
    parts.join(" ")
}

fn render_table(t: &latexsnipper_ast::TableBlock) -> String {
    if t.rows.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push("<table>".to_string());

    // Header
    lines.push("  <thead>".to_string());
    lines.push("    <tr>".to_string());
    for cell in &t.rows[0] {
        let text: String = cell.inlines.iter().filter_map(|i| {
            if let Inline::Text(t) = i { Some(xml_escape(&t.text)) } else { None }
        }).collect();
        lines.push(format!("      <th>{}</th>", text));
    }
    lines.push("    </tr>".to_string());
    lines.push("  </thead>".to_string());

    // Body
    if t.rows.len() > 1 {
        lines.push("  <tbody>".to_string());
        for row in &t.rows[1..] {
            lines.push("    <tr>".to_string());
            for cell in row {
                let text: String = cell.inlines.iter().filter_map(|i| {
                    if let Inline::Text(t) = i { Some(xml_escape(&t.text)) } else { None }
                }).collect();
                lines.push(format!("      <td>{}</td>", text));
            }
            lines.push("    </tr>".to_string());
        }
        lines.push("  </tbody>".to_string());
    }

    lines.push("</table>".to_string());
    lines.join("\n")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
