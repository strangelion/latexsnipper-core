use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline};
use latexsnipper_foundation::Result;

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
                let rendered = render_block(block);
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

fn render_block(block: &Block) -> String {
    match block {
        Block::Heading(h) => {
            let prefix = "=".repeat(h.level as usize);
            let text = render_inlines(&h.inlines);
            format!("{} {}", prefix, text)
        }
        Block::Paragraph(p) => render_paragraph(p),
        Block::Formula(f) => {
            let content = convert_formula_to_typst(&f.formula);
            if f.formula.display_mode {
                format!("$ {} $", content)
            } else {
                content
            }
        }
        Block::Table(t) => render_table(t),
        Block::Figure(f) => {
            if let Some(caption) = &f.caption {
                format!("// {}", caption)
            } else {
                String::new()
            }
        }
        Block::List(l) => render_list(l),
        Block::Quote(q) => render_quote(q),
        Block::Code(c) => render_code(c),
        Block::HorizontalRule(_) => "#line(length: 100%)".to_string(),
        Block::Handwriting(hw) => {
            let text = render_inlines(&hw.inlines);
            format!("#text(\"{}\")", text)
        }
        Block::DescriptionList(dl) => render_description_list(dl),
        Block::TableOfContents => "目录".to_string(),
        Block::Theorem(t) => render_theorem(t),
        Block::Proof(p) => render_proof(p),
        Block::Minipage(m) => render_blocks(&m.content),
        Block::Float(f) => {
            let content = render_blocks(&f.content);
            if let Some(caption) = &f.caption {
                let caption_text = render_inlines(caption);
                format!("{}\n_{}_", content, caption_text)
            } else {
                content
            }
        }
    }
}

fn render_inlines(inlines: &[Inline]) -> String {
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
            Inline::Image(_) => {
                parts.push("#image(\"image.png\")".to_string());
            }
            Inline::Footnote { content } => {
                let inner = render_inlines(&[*content.clone()]);
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
        }
    }
    parts.join(" ")
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

fn render_paragraph(p: &latexsnipper_ast::ParagraphBlock) -> String {
    render_inlines(&p.inlines)
}

fn render_list(l: &latexsnipper_ast::ListBlock) -> String {
    let mut items = Vec::new();
    for item in &l.items {
        let text = render_inlines(&item.inlines);
        if l.ordered {
            items.push(format!("+ {}", text));
        } else {
            items.push(format!("- {}", text));
        }
    }
    items.join("\n")
}

fn render_description_list(dl: &latexsnipper_ast::DescriptionListBlock) -> String {
    let mut items = Vec::new();
    for item in &dl.items {
        let content = render_blocks(&item.content);
        if let Some(label) = &item.label {
            let label_text = render_inlines(label);
            items.push(format!("/ {}\n  {}", label_text, content));
        } else {
            items.push(format!("/ {}\n", content));
        }
    }
    items.join("\n\n")
}

fn render_theorem(t: &latexsnipper_ast::TheoremBlock) -> String {
    let content = render_blocks(&t.content);
    format!("*{}.* {}", t.name, content)
}

fn render_proof(p: &latexsnipper_ast::ProofBlock) -> String {
    let content = render_blocks(&p.content);
    format!("*Proof.* {} □", content)
}

fn render_blocks(blocks: &[latexsnipper_ast::Block]) -> String {
    blocks.iter().map(render_block).collect::<Vec<_>>().join("\n")
}

fn render_quote(q: &latexsnipper_ast::QuoteBlock) -> String {
    let mut content = Vec::new();
    for block in &q.blocks {
        let rendered = render_block(block);
        if !rendered.is_empty() {
            content.push(rendered);
        }
    }
    let text = content.join("\n");
    if let Some(attr) = &q.attribution {
        format!("#quote[{}]\n#align(right)[— {}]", text, attr)
    } else {
        format!("#quote[{}]", text)
    }
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
    lines.push(format!("table(columns: {},", cols));

    // Check if any styling is needed
    let has_style = t.rows.iter().any(|row| {
        row.iter().any(|cell| {
            cell.border_style.is_some()
                || cell.background.is_some()
                || cell.alignment.is_some()
                || cell.colspan > 1
                || cell.rowspan > 1
        })
    });

    if has_style {
        lines.push("  stroke: 1pt,".to_string());
    }

    for row in &t.rows {
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

                // Handle colspan/rowspan with cell() function
                if cell.colspan > 1 || cell.rowspan > 1 {
                    let mut args = vec![format!("[{}]", text)];
                    if cell.colspan > 1 {
                        args.push(format!("colspan: {}", cell.colspan));
                    }
                    if cell.rowspan > 1 {
                        args.push(format!("rowspan: {}", cell.rowspan));
                    }
                    format!("cell({})", args.join(", "))
                } else {
                    format!("[{}]", text)
                }
            })
            .collect();
        lines.push(format!("  ({}),", cells.join(", ")));
    }

    lines.push(")".to_string());
    lines.join("\n")
}
