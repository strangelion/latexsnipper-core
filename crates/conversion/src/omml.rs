use latexsnipper_ast::{Document, Block, Inline, Formula, FormulaSource};
use latexsnipper_foundation::Result;

use crate::converter::Converter;
use crate::latex_utils::*;

pub struct OmmlConverter;

impl Converter for OmmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();
        for page in &doc.pages {
            for block in &page.blocks {
                match block {
                    Block::Formula(f) => parts.push(convert_formula_to_omml(&f.formula)),
                    Block::Paragraph(p) => {
                        for inline in &p.inlines {
                            match inline {
                                Inline::Text(t) => parts.push(wrap_mtext(&t.text)),
                                Inline::Formula(f) => parts.push(convert_formula_to_omml(f)),
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
        FormulaSource::Typst(s) => latex_to_omml(&typst_to_latex(s)),
        FormulaSource::MathML(s) => format!("<m:oMath>\n{}\n</m:oMath>", s),
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
            return format!("<m:f>\n  <m:num>{}</m:num>\n  <m:den>{}</m:den>\n</m:f>",
                latex_to_omml(num), latex_to_omml(den));
        }
    }

    if let Some(inner) = latex.strip_prefix("\\sqrt{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!("<m:rad>\n  <m:radPr><m:degHide m:val=\"1\"/></m:radPr>\n  <m:deg/>\n  <m:e>{}</m:e>\n</m:rad>",
            latex_to_omml(content));
    }

    if let Some(inner) = latex.strip_prefix("\\sqrt[") {
        if let Some((degree, rest)) = inner.split_once(']') {
            let content = rest.strip_prefix('{').unwrap_or(rest).strip_suffix('}').unwrap_or(rest);
            return format!("<m:rad>\n  <m:deg>{}</m:deg>\n  <m:e>{}</m:e>\n</m:rad>",
                latex_to_omml(degree), latex_to_omml(content));
        }
    }

    if let Some(inner) = latex.strip_prefix("\\overbrace{") {
        if let Some((content, rest)) = split_brace_pair(inner) {
            let label = rest.trim_start().strip_prefix("^{").unwrap_or("").strip_suffix('}').unwrap_or("");
            return format!("<m:bar>\n  <m:barPr><m:pos m:val=\"top\"/></m:barPr>\n  <m:e>{}</m:e>\n</m:bar>",
                if label.is_empty() { latex_to_omml(content) }
                else { format!("<m:sSup><m:e>{}</m:e><m:sup><m:r><m:t>{}</m:t></m:r></m:sup></m:sSup>",
                    latex_to_omml(content), label) });
        }
    }

    if let Some(inner) = latex.strip_prefix("\\underbrace{") {
        if let Some((content, rest)) = split_brace_pair(inner) {
            let label = rest.trim_start().strip_prefix("_{").unwrap_or("").strip_suffix('}').unwrap_or("");
            return format!("<m:bar>\n  <m:barPr><m:pos m:val=\"bottom\"/></m:barPr>\n  <m:e>{}</m:e>\n</m:bar>",
                if label.is_empty() { latex_to_omml(content) }
                else { format!("<m:sSub><m:e>{}</m:e><m:sub><m:r><m:t>{}</m:t></m:r></m:sub></m:sSub>",
                    latex_to_omml(content), label) });
        }
    }

    // Matrix environments
    if let Some(inner) = extract_env(latex, "matrix") { return matrix_to_omml(inner, "m:m"); }
    if let Some(inner) = extract_env(latex, "pmatrix") { return matrix_to_omml_parens(inner, "(", ")"); }
    if let Some(inner) = extract_env(latex, "bmatrix") { return matrix_to_omml_parens(inner, "[", "]"); }
    if let Some(inner) = extract_env(latex, "vmatrix") { return matrix_to_omml_parens(inner, "|", "|"); }
    if let Some(inner) = extract_env(latex, "cases") { return cases_to_omml(inner); }
    if let Some(inner) = extract_env(latex, "aligned") { return aligned_to_omml(inner); }

    // Array environment (similar to matrix but with column alignment)
    if let Some(inner) = extract_env(latex, "array") { return matrix_to_omml(inner, "m:m"); }

    // \phantom — zero-width placeholder
    if let Some(inner) = latex.strip_prefix("\\phantom{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!("<m:r><m:t>{}</m:t></m:r>", " ".repeat(content.len()));
    }

    if let Some((base, sup)) = split_superscript(latex) {
        return format!("<m:sSup>\n  <m:e>{}</m:e>\n  <m:sup>{}</m:sup>\n</m:sSup>",
            latex_to_omml(base), latex_to_omml(sup));
    }

    if let Some((base, sub)) = split_subscript(latex) {
        return format!("<m:sSub>\n  <m:e>{}</m:e>\n  <m:sub>{}</m:sub>\n</m:sSub>",
            latex_to_omml(base), latex_to_omml(sub));
    }

    if let Some(sym) = map_large_op(latex) {
        return format!("<m:nary>\n  <m:naryPr><m:chr m:val=\"{}\"/></m:naryPr>\n</m:nary>", sym);
    }

    if let Some(sym) = map_symbol_unicode(latex) {
        return format!("<m:r><m:t>{}</m:t></m:r>", sym);
    }

    wrap_mtext(latex)
}

fn wrap_mtext(text: &str) -> String {
    format!("<m:r><m:t>{}</m:t></m:r>", xml_escape(text))
}

fn matrix_to_omml(content: &str, _tag: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut result = String::from("<m:mRow>\n");
    for row in &rows {
        let cells: Vec<String> = row.iter()
            .map(|cell| format!("  <m:e>{}</m:e>", latex_to_omml(cell.trim())))
            .collect();
        result.push_str(&format!("  <m:r>\n{}\n  </m:r>\n", cells.join("\n")));
    }
    result.push_str("</m:mRow>");
    result
}

fn matrix_to_omml_parens(content: &str, open: &str, close: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut cells_xml = Vec::new();
    for row in &rows {
        let cell_xml: Vec<String> = row.iter()
            .map(|cell| format!("  <m:e>{}</m:e>", latex_to_omml(cell.trim())))
            .collect();
        cells_xml.push(format!("  <m:r>\n{}\n  </m:r>", cell_xml.join("\n")));
    }
    format!("<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n{}\n</m:d>",
        xml_escape(open), xml_escape(close), cells_xml.join("\n"))
}

fn cases_to_omml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let left = row.get(0).map(|s| latex_to_omml(s.trim())).unwrap_or_default();
        let right = row.get(1).map(|s| latex_to_omml(s.trim())).unwrap_or_default();
        rows_xml.push(format!("  <m:r>\n    <m:e>{}</m:e>\n    <m:e>{}</m:e>\n  </m:r>", left, right));
    }
    format!("<m:d>\n  <m:dPr><m:begChr m:val=\"{{\"/><m:endChr m:val=\"}}\"/></m:dPr>\n{}\n</m:d>",
        rows_xml.join("\n"))
}

fn aligned_to_omml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let cells: Vec<String> = row.iter()
            .map(|cell| format!("  <m:e>{}</m:e>", latex_to_omml(cell.trim())))
            .collect();
        rows_xml.push(format!("  <m:r>\n{}\n  </m:r>", cells.join("\n")));
    }
    format!("<m:mRow>\n{}\n</m:mRow>", rows_xml.join("\n"))
}
