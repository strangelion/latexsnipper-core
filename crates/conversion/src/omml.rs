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

    // Fraction: \frac{num}{den}
    if let Some(inner) = latex.strip_prefix("\\frac{") {
        if let Some((num, den)) = split_brace_pair(inner) {
            return format!(
                "<m:f>\n  <m:num>{}</m:num>\n  <m:den>{}</m:den>\n</m:f>",
                latex_to_omml(num),
                latex_to_omml(den)
            );
        }
    }

    // Square root: \sqrt{x}
    if let Some(inner) = latex.strip_prefix("\\sqrt{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!("<m:rad>\n  <m:radPr><m:degHide m:val=\"1\"/></m:radPr>\n  <m:deg/>\n  <m:e>{}</m:e>\n</m:rad>", latex_to_omml(content));
    }

    // Overbrace: \overbrace{content}^{label}
    if let Some(inner) = latex.strip_prefix("\\overbrace{") {
        if let Some((content, rest)) = split_brace_pair(inner) {
            let label = rest.trim_start().strip_prefix("^{").unwrap_or("").strip_suffix('}').unwrap_or("");
            return format!(
                "<m:bar>\n  <m:barPr><m:pos m:val=\"top\"/></m:barPr>\n  <m:e>{}</m:e>\n</m:bar>",
                if label.is_empty() { latex_to_omml(content) } else { format!("<m:sSup><m:e>{}</m:e><m:sup><m:r><m:t>{}</m:t></m:r></m:sup></m:sSup>", latex_to_omml(content), label) }
            );
        }
    }

    // Underbrace: \underbrace{content}_{label}
    if let Some(inner) = latex.strip_prefix("\\underbrace{") {
        if let Some((content, rest)) = split_brace_pair(inner) {
            let label = rest.trim_start().strip_prefix("_{").unwrap_or("").strip_suffix('}').unwrap_or("");
            return format!(
                "<m:bar>\n  <m:barPr><m:pos m:val=\"bottom\"/></m:barPr>\n  <m:e>{}</m:e>\n</m:bar>",
                if label.is_empty() { latex_to_omml(content) } else { format!("<m:sSub><m:e>{}</m:e><m:sub><m:r><m:t>{}</m:t></m:r></m:sub></m:sSub>", latex_to_omml(content), label) }
            );
        }
    }

    // Square root with degree: \sqrt[n]{x}
    if let Some(inner) = latex.strip_prefix("\\sqrt[") {
        if let Some((degree, rest)) = inner.split_once(']') {
            let content = rest.strip_prefix('{').unwrap_or(rest).strip_suffix('}').unwrap_or(rest);
            return format!("<m:rad>\n  <m:deg>{}</m:deg>\n  <m:e>{}</m:e>\n</m:rad>", latex_to_omml(degree), latex_to_omml(content));
        }
    }

    // Matrix: \begin{matrix}...\end{matrix} and variants
    if let Some(inner) = extract_env(latex, "matrix") {
        return matrix_to_omml(inner, "m:m");
    }
    if let Some(inner) = extract_env(latex, "pmatrix") {
        return matrix_to_omml_parens(inner, "(", ")");
    }
    if let Some(inner) = extract_env(latex, "bmatrix") {
        return matrix_to_omml_parens(inner, "[", "]");
    }
    if let Some(inner) = extract_env(latex, "vmatrix") {
        return matrix_to_omml_parens(inner, "|", "|");
    }

    // Cases: \begin{cases}...\end{cases}
    if let Some(inner) = extract_env(latex, "cases") {
        return cases_to_omml(inner);
    }

    // Aligned: \begin{aligned}...\end{aligned}
    if let Some(inner) = extract_env(latex, "aligned") {
        return aligned_to_omml(inner);
    }

    // Superscript: a^{b}
    if let Some((base, sup)) = split_superscript(latex) {
        return format!(
            "<m:sSup>\n  <m:e>{}</m:e>\n  <m:sup>{}</m:sup>\n</m:sSup>",
            latex_to_omml(base),
            latex_to_omml(sup)
        );
    }

    // Subscript: a_{b}
    if let Some((base, sub)) = split_subscript(latex) {
        return format!(
            "<m:sSub>\n  <m:e>{}</m:e>\n  <m:sub>{}</m:sub>\n</m:sSub>",
            latex_to_omml(base),
            latex_to_omml(sub)
        );
    }

    // Large operators: \sum, \int, \prod
    if let Some(sym) = map_large_op(latex) {
        return format!("<m:nary>\n  <m:naryPr><m:chr m:val=\"{}\"/></m:naryPr>\n</m:nary>", sym);
    }

    // Greek letters and symbols
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

// ── Environment extraction ──

/// Extract content from \begin{env}...\end{env}
fn extract_env<'a>(latex: &'a str, env: &str) -> Option<&'a str> {
    let begin_tag = format!("\\begin{{{}}}", env);
    let end_tag = format!("\\end{{{}}}", env);

    let start = latex.find(&begin_tag)?;
    let after_begin = &latex[start + begin_tag.len()..];
    let end = after_begin.find(&end_tag)?;
    Some(after_begin[..end].trim())
}

// ── Matrix conversion ──

/// Convert matrix content to OMML table.
fn matrix_to_omml(content: &str, tag: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut result = String::from("<m:mRow>\n");

    for row in &rows {
        let cells: Vec<String> = row.iter().map(|cell| {
            format!("  <m:e>{}</m:e>", latex_to_omml(cell.trim()))
        }).collect();
        result.push_str(&format!("  <m:r>\n{}\n  </m:r>\n", cells.join("\n")));
    }

    result.push_str("</m:mRow>");
    result
}

/// Convert matrix with parentheses/brackets.
fn matrix_to_omml_parens(content: &str, open: &str, close: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut cells_xml = Vec::new();

    for row in &rows {
        let cell_xml: Vec<String> = row.iter().map(|cell| {
            format!("  <m:e>{}</m:e>", latex_to_omml(cell.trim()))
        }).collect();
        cells_xml.push(format!("  <m:r>\n{}\n  </m:r>", cell_xml.join("\n")));
    }

    format!("<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n{}\n</m:d>",
        xml_escape(open), xml_escape(close), cells_xml.join("\n"))
}

/// Split matrix rows by \\ separator.
fn split_matrix_rows(content: &str) -> Vec<Vec<&str>> {
    content.split('\\')
        .filter(|s| !s.trim().is_empty() && s.trim() != "\\")
        .map(|row| {
            row.split('&')
                .filter(|s| !s.trim().is_empty())
                .collect()
        })
        .filter(|row: &Vec<&str>| !row.is_empty())
        .collect()
}

// ── Cases conversion ──

/// Convert cases environment to OMML.
fn cases_to_omml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();

    for row in &rows {
        let left = row.get(0).map(|s| latex_to_omml(s.trim())).unwrap_or_default();
        let right = row.get(1).map(|s| latex_to_omml(s.trim())).unwrap_or_default();
        rows_xml.push(format!("  <m:r>\n    <m:e>{}</m:e>\n    <m:e>{}</m:e>\n  </m:r>", left, right));
    }

    let beg_chr = r#"{"#;
    let end_chr = "}";
    format!("<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n{}\n</m:d>",
        beg_chr, end_chr, rows_xml.join("\n"))
}

// ── Aligned conversion ──

/// Convert aligned environment to OMML.
fn aligned_to_omml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();

    for row in &rows {
        let cells: Vec<String> = row.iter().map(|cell| {
            format!("  <m:e>{}</m:e>", latex_to_omml(cell.trim()))
        }).collect();
        rows_xml.push(format!("  <m:r>\n{}\n  </m:r>", cells.join("\n")));
    }

    format!("<m:mRow>\n{}\n</m:mRow>", rows_xml.join("\n"))
}

// ── Large operators ──

fn map_large_op(latex: &str) -> Option<&str> {
    match latex {
        "\\sum" => Some("∑"),
        "\\prod" => Some("∏"),
        "\\int" => Some("∫"),
        "\\iint" => Some("∬"),
        "\\iiint" => Some("∭"),
        "\\oint" => Some("∮"),
        "\\bigcup" => Some("⋃"),
        "\\bigcap" => Some("⋂"),
        _ => None,
    }
}
