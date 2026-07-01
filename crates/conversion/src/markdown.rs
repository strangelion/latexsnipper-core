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
    }
}

fn render_inlines(inlines: &[Inline], mode: &MarkdownMode) -> String {
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
