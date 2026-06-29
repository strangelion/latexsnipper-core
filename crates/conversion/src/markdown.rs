use latexsnipper_ast::{Document, Block, Inline, Formula, FormulaSource};
use latexsnipper_foundation::Result;

use crate::converter::Converter;

/// Converts Document AST to Markdown inline format ($...$).
pub struct MarkdownInlineConverter;

impl Converter for MarkdownInlineConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_markdown(doc, MarkdownMode::Inline)
    }
    fn name(&self) -> &str { "markdown_inline" }
    fn extension(&self) -> &str { "md" }
    fn mime_type(&self) -> &str { "text/markdown" }
}

/// Converts Document AST to Markdown block format ($$...$$).
pub struct MarkdownBlockConverter;

impl Converter for MarkdownBlockConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_markdown(doc, MarkdownMode::Block)
    }
    fn name(&self) -> &str { "markdown_block" }
    fn extension(&self) -> &str { "md" }
    fn mime_type(&self) -> &str { "text/markdown" }
}

enum MarkdownMode {
    Inline,
    Block,
}

fn convert_markdown(doc: &Document, mode: MarkdownMode) -> Result<String> {
    let mut parts = Vec::new();

    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Formula(f) => {
                    let content = convert_formula_to_markdown(&f.formula);
                    let formatted = match &mode {
                        MarkdownMode::Inline => format!("${}$", content),
                        MarkdownMode::Block => format!("$$\n{}\n$$", content),
                    };
                    parts.push(formatted);
                }
                Block::Paragraph(p) => {
                    let text = render_paragraph(p, &mode);
                    if !text.is_empty() {
                        parts.push(text);
                    }
                }
                Block::Table(t) => {
                    parts.push(render_table(t));
                }
                Block::Figure(f) => {
                    if let Some(caption) = &f.caption {
                        if let Some(data) = &f.image_data {
                            parts.push(format!("![{}](data:image/png;base64,{})", caption, data));
                        } else {
                            parts.push(format!("![{}](image.png)", caption));
                        }
                    }
                }
            }
        }
    }

    Ok(parts.join("\n\n"))
}

fn convert_formula_to_markdown(f: &Formula) -> String {
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

fn render_paragraph(p: &latexsnipper_ast::ParagraphBlock, mode: &MarkdownMode) -> String {
    let mut parts = Vec::new();
    for inline in &p.inlines {
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
                let formatted = match mode {
                    MarkdownMode::Inline => format!("${}$", content),
                    MarkdownMode::Block => format!("$$\n{}\n$$", content),
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

fn render_table(t: &latexsnipper_ast::TableBlock) -> String {
    if t.rows.is_empty() {
        return String::new();
    }

    let cols = t.rows[0].len();
    let mut lines = Vec::new();

    let header: Vec<String> = t.rows[0].iter().map(|cell| {
        let text: String = cell.inlines.iter().filter_map(|i| {
            if let Inline::Text(t) = i { Some(t.text.as_str()) } else { None }
        }).collect();
        text
    }).collect();
    lines.push(format!("| {} |", header.join(" | ")));
    lines.push(format!("| {} |", "---|".repeat(cols)));

    for row in &t.rows[1..] {
        let cells: Vec<String> = row.iter().map(|cell| {
            let text: String = cell.inlines.iter().filter_map(|i| {
                if let Inline::Text(t) = i { Some(t.text.as_str()) } else { None }
            }).collect();
            text
        }).collect();
        lines.push(format!("| {} |", cells.join(" | ")));
    }

    lines.join("\n")
}
