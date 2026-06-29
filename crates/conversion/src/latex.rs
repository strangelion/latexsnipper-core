use latexsnipper_ast::{Document, Block, Inline};
use latexsnipper_foundation::Result;

use crate::converter::Converter;

/// Converts Document AST to LaTeX format (inline $...$).
pub struct LatexConverter;

impl Converter for LatexConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_latex(doc, LatexMode::Inline)
    }
    fn name(&self) -> &str { "latex" }
    fn extension(&self) -> &str { "tex" }
    fn mime_type(&self) -> &str { "application/x-latex" }
}

/// Converts Document AST to LaTeX display format (\[...\]).
pub struct LatexDisplayConverter;

impl Converter for LatexDisplayConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_latex(doc, LatexMode::Display)
    }
    fn name(&self) -> &str { "latex_display" }
    fn extension(&self) -> &str { "tex" }
    fn mime_type(&self) -> &str { "application/x-latex" }
}

/// Converts Document AST to LaTeX equation format (\begin{equation}...\end{equation}).
pub struct LatexEquationConverter;

impl Converter for LatexEquationConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_latex(doc, LatexMode::Equation)
    }
    fn name(&self) -> &str { "latex_equation" }
    fn extension(&self) -> &str { "tex" }
    fn mime_type(&self) -> &str { "application/x-latex" }
}

enum LatexMode {
    Inline,
    Display,
    Equation,
}

fn convert_latex(doc: &Document, mode: LatexMode) -> Result<String> {
    let mut parts = Vec::new();

    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Formula(f) => {
                    let latex = f.formula.as_latex();
                    let formatted = match &mode {
                        LatexMode::Inline => format!("${}$", latex),
                        LatexMode::Display => format!("\\[\n{}\n\\]", latex),
                        LatexMode::Equation => format!("\\begin{{equation}}\n{}\n\\end{{equation}}", latex),
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
                        parts.push(format!("{{{}}}", caption));
                    }
                }
            }
        }
    }

    Ok(parts.join("\n\n"))
}

fn render_paragraph(p: &latexsnipper_ast::ParagraphBlock, mode: &LatexMode) -> String {
    let mut parts = Vec::new();
    for inline in &p.inlines {
        match inline {
            Inline::Text(t) => {
                let mut text = t.text.clone();
                if t.bold == Some(true) {
                    text = format!("\\textbf{{{}}}", text);
                }
                if t.italic == Some(true) {
                    text = format!("\\textit{{{}}}", text);
                }
                parts.push(text);
            }
            Inline::Formula(f) => {
                let latex = f.as_latex();
                let formatted = match mode {
                    LatexMode::Inline => format!("${}$", latex),
                    LatexMode::Display => format!("\\[\n{}\n\\]", latex),
                    LatexMode::Equation => format!("\\begin{{equation}}\n{}\n\\end{{equation}}", latex),
                };
                parts.push(formatted);
            }
            Inline::Image(_) => {
                parts.push("\\includegraphics{}".to_string());
            }
        }
    }
    parts.join(" ")
}

fn render_table(t: &latexsnipper_ast::TableBlock) -> String {
    let cols = t.rows.first().map(|r| r.len()).unwrap_or(0);
    if cols == 0 {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push(format!("{{|{}|}}", "c|".repeat(cols)));
    lines.push("\\hline".to_string());

    for row in &t.rows {
        let cells: Vec<String> = row.iter().map(|cell| {
            let text: String = cell.inlines.iter().filter_map(|i| {
                if let Inline::Text(t) = i { Some(t.text.as_str()) } else { None }
            }).collect();
            text
        }).collect();
        lines.push(format!("{} \\\\", cells.join(" & ")));
        lines.push("\\hline".to_string());
    }

    format!("\\begin{{tabular}}{}\n{}\n\\end{{tabular}}",
        format!("{{|{}|}}", "c|".repeat(cols)),
        lines[1..].join("\n"))
}
