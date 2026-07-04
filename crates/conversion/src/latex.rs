use latexsnipper_ast::{Block, CiteStyle, Document, Inline};
use latexsnipper_foundation::Result;

use crate::converter::Converter;

/// Converts Document AST to LaTeX format.
/// Formulas use $$ for display mode, $ for inline mode.
pub struct LatexConverter;

impl Converter for LatexConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = vec![
            "\\documentclass{article}".to_string(),
            "\\usepackage{amsmath}".to_string(),
            "\\usepackage{amssymb}".to_string(),
            "\\begin{document}".to_string(),
        ];

        for page in &doc.pages {
            for block in &page.blocks {
                let rendered = render_block(block);
                if !rendered.is_empty() {
                    parts.push(rendered);
                }
            }
        }

        parts.push("\\end{document}".to_string());
        Ok(parts.join("\n\n"))
    }
    fn name(&self) -> &str {
        "latex"
    }
    fn extension(&self) -> &str {
        "tex"
    }
    fn mime_type(&self) -> &str {
        "application/x-latex"
    }
}

/// Converts Document AST to LaTeX display format (\[...\]).
pub struct LatexDisplayConverter;

impl Converter for LatexDisplayConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();
        for page in &doc.pages {
            for block in &page.blocks {
                let rendered = render_block_display(block);
                if !rendered.is_empty() {
                    parts.push(rendered);
                }
            }
        }
        Ok(parts.join("\n\n"))
    }
    fn name(&self) -> &str {
        "latex_display"
    }
    fn extension(&self) -> &str {
        "tex"
    }
    fn mime_type(&self) -> &str {
        "application/x-latex"
    }
}

/// Converts Document AST to LaTeX equation format.
pub struct LatexEquationConverter;

impl Converter for LatexEquationConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();
        for page in &doc.pages {
            for block in &page.blocks {
                let rendered = render_block_equation(block);
                if !rendered.is_empty() {
                    parts.push(rendered);
                }
            }
        }
        Ok(parts.join("\n\n"))
    }
    fn name(&self) -> &str {
        "latex_equation"
    }
    fn extension(&self) -> &str {
        "tex"
    }
    fn mime_type(&self) -> &str {
        "application/x-latex"
    }
}

fn render_block(block: &Block) -> String {
    match block {
        Block::Heading(h) => {
            let command = match h.level {
                1 => "\\section",
                2 => "\\subsection",
                3 => "\\subsubsection",
                4 => "\\paragraph",
                5 => "\\subparagraph",
                _ => "\\section",
            };
            let text = render_inlines(&h.inlines);
            format!("{}{{{}}}", command, text)
        }
        Block::Paragraph(p) => {
            let text = render_inlines(&p.inlines);
            if text.is_empty() {
                String::new()
            } else {
                text
            }
        }
        Block::Formula(f) => {
            let latex = f.formula.as_latex();
            if f.formula.display_mode {
                format!("$$\n{}\n$$", latex)
            } else {
                format!("${}$", latex)
            }
        }
        Block::Table(t) => render_table(t),
        Block::Figure(f) => {
            if let Some(caption) = &f.caption {
                if let Some(data) = &f.image_data {
                    format!(
                        "\\includegraphics[width=0.8\\textwidth]{{{}}}\n\\caption{{{}}}",
                        data, caption
                    )
                } else {
                    format!("\\caption{{{}}}", caption)
                }
            } else {
                String::new()
            }
        }
        Block::List(l) => render_list(l),
        Block::Quote(q) => render_quote(q),
        Block::Code(c) => render_code(c),
        Block::HorizontalRule(_) => "\\bigskip\\hrule\\bigskip".to_string(),
        Block::Handwriting(hw) => {
            let text = render_inlines(&hw.inlines);
            format!("\\texttt{{{}}}", text)
        }
        Block::DescriptionList(dl) => render_description_list(dl),
        Block::TableOfContents => "\\tableofcontents".to_string(),
        Block::Theorem(t) => render_theorem(t),
        Block::Proof(p) => render_proof(p),
        Block::Minipage(m) => render_minipage(m),
        Block::Float(f) => render_float(f),
    }
}

fn render_blocks(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(render_block)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_block_display(block: &Block) -> String {
    match block {
        Block::Formula(f) => {
            let latex = f.formula.as_latex();
            format!("\\[\n{}\n\\]", latex)
        }
        _ => render_block(block),
    }
}

fn render_block_equation(block: &Block) -> String {
    match block {
        Block::Formula(f) => {
            let latex = f.formula.as_latex();
            format!("\\begin{{equation}}\n{}\n\\end{{equation}}", latex)
        }
        _ => render_block(block),
    }
}

fn render_inlines(inlines: &[Inline]) -> String {
    let mut parts = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let mut text = t.text.clone();
                if t.bold == Some(true) {
                    text = format!("\\textbf{{{}}}", text);
                }
                if t.italic == Some(true) {
                    text = format!("\\textit{{{}}}", text);
                }
                if t.underline == Some(true) {
                    text = format!("\\underline{{{}}}", text);
                }
                parts.push(text);
            }
            Inline::Formula(f) => {
                let latex = f.as_latex();
                let formatted = if f.display_mode {
                    format!("$$\n{}\n$$", latex)
                } else {
                    format!("${}$", latex)
                };
                parts.push(formatted);
            }
            Inline::Image(_) => {
                parts.push("\\includegraphics{}".to_string());
            }
            Inline::Footnote { content } => {
                let inner = render_inlines(&[*content.clone()]);
                parts.push(format!("\\footnote{{{}}}", inner));
            }
            Inline::Label { key } => {
                parts.push(format!("\\label{{{}}}", key));
            }
            Inline::Reference { key, eq_ref } => {
                if *eq_ref {
                    parts.push(format!("\\eqref{{{}}}", key));
                } else {
                    parts.push(format!("\\ref{{{}}}", key));
                }
            }
            Inline::Citation { key, style } => {
                let cmd = match style {
                    CiteStyle::Author => "citet",
                    CiteStyle::Parenthetical => "citep",
                    CiteStyle::Plain => "cite",
                };
                parts.push(format!("\\{}{{{}}}", cmd, key));
            }
        }
    }
    parts.join(" ")
}

fn render_list(l: &latexsnipper_ast::ListBlock) -> String {
    let env = if l.ordered { "enumerate" } else { "itemize" };
    let mut items = Vec::new();
    for item in &l.items {
        let text = render_inlines(&item.inlines);
        items.push(format!("  \\item {}", text));
    }
    format!("\\begin{{{}}}\n{}\n\\end{{{}}}", env, items.join("\n"), env)
}

fn render_description_list(dl: &latexsnipper_ast::DescriptionListBlock) -> String {
    let mut items = Vec::new();
    for item in &dl.items {
        let content = render_blocks(&item.content);
        if let Some(label) = &item.label {
            let label_text = render_inlines(label);
            items.push(format!("  \\item[{}] {}", label_text, content));
        } else {
            items.push(format!("  \\item {}", content));
        }
    }
    format!(
        "\\begin{{description}}\n{}\n\\end{{description}}",
        items.join("\n")
    )
}

fn render_theorem(t: &latexsnipper_ast::TheoremBlock) -> String {
    let content = render_blocks(&t.content);
    format!("\\begin{{{}}}\n{}\n\\end{{{}}}", t.name, content, t.name)
}

fn render_proof(p: &latexsnipper_ast::ProofBlock) -> String {
    let content = render_blocks(&p.content);
    format!("\\begin{{proof}}\n{}\n\\end{{proof}}", content)
}

fn render_minipage(m: &latexsnipper_ast::MinipageBlock) -> String {
    let content = render_blocks(&m.content);
    format!(
        "\\begin{{minipage}}{{{}}}\n{}\n\\end{{minipage}}",
        m.width, content
    )
}

fn render_float(f: &latexsnipper_ast::FloatBlock) -> String {
    let content = render_blocks(&f.content);
    let mut result = format!("\\begin{{{}}}\n{}", f.env, content);
    if let Some(caption) = &f.caption {
        let caption_text = render_inlines(caption);
        result.push_str(&format!("\n\\caption{{{}}}", caption_text));
    }
    result.push_str(&format!("\n\\end{{{}}}", f.env));
    result
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
        format!(
            "\\begin{{quote}}\n{}\n\\hfill --- {}\n\\end{{quote}}",
            text, attr
        )
    } else {
        format!("\\begin{{quote}}\n{}\n\\end{{quote}}", text)
    }
}

fn render_code(c: &latexsnipper_ast::CodeBlock) -> String {
    let env = match &c.language {
        Some(lang) => match lang.as_str() {
            "rust" | "python" | "javascript" | "java" | "cpp" | "c" => "lstlisting",
            _ => "verbatim",
        },
        None => "verbatim",
    };
    match &c.language {
        Some(lang) => format!(
            "\\begin{{{env}}}[language={lang}]\n{code}\n\\end{{{env}}}",
            env = env,
            lang = lang,
            code = c.code
        ),
        None => format!(
            "\\begin{{{env}}}\n{code}\n\\end{{{env}}}",
            env = env,
            code = c.code
        ),
    }
}

fn render_table(t: &latexsnipper_ast::TableBlock) -> String {
    let cols = t.rows.first().map(|r| r.len()).unwrap_or(0);
    if cols == 0 {
        return String::new();
    }

    let mut lines = Vec::new();

    // Build column spec based on alignment
    let col_spec: String = t
        .rows
        .first()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    let align = cell
                        .alignment
                        .as_ref()
                        .unwrap_or(&latexsnipper_ast::CellAlignment::Center);
                    match align {
                        latexsnipper_ast::CellAlignment::Left => "l",
                        latexsnipper_ast::CellAlignment::Center => "c",
                        latexsnipper_ast::CellAlignment::Right => "r",
                        latexsnipper_ast::CellAlignment::Justify => "p{5cm}",
                    }
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .unwrap_or_else(|| "c|".repeat(cols));

    lines.push(format!("\\begin{{tabular}}{{|{}|}}", col_spec));
    lines.push("\\hline".to_string());

    for row in &t.rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                let mut parts = Vec::new();
                // Handle colspan with \multicolumn
                if cell.colspan > 1 {
                    let align = cell
                        .alignment
                        .as_ref()
                        .unwrap_or(&latexsnipper_ast::CellAlignment::Center);
                    let align_char = match align {
                        latexsnipper_ast::CellAlignment::Left => "l",
                        latexsnipper_ast::CellAlignment::Center => "c",
                        latexsnipper_ast::CellAlignment::Right => "r",
                        latexsnipper_ast::CellAlignment::Justify => "p{5cm}",
                    };
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
                    parts.push(format!(
                        "\\multicolumn{{{}}}{{|{}|}}{{{}}}",
                        cell.colspan, align_char, text
                    ));
                } else {
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
                    parts.push(text);
                }
                parts.join("")
            })
            .collect();
        lines.push(format!("{} \\\\", cells.join(" & ")));
        lines.push("\\hline".to_string());
    }

    lines.push("\\end{{tabular}}".to_string());
    lines.join("\n")
}
