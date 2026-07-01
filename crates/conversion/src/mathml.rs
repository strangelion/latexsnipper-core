use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline};
use latexsnipper_foundation::Result;

use crate::converter::Converter;
use crate::latex_utils::*;

pub struct MathmlConverter;
impl Converter for MathmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Standard)
    }
    fn name(&self) -> &str {
        "mathml"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn mime_type(&self) -> &str {
        "application/mathml+xml"
    }
}

pub struct MathmlMmlConverter;
impl Converter for MathmlMmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Mml)
    }
    fn name(&self) -> &str {
        "mathml_mml"
    }
    fn extension(&self) -> &str {
        "mml"
    }
    fn mime_type(&self) -> &str {
        "application/mathml+xml"
    }
}

pub struct MathmlMConverter;
impl Converter for MathmlMConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::M)
    }
    fn name(&self) -> &str {
        "mathml_m"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn mime_type(&self) -> &str {
        "application/mathml+xml"
    }
}

pub struct MathmlAttrConverter;
impl Converter for MathmlAttrConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Attr)
    }
    fn name(&self) -> &str {
        "mathml_attr"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn mime_type(&self) -> &str {
        "application/mathml+xml"
    }
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
                Block::Formula(f) => parts.push(convert_formula_to_mathml(&f.formula, &mode)),
                Block::Paragraph(p) => {
                    for inline in &p.inlines {
                        match inline {
                            Inline::Text(t) => {
                                parts.push(format!("<mtext>{}</mtext>", xml_escape(&t.text)))
                            }
                            Inline::Formula(f) => parts.push(convert_formula_to_mathml(f, &mode)),
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
        MathmlMode::Standard => Ok(format!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</math>",
            content
        )),
        MathmlMode::Mml => Ok(format!(
            "<mml:math xmlns:mml=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</mml:math>",
            content
        )),
        MathmlMode::M => Ok(format!(
            "<m:math xmlns:m=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</m:math>",
            content
        )),
        MathmlMode::Attr => Ok(format!(
            "<math mathmode=\"inline\" xmlns=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</math>",
            content
        )),
    }
}

fn convert_formula_to_mathml(f: &Formula, _mode: &MathmlMode) -> String {
    let content = match &f.source {
        FormulaSource::Latex(s) => latex_to_mathml(s),
        FormulaSource::MathML(s) => s.clone(),
        FormulaSource::Omml(s) => format!("<mrow><mi>{}</mi></mrow>", xml_escape(s)),
        FormulaSource::Typst(s) => latex_to_mathml(&typst_to_latex(s)),
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

    if let Some(inner) = latex.strip_prefix("\\sqrt[") {
        if let Some((degree, rest)) = inner.split_once(']') {
            let content = rest
                .strip_prefix('{')
                .unwrap_or(rest)
                .strip_suffix('}')
                .unwrap_or(rest);
            return format!(
                "<mroot><mrow>{}</mrow><mrow>{}</mrow></mroot>",
                latex_to_mathml(content),
                latex_to_mathml(degree)
            );
        }
    }

    // Matrix environments
    if let Some(inner) = extract_env(latex, "matrix") {
        return matrix_to_mathml(inner, None);
    }
    if let Some(inner) = extract_env(latex, "pmatrix") {
        return matrix_to_mathml(inner, Some(("(", ")")));
    }
    if let Some(inner) = extract_env(latex, "bmatrix") {
        return matrix_to_mathml(inner, Some(("[", "]")));
    }
    if let Some(inner) = extract_env(latex, "vmatrix") {
        return matrix_to_mathml(inner, Some(("|", "|")));
    }
    if let Some(inner) = extract_env(latex, "cases") {
        return cases_to_mathml(inner);
    }
    if let Some(inner) = extract_env(latex, "aligned") {
        return aligned_to_mathml(inner);
    }
    if let Some(inner) = extract_env(latex, "array") {
        return matrix_to_mathml(inner, None);
    }

    // \phantom
    if let Some(inner) = latex.strip_prefix("\\phantom{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!(
            "<mpadded width=\"0\" height=\"0\" depth=\"0\"><mrow>{}</mrow></mpadded>",
            latex_to_mathml(content)
        );
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

fn matrix_to_mathml(content: &str, delimiters: Option<(&str, &str)>) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                format!(
                    "    <mtd><mrow>{}</mrow></mtd>",
                    latex_to_mathml(cell.trim())
                )
            })
            .collect();
        rows_xml.push(format!("  <mtr>\n{}\n  </mtr>", cells.join("\n")));
    }
    let table = format!("<mtable>\n{}\n</mtable>", rows_xml.join("\n"));
    match delimiters {
        Some((open, close)) => format!("<mrow><mo>{}</mo>{}</mrow><mo>{}</mo>", open, table, close),
        None => table,
    }
}

fn cases_to_mathml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let left = row
            .first()
            .map(|s| latex_to_mathml(s.trim()))
            .unwrap_or_default();
        let right = row
            .get(1)
            .map(|s| latex_to_mathml(s.trim()))
            .unwrap_or_default();
        rows_xml.push(format!(
            "  <mtr><mtd><mrow>{}</mrow></mtd><mtd><mrow>{}</mrow></mtd></mtr>",
            left, right
        ));
    }
    format!(
        "<mrow><mo>{{</mo><mtable>\n{}\n</mtable><mo>}}</mo></mrow>",
        rows_xml.join("\n")
    )
}

fn aligned_to_mathml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                format!(
                    "    <mtd><mrow>{}</mrow></mtd>",
                    latex_to_mathml(cell.trim())
                )
            })
            .collect();
        rows_xml.push(format!("  <mtr>\n{}\n  </mtr>", cells.join("\n")));
    }
    format!("<mtable>\n{}\n</mtable>", rows_xml.join("\n"))
}
