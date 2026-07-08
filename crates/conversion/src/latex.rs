use latexsnipper_ast::{Block, CiteStyle, Document, Inline, MediaAsset};
use latexsnipper_foundation::Result;

use crate::asset_helper::{resolve_asset_ref, resolve_image_latex};
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
            "\\usepackage{multirow}".to_string(),
            "\\begin{document}".to_string(),
        ];

        for page in &doc.pages {
            for block in &page.blocks {
                let rendered = render_block(block, &doc.assets);
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
                let rendered = render_block_display(block, &doc.assets);
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
                let rendered = render_block_equation(block, &doc.assets);
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

fn render_block(block: &Block, assets: &[MediaAsset]) -> String {
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
            let text = render_inlines(&h.inlines, assets);
            format!("{}{{{}}}", command, text)
        }
        Block::Paragraph(p) => {
            let text = render_inlines(&p.inlines, assets);
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
        Block::Table(t) => render_table(t, assets),
        Block::Figure(f) => {
            let caption = f.caption_plain_text();
            if let Some(data) = &f.image_data {
                format!(
                    "\\includegraphics[width=0.8\\textwidth]{{{}}}\n\\caption{{{}}}",
                    data, caption
                )
            } else {
                let src = resolve_asset_ref(assets, &f.asset_id);
                if src.is_empty() {
                    if caption.is_empty() {
                        String::new()
                    } else {
                        format!("\\caption{{{}}}", caption)
                    }
                } else {
                    format!(
                        "\\includegraphics[width=0.8\\textwidth]{{{}}}\n\\caption{{{}}}",
                        src, caption
                    )
                }
            }
        }
        Block::List(l) => render_list(l, assets),
        Block::Quote(q) => render_quote(q, assets),
        Block::Code(c) => render_code(c),
        Block::HorizontalRule(_) => "\\bigskip\\hrule\\bigskip".to_string(),
        Block::Handwriting(hw) => {
            let text = render_inlines(&hw.inlines, assets);
            format!("\\texttt{{{}}}", text)
        }
        Block::DescriptionList(dl) => render_description_list(dl, assets),
        Block::TableOfContents => "\\tableofcontents".to_string(),
        Block::Theorem(t) => render_theorem(t, assets),
        Block::Proof(p) => render_proof(p, assets),
        Block::Minipage(m) => render_minipage(m, assets),
        Block::Float(f) => render_float(f, assets),
        Block::TextBox(tb) => render_blocks(&tb.content, assets),
        Block::Chart(c) => format!("% [Chart: {:?}]\n\\textit{{[chart]}}", c.chart_type),
        Block::Shape(s) => format!("% [Shape: {:?}]\n\\textit{{[shape]}}", s.shape_type),
        Block::EmbeddedObject(e) => {
            format!("% [Embedded object: {:?}]\n\\textit{{[embedded]}}", e.kind)
        }
        Block::Annotation(a) => format!("% [Annotation: {:?}]\n\\textit{{[annotation]}}", a.kind),
        Block::PageBreak(_) => {
            "% [PageBreak]\n\\newpage".to_string()
        }
        Block::SectionBreak(sb) => {
            format!("% [SectionBreak: {:?}]\n\\textit{{[section break]}}", sb.kind)
        }
        Block::HeaderFooter(hf) => {
            format!(
                "% [HeaderFooter: {:?} {:?}]\n\\textit{{[header/footer]}}",
                hf.kind, hf.applies_to
            )
        }
    }
}

fn render_blocks(blocks: &[Block], assets: &[MediaAsset]) -> String {
    blocks
        .iter()
        .map(|b| render_block(b, assets))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_block_display(block: &Block, assets: &[MediaAsset]) -> String {
    match block {
        Block::Formula(f) => {
            let latex = f.formula.as_latex();
            format!("\\[\n{}\n\\]", latex)
        }
        _ => render_block(block, assets),
    }
}

fn render_block_equation(block: &Block, assets: &[MediaAsset]) -> String {
    match block {
        Block::Formula(f) => {
            let latex = f.formula.as_latex();
            format!("\\begin{{equation}}\n{}\n\\end{{equation}}", latex)
        }
        _ => render_block(block, assets),
    }
}

fn render_inlines(inlines: &[Inline], assets: &[MediaAsset]) -> String {
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
            Inline::Image(img) => {
                parts.push(resolve_image_latex(assets, &img.asset_id));
            }
            Inline::Footnote { content } => {
                let inner = render_inlines(&[*content.clone()], assets);
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
            Inline::LineBreak | Inline::SoftBreak => {
                parts.push("\\newline ".to_string());
            }
            Inline::Span(s) => {
                parts.push(render_inlines(&s.content, assets));
            }
            Inline::Link(l) => {
                let text = render_inlines(&l.content, assets);
                parts.push(format!("\\href{{{}}}{{{}}}", l.target, text));
            }
            Inline::Code(c) => {
                parts.push(format!("\\texttt{{{}}}", c.code));
            }
            Inline::Superscript(inner) => {
                let text = render_inlines(inner, assets);
                parts.push(format!("$^{{{}}}$", text));
            }
            Inline::Subscript(inner) => {
                let text = render_inlines(inner, assets);
                parts.push(format!("$_{{{}}}$", text));
            }
        }
    }
    parts.join(" ")
}

fn render_list(l: &latexsnipper_ast::ListBlock, assets: &[MediaAsset]) -> String {
    let env = if l.is_ordered() { "enumerate" } else { "itemize" };
    let mut items = Vec::new();
    for item in &l.items {
        let text = render_blocks(&item.content, assets);
        items.push(format!("\\item {}", text));
    }
    format!("\\begin{{{}}}\n{}\n\\end{{{}}}", env, items.join("\n"), env)
}

fn render_quote(q: &latexsnipper_ast::QuoteBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&q.blocks, assets);
    let mut result = format!("\\begin{{quote}}\n{}\n\\end{{quote}}", content);
    if let Some(attr) = &q.attribution {
        result.push_str(&format!("\n\\attribution{{{}}}", attr));
    }
    result
}

fn render_code(c: &latexsnipper_ast::CodeBlock) -> String {
    if let Some(lang) = &c.language {
        format!(
            "\\begin{{lstlisting}}[language={}]\n{}\n\\end{{lstlisting}}",
            lang, c.code
        )
    } else {
        format!("\\begin{{lstlisting}}\n{}\n\\end{{lstlisting}}", c.code)
    }
}

fn render_description_list(
    dl: &latexsnipper_ast::DescriptionListBlock,
    assets: &[MediaAsset],
) -> String {
    let mut items = Vec::new();
    for item in &dl.items {
        if let Some(label) = &item.label {
            let label_text = render_inlines(label, assets);
            items.push(format!("\\item[{}]", label_text));
        }
        for block in &item.content {
            let content = render_block(block, assets);
            items.push(content);
        }
    }
    format!(
        "\\begin{{description}}\n{}\n\\end{{description}}",
        items.join("\n")
    )
}

fn render_theorem(t: &latexsnipper_ast::TheoremBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&t.content, assets);
    let number = t.number.as_deref().unwrap_or("");
    format!(
        "\\begin{{{}}}{}\n{}\n\\end{{{}}}",
        t.name, number, content, t.name
    )
}

fn render_proof(p: &latexsnipper_ast::ProofBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&p.content, assets);
    format!("\\begin{{proof}}\n{}\n\\end{{proof}}", content)
}

fn render_minipage(m: &latexsnipper_ast::MinipageBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&m.content, assets);
    format!(
        "\\begin{{minipage}}{{{}}}\n{}\n\\end{{minipage}}",
        m.width, content
    )
}

fn render_float(f: &latexsnipper_ast::FloatBlock, assets: &[MediaAsset]) -> String {
    let content = render_blocks(&f.content, assets);
    let mut result = format!("\\begin{{{}}}", f.env);
    if let Some(placement) = &f.placement {
        result.push_str(&format!("[{}]", placement));
    }
    result.push('\n');
    result.push_str(&content);
    if let Some(caption) = &f.caption {
        let caption_text = render_inlines(caption, assets);
        result.push_str(&format!("\n\\caption{{{}}}", caption_text));
    }
    result.push_str(&format!("\n\\end{{{}}}", f.env));
    result
}

fn render_table(t: &latexsnipper_ast::TableBlock, assets: &[MediaAsset]) -> String {
    if t.rows.is_empty() {
        return String::new();
    }

    let col_count = t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    let col_spec = "|".to_string() + &"c|".repeat(col_count);

    let mut rows = Vec::new();
    for row in &t.rows {
        let cells: Vec<String> = row
            .cells
            .iter()
            .map(|cell| {
                let cell_inlines = cell.collect_inlines();
                let content = render_inlines(&cell_inlines, assets);
                let mut result = String::new();
                if cell.colspan > 1 {
                    result.push_str(&format!(
                        "\\multicolumn{{{}}}{{|c|}}{{{}}}",
                        cell.colspan, content
                    ));
                } else {
                    result.push_str(&content);
                }
                result
            })
            .collect();
        rows.push(format!("{} \\\\", cells.join(" & ")));
    }

    format!(
        "\\begin{{tabular}}{{{}}}\n{}\n\\end{{tabular}}",
        col_spec,
        rows.join("\n")
    )
}
