use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline, MediaAsset};
use latexsnipper_foundation::Result;

use crate::asset_helper::{resolve_asset_ref, resolve_image_typst};
use crate::converter::Converter;
use crate::latex_parser::parse_latex;
use crate::latex_to_typst::latex_ast_to_typst;

/// Converts Document AST to Typst format.
pub struct TypstConverter;

impl Converter for TypstConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();

        for page in &doc.pages {
            for block in &page.blocks {
                let rendered = render_block(block, &doc.assets);
                if !rendered.is_empty() {
                    parts.push(rendered);
                }
            }
        }

        Ok(parts.join("\n\n"))
    }

    fn name(&self) -> &str {
        "typst"
    }
    fn extension(&self) -> &str {
        "typ"
    }
    fn mime_type(&self) -> &str {
        "text/plain"
    }
}

fn render_block(block: &Block, assets: &[MediaAsset]) -> String {
    match block {
        Block::Heading(h) => {
            let prefix = "=".repeat(h.level as usize);
            let text = render_inlines(&h.inlines, assets);
            format!("{} {}", prefix, text)
        }
        Block::Paragraph(p) => render_paragraph(p, assets),
        Block::Formula(f) => {
            let content = convert_formula_to_typst(&f.formula);
            if f.formula.display_mode {
                format!("$ {} $", content)
            } else {
                content
            }
        }
        Block::Table(t) => render_table(t, assets),
        Block::Figure(f) => {
            let caption = f.caption_plain_text();
            let src = resolve_asset_ref(assets, &f.asset_id);
            if src.is_empty() {
                if caption.is_empty() {
                    String::new()
                } else {
                    format!("// {}", caption)
                }
            } else {
                format!("#image(\"{}\")\n// {}", src, caption)
            }
        }
        Block::List(l) => render_list(l, assets),
        Block::Quote(q) => render_quote(q, assets),
        Block::Code(c) => render_code(c),
        Block::HorizontalRule(_) => "#line(length: 100%)".to_string(),
        Block::Handwriting(hw) => {
            let text = render_inlines(&hw.inlines, assets);
            format!("#text(\"{}\")", text)
        }
        Block::DescriptionList(dl) => render_description_list(dl, assets),
        Block::TableOfContents => "目录".to_string(),
        Block::Theorem(t) => render_theorem(t, assets),
        Block::Proof(p) => render_proof(p, assets),
        Block::Minipage(m) => render_blocks(&m.content, assets),
        Block::Float(f) => {
            let content = render_blocks(&f.content, assets);
            if let Some(caption) = &f.caption {
                let caption_text = render_inlines(caption, assets);
                format!("{}\n_{}_", content, caption_text)
            } else {
                content
            }
        }
        Block::TextBox(tb) => render_blocks(&tb.content, assets),
        Block::Chart(c) => format!("*[Chart: {:?}]*", c.chart_type),
        Block::Shape(s) => format!("*[Shape: {:?}]*", s.shape_type),
        Block::EmbeddedObject(e) => format!("*[Embedded: {:?}]*", e.kind),
        Block::Annotation(a) => format!("*[Annotation: {:?}]*", a.kind),
    }
}

fn render_inlines(inlines: &[Inline], assets: &[MediaAsset]) -> String {
    let mut parts = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let mut text = t.text.clone();
                if t.bold == Some(true) {
                    text = format!("*{}*", text);
                }
                if t.italic == Some(true) {
                    text = format!("_{}_", text);
                }
                if t.underline == Some(true) {
                    text = format!("#underline[{}]", text);
                }
                parts.push(text);
            }
            Inline::Formula(f) => {
                let content = convert_formula_to_typst(f);
                let formatted = if f.display_mode {
                    format!("$ {} $", content)
                } else {
                    content
                };
                parts.push(formatted);
            }
            Inline::Image(img) => {
                parts.push(resolve_image_typst(assets, &img.asset_id));
            }
            Inline::Footnote { content } => {
                let inner = render_inlines(&[*content.clone()], assets);
                parts.push(format!("#footnote({})", inner));
            }
            Inline::Label { key } => {
                parts.push(format!("<label={}>", key));
            }
            Inline::Reference { key, .. } => {
                parts.push(format!("@{}", key));
            }
            Inline::Citation { key, .. } => {
                parts.push(format!("@{}", key));
            }
            Inline::LineBreak | Inline::SoftBreak => {
                parts.push("\n".to_string());
            }
            Inline::Span(s) => {
                parts.push(render_inlines(&s.content, assets));
            }
            Inline::Link(l) => {
                let text = render_inlines(&l.content, assets);
                parts.push(format!("#link(\"{}\")[{}]", l.target, text));
            }
            Inline::Code(c) => {
                parts.push(format!("`{}`", c.code));
            }
            Inline::Superscript(inner) => {
                parts.push(format!("super({})", render_inlines(inner, assets)));
            }
            Inline::Subscript(inner) => {
                parts.push(format!("sub({})", render_inlines(inner, assets)));
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

fn render_paragraph(p: &latexsnipper_ast::ParagraphBlock, assets: &[MediaAsset]) -> String {
    render_inlines(&p.inlines, assets)
}

fn render_list(l: &latexsnipper_ast::ListBlock, assets: &[MediaAsset]) -> String {
    let mut items = Vec::new();
    for item in &l.items {
        let text = render_inlines(&item.inlines, assets);
        let prefix = if l.ordered { "+" } else { "-" };
        items.push(format!("{} {}", prefix, text));
    }
    items.join("\n")
}

fn render_quote(q: &latexsnipper_ast::QuoteBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&q.blocks, assets);
    let mut result = content
        .lines()
        .map(|line| format!("> {}", line))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(attr) = &q.attribution {
        result.push_str(&format!("\n> — {}", attr));
    }
    result
}

fn render_code(c: &latexsnipper_ast::CodeBlock) -> String {
    if let Some(lang) = &c.language {
        format!("```{}\n{}\n```", lang, c.code)
    } else {
        format!("```\n{}\n```", c.code)
    }
}

fn render_description_list(
    dl: &latexsnipper_ast::DescriptionListBlock,
    assets: &[MediaAsset],
) -> String {
    let mut parts = Vec::new();
    for item in &dl.items {
        if let Some(label) = &item.label {
            let label_text = render_inlines(label, assets);
            parts.push(format!("/ {}", label_text));
        }
        for block in &item.content {
            let content = render_block(block, assets);
            if !content.is_empty() {
                parts.push(format!("  {}", content));
            }
        }
    }
    parts.join("\n")
}

fn render_theorem(t: &latexsnipper_ast::TheoremBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&t.content, assets);
    let number = t.number.as_deref().unwrap_or("");
    format!("__{}. {}__\n{}", t.name, number, content)
}

fn render_proof(p: &latexsnipper_ast::ProofBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&p.content, assets);
    format!("_Proof._\n{}□", content)
}

fn render_table(t: &latexsnipper_ast::TableBlock, assets: &[MediaAsset]) -> String {
    if t.rows.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();

    // Typst table with columns
    let col_count = t.rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let align = "auto,".repeat(col_count);
    lines.push(format!("#table(columns: ({}),", align));

    for row in &t.rows {
        for cell in row {
            let content = render_inlines(&cell.inlines, assets);
            let mut args = String::new();
            if cell.colspan > 1 {
                args.push_str(&format!(" colspan: {}", cell.colspan));
            }
            if cell.rowspan > 1 {
                args.push_str(&format!(" rowspan: {}", cell.rowspan));
            }
            if args.is_empty() {
                lines.push(format!("  [{}],", content));
            } else {
                lines.push(format!("  table.cell({})[{}],", args.trim_start(), content));
            }
        }
    }

    lines.push(")".to_string());
    lines.join("\n")
}

fn convert_formula_to_typst(f: &Formula) -> String {
    match &f.source {
        FormulaSource::Typst(s) => s.clone(),
        FormulaSource::Latex(s) => {
            let ast = parse_latex(s);
            latex_ast_to_typst(&ast)
        }
        FormulaSource::Omml(s) => {
            let ast = parse_latex(s);
            latex_ast_to_typst(&ast)
        }
        FormulaSource::MathML(s) => format!("\"{}\"", s),
    }
}
