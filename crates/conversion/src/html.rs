use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline, MediaAsset};
use latexsnipper_foundation::Result;

use crate::asset_helper::{resolve_asset_ref, resolve_image_html};
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
                let rendered = render_block(block, &doc.assets);
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

fn render_block(block: &Block, assets: &[MediaAsset]) -> String {
    match block {
        Block::Heading(h) => {
            let tag = format!("h{}", h.level);
            let text = render_inlines(&h.inlines, assets);
            format!("<{}>{}</{}>", tag, text, tag)
        }
        Block::Paragraph(p) => {
            let text = render_inlines(&p.inlines, assets);
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
        Block::Table(t) => render_table(t, assets),
        Block::Figure(f) => {
            let caption = f.caption.as_deref().unwrap_or("figure");
            if let Some(data) = &f.image_data {
                format!(
                    "<figure><img src=\"data:image/png;base64,{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>",
                    data, caption, caption
                )
            } else {
                let src = resolve_asset_ref(assets, &f.asset_id);
                if src.is_empty() {
                    format!(
                        "<figure><img src=\"image.png\" alt=\"{}\"><figcaption>{}</figcaption></figure>",
                        caption, caption
                    )
                } else {
                    format!(
                        "<figure><img src=\"{}\" alt=\"{}\"><figcaption>{}</figcaption></figure>",
                        src, caption, caption
                    )
                }
            }
        }
        Block::List(l) => render_list(l, assets),
        Block::Quote(q) => render_quote(q, assets),
        Block::Code(c) => render_code(c),
        Block::HorizontalRule(_) => "<hr>".to_string(),
        Block::Handwriting(hw) => {
            let text = render_inlines(&hw.inlines, assets);
            format!("<div class=\"handwriting\">{}</div>", text)
        }
        Block::DescriptionList(dl) => render_description_list(dl, assets),
        Block::TableOfContents => "<div class=\"toc\">目录</div>".to_string(),
        Block::Theorem(t) => render_theorem(t, assets),
        Block::Proof(p) => render_proof(p, assets),
        Block::Minipage(m) => {
            let content = render_blocks(&m.content, assets);
            format!(
                "<div class=\"minipage\" style=\"width:{}\">{}</div>",
                m.width, content
            )
        }
        Block::Float(f) => {
            let content = render_blocks(&f.content, assets);
            let mut result = format!("<div class=\"{}\">", f.env);
            result.push_str(&content);
            if let Some(caption) = &f.caption {
                let caption_text = render_inlines(caption, assets);
                result.push_str(&format!("<figcaption>{}</figcaption>", caption_text));
            }
            result.push_str("</div>");
            result
        }
        Block::TextBox(tb) => {
            let content = render_blocks(&tb.content, assets);
            format!("<div class=\"textbox\">{}</div>", content)
        }
        Block::Chart(_) => "<div class=\"chart\">[chart]</div>".to_string(),
        Block::Shape(_) => String::new(),
        Block::EmbeddedObject(_) => "<div class=\"embedded\">[embedded]</div>".to_string(),
        Block::Annotation(_) => String::new(),
    }
}

fn render_inlines(inlines: &[Inline], assets: &[MediaAsset]) -> String {
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
            Inline::Image(img) => {
                parts.push(resolve_image_html(assets, &img.asset_id, "image"));
            }
            Inline::Footnote { content } => {
                let inner = render_inlines(&[*content.clone()], assets);
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
            Inline::LineBreak => parts.push("<br>".to_string()),
            Inline::SoftBreak => parts.push("\n".to_string()),
            Inline::Span(s) => {
                parts.push(render_inlines(&s.content, assets));
            }
            Inline::Link(l) => {
                let text = render_inlines(&l.content, assets);
                parts.push(format!("<a href=\"{}\">{}</a>", l.target, text));
            }
            Inline::Code(c) => {
                parts.push(format!("<code>{}</code>", xml_escape(&c.code)));
            }
            Inline::Superscript(inner) => {
                let text = render_inlines(inner, assets);
                parts.push(format!("<sup>{}</sup>", text));
            }
            Inline::Subscript(inner) => {
                let text = render_inlines(inner, assets);
                parts.push(format!("<sub>{}</sub>", text));
            }
        }
    }
    parts.join(" ")
}

fn render_blocks(blocks: &[Block], assets: &[MediaAsset]) -> String {
    blocks
        .iter()
        .map(|b| render_block(b, assets))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_list(l: &latexsnipper_ast::ListBlock, assets: &[MediaAsset]) -> String {
    let tag = if l.ordered { "ol" } else { "ul" };
    let mut items = Vec::new();
    for item in &l.items {
        let text = render_inlines(&item.inlines, assets);
        items.push(format!("<li>{}</li>", text));
    }
    format!("<{}>{}</{}>", tag, items.join(""), tag)
}

fn render_quote(q: &latexsnipper_ast::QuoteBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&q.blocks, assets);
    let mut result = format!("<blockquote>{}</blockquote>", content);
    if let Some(attr) = &q.attribution {
        result.push_str(&format!("<figcaption>— {}</figcaption>", attr));
    }
    result
}

fn render_code(c: &latexsnipper_ast::CodeBlock) -> String {
    if let Some(lang) = &c.language {
        format!("<pre><code class=\"language-{}\">{}</code></pre>", lang, xml_escape(&c.code))
    } else {
        format!("<pre><code>{}</code></pre>", xml_escape(&c.code))
    }
}

fn render_description_list(
    dl: &latexsnipper_ast::DescriptionListBlock,
    assets: &[MediaAsset],
) -> String {
    let mut parts = Vec::new();
    parts.push("<dl>".to_string());
    for item in &dl.items {
        if let Some(label) = &item.label {
            let label_text = render_inlines(label, assets);
            parts.push(format!("<dt>{}</dt>", label_text));
        }
        for block in &item.content {
            let content = render_block(block, assets);
            parts.push(format!("<dd>{}</dd>", content));
        }
    }
    parts.push("</dl>".to_string());
    parts.join("\n")
}

fn render_theorem(t: &latexsnipper_ast::TheoremBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&t.content, assets);
    let number = t.number.as_deref().unwrap_or("");
    format!("<div class=\"theorem\"><strong>{}. {}</strong>\n{}</div>", t.name, number, content)
}

fn render_proof(p: &latexsnipper_ast::ProofBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&p.content, assets);
    format!("<div class=\"proof\"><em>Proof.</em>\n{}<span class=\"qed\">□</span></div>", content)
}

fn render_table(t: &latexsnipper_ast::TableBlock, assets: &[MediaAsset]) -> String {
    if t.rows.is_empty() {
        return String::new();
    }

    let mut rows = Vec::new();
    for row in &t.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                let content = render_inlines(&cell.inlines, assets);
                let mut attrs = String::new();
                if cell.colspan > 1 {
                    attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
                }
                if cell.rowspan > 1 {
                    attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
                }
                // Inline border styles
                let mut style = "border: 1pt solid black; padding: 4pt;".to_string();
                if let Some(bw) = cell.border_width {
                    let mut border = format!("border: {}px", bw);
                    let bs = cell.border_style.map(|s| match s {
                        latexsnipper_ast::BorderStyle::None => "none",
                        latexsnipper_ast::BorderStyle::Solid => "solid",
                        latexsnipper_ast::BorderStyle::Dashed => "dashed",
                        latexsnipper_ast::BorderStyle::Dotted => "dotted",
                        latexsnipper_ast::BorderStyle::Double => "double",
                        _ => "solid",
                    }).unwrap_or("solid");
                    border.push_str(&format!(" {}", bs));
                    if let Some(bc) = &cell.border_color {
                        border.push_str(&format!(" {}", bc));
                    }
                    border.push(';');
                    style = border;
                    style.push_str(" padding: 4pt;");
                } else if let Some(bc) = &cell.border_color {
                    style = format!("border: 1px {} solid; padding: 4pt;", bc);
                }
                if let Some(align) = &cell.alignment {
                    let align_val = match align {
                        latexsnipper_ast::CellAlignment::Left => "left",
                        latexsnipper_ast::CellAlignment::Center => "center",
                        latexsnipper_ast::CellAlignment::Right => "right",
                        latexsnipper_ast::CellAlignment::Justify => "justify",
                    };
                    style.push_str(&format!(" text-align: {};", align_val));
                }
                if let Some(bg) = &cell.background {
                    style.push_str(&format!(" background-color: {};", bg));
                }
                attrs.push_str(&format!(" style=\"{}\"", style));
                format!("<td{}>{}</td>", attrs, content)
            })
            .collect();
        rows.push(format!("<tr>{}</tr>", cells.join("")));
    }

    format!("<table>{}</table>", rows.join(""))
}

fn convert_formula_to_html(f: &Formula) -> String {
    match &f.source {
        FormulaSource::Latex(s) => s.clone(),
        FormulaSource::Typst(s) => s.to_string(),
        FormulaSource::Omml(s) => s.clone(),
        FormulaSource::MathML(s) => s.clone(),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
