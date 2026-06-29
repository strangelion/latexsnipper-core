use latexsnipper_ast::{Document, Block, Inline, Formula, FormulaSource};
use latexsnipper_foundation::Result;

use crate::converter::Converter;

/// Converts Document AST to Typst format.
pub struct TypstConverter;

impl Converter for TypstConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();

        for page in &doc.pages {
            for block in &page.blocks {
                match block {
                    Block::Formula(f) => {
                        let content = convert_formula_to_typst(&f.formula);
                        if f.formula.display_mode {
                            parts.push(format!("$ {} $", content));
                        } else {
                            parts.push(content);
                        }
                    }
                    Block::Paragraph(p) => {
                        let text = render_paragraph(p);
                        if !text.is_empty() {
                            parts.push(text);
                        }
                    }
                    Block::Table(t) => {
                        parts.push(render_table(t));
                    }
                    Block::Figure(f) => {
                        if let Some(caption) = &f.caption {
                            parts.push(format!("// {}", caption));
                        }
                    }
                }
            }
        }

        Ok(parts.join("\n\n"))
    }

    fn name(&self) -> &str { "typst" }
    fn extension(&self) -> &str { "typ" }
    fn mime_type(&self) -> &str { "text/plain" }
}

fn convert_formula_to_typst(f: &Formula) -> String {
    match &f.source {
        FormulaSource::Typst(s) => s.clone(),
        FormulaSource::Latex(s) => latex_to_typst(s),
        FormulaSource::Omml(s) => latex_to_typst(s),
        FormulaSource::MathML(s) => format!("\"{}\"", s),
    }
}

fn latex_to_typst(latex: &str) -> String {
    let mut result = latex.to_string();

    // Handle \frac{num}{den} → (num)/(den) using proper parsing
    result = convert_frac_to_typst(&result);

    let mappings = [
        ("\\sqrt{", "sqrt("),
        ("\\int", "integral"), ("\\sum", "sum"), ("\\prod", "product"),
        ("\\infty", "infinity"), ("\\pi", "pi"),
        ("\\alpha", "alpha"), ("\\beta", "beta"), ("\\gamma", "gamma"),
        ("\\delta", "delta"), ("\\theta", "theta"), ("\\lambda", "lambda"),
        ("\\sigma", "sigma"), ("\\omega", "omega"),
        ("\\pm", "plus.minus"), ("\\times", "times"), ("\\div", "div"),
        ("\\cdot", "dot"), ("\\leq", "lt.eq"), ("\\geq", "gt.eq"),
        ("\\neq", "neq"), ("\\approx", "approx"),
        ("\\rightarrow", "rightarrow"), ("\\leftarrow", "leftarrow"),
        ("\\in", "in"), ("\\notin", "notin"), ("\\subset", "subset"),
        ("\\cup", "union"), ("\\cap", "intersect"),
    ];

    for (from, to) in &mappings {
        result = result.replace(from, to);
    }

    result = result.replace("\\", "");
    result = result.replace("{", "");
    result = result.replace("}", "");

    result
}

fn convert_frac_to_typst(latex: &str) -> String {
    let mut result = String::new();
    let mut remaining = latex;

    while let Some(pos) = remaining.find("\\frac{") {
        result.push_str(&remaining[..pos]);
        let after = &remaining[pos + 6..];

        if let Some((num, den, consumed)) = parse_frac_args(after) {
            result.push_str(&format!("({})/({})", num, den));
            remaining = &after[consumed..];
        } else {
            result.push_str("\\frac{");
            remaining = &after;
        }
    }

    result.push_str(remaining);
    result
}

fn parse_frac_args(s: &str) -> Option<(&str, &str, usize)> {
    let s = s.trim_start();
    let offset = s.len() - s.trim_start().len();

    // After \frac{, the input is like "a+b}{c}" where the first { is already consumed
    // We need to find the first } that closes the first argument
    // Then find {content} for the second argument

    // Find first }
    let first_end = s.find('}')?;
    let first = &s[..first_end];

    // Find second argument after }
    let rest = &s[first_end + 1..];
    let rest = rest.trim_start();

    let second = if rest.starts_with('{') {
        let mut d = 0i32;
        let mut close = None;
        for (i, b) in rest.bytes().enumerate() {
            match b {
                b'{' => d += 1,
                b'}' => {
                    d -= 1;
                    if d == 0 {
                        close = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let c = close?;
        &rest[1..c]
    } else {
        return None;
    };

    // Calculate total consumed length
    let consumed = offset + first_end + 1 + rest.len();

    Some((first, second, consumed))
}

fn render_paragraph(p: &latexsnipper_ast::ParagraphBlock) -> String {
    let mut parts = Vec::new();
    for inline in &p.inlines {
        match inline {
            Inline::Text(t) => {
                let mut text = t.text.clone();
                if t.bold == Some(true) {
                    text = format!("*{}*", text);
                }
                if t.italic == Some(true) {
                    text = format!("_{}_", text);
                }
                parts.push(text);
            }
            Inline::Formula(f) => {
                let content = convert_formula_to_typst(f);
                if f.display_mode {
                    parts.push(format!("$ {} $", content));
                } else {
                    parts.push(content);
                }
            }
            Inline::Image(_) => {
                parts.push("#image(\"image.png\")".to_string());
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
    lines.push(format!("table(columns: {},", cols));

    for row in &t.rows {
        let cells: Vec<String> = row.iter().map(|cell| {
            let text: String = cell.inlines.iter().filter_map(|i| {
                if let Inline::Text(t) = i { Some(t.text.as_str()) } else { None }
            }).collect();
            format!("[{}]", text)
        }).collect();
        lines.push(format!("  ({}),", cells.join(", ")));
    }

    lines.push(")".to_string());
    lines.join("\n")
}
