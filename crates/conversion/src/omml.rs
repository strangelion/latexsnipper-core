use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline};
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
    fn name(&self) -> &str {
        "omml"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn mime_type(&self) -> &str {
        "application/officeDocument+xml"
    }
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

    // \textcolor{...}{...}
    if let Some(content) = latex.strip_prefix("\\textcolor{") {
        // Parse: color}{content}
        if let Some(close_brace) = content.find('}') {
            let color = &content[..close_brace];
            let rest = &content[close_brace + 1..];
            // rest should start with '{' for the content block
            let inner = rest
                .strip_prefix('{')
                .unwrap_or(rest)
                .strip_suffix('}')
                .unwrap_or(rest);
            let hex = color_name_to_hex(color.trim());
            let rendered = latex_to_omml(inner);
            return wrap_with_color(&rendered, &hex);
        }
    }
    if let Some(content) = latex.strip_prefix("\\color{") {
        // \color{blue} applies to rest of expression; extract color name
        if let Some(close) = content.find('}') {
            let color = &content[..close];
            let hex = color_name_to_hex(color.trim());
            let rest = &content[close + 1..];
            if rest.is_empty() {
                return wrap_with_color(&wrap_mtext(color), &hex);
            }
            // Apply color to the rest of the expression
            let rendered = latex_to_omml(rest);
            return wrap_with_color(&rendered, &hex);
        }
    }

    // \mathbf{...} — bold
    if let Some(content) = latex.strip_prefix("\\mathbf{") {
        if let Some(inner) = extract_brace_content(content) {
            let rendered = latex_to_omml(inner);
            return wrap_with_bold(&rendered);
        }
    }
    // \boldsymbol{...} — bold
    if let Some(content) = latex.strip_prefix("\\boldsymbol{") {
        if let Some(inner) = extract_brace_content(content) {
            let rendered = latex_to_omml(inner);
            return wrap_with_bold(&rendered);
        }
    }

    // \hat{x}, \vec{v}, \bar{x}, \dot{x}, \ddot{x}, \tilde{x}, \check{x}
    if let Some(content) = latex.strip_prefix("\\hat{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<m:acc>\n  <m:accPr><m:chr m:val=\"\u{0302}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
            latex_to_omml(inner)
        );
    }
    if let Some(content) = latex.strip_prefix("\\vec{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<m:acc>\n  <m:accPr><m:chr m:val=\"\u{20D7}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
            latex_to_omml(inner)
        );
    }
    if let Some(content) = latex.strip_prefix("\\bar{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<m:acc>\n  <m:accPr><m:chr m:val=\"\u{0305}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
            latex_to_omml(inner)
        );
    }
    if let Some(content) = latex.strip_prefix("\\dot{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<m:acc>\n  <m:accPr><m:chr m:val=\"\u{0307}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
            latex_to_omml(inner)
        );
    }
    if let Some(content) = latex.strip_prefix("\\ddot{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<m:acc>\n  <m:accPr><m:chr m:val=\"\u{0308}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
            latex_to_omml(inner)
        );
    }
    if let Some(content) = latex.strip_prefix("\\tilde{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<m:acc>\n  <m:accPr><m:chr m:val=\"\u{0303}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
            latex_to_omml(inner)
        );
    }
    if let Some(content) = latex.strip_prefix("\\check{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<m:acc>\n  <m:accPr><m:chr m:val=\"\u{030C}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
            latex_to_omml(inner)
        );
    }
    if let Some(content) = latex.strip_prefix("\\breve{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<m:acc>\n  <m:accPr><m:chr m:val=\"\u{0306}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
            latex_to_omml(inner)
        );
    }

    // \text{...}
    if let Some(content) = latex.strip_prefix("\\text{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!("<m:r><m:rPr><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:h-ansi=\"Cambria Math\"/><w:rStyle w:val=\"a\"/></w:rPr></m:rPr><m:t>{}</m:t></m:r>", xml_escape(inner));
    }

    // \left( ... \right)
    if latex.starts_with("\\left") {
        if let Some(content) = extract_delimited(latex) {
            return content;
        }
    }

    // \lim, \log, \sin, \cos, \tan, \ln, \exp
    for func in &[
        "\\lim", "\\log", "\\sin", "\\cos", "\\tan", "\\ln", "\\exp", "\\min", "\\max", "\\det",
        "\\gcd", "\\sup", "\\inf", "\\limsup", "\\liminf",
    ] {
        if let Some(rest) = latex.strip_prefix(func) {
            let fname = &func[1..];
            if rest.is_empty() {
                return format!(
                    "<m:func>\n  <m:fName><m:r><m:t>{}</m:t></m:r></m:fName>\n  <m:e/>\n</m:func>",
                    fname
                );
            }
            if let Some(sub_rest) = rest.strip_prefix('_') {
                if let Some((sub_base, sub_rest)) = split_brace_pair(sub_rest) {
                    let sub = latex_to_omml(sub_base);
                    if let Some((sup_base, _)) = sub_rest.split_once('^') {
                        let sup = sup_base
                            .strip_prefix('{')
                            .unwrap_or(sup_base)
                            .strip_suffix('}')
                            .unwrap_or(sup_base);
                        return format!("<m:sSubSup><m:e><m:func><m:fName><m:r><m:t>{}</m:t></m:r></m:fName><m:e/></m:func></m:e><m:sub>{}</m:sub><m:sup>{}</m:sup></m:sSubSup>", fname, sub, latex_to_omml(sup));
                    }
                    return format!("<m:sSub><m:e><m:func><m:fName><m:r><m:t>{}</m:t></m:r></m:fName><m:e/></m:func></m:e><m:sub>{}</m:sub></m:sSub>", fname, sub);
                }
            }
            return format!("<m:func>\n  <m:fName><m:r><m:t>{}</m:t></m:r></m:fName>\n  <m:e>{}</m:e>\n</m:func>", fname, latex_to_omml(rest));
        }
    }

    // \operatorname{...}
    if let Some(inner) = latex.strip_prefix("\\operatorname{") {
        let name = inner.strip_suffix('}').unwrap_or(inner);
        return wrap_mtext(&format!("{} ", name));
    }

    // \mathbb{...}, \mathcal{...}, \mathfrak{...}, \mathbf{...}, \mathrm{...}, \mathit{...}, \mathsf{...}, \mathtt{...}
    for prefix in &[
        "\\mathbb{",
        "\\mathcal{",
        "\\mathfrak{",
        "\\mathbf{",
        "\\mathrm{",
        "\\mathit{",
        "\\mathsf{",
        "\\mathtt{",
    ] {
        if let Some(inner) = latex.strip_prefix(prefix) {
            let content = inner.strip_suffix('}').unwrap_or(inner);
            return wrap_mtext(content);
        }
    }

    // \langle ... \rangle
    if latex.starts_with("\\langle") || latex.starts_with("\\left\\langle") {
        if let Some(inner) = latex.strip_prefix("\\left\\langle") {
            if let Some(pos) = inner.find("\\right\\rangle") {
                let content = &inner[..pos];
                let d = latex_to_omml(content);
                return format!(
                    "<m:d>\n  <m:dPr><m:begChr m:val=\"\u{27E8}\"/><m:endChr m:val=\"\u{27E9}\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
                    d
                );
            }
        }
        if let Some(inner) = latex.strip_prefix("\\langle") {
            let content = inner.strip_suffix("\\rangle").unwrap_or(inner);
            let content = content.strip_prefix(' ').unwrap_or(content);
            return format!(
                "<m:d>\n  <m:dPr><m:begChr m:val=\"\u{27E8}\"/><m:endChr m:val=\"\u{27E9}\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
                latex_to_omml(content)
            );
        }
    }

    // \binom{n}{k}
    if let Some(inner) = latex.strip_prefix("\\binom{") {
        if let Some((n, k)) = split_brace_pair(inner) {
            return format!(
                "<m:d>\n  <m:dPr><m:begChr m:val=\"(\"/><m:endChr m:val=\")\"/></m:dPr>\n  <m:e><m:f>\n  <m:num>{}</m:num>\n  <m:den>{}</m:den>\n</m:f></m:e>\n</m:d>",
                latex_to_omml(n),
                latex_to_omml(k)
            );
        }
    }

    // \otimes, \oplus, \nabla, \partial — single symbols used as prefix
    for cmd in &["\\otimes", "\\oplus", "\\nabla", "\\partial"] {
        if let Some(rest) = latex.strip_prefix(cmd) {
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            if rest.is_empty() {
                return wrap_mtext(map_omml_symbol(cmd).unwrap_or(cmd));
            }
            return format!(
                "{}{}",
                wrap_mtext(map_omml_symbol(cmd).unwrap_or(cmd)),
                latex_to_omml(rest)
            );
        }
    }

    // \frac{...}{...}
    if let Some(inner) = latex.strip_prefix("\\frac") {
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
        return format!("<m:rad>\n  <m:radPr><m:degHide m:val=\"1\"/></m:radPr>\n  <m:deg/>\n  <m:e>{}</m:e>\n</m:rad>",
            latex_to_omml(content));
    }

    if let Some(inner) = latex.strip_prefix("\\sqrt[") {
        if let Some((degree, rest)) = inner.split_once(']') {
            let content = rest
                .strip_prefix('{')
                .unwrap_or(rest)
                .strip_suffix('}')
                .unwrap_or(rest);
            return format!(
                "<m:rad>\n  <m:deg>{}</m:deg>\n  <m:e>{}</m:e>\n</m:rad>",
                latex_to_omml(degree),
                latex_to_omml(content)
            );
        }
    }

    if let Some(inner) = latex.strip_prefix("\\overbrace{") {
        if let Some((content, rest)) = split_brace_pair(inner) {
            let label = rest
                .trim_start()
                .strip_prefix("^{")
                .unwrap_or("")
                .strip_suffix('}')
                .unwrap_or("");
            return format!(
                "<m:bar>\n  <m:barPr><m:pos m:val=\"top\"/></m:barPr>\n  <m:e>{}</m:e>\n</m:bar>",
                if label.is_empty() {
                    latex_to_omml(content)
                } else {
                    format!(
                        "<m:sSup><m:e>{}</m:e><m:sup><m:r><m:t>{}</m:t></m:r></m:sup></m:sSup>",
                        latex_to_omml(content),
                        label
                    )
                }
            );
        }
    }

    if let Some(inner) = latex.strip_prefix("\\underbrace{") {
        if let Some((content, rest)) = split_brace_pair(inner) {
            let label = rest
                .trim_start()
                .strip_prefix("_{")
                .unwrap_or("")
                .strip_suffix('}')
                .unwrap_or("");
            return format!("<m:bar>\n  <m:barPr><m:pos m:val=\"bottom\"/></m:barPr>\n  <m:e>{}</m:e>\n</m:bar>",
                if label.is_empty() { latex_to_omml(content) }
                else { format!("<m:sSub><m:e>{}</m:e><m:sub><m:r><m:t>{}</m:t></m:r></m:sub></m:sSub>",
                    latex_to_omml(content), label) });
        }
    }

    // Matrix environments
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
    if let Some(inner) = extract_env(latex, "cases") {
        return cases_to_omml(inner);
    }
    if let Some(inner) = extract_env(latex, "aligned") {
        return aligned_to_omml(inner);
    }

    // Array environment (similar to matrix but with column alignment)
    if let Some(inner) = extract_env(latex, "array") {
        return matrix_to_omml(inner, "m:m");
    }

    // \phantom — zero-width placeholder
    if let Some(inner) = latex.strip_prefix("\\phantom{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!("<m:r><m:t>{}</m:t></m:r>", " ".repeat(content.len()));
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

    if let Some(sym) = map_large_op(latex) {
        return format!(
            "<m:nary>\n  <m:naryPr><m:chr m:val=\"{}\"/></m:naryPr>\n</m:nary>",
            sym
        );
    }

    if let Some(sym) = map_symbol_unicode(latex) {
        return format!("<m:r><m:t>{}</m:t></m:r>", sym);
    }

    wrap_mtext(latex)
}

fn wrap_mtext(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    format!("<m:r><m:t>{}</m:t></m:r>", escaped)
}

fn map_omml_symbol(latex: &str) -> Option<&str> {
    match latex {
        "\\otimes" | "\\otimes " => Some("\u{2297}"),
        "\\oplus" | "\\oplus " => Some("\u{2295}"),
        "\\odot" | "\\odot " => Some("\u{2299}"),
        "\\nabla" | "\\nabla " => Some("\u{2207}"),
        "\\partial" | "\\partial " => Some("\u{2202}"),
        "\\infty" | "\\infty " => Some("\u{221E}"),
        "\\pm" | "\\pm " => Some("\u{00B1}"),
        "\\mp" | "\\mp " => Some("\u{2213}"),
        "\\times" | "\\times " => Some("\u{00D7}"),
        "\\div" | "\\div " => Some("\u{00F7}"),
        "\\cdot" | "\\cdot " => Some("\u{22C5}"),
        "\\leq" | "\\leq " | "\\le" | "\\le " => Some("\u{2264}"),
        "\\geq" | "\\geq " | "\\ge" | "\\ge " => Some("\u{2265}"),
        "\\neq" | "\\neq " | "\\ne" | "\\ne " => Some("\u{2260}"),
        "\\approx" | "\\approx " => Some("\u{2248}"),
        "\\equiv" | "\\equiv " => Some("\u{2261}"),
        "\\sim" | "\\sim " => Some("\u{223C}"),
        "\\cong" | "\\cong " => Some("\u{2245}"),
        "\\propto" | "\\propto " => Some("\u{221D}"),
        "\\in" | "\\in " => Some("\u{2208}"),
        "\\notin" | "\\notin " | "\\not\\in" => Some("\u{2209}"),
        "\\subset" | "\\subset " => Some("\u{2282}"),
        "\\supset" | "\\supset " => Some("\u{2283}"),
        "\\subseteq" | "\\subseteq " => Some("\u{2286}"),
        "\\supseteq" | "\\supseteq " => Some("\u{2287}"),
        "\\cup" | "\\cup " => Some("\u{222A}"),
        "\\cap" | "\\cap " => Some("\u{2229}"),
        "\\setminus" | "\\setminus " => Some("\u{2216}"),
        "\\emptyset" | "\\emptyset " => Some("\u{2205}"),
        "\\forall" | "\\forall " => Some("\u{2200}"),
        "\\exists" | "\\exists " => Some("\u{2203}"),
        "\\neg" | "\\neg " | "\\lnot" | "\\lnot " => Some("\u{00AC}"),
        "\\wedge" | "\\wedge " => Some("\u{2227}"),
        "\\vee" | "\\vee " => Some("\u{2228}"),
        "\\rightarrow" | "\\rightarrow " | "\\to" | "\\to " => Some("\u{2192}"),
        "\\leftarrow" | "\\leftarrow " => Some("\u{2190}"),
        "\\leftrightarrow" | "\\leftrightarrow " => Some("\u{2194}"),
        "\\Rightarrow" | "\\Rightarrow " => Some("\u{21D2}"),
        "\\Leftarrow" | "\\Leftarrow " => Some("\u{21D0}"),
        "\\Leftrightarrow" | "\\Leftrightarrow " => Some("\u{21D4}"),
        "\\mapsto" | "\\mapsto " => Some("\u{21A6}"),
        "\\uparrow" | "\\uparrow " => Some("\u{2191}"),
        "\\downarrow" | "\\downarrow " => Some("\u{2193}"),
        "\\circ" | "\\circ " => Some("\u{2218}"),
        "\\star" | "\\star " => Some("\u{22C6}"),
        "\\dagger" | "\\dagger " => Some("\u{2020}"),
        "\\ddagger" | "\\ddagger " => Some("\u{2021}"),
        "\\angle" | "\\angle " => Some("\u{2220}"),
        "\\perp" | "\\perp " => Some("\u{22A5}"),
        "\\parallel" | "\\parallel " | "\\| " => Some("\u{2225}"),
        "\\mid" | "\\mid " => Some("\u{2223}"),
        "\\therefore" | "\\therefore " => Some("\u{2234}"),
        "\\because" | "\\because " => Some("\u{2235}"),
        "\\wp" | "\\wp " => Some("\u{2118}"),
        "\\Re" | "\\Re " => Some("\u{211C}"),
        "\\Im" | "\\Im " => Some("\u{2111}"),
        "\\aleph" | "\\aleph " => Some("\u{2135}"),
        "\\hbar" | "\\hbar " => Some("\u{210F}"),
        "\\ell" | "\\ell " => Some("\u{2113}"),
        "\\prime" | "\\prime " => Some("\u{2032}"),
        "\\ldots" | "\\ldots " | "\\dots" | "\\dots " => Some("\u{2026}"),
        "\\cdots" | "\\cdots " => Some("\u{22EF}"),
        "\\vdots" | "\\vdots " => Some("\u{22EE}"),
        "\\ddots" | "\\ddots " => Some("\u{22F1}"),
        _ => None,
    }
}

fn extract_delimited(latex: &str) -> Option<String> {
    let mut rest = latex;
    let mut open = "(";
    let mut close = ")";
    if let Some(r) = latex.strip_prefix("\\left") {
        if let Some(ch) = r.chars().next() {
            open = match ch {
                '[' => "[",
                '{' | '|' => "|",
                _ => "(",
            };
            close = match ch {
                '[' => "]",
                '|' => "|",
                _ => ")",
            };
            rest = &r[ch.len_utf8()..];
        }
    } else {
        return None;
    }

    // Find matching \right
    let right_pattern = format!("\\right{}", close);
    if let Some(pos) = rest.find(&right_pattern) {
        let inner = &rest[..pos];
        let d = latex_to_omml(inner);
        return Some(format!(
            "<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
            xml_escape(open), xml_escape(close), d
        ));
    }
    None
}

fn matrix_to_omml(content: &str, _tag: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut result = String::from("<m:mRow>\n");
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
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
        let cell_xml: Vec<String> = row
            .iter()
            .map(|cell| format!("  <m:e>{}</m:e>", latex_to_omml(cell.trim())))
            .collect();
        cells_xml.push(format!("  <m:r>\n{}\n  </m:r>", cell_xml.join("\n")));
    }
    format!(
        "<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n{}\n</m:d>",
        xml_escape(open),
        xml_escape(close),
        cells_xml.join("\n")
    )
}

fn cases_to_omml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let left = row
            .first()
            .map(|s| latex_to_omml(s.trim()))
            .unwrap_or_default();
        let right = row
            .get(1)
            .map(|s| latex_to_omml(s.trim()))
            .unwrap_or_default();
        rows_xml.push(format!(
            "  <m:r>\n    <m:e>{}</m:e>\n    <m:e>{}</m:e>\n  </m:r>",
            left, right
        ));
    }
    format!(
        "<m:d>\n  <m:dPr><m:begChr m:val=\"{{\"/><m:endChr m:val=\"}}\"/></m:dPr>\n{}\n</m:d>",
        rows_xml.join("\n")
    )
}

fn aligned_to_omml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| format!("  <m:e>{}</m:e>", latex_to_omml(cell.trim())))
            .collect();
        rows_xml.push(format!("  <m:r>\n{}\n  </m:r>", cells.join("\n")));
    }
    format!("<m:mRow>\n{}\n</m:mRow>", rows_xml.join("\n"))
}

// ── Color / Font helpers ──────────────────────────────────────

fn color_name_to_hex(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "red" => "FF0000".to_string(),
        "green" => "00FF00".to_string(),
        "blue" => "0000FF".to_string(),
        "yellow" => "FFFF00".to_string(),
        "cyan" => "00FFFF".to_string(),
        "magenta" | "fuchsia" => "FF00FF".to_string(),
        "black" => "000000".to_string(),
        "white" => "FFFFFF".to_string(),
        "gray" | "grey" => "808080".to_string(),
        "orange" => "FFA500".to_string(),
        "purple" => "800080".to_string(),
        "pink" => "FFC0CB".to_string(),
        "brown" => "A52A2A".to_string(),
        "darkgreen" | "dark green" => "006400".to_string(),
        "darkblue" | "dark blue" => "00008B".to_string(),
        "lightblue" | "light blue" => "ADD8E6".to_string(),
        "lightgray" | "light grey" => "D3D3D3".to_string(),
        s if s.starts_with('#') && s.len() == 7 => s[1..].to_string(),
        s if s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit()) => s.to_string(),
        _ => "000000".to_string(),
    }
}

fn wrap_with_color(omml_content: &str, hex: &str) -> String {
    format!(
        "<m:r><m:rPr><w:rPr><w:color w:val=\"{}\"/></w:rPr></m:rPr>{}</m:r>",
        hex, omml_content
    )
}

fn wrap_with_bold(omml_content: &str) -> String {
    format!(
        "<m:r><m:rPr><w:rPr><w:b/></w:rPr></m:rPr>{}</m:r>",
        omml_content
    )
}

/// Extract content inside the first `{...}` block.
/// For input "content}rest", returns Some("content").
fn extract_brace_content(s: &str) -> Option<&str> {
    if let Some(close) = s.find('}') {
        Some(&s[..close])
    } else {
        None
    }
}
