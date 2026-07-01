use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline};
use latexsnipper_foundation::Result;

use crate::converter::Converter;

/// Converts Document AST to HTML format with MathJax rendering.
pub struct HtmlConverter;

impl Converter for HtmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();

        parts.push(
            r#"<!DOCTYPE html>
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
"#
            .to_string(),
        );

        for page in &doc.pages {
            for block in &page.blocks {
                let rendered = render_block(block);
                if !rendered.is_empty() {
                    parts.push(rendered);
                }
            }
        }

        parts.push("</body>\n</html>".to_string());

        Ok(parts.join("\n"))
    }

    fn name(&self) -> &str {
        "html"
    }
    fn extension(&self) -> &str {
        "html"
    }
    fn mime_type(&self) -> &str {
        "text/html"
    }
}

fn render_block(block: &Block) -> String {
    match block {
        Block::Heading(h) => {
            let tag = format!("h{}", h.level);
            let text = render_inlines(&h.inlines);
            format!("<{}>{}</{}>", tag, text, tag)
        }
        Block::Paragraph(p) => {
            let text = render_inlines(&p.inlines);
            if text.is_empty() {
                String::new()
            } else {
                format!("<p>{}</p>", text)
            }
        }
        Block::Formula(f) => {
            let content = convert_formula_to_html(&f.formula);
            if f.formula.display_mode {
                format!("$$\n{}\n$$", content)
            } else {
                format!("${}$", content)
            }
        }
        Block::Table(t) => render_table(t),
        Block::Figure(f) => {
            if let Some(caption) = &f.caption {
                if let Some(data) = &f.image_data {
                    format!(
                        "<figure><img src=\"data:image/png;base64,{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>",
                        data, caption, caption
                    )
                } else {
                    format!(
                        "<figure><img src=\"image.png\" alt=\"{}\"><figcaption>{}</figcaption></figure>",
                        caption, caption
                    )
                }
            } else {
                String::new()
            }
        }
        Block::List(l) => render_list(l),
        Block::Quote(q) => render_quote(q),
        Block::Code(c) => render_code(c),
        Block::HorizontalRule(_) => "<hr>".to_string(),
    }
}

fn render_inlines(inlines: &[Inline]) -> String {
    let mut parts = Vec::new();
    for inline in inlines {
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

fn render_list(l: &latexsnipper_ast::ListBlock) -> String {
    let tag = if l.ordered { "ol" } else { "ul" };
    let mut items = Vec::new();
    for item in &l.items {
        let text = render_inlines(&item.inlines);
        items.push(format!("  <li>{}</li>", text));
    }
    format!("<{}>\n{}\n</{}>", tag, items.join("\n"), tag)
}

fn render_quote(q: &latexsnipper_ast::QuoteBlock) -> String {
    let mut content = Vec::new();
    for block in &q.blocks {
        let rendered = render_block(block);
        if !rendered.is_empty() {
            content.push(format!("  {}", rendered));
        }
    }
    let text = content.join("\n");
    if let Some(attr) = &q.attribution {
        format!(
            "<blockquote>\n{}\n<footer>— {}</footer>\n</blockquote>",
            text, attr
        )
    } else {
        format!("<blockquote>\n{}\n</blockquote>", text)
    }
}

fn render_code(c: &latexsnipper_ast::CodeBlock) -> String {
    match &c.language {
        Some(lang) => format!(
            "<pre><code class=\"language-{}\">{}</code></pre>",
            lang,
            xml_escape(&c.code)
        ),
        None => format!("<pre><code>{}</code></pre>", xml_escape(&c.code)),
    }
}

fn render_table(t: &latexsnipper_ast::TableBlock) -> String {
    if t.rows.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push("<table>".to_string());

    lines.push("  <thead>".to_string());
    lines.push("    <tr>".to_string());
    for cell in &t.rows[0] {
        let text: String = cell
            .inlines
            .iter()
            .filter_map(|i| {
                if let Inline::Text(t) = i {
                    Some(xml_escape(&t.text))
                } else {
                    None
                }
            })
            .collect();
        lines.push(format!("      <th>{}</th>", text));
    }
    lines.push("    </tr>".to_string());
    lines.push("  </thead>".to_string());

    if t.rows.len() > 1 {
        lines.push("  <tbody>".to_string());
        for row in &t.rows[1..] {
            lines.push("    <tr>".to_string());
            for cell in row {
                let text: String = cell
                    .inlines
                    .iter()
                    .filter_map(|i| {
                        if let Inline::Text(t) = i {
                            Some(xml_escape(&t.text))
                        } else {
                            None
                        }
                    })
                    .collect();
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
