use latexsnipper_ast::{Document, Block, Inline, Formula, FormulaSource};
use latexsnipper_foundation::Result;

use crate::converter::Converter;

/// Converts Document AST to OMML (Office Math Markup Language).
pub struct OmmlConverter;

impl Converter for OmmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();

        for page in &doc.pages {
            for block in &page.blocks {
                match block {
                    Block::Formula(f) => {
                        parts.push(convert_formula_to_omml(&f.formula));
                    }
                    Block::Paragraph(p) => {
                        for inline in &p.inlines {
                            match inline {
                                Inline::Text(t) => {
                                    parts.push(wrap_mtext(&t.text));
                                }
                                Inline::Formula(f) => {
                                    parts.push(convert_formula_to_omml(f));
                                }
                                Inline::Image(_) => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(parts.join("\n"))
    }

    fn name(&self) -> &str { "omml" }
    fn extension(&self) -> &str { "xml" }
    fn mime_type(&self) -> &str { "application/officeDocument+xml" }
}

fn convert_formula_to_omml(f: &Formula) -> String {
    let content = match &f.source {
        FormulaSource::Latex(s) => latex_to_omml(s),
        FormulaSource::Omml(s) => s.clone(),
        FormulaSource::Typst(s) => latex_to_omml(&typst_to_latex_approx(s)),
        FormulaSource::MathML(s) => wrap_omath(s),
    };

    if f.display_mode {
        format!("<m:oMathPara>{}\n</m:oMathPara>", content)
    } else {
        format!("<m:oMath>{}\n</m:oMath>", content)
    }
}

fn latex_to_omml(latex: &str) -> String {
    let latex = latex.trim();

    if let Some(inner) = latex.strip_prefix("\\frac{") {
        if let Some((num, den)) = split_brace_pair(inner) {
            return format!(
                "<m:f>\n  <m:num>{}</m:num>\n  <m:den>{}</m:den>\n</m:f>",
                latex_to_omml(num),
                latex_to_omml(den)
            );
        }
    }

    if let Some(inner) = latex.strip_prefix("\\sqrt{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!("<m:rad>\n  <m:radPr><m:degHide m:val=\"1\"/></m:radPr>\n  <m:deg/>\n  <m:e>{}</m:e>\n</m:rad>", latex_to_omml(content));
    }

    if let Some((base, sup)) = split_superscript(latex) {
        return format!(
            "<m:sSup>\n  <m:e>{}</m:e>\n  <m:sup>{}</m:sup>\n</m:sSup>",
            latex_to_omml(base),
            latex_to_omml(sup)
        );
    }

    if let Some((base, sub)) = split_subscript(latex) {
        return format!(
            "<m:sSub>\n  <m:e>{}</m:e>\n  <m:sub>{}</m:sub>\n</m:sSub>",
            latex_to_omml(base),
            latex_to_omml(sub)
        );
    }

    if let Some(sym) = map_symbol(latex) {
        return format!("<m:r><m:t>{}</m:t></m:r>", sym);
    }

    wrap_mtext(latex)
}

fn typst_to_latex_approx(typst: &str) -> String {
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

fn map_symbol(latex: &str) -> Option<&str> {
    match latex {
        "\\alpha" | "alpha" => Some("α"),
        "\\beta" | "beta" => Some("β"),
        "\\gamma" | "gamma" => Some("γ"),
        "\\delta" | "delta" => Some("δ"),
        "\\theta" | "theta" => Some("θ"),
        "\\lambda" | "lambda" => Some("λ"),
        "\\sigma" | "sigma" => Some("σ"),
        "\\omega" | "omega" => Some("ω"),
        "\\pi" | "pi" => Some("π"),
        "\\infty" | "infinity" => Some("∞"),
        "\\pm" | "plus.minus" => Some("±"),
        "\\times" | "times" => Some("×"),
        "\\div" | "div" => Some("÷"),
        "\\cdot" | "dot" => Some("·"),
        "\\leq" | "lt.eq" => Some("≤"),
        "\\geq" | "gt.eq" => Some("≥"),
        "\\neq" | "neq" => Some("≠"),
        "\\approx" | "approx" => Some("≈"),
        "\\rightarrow" | "rightarrow" => Some("→"),
        "\\leftarrow" | "leftarrow" => Some("←"),
        _ => None,
    }
}

fn split_brace_pair(s: &str) -> Option<(&str, &str)> {
    // Parse LaTeX brace pairs: {content1}{content2} or content1}{content2}
    let s = s.trim();

    // Find the first closing brace at depth 0
    let mut depth = 0i32;
    let mut first_end = None;

    for (i, b) in s.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        first_end = Some(i);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    let end = first_end?;

    // Extract first argument
    let first = if s.starts_with('{') {
        &s[1..end]
    } else {
        &s[..end]
    };

    // Find second argument after }
    let rest = &s[end + 1..];
    let rest = rest.trim_start();

    // Second argument may be {content} or just content
    let second = if rest.starts_with('{') {
        // Find matching }
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
        // No braces, take until next } or end
        rest.find('}')
            .map(|i| &rest[..i])
            .unwrap_or(rest)
    };

    Some((first, second))
}

fn split_superscript(s: &str) -> Option<(&str, &str)> {
    let pos = s.find("^{")?;
    let base = &s[..pos];
    let after = &s[pos + 2..];
    let end = after.find('}')?;
    Some((base, &after[..end]))
}

fn split_subscript(s: &str) -> Option<(&str, &str)> {
    let pos = s.find("_{")?;
    let base = &s[..pos];
    let after = &s[pos + 2..];
    let end = after.find('}')?;
    Some((base, &after[..end]))
}

fn wrap_mtext(text: &str) -> String {
    format!("<m:r><m:t>{}</m:t></m:r>", xml_escape(text))
}

fn wrap_omath(content: &str) -> String {
    format!("<m:oMath>\n{}\n</m:oMath>", content)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
