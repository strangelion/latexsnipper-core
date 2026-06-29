use latexsnipper_ast::{Document, Block, Inline, Formula, FormulaSource};
use latexsnipper_foundation::Result;

use crate::converter::Converter;

/// Converts Document AST to MathML format (standard namespace).
pub struct MathmlConverter;

impl Converter for MathmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Standard)
    }
    fn name(&self) -> &str { "mathml" }
    fn extension(&self) -> &str { "xml" }
    fn mime_type(&self) -> &str { "application/mathml+xml" }
}

/// Converts Document AST to MathML format (mml: prefix).
pub struct MathmlMmlConverter;

impl Converter for MathmlMmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Mml)
    }
    fn name(&self) -> &str { "mathml_mml" }
    fn extension(&self) -> &str { "mml" }
    fn mime_type(&self) -> &str { "application/mathml+xml" }
}

/// Converts Document AST to MathML format (m: prefix).
pub struct MathmlMConverter;

impl Converter for MathmlMConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::M)
    }
    fn name(&self) -> &str { "mathml_m" }
    fn extension(&self) -> &str { "xml" }
    fn mime_type(&self) -> &str { "application/mathml+xml" }
}

/// Converts Document AST to MathML format (attribute form).
pub struct MathmlAttrConverter;

impl Converter for MathmlAttrConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Attr)
    }
    fn name(&self) -> &str { "mathml_attr" }
    fn extension(&self) -> &str { "xml" }
    fn mime_type(&self) -> &str { "application/mathml+xml" }
}

enum MathmlMode {
    Standard,
    Mml,
    M,
    Attr,
}

fn convert_mathml(doc: &Document, mode: MathmlMode) -> Result<String> {
    let mut parts = Vec::new();

    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Formula(f) => {
                    parts.push(convert_formula_to_mathml(&f.formula, &mode));
                }
                Block::Paragraph(p) => {
                    for inline in &p.inlines {
                        match inline {
                            Inline::Text(t) => {
                                parts.push(format!("<mtext>{}</mtext>", xml_escape(&t.text)));
                            }
                            Inline::Formula(f) => {
                                parts.push(convert_formula_to_mathml(f, &mode));
                            }
                            Inline::Image(_) => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let content = parts.join("\n");
    match &mode {
        MathmlMode::Standard => Ok(format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</math>", content)),
        MathmlMode::Mml => Ok(format!("<mml:math xmlns:mml=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</mml:math>", content)),
        MathmlMode::M => Ok(format!("<m:math xmlns:m=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</m:math>", content)),
        MathmlMode::Attr => Ok(format!("<math mathmode=\"inline\" xmlns=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</math>", content)),
    }
}

fn convert_formula_to_mathml(f: &Formula, _mode: &MathmlMode) -> String {
    let content = match &f.source {
        FormulaSource::Latex(s) => latex_to_mathml(s),
        FormulaSource::MathML(s) => s.clone(),
        FormulaSource::Omml(s) => format!("<mrow><mi>{}</mi></mrow>", xml_escape(s)),
        FormulaSource::Typst(s) => latex_to_mathml(&typst_to_latex_approx(s)),
    };

    if f.display_mode {
        format!("<displaymath>\n{}\n</displaymath>", content)
    } else {
        format!("<inlinemath>\n{}\n</inlinemath>", content)
    }
}

fn latex_to_mathml(latex: &str) -> String {
    let latex = latex.trim();

    if let Some(inner) = latex.strip_prefix("\\frac{") {
        if let Some((num, den)) = split_brace_pair(inner) {
            return format!(
                "<mfrac>\n  <mrow>{}</mrow>\n  <mrow>{}</mrow>\n</mfrac>",
                latex_to_mathml(num),
                latex_to_mathml(den)
            );
        }
    }

    if let Some(inner) = latex.strip_prefix("\\sqrt{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!("<msqrt><mrow>{}</mrow></msqrt>", latex_to_mathml(content));
    }

    if let Some((base, sup)) = split_superscript(latex) {
        return format!(
            "<msup>\n  <mrow>{}</mrow>\n  <mrow>{}</mrow>\n</msup>",
            latex_to_mathml(base),
            latex_to_mathml(sup)
        );
    }

    if let Some((base, sub)) = split_subscript(latex) {
        return format!(
            "<msub>\n  <mrow>{}</mrow>\n  <mrow>{}</mrow>\n</msub>",
            latex_to_mathml(base),
            latex_to_mathml(sub)
        );
    }

    if let Some(sym) = map_symbol_mathml(latex) {
        return sym.to_string();
    }

    if latex.len() == 1 && latex.chars().next().unwrap().is_alphabetic() {
        format!("<mi>{}</mi>", latex)
    } else if latex.parse::<f64>().is_ok() {
        format!("<mn>{}</mn>", latex)
    } else {
        format!("<mi>{}</mi>", xml_escape(latex))
    }
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

fn map_symbol_mathml(latex: &str) -> Option<&str> {
    match latex {
        "\\alpha" | "alpha" => Some("<mi>&alpha;</mi>"),
        "\\beta" | "beta" => Some("<mi>&beta;</mi>"),
        "\\gamma" | "gamma" => Some("<mi>&gamma;</mi>"),
        "\\delta" | "delta" => Some("<mi>&delta;</mi>"),
        "\\theta" | "theta" => Some("<mi>&theta;</mi>"),
        "\\lambda" | "lambda" => Some("<mi>&lambda;</mi>"),
        "\\sigma" | "sigma" => Some("<mi>&sigma;</mi>"),
        "\\omega" | "omega" => Some("<mi>&omega;</mi>"),
        "\\pi" | "pi" => Some("<mi>&pi;</mi>"),
        "\\infty" | "infinity" => Some("<mi>&infin;</mi>"),
        "\\pm" | "plus.minus" => Some("<mo>&pm;</mo>"),
        "\\times" | "times" => Some("<mo>&times;</mo>"),
        "\\div" | "div" => Some("<mo>&divide;</mo>"),
        "\\cdot" | "dot" => Some("<mo>&sdot;</mo>"),
        "\\leq" | "lt.eq" => Some("<mo>&leq;</mo>"),
        "\\geq" | "gt.eq" => Some("<mo>&geq;</mo>"),
        "\\neq" | "neq" => Some("<mo>&neq;</mo>"),
        "\\approx" | "approx" => Some("<mo>&approx;</mo>"),
        "\\rightarrow" | "rightarrow" => Some("<mo>&rarr;</mo>"),
        "\\leftarrow" | "leftarrow" => Some("<mo>&larr;</mo>"),
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

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
