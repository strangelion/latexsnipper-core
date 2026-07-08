use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline, MediaAsset};
use latexsnipper_foundation::Result;

use crate::asset_helper::{resolve_asset_ref, resolve_image_markdown};
use crate::converter::Converter;

/// Converts Document AST to Markdown with MathJax ($...$ inline, $$...$$ display).
pub struct MarkdownInlineConverter;

impl Converter for MarkdownInlineConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_markdown(doc, MarkdownMode::Inline)
    }
    fn name(&self) -> &str {
        "markdown_inline"
    }
    fn extension(&self) -> &str {
        "md"
    }
    fn mime_type(&self) -> &str {
        "text/markdown"
    }
}

/// Converts Document AST to Markdown with block formulas ($$...$$).
pub struct MarkdownBlockConverter;

impl Converter for MarkdownBlockConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_markdown(doc, MarkdownMode::Block)
    }
    fn name(&self) -> &str {
        "markdown_block"
    }
    fn extension(&self) -> &str {
        "md"
    }
    fn mime_type(&self) -> &str {
        "text/markdown"
    }
}

enum MarkdownMode {
    Inline,
    Block,
}

fn convert_markdown(doc: &Document, mode: MarkdownMode) -> Result<String> {
    let mut parts = Vec::new();

    for page in &doc.pages {
        for block in &page.blocks {
            let rendered = render_block(block, &mode, &doc.assets);
            if !rendered.is_empty() {
                parts.push(rendered);
            }
        }
    }

    Ok(parts.join("\n\n"))
}

fn render_block(block: &Block, mode: &MarkdownMode, assets: &[MediaAsset]) -> String {
    match block {
        Block::Heading(h) => {
            let prefix = "#".repeat(h.level as usize);
            let text = render_inlines(&h.inlines, mode, assets);
            format!("{} {}", prefix, text)
        }
        Block::Paragraph(p) => {
            let text = render_inlines(&p.inlines, mode, assets);
            if text.is_empty() {
                String::new()
            } else {
                text
            }
        }
        Block::Formula(f) => {
            let content = convert_formula_to_markdown(&f.formula);
            if f.formula.display_mode {
                format!("$$\n{}\n$$", content)
            } else {
                format!("${}$", content)
            }
        }
        Block::Table(t) => render_table(t, mode, assets),
        Block::Figure(f) => {
            let caption = f.caption_plain_text();
            let caption_str = if caption.is_empty() {
                "figure"
            } else {
                &caption
            };
            if let Some(data) = &f.image_data {
                // Legacy path: inline base64 data
                format!("![{}](data:image/png;base64,{})", caption_str, data)
            } else {
                let src = resolve_asset_ref(assets, &f.asset_id);
                if src.is_empty() {
                    format!("![{}](image.png)", caption_str)
                } else {
                    format!("![{}]({})", caption_str, src)
                }
            }
        }
        Block::List(l) => render_list(l, mode, assets),
        Block::Quote(q) => render_quote(q, mode, assets),
        Block::Code(c) => render_code(c),
        Block::HorizontalRule(_) => "\n---\n".to_string(),
        Block::Handwriting(hw) => {
            let text = render_inlines(&hw.inlines, mode, assets);
            format!("> *Handwriting:* {}", text)
        }
        Block::DescriptionList(dl) => render_description_list(dl, mode, assets),
        Block::TableOfContents => "目录".to_string(),
        Block::Theorem(t) => render_theorem(t, mode, assets),
        Block::Proof(p) => render_proof(p, mode, assets),
        Block::Minipage(m) => render_blocks(&m.content, mode, assets),
        Block::Float(f) => {
            let content = render_blocks(&f.content, mode, assets);
            if let Some(caption) = &f.caption {
                let caption_text = render_inlines(caption, mode, assets);
                format!("{}\n*{}*", content, caption_text)
            } else {
                content
            }
        }
        Block::TextBox(tb) => render_blocks(&tb.content, mode, assets),
        Block::Chart(c) => format!("*[Chart: {:?}]*", c.chart_type),
        Block::Shape(s) => format!("*[Shape: {:?}]*", s.shape_type),
        Block::EmbeddedObject(e) => format!("*[Embedded: {:?}]*", e.kind),
        Block::Annotation(a) => format!("*[Annotation: {:?}]*", a.kind),
        Block::PageBreak(_) => "*[PageBreak]*".to_string(),
        Block::SectionBreak(sb) => format!("*[SectionBreak: {:?}]*", sb.kind),
        Block::HeaderFooter(hf) => format!("*[HeaderFooter: {:?} {:?}]*", hf.kind, hf.applies_to),
    }
}

fn render_inlines(inlines: &[Inline], _mode: &MarkdownMode, assets: &[MediaAsset]) -> String {
    let mut parts = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let mut text = t.text.clone();
                if t.bold == Some(true) {
                    text = format!("**{}**", text);
                }
                if t.italic == Some(true) {
                    text = format!("*{}*", text);
                }
                parts.push(text);
            }
            Inline::Formula(f) => {
                let content = convert_formula_to_markdown(f);
                let formatted = if f.display_mode {
                    format!("$$\n{}\n$$", content)
                } else {
                    format!("${}$", content)
                };
                parts.push(formatted);
            }
            Inline::Image(img) => {
                parts.push(resolve_image_markdown(assets, &img.asset_id, "image"));
            }
            Inline::Footnote { content } => {
                let inner = render_inlines(&[*content.clone()], _mode, assets);
                parts.push(format!("[^{}]", inner));
            }
            Inline::Label { .. } => {}
            Inline::Reference { key, .. } => {
                parts.push(format!("[@{}]", key));
            }
            Inline::Citation { key, .. } => {
                parts.push(format!("[@{}]", key));
            }
            Inline::LineBreak | Inline::SoftBreak => {
                parts.push("\n".to_string());
            }
            Inline::Span(s) => {
                parts.push(render_inlines(&s.content, _mode, assets));
            }
            Inline::Link(l) => {
                let text = render_inlines(&l.content, _mode, assets);
                parts.push(format!("[{}]({})", text, l.target));
            }
            Inline::Code(c) => {
                parts.push(format!("`{}`", c.code));
            }
            Inline::Superscript(inner) => {
                parts.push(render_inlines(inner, _mode, assets));
            }
            Inline::Subscript(inner) => {
                parts.push(render_inlines(inner, _mode, assets));
            }
        }
    }
    parts.join(" ")
}

// ── Helper forwarders to avoid changing every helper signature ──
// Each passes the `assets` slice down the render chain.

fn render_blocks(blocks: &[Block], mode: &MarkdownMode, assets: &[MediaAsset]) -> String {
    blocks
        .iter()
        .map(|b| render_block(b, mode, assets))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_list(
    l: &latexsnipper_ast::ListBlock,
    mode: &MarkdownMode,
    assets: &[MediaAsset],
) -> String {
    let mut parts = Vec::new();
    for item in &l.items {
        let text = render_inlines(&item.inlines, mode, assets);
        let prefix = if l.ordered { "1." } else { "-" };
        parts.push(format!("{} {}", prefix, text));
    }
    parts.join("\n")
}

fn render_quote(
    q: &latexsnipper_ast::QuoteBlock,
    mode: &MarkdownMode,
    assets: &[MediaAsset],
) -> String {
    let content = render_blocks(&q.blocks, mode, assets);
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
    mode: &MarkdownMode,
    assets: &[MediaAsset],
) -> String {
    let mut parts = Vec::new();
    for item in &dl.items {
        if let Some(label) = &item.label {
            let label_text = render_inlines(label, mode, assets);
            parts.push(format!("- **{}**", label_text));
        }
        for block in &item.content {
            let content = render_block(block, mode, assets);
            if !content.is_empty() {
                parts.push(format!("  {}", content));
            }
        }
    }
    parts.join("\n")
}

fn render_theorem(
    t: &latexsnipper_ast::TheoremBlock,
    mode: &MarkdownMode,
    assets: &[MediaAsset],
) -> String {
    let content = render_blocks(&t.content, mode, assets);
    let number = t.number.as_deref().unwrap_or("");
    format!("**{}. {}**\n{}", t.name, number, content)
}

fn render_proof(
    p: &latexsnipper_ast::ProofBlock,
    mode: &MarkdownMode,
    assets: &[MediaAsset],
) -> String {
    let content = render_blocks(&p.content, mode, assets);
    format!("*Proof.*\n{}□", content)
}

fn render_table(
    t: &latexsnipper_ast::TableBlock,
    mode: &MarkdownMode,
    assets: &[MediaAsset],
) -> String {
    if t.rows.is_empty() {
        return String::new();
    }

    let mut lines = Vec::new();

    // Header row
    let header: Vec<String> = t.rows[0]
        .iter()
        .map(|cell| render_inlines(&cell.inlines, mode, assets))
        .collect();
    lines.push(format!("| {} |", header.join(" | ")));

    // Separator
    let sep: Vec<&str> = t.rows[0].iter().map(|_| "---").collect();
    lines.push(format!("| {} |", sep.join(" | ")));

    // Data rows
    for row in t.rows.iter().skip(1) {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| render_inlines(&cell.inlines, mode, assets))
            .collect();
        lines.push(format!("| {} |", cells.join(" | ")));
    }

    lines.join("\n")
}

fn convert_formula_to_markdown(f: &Formula) -> String {
    match &f.source {
        FormulaSource::Latex(s) => s.clone(),
        FormulaSource::Typst(s) => typst_to_latex(s),
        FormulaSource::Omml(s) => s.clone(),
        FormulaSource::MathML(s) => format!("\"{}\"", s),
    }
}

fn typst_to_latex(s: &str) -> String {
    s.to_string()
}
