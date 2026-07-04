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
        Block::Handwriting(hw) => {
            let text = render_inlines(&hw.inlines);
            format!("<div class=\"handwriting\">{}</div>", text)
        }
        Block::DescriptionList(dl) => render_description_list(dl),
        Block::TableOfContents => "<div class=\"toc\">目录</div>".to_string(),
        Block::Theorem(t) => render_theorem(t),
        Block::Proof(p) => render_proof(p),
        Block::Minipage(m) => {
            let content = render_blocks(&m.content);
            format!(
                "<div class=\"minipage\" style=\"width:{}\">{}</div>",
                m.width, content
            )
        }
        Block::Float(f) => {
            let content = render_blocks(&f.content);
            let mut result = format!("<div class=\"{}\">", f.env);
            result.push_str(&content);
            if let Some(caption) = &f.caption {
                let caption_text = render_inlines(caption);
                result.push_str(&format!("<figcaption>{}</figcaption>", caption_text));
            }
            result.push_str("</div>");
            result
        }
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
                if t.underline == Some(true) {
                    text = format!("<u>{}</u>", text);
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
            Inline::Footnote { content } => {
                let inner = render_inlines(&[*content.clone()]);
                parts.push(format!("<sup>{}</sup>", inner));
            }
            Inline::Label { key } => {
                parts.push(format!("<a id=\"{}\"></a>", key));
            }
            Inline::Reference { key, eq_ref } => {
                if *eq_ref {
                    parts.push(format!("({})", key));
                } else {
                    parts.push(format!("<a href=\"#{}\">{}</a>", key, key));
                }
            }
            Inline::Citation { key, .. } => {
                parts.push(format!("<cite>{}</cite>", key));
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

fn render_description_list(dl: &latexsnipper_ast::DescriptionListBlock) -> String {
    let mut items = Vec::new();
    for item in &dl.items {
        let content = render_blocks(&item.content);
        if let Some(label) = &item.label {
            let label_text = render_inlines(label);
            items.push(format!(
                "  <dt><strong>{}</strong></dt>\n  <dd>{}</dd>",
                label_text, content
            ));
        } else {
            items.push(format!("  <dd>{}</dd>", content));
        }
    }
    format!("<dl>\n{}\n</dl>", items.join("\n"))
}

fn render_theorem(t: &latexsnipper_ast::TheoremBlock) -> String {
    let content = render_blocks(&t.content);
    format!(
        "<div class=\"theorem\"><strong>{}.</strong> {}</div>",
        t.name, content
    )
}

fn render_proof(p: &latexsnipper_ast::ProofBlock) -> String {
    let content = render_blocks(&p.content);
    format!(
        "<div class=\"proof\"><strong>Proof.</strong> {} □</div>",
        content
    )
}

fn render_blocks(blocks: &[latexsnipper_ast::Block]) -> String {
    blocks
        .iter()
        .map(render_block)
        .collect::<Vec<_>>()
        .join("\n")
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
    lines.push(
        r#"<table border="1" cellpadding="4" cellspacing="0" style="border-collapse: collapse; width: 100%;">"#.to_string(),
    );

    lines.push("  <thead>".to_string());
    if let Some(first_row) = t.rows.first() {
        lines.push("    <tr>".to_string());
        for cell in first_row {
            let content = render_cell_content(&cell.inlines);
            let mut attrs = String::new();
            if cell.colspan > 1 {
                attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
            }
            if cell.rowspan > 1 {
                attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
            }
            // Add style attribute for border, background, alignment
            let mut style_parts = Vec::new();
            if let Some(ref border) = cell.border_style {
                let border_str = match border {
                    latexsnipper_ast::BorderStyle::None => "none",
                    latexsnipper_ast::BorderStyle::Solid => "solid",
                    latexsnipper_ast::BorderStyle::Dashed => "dashed",
                    latexsnipper_ast::BorderStyle::Dotted => "dotted",
                    latexsnipper_ast::BorderStyle::Double => "double",
                    latexsnipper_ast::BorderStyle::Groove => "groove",
                    latexsnipper_ast::BorderStyle::Ridge => "ridge",
                    latexsnipper_ast::BorderStyle::Inset => "inset",
                    latexsnipper_ast::BorderStyle::Outset => "outset",
                };
                let width = cell.border_width.unwrap_or(1);
                let color = cell.border_color.as_deref().unwrap_or("black");
                style_parts.push(format!("border: {}px {} {}", width, border_str, color));
            }
            if let Some(ref bg) = cell.background {
                style_parts.push(format!("background-color: {}", bg));
            }
            if let Some(ref align) = cell.alignment {
                let align_str = match align {
                    latexsnipper_ast::CellAlignment::Left => "left",
                    latexsnipper_ast::CellAlignment::Center => "center",
                    latexsnipper_ast::CellAlignment::Right => "right",
                    latexsnipper_ast::CellAlignment::Justify => "justify",
                };
                style_parts.push(format!("text-align: {}", align_str));
            }
            if !style_parts.is_empty() {
                attrs.push_str(&format!(" style=\"{}\"", style_parts.join("; ")));
            }
            lines.push(format!("      <th{}>{}</th>", attrs, content));
        }
        lines.push("    </tr>".to_string());
    }
    lines.push("  </thead>".to_string());

    if t.rows.len() > 1 {
        lines.push("  <tbody>".to_string());
        for row in &t.rows[1..] {
            lines.push("    <tr>".to_string());
            for cell in row {
                let content = render_cell_content(&cell.inlines);
                let mut attrs = String::new();
                if cell.colspan > 1 {
                    attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
                }
                if cell.rowspan > 1 {
                    attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
                }
                // Add style attribute for border, background, alignment
                let mut style_parts = Vec::new();
                if let Some(ref border) = cell.border_style {
                    let border_str = match border {
                        latexsnipper_ast::BorderStyle::None => "none",
                        latexsnipper_ast::BorderStyle::Solid => "solid",
                        latexsnipper_ast::BorderStyle::Dashed => "dashed",
                        latexsnipper_ast::BorderStyle::Dotted => "dotted",
                        latexsnipper_ast::BorderStyle::Double => "double",
                        latexsnipper_ast::BorderStyle::Groove => "groove",
                        latexsnipper_ast::BorderStyle::Ridge => "ridge",
                        latexsnipper_ast::BorderStyle::Inset => "inset",
                        latexsnipper_ast::BorderStyle::Outset => "outset",
                    };
                    let width = cell.border_width.unwrap_or(1);
                    let color = cell.border_color.as_deref().unwrap_or("black");
                    style_parts.push(format!("border: {}px {} {}", width, border_str, color));
                }
                if let Some(ref bg) = cell.background {
                    style_parts.push(format!("background-color: {}", bg));
                }
                if let Some(ref align) = cell.alignment {
                    let align_str = match align {
                        latexsnipper_ast::CellAlignment::Left => "left",
                        latexsnipper_ast::CellAlignment::Center => "center",
                        latexsnipper_ast::CellAlignment::Right => "right",
                        latexsnipper_ast::CellAlignment::Justify => "justify",
                    };
                    style_parts.push(format!("text-align: {}", align_str));
                }
                if !style_parts.is_empty() {
                    attrs.push_str(&format!(" style=\"{}\"", style_parts.join("; ")));
                }
                lines.push(format!("      <td{}>{}</td>", attrs, content));
            }
            lines.push("    </tr>".to_string());
        }
        lines.push("  </tbody>".to_string());
    }

    lines.push("</table>".to_string());
    lines.join("\n")
}

fn render_cell_content(inlines: &[Inline]) -> String {
    let parts: Vec<String> = inlines
        .iter()
        .map(|i| match i {
            Inline::Text(t) => {
                let mut text = xml_escape(&t.text);
                if t.bold == Some(true) {
                    text = format!("<strong>{}</strong>", text);
                }
                if t.italic == Some(true) {
                    text = format!("<em>{}</em>", text);
                }
                text
            }
            Inline::Formula(f) => {
                let content = convert_formula_to_html(f);
                if f.display_mode {
                    format!("$$\n{}\n$$", content)
                } else {
                    format!("${}$", content)
                }
            }
            Inline::Image(_) => "<img src=\"image.png\" alt=\"image\">".to_string(),
            Inline::Footnote { content } => {
                let inner = render_cell_content(&[*content.clone()]);
                format!("<sup>{}</sup>", inner)
            }
            Inline::Label { key } => format!("<a id=\"{}\"></a>", key),
            Inline::Reference { key, .. } => format!("<a href=\"#{}\">{}</a>", key, key),
            Inline::Citation { key, .. } => format!("<cite>{}</cite>", key),
        })
        .collect();
    parts.join(" ")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
