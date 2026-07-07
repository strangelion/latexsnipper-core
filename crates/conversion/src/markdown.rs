use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline};
use latexsnipper_foundation::Result;

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
            let rendered = render_block(block, &mode);
            if !rendered.is_empty() {
                parts.push(rendered);
            }
        }
    }

    Ok(parts.join("\n\n"))
}

fn render_block(block: &Block, mode: &MarkdownMode) -> String {
    match block {
        Block::Heading(h) => {
            let prefix = "#".repeat(h.level as usize);
            let text = render_inlines(&h.inlines, mode);
            format!("{} {}", prefix, text)
        }
        Block::Paragraph(p) => {
            let text = render_inlines(&p.inlines, mode);
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
        Block::Table(t) => render_table(t),
        Block::Figure(f) => {
            if let Some(caption) = &f.caption {
                if let Some(data) = &f.image_data {
                    format!("![{}](data:image/png;base64,{})", caption, data)
                } else {
                    format!("![{}](image.png)", caption)
                }
            } else {
                String::new()
            }
        }
        Block::List(l) => render_list(l, mode),
        Block::Quote(q) => render_quote(q, mode),
        Block::Code(c) => render_code(c),
        Block::HorizontalRule(_) => "\n---\n".to_string(),
        Block::Handwriting(hw) => {
            let text = render_inlines(&hw.inlines, mode);
            format!("> *Handwriting:* {}", text)
        }
        Block::DescriptionList(dl) => render_description_list(dl, mode),
        Block::TableOfContents => "目录".to_string(),
        Block::Theorem(t) => render_theorem(t, mode),
        Block::Proof(p) => render_proof(p, mode),
        Block::Minipage(m) => render_blocks(&m.content, mode),
        Block::Float(f) => {
            let content = render_blocks(&f.content, mode);
            if let Some(caption) = &f.caption {
                let caption_text = render_inlines(caption, mode);
                format!("{}\n*{}*", content, caption_text)
            } else {
                content
            }
        }
    }
}

fn render_inlines(inlines: &[Inline], _mode: &MarkdownMode) -> String {
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
            Inline::Image(_) => {
                parts.push("![image](image.png)".to_string());
            }
            Inline::Footnote { content } => {
                let inner = render_inlines(&[*content.clone()], _mode);
                parts.push(format!("[^{}]", inner));
            }
            Inline::Label { .. } => {
                // Labels are not rendered in Markdown
            }
            Inline::Reference { key, .. } => {
                parts.push(format!("[@{}]", key));
            }
            Inline::Citation { key, .. } => {
                parts.push(format!("[@{}]", key));
            }
        }
    }
    parts.join(" ")
}

fn convert_formula_to_markdown(f: &Formula) -> String {
    match &f.source {
        FormulaSource::Latex(s) => s.clone(),
        FormulaSource::Typst(s) => typst_to_latex(s),
        FormulaSource::Omml(s) => s.clone(),
        FormulaSource::MathML(s) => format!("\"{}\"", s),
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

fn render_list(l: &latexsnipper_ast::ListBlock, mode: &MarkdownMode) -> String {
    let mut items = Vec::new();
    for (i, item) in l.items.iter().enumerate() {
        let prefix = if l.ordered {
            format!("{}.", i + 1)
        } else {
            "-".to_string()
        };
        let text = render_inlines(&item.inlines, mode);
        items.push(format!("{} {}", prefix, text));
    }
    items.join("\n")
}

fn render_description_list(
    dl: &latexsnipper_ast::DescriptionListBlock,
    mode: &MarkdownMode,
) -> String {
    let mut items = Vec::new();
    for item in &dl.items {
        let content = render_blocks(&item.content, mode);
        if let Some(label) = &item.label {
            let label_text = render_inlines(label, mode);
            items.push(format!("**{}**\n: {}", label_text, content));
        } else {
            items.push(format!(": {}", content));
        }
    }
    items.join("\n\n")
}

fn render_theorem(t: &latexsnipper_ast::TheoremBlock, mode: &MarkdownMode) -> String {
    let content = render_blocks(&t.content, mode);
    format!("**{}.** {}", t.name, content)
}

fn render_proof(p: &latexsnipper_ast::ProofBlock, mode: &MarkdownMode) -> String {
    let content = render_blocks(&p.content, mode);
    format!("**Proof.** {} □", content)
}

fn render_blocks(blocks: &[latexsnipper_ast::Block], mode: &MarkdownMode) -> String {
    blocks
        .iter()
        .map(|b| render_block(b, mode))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_quote(q: &latexsnipper_ast::QuoteBlock, mode: &MarkdownMode) -> String {
    let mut lines = Vec::new();
    for block in &q.blocks {
        let rendered = render_block(block, mode);
        for line in rendered.lines() {
            lines.push(format!("> {}", line));
        }
    }
    if let Some(attr) = &q.attribution {
        lines.push(format!("> — {}", attr));
    }
    lines.join("\n")
}

fn render_code(c: &latexsnipper_ast::CodeBlock) -> String {
    match &c.language {
        Some(lang) => format!("```{}\n{}\n```", lang, c.code),
        None => format!("```\n{}\n```", c.code),
    }
}

fn render_table(t: &latexsnipper_ast::TableBlock) -> String {
    if t.rows.is_empty() {
        return String::new();
    }

    // Check if any cell has colspan/rowspan — if so, use HTML table
    let has_merge = t
        .rows
        .iter()
        .any(|row| row.iter().any(|cell| cell.colspan > 1 || cell.rowspan > 1));

    if has_merge {
        return render_html_table_for_markdown(t);
    }

    // Plain Markdown pipe table
    let cols = t.rows[0].len();
    let mut lines = Vec::new();

    let header: Vec<String> = t.rows[0]
        .iter()
        .map(|cell| {
            let text: String = cell
                .inlines
                .iter()
                .filter_map(|i| {
                    if let Inline::Text(t) = i {
                        Some(t.text.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            text
        })
        .collect();
    lines.push(format!("| {} |", header.join(" | ")));
    lines.push(format!("| {} |", "---|".repeat(cols)));

    for row in &t.rows[1..] {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                let text: String = cell
                    .inlines
                    .iter()
                    .filter_map(|i| {
                        if let Inline::Text(t) = i {
                            Some(t.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                text
            })
            .collect();
        lines.push(format!("| {} |", cells.join(" | ")));
    }

    lines.join("\n")
}

/// Render table as HTML for Markdown when cells have colspan/rowspan.
fn render_html_table_for_markdown(t: &latexsnipper_ast::TableBlock) -> String {
    let mut lines = vec!["<table>".to_string()];

    if let Some(first_row) = t.rows.first() {
        lines.push("  <thead><tr>".to_string());
        for cell in first_row {
            let text = extract_cell_text_plain(&cell.inlines);
            let mut attrs = String::new();
            if cell.colspan > 1 {
                attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
            }
            if cell.rowspan > 1 {
                attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
            }
            lines.push(format!("    <th{}>{}</th>", attrs, text));
        }
        lines.push("  </tr></thead>".to_string());
    }

    if t.rows.len() > 1 {
        lines.push("  <tbody>".to_string());
        for row in &t.rows[1..] {
            lines.push("    <tr>".to_string());
            for cell in row {
                let text = extract_cell_text_plain(&cell.inlines);
                let mut attrs = String::new();
                if cell.colspan > 1 {
                    attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
                }
                if cell.rowspan > 1 {
                    attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
                }
                lines.push(format!("    <td{}>{}</td>", attrs, text));
            }
            lines.push("    </tr>".to_string());
        }
        lines.push("  </tbody>".to_string());
    }

    lines.push("</table>".to_string());
    lines.join("\n")
}

fn extract_cell_text_plain(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .filter_map(|i| {
            if let Inline::Text(t) = i {
                Some(t.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
