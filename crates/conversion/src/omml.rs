use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline};
use latexsnipper_foundation::Result;

use crate::converter::Converter;
use crate::latex_ast::LatexNode;
use crate::latex_parser::parse_latex;
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

/// Convert a LaTeX string to OMML by parsing to AST first, then walking the AST.
pub fn latex_to_omml(latex: &str) -> String {
    let ast = parse_latex(latex);
    let omml = ast_to_omml(&ast);
    fix_omml(&omml)
}

/// Walk the AST and generate OMML XML.
fn ast_to_omml(node: &LatexNode) -> String {
    match node {
        LatexNode::Text(s) => {
            if s.is_empty() {
                return String::new();
            }
            wrap_mtext(s)
        }

        LatexNode::Greek(name) => {
            let sym = map_greek_unicode(name);
            wrap_mtext(sym)
        }

        LatexNode::Symbol(name) => {
            if let Some(sym) = map_symbol_unicode(&format!("\\{}", name)) {
                wrap_mtext(sym)
            } else if let Some(sym) = map_omml_symbol(&format!("\\{}", name)) {
                wrap_mtext(sym)
            } else {
                wrap_mtext(name)
            }
        }

        LatexNode::Operator(name) => {
            match name.as_str() {
                "sum" | "prod" | "coprod" | "int" | "iint" | "iiint" | "oint"
                | "bigcup" | "bigcap" => {
                    if let Some(sym) = map_large_op(&format!("\\{}", name)) {
                        format!(
                            "<m:nary><m:naryPr><m:chr m:val=\"{}\"/></m:naryPr></m:nary>",
                            sym
                        )
                    } else {
                        wrap_mtext(name)
                    }
                }
                _ => {
                    // Functions like \lim, \log, \sin, etc.
                    format!(
                        "<m:func>\n  <m:fName><m:r><m:rPr><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:h-ansi=\"Cambria Math\"/></w:rPr></m:rPr><m:t>{}</m:t></m:r></m:fName>\n  <m:e/>\n</m:func>",
                        name
                    )
                }
            }
        }

        LatexNode::Relation(name) => {
            if let Some(sym) = map_symbol_unicode(&format!("\\{}", name)) {
                wrap_mtext(sym)
            } else if let Some(sym) = map_omml_symbol(&format!("\\{}", name)) {
                wrap_mtext(sym)
            } else {
                wrap_mtext(name)
            }
        }

        LatexNode::Fraction { num, den } => {
            format!(
                "<m:f>\n  <m:num>{}</m:num>\n  <m:den>{}</m:den>\n</m:f>",
                ast_to_omml(num),
                ast_to_omml(den)
            )
        }

        LatexNode::SquareRoot { index, content } => {
            match index {
                Some(idx) => format!(
                    "<m:rad>\n  <m:radPr/>\n  <m:deg>{}</m:deg>\n  <m:e>{}</m:e>\n</m:rad>",
                    ast_to_omml(idx),
                    ast_to_omml(content)
                ),
                None => format!(
                    "<m:rad>\n  <m:radPr><m:degHide m:val=\"1\"/></m:radPr>\n  <m:deg/>\n  <m:e>{}</m:e>\n</m:rad>",
                    ast_to_omml(content)
                ),
            }
        }

        LatexNode::Superscript { base, exp } => {
            // Check if base is a Subscript wrapping an Operator (e.g. \sum_{i=1}^{n})
            if let LatexNode::Subscript { base: inner_base, sub } = base.as_ref() {
                if let LatexNode::Operator(name) = inner_base.as_ref() {
                    if is_large_op(name) {
                        let cmd = format!("\\{}", name);
                        let sym = map_large_op(&cmd).unwrap_or(name.as_str());
                        return format!(
                            "<m:nary><m:naryPr><m:chr m:val=\"{}\"/></m:naryPr><m:sub>{}</m:sub><m:sup>{}</m:sup></m:nary>",
                            sym, flat_text_run(sub), flat_text_run(exp)
                        );
                    }
                }
            }
            // Check if base is a bare Operator (e.g. \sum^{n})
            if let LatexNode::Operator(name) = base.as_ref() {
                if is_large_op(name) {
                    let cmd = format!("\\{}", name);
                    let sym = map_large_op(&cmd).unwrap_or(name.as_str());
                    return format!(
                        "<m:nary><m:naryPr><m:chr m:val=\"{}\"/></m:naryPr><m:sup>{}</m:sup></m:nary>",
                        sym, flat_text_run(exp)
                    );
                }
            }
            format!(
                "<m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup>",
                ast_to_omml(base),
                ast_to_omml(exp)
            )
        }

        LatexNode::Subscript { base, sub } => {
            // Check if base is an Operator (e.g. \sum_{i=1})
            if let LatexNode::Operator(name) = base.as_ref() {
                if is_large_op(name) {
                    let cmd = format!("\\{}", name);
                    let sym = map_large_op(&cmd).unwrap_or(name.as_str());
                    return format!(
                        "<m:nary><m:naryPr><m:chr m:val=\"{}\"/></m:naryPr><m:sub>{}</m:sub></m:nary>",
                        sym, flat_text_run(sub)
                    );
                }
                // Non-large operators like \lim → use m:func with sub
                let inner = ast_to_omml(base);
                return format!(
                    "<m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub>",
                    inner, ast_to_omml(sub)
                );
            }
            format!(
                "<m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub>",
                ast_to_omml(base),
                ast_to_omml(sub)
            )
        }

        LatexNode::Accent { chr, content } => {
            format!(
                "<m:acc>\n  <m:accPr><m:chr m:val=\"{}\"/></m:accPr>\n  <m:e>{}</m:e>\n</m:acc>",
                chr,
                ast_to_omml(content)
            )
        }

        LatexNode::FontModifier { font, content } => {
            let inner = ast_to_omml(content);
            match font.as_str() {
                "mathbf" | "boldsymbol" | "bm" => wrap_with_bold(&inner),
                "mathbb" => wrap_mtext(&extract_text_from_omml(&inner)),
                "mathcal" | "mathfrak" | "mathit" | "mathsf" | "mathtt" | "mathrm" | "mathnormal" => {
                    // For these, just render the content with appropriate font
                    inner
                }
                _ => inner,
            }
        }

        LatexNode::OperatorName { args, .. } => {
            // \operatorname{Spec} — render as upright text
            let name = args.first().map(|a| extract_text_from_omml(&ast_to_omml(a))).unwrap_or_default();
            format!(
                "<m:r><m:rPr><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:h-ansi=\"Cambria Math\"/></w:rPr></m:rPr><m:t>{} </m:t></m:r>",
                xml_escape(&name)
            )
        }

        LatexNode::Overbrace { content, label } => {
            let inner = ast_to_omml(content);
            match label {
                Some(lbl) => format!(
                    "<m:bar>\n  <m:barPr><m:pos m:val=\"top\"/></m:barPr>\n  <m:e><m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup></m:e>\n</m:bar>",
                    inner,
                    ast_to_omml(lbl)
                ),
                None => format!(
                    "<m:bar>\n  <m:barPr><m:pos m:val=\"top\"/></m:barPr>\n  <m:e>{}</m:e>\n</m:bar>",
                    inner
                ),
            }
        }

        LatexNode::Underbrace { content, label } => {
            let inner = ast_to_omml(content);
            match label {
                Some(lbl) => format!(
                    "<m:bar>\n  <m:barPr><m:pos m:val=\"bottom\"/></m:barPr>\n  <m:e><m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub></m:e>\n</m:bar>",
                    inner,
                    ast_to_omml(lbl)
                ),
                None => format!(
                    "<m:bar>\n  <m:barPr><m:pos m:val=\"bottom\"/></m:barPr>\n  <m:e>{}</m:e>\n</m:bar>",
                    inner
                ),
            }
        }

        LatexNode::Delimited { left, content, right } => {
            let content_xml: Vec<String> = content.iter().map(ast_to_omml).collect();
            format!(
                "<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
                xml_escape(left),
                xml_escape(right),
                content_xml.join("")
            )
        }

        LatexNode::Matrix { env, rows } => {
            matrix_to_omml(rows, env)
        }

        LatexNode::Cases(rows) => {
            cases_to_omml(rows)
        }

        LatexNode::Command { name, args } => {
            match name.as_str() {
                "binom" if args.len() == 2 => {
                    format!(
                        "<m:d>\n  <m:dPr><m:begChr m:val=\"(\"/><m:endChr m:val=\")\"/></m:dPr>\n  <m:e><m:f>\n  <m:num>{}</m:num>\n  <m:den>{}</m:den>\n</m:f></m:e>\n</m:d>",
                        ast_to_omml(&args[0]),
                        ast_to_omml(&args[1])
                    )
                }
                "text" | "textbf" | "textit" | "textrm" | "textsf" | "texttt" => {
                    let text = extract_text_from_args(args);
                    format!(
                        "<m:r><m:rPr><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:h-ansi=\"Cambria Math\"/><w:rStyle w:val=\"a\"/></w:rPr></m:rPr><m:t>{}</m:t></m:r>",
                        xml_escape(&text)
                    )
                }
                "textcolor" if args.len() >= 2 => {
                    let color_name = extract_text_from_omml(&ast_to_omml(&args[0]));
                    let hex = color_name_to_hex(&color_name);
                    let inner = ast_to_omml(&args[1]);
                    wrap_with_color(&inner, &hex)
                }
                "color" if args.len() >= 2 => {
                    let color_name = extract_text_from_omml(&ast_to_omml(&args[0]));
                    let hex = color_name_to_hex(&color_name);
                    let inner = ast_to_omml(&args[1]);
                    wrap_with_color(&inner, &hex)
                }
                "colorbox" if args.len() >= 2 => {
                    ast_to_omml(&args[1])
                }
                "tiny" | "scriptsize" | "footnotesize" | "small" | "normalsize" | "large"
                | "Large" | "LARGE" | "huge" | "Huge" => {
                    if let Some(arg) = args.first() {
                        wrap_with_size(&ast_to_omml(arg), latex_size_to_half_points(name))
                    } else {
                        String::new()
                    }
                }
                "phantom" => {
                    let text = extract_text_from_args(args);
                    format!("<m:r><m:t>{}</m:t></m:r>", " ".repeat(text.len()))
                }
                _ => {
                    // Unknown command — render arguments as sequence
                    let parts: Vec<String> = args.iter().map(ast_to_omml).collect();
                    parts.join("")
                }
            }
        }

        LatexNode::Group(nodes) => {
            let parts: Vec<String> = nodes.iter().map(ast_to_omml).collect();
            parts.join("")
        }

        LatexNode::Math { content, .. } => {
            let parts: Vec<String> = content.iter().map(ast_to_omml).collect();
            parts.join("")
        }

        LatexNode::Sequence(nodes) => {
            let parts: Vec<String> = nodes.iter().map(ast_to_omml).collect();
            parts.join("")
        }
    }
}

// ── Helper functions ──

fn is_large_op(name: &str) -> bool {
    matches!(
        name,
        "sum" | "prod" | "coprod" | "int" | "iint" | "iiint" | "oint" | "bigcup" | "bigcap"
    )
}

/// Extract plain text from an AST node for use in nary sub/sup positions.
/// Flattens nested structures to simple text runs.
fn flatten_to_text(node: &LatexNode) -> String {
    match node {
        LatexNode::Text(s) => s.clone(),
        LatexNode::Greek(name) => map_greek_unicode(name).to_string(),
        LatexNode::Symbol(name) => {
            let key = format!("\\{}", name);
            map_symbol_unicode(&key)
                .or_else(|| map_omml_symbol(&key))
                .unwrap_or(name)
                .to_string()
        }
        LatexNode::Group(nodes) => nodes.iter().map(flatten_to_text).collect(),
        LatexNode::Sequence(nodes) => nodes.iter().map(flatten_to_text).collect(),
        LatexNode::Subscript { base, sub } => {
            format!("{}{}", flatten_to_text(base), flatten_to_text(sub))
        }
        LatexNode::Superscript { base, exp } => {
            format!("{}{}", flatten_to_text(base), flatten_to_text(exp))
        }
        LatexNode::Math { content, .. } => content.iter().map(flatten_to_text).collect(),
        _ => String::new(),
    }
}

/// Render nary sub/sup content as flat text runs (avoids nested math in Word)
fn flat_text_run(node: &LatexNode) -> String {
    let text = flatten_to_text(node);
    if text.is_empty() {
        String::new()
    } else {
        wrap_mtext(&text)
    }
}

fn wrap_mtext(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    format!("<m:r><m:t>{}</m:t></m:r>", escaped)
}

fn map_greek_unicode(name: &str) -> &str {
    match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" | "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" | "vartheta" => "θ",
        "iota" => "ι",
        "kappa" | "varkappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" | "varpi" => "π",
        "rho" | "varrho" => "ρ",
        "sigma" | "varsigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" | "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "digamma" => "ϝ",
        "omicron" => "ο",
        "sampi" | "Sampi" => "Ϡ",
        "backepsilon" => "∍",
        "varDelta" => "𝛥",
        "varGamma" => "𝛤",
        "varLambda" => "𝛬",
        "varPi" => "𝛱",
        "varTheta" => "𝛩",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        _ => name,
    }
}

fn wrap_with_color(omml_content: &str, hex: &str) -> String {
    let color_tag = format!("<w:color w:val=\"{}\"/>", hex);
    if omml_content.contains(&color_tag) {
        return omml_content.to_string();
    }

    let mut result = omml_content.to_string();

    // Inject color into existing <w:rPr> blocks
    result = result.replace("<w:rPr>", &format!("<w:rPr>{}", color_tag));

    // For bare <m:r><m:t> without any <m:rPr>, add italic + color + font
    let bare_run_replacement = format!(
        "<m:r><m:rPr><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:h-ansi=\"Cambria Math\"/><w:i/>{}</w:rPr></m:rPr><m:t>",
        color_tag
    );
    result = result.replace("<m:r><m:t>", &bare_run_replacement);

    result
}

fn wrap_with_bold(omml_content: &str) -> String {
    format!(
        "<m:r><m:rPr><w:rPr><w:b/></w:rPr></m:rPr>{}</m:r>",
        omml_content
    )
}

fn wrap_with_size(omml_content: &str, half_points: u16) -> String {
    format!(
        "<m:r><m:rPr><w:rPr><w:sz w:val=\"{}\"/></w:rPr></m:rPr>{}</m:r>",
        half_points, omml_content
    )
}

fn latex_size_to_half_points(name: &str) -> u16 {
    match name {
        "tiny" => 10,
        "scriptsize" => 14,
        "footnotesize" => 16,
        "small" => 18,
        "normalsize" => 20,
        "large" => 24,
        "Large" => 29,
        "LARGE" => 35,
        "huge" => 41,
        "Huge" => 50,
        _ => 20,
    }
}

/// Extract plain text from an OMML fragment.
fn extract_text_from_omml(omml: &str) -> String {
    let mut result = String::new();
    let mut in_t = false;
    let mut chars = omml.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            let tag: String = chars.by_ref().take_while(|&c| c != '>').collect();
            if tag == "m:t" || tag.starts_with("m:t ") {
                in_t = true;
            } else if tag == "/m:t" || tag.starts_with("/m:t") {
                in_t = false;
            }
        } else if in_t {
            result.push(ch);
        }
    }
    result
}

/// Extract text from a list of AST args.
fn extract_text_from_args(args: &[LatexNode]) -> String {
    args.iter()
        .map(|a| extract_text_from_omml(&ast_to_omml(a)))
        .collect()
}

fn matrix_to_omml(rows: &[Vec<LatexNode>], env: &str) -> String {
    let (open, close) = match env {
        "pmatrix" => ("(", ")"),
        "bmatrix" => ("[", "]"),
        "Bmatrix" => ("{", "}"),
        "vmatrix" => ("|", "|"),
        "Vmatrix" => ("‖", "‖"),
        _ => ("", ""),
    };

    let mut cells_xml = Vec::new();
    for row in rows {
        let cell_xml: Vec<String> = row
            .iter()
            .map(|cell| format!("  <m:e>{}</m:e>", ast_to_omml(cell)))
            .collect();
        cells_xml.push(format!("  <m:r>\n{}\n  </m:r>", cell_xml.join("\n")));
    }

    if open.is_empty() && close.is_empty() {
        format!("<m:mRow>\n{}\n</m:mRow>", cells_xml.join("\n"))
    } else {
        format!(
            "<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n{}\n</m:d>",
            xml_escape(open),
            xml_escape(close),
            cells_xml.join("\n")
        )
    }
}

fn cases_to_omml(rows: &[Vec<LatexNode>]) -> String {
    let mut rows_xml = Vec::new();
    for row in rows {
        let left = row.first().map(ast_to_omml).unwrap_or_default();
        let right = row.get(1).map(ast_to_omml).unwrap_or_default();
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
        s if s.starts_with('#') && s.len() == 7 => s[1..].to_uppercase(),
        s if s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit()) => s.to_uppercase(),
        _ => "000000".to_string(),
    }
}

/// Post-process OMML to fix common issues.
fn fix_omml(omml: &str) -> String {
    let mut s = omml.to_string();

    // Remove XML declaration if present
    if let Some(pos) = s.find("<?xml") {
        if let Some(end) = s[pos..].find("?>") {
            s.replace_range(..pos + end + 2, "");
        }
    }

    // Fix empty <m:t/>
    s = s.replace("<m:t/>", "<m:t> </m:t>");

    // Fix XSLT tag typos
    s = s.replace("<m:eqAr>", "<m:eqArr>");
    s = s.replace("</m:eqAr>", "</m:eqArr>");

    // Remove mml namespace prefix remnants
    s = s.replace(" xmlns:mml=\"http://www.w3.org/1998/Math/MathML\"", "");

    // If OMML only has bare <m:r><m:t>text</m:r> without any math structure,
    // add italic formatting and Cambria Math font
    if !s.contains("<m:f>")
        && !s.contains("<m:sSup>")
        && !s.contains("<m:sSub>")
        && !s.contains("<m:nary>")
        && !s.contains("<m:eqArr>")
        && !s.contains("<m:d>")
        && !s.contains("<m:rad>")
        && !s.contains("<m:acc>")
        && !s.contains("<m:func>")
        && !s.contains("<m:bar>")
        && !s.contains("<m:mRow>")
    {
        s = s.replace(
            "<m:r><m:t>",
            "<m:r><m:rPr><w:rPr><w:rFonts w:ascii=\"Cambria Math\" w:h-ansi=\"Cambria Math\"/><w:i/></w:rPr></m:rPr><m:t>",
        );
    }

    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integral_encoding() {
        // Test that ∫ is correctly encoded as 3-byte UTF-8
        let result = latex_to_omml("\\int");
        eprintln!("integral OMML: {}", result);
        // Find the chr val and check encoding
        if let Some(idx) = result.find("m:val=\"") {
            let start = idx + "m:val=\"".len();
            // Find the closing quote using byte position
            let rest = &result[start..];
            if let Some(end) = rest.find('"') {
                let val = &rest[..end];
                let bytes = val.as_bytes();
                eprintln!("chr val bytes: {:?}", bytes);
                assert_eq!(bytes, &[0xE2, 0x88, 0xAB], "∫ should be 3 UTF-8 bytes");
            }
        }
    }

    #[test]
    fn debug_sum_limits() {
        let result = latex_to_omml("\\sum_{i=1}^{n} x_i");
        eprintln!("=== sum with limits ===\n{}", result);
    }

    #[test]
    fn debug_lim_limits() {
        let result = latex_to_omml("\\lim_{x \\to 0} f(x)");
        eprintln!("=== lim with limits ===\n{}", result);
    }

    #[test]
    fn debug_integral_limits() {
        let result = latex_to_omml("\\int_{0}^{1} x\\,dx");
        eprintln!("=== integral with limits ===\n{}", result);
    }

    #[test]
    fn debug_arrow_limits() {
        let result = latex_to_omml("x \\to^{a}_{b}");
        eprintln!("=== arrow with limits ===\n{}", result);
    }

    #[test]
    fn test_simple_text() {
        let result = latex_to_omml("hello");
        assert!(result.contains("<m:t>hello</m:t>"), "got: {}", result);
        assert!(result.contains("Cambria Math"), "got: {}", result);
    }

    #[test]
    fn test_greek_letter() {
        let result = latex_to_omml("\\alpha");
        assert!(result.contains("<m:t>α</m:t>"), "got: {}", result);
    }

    #[test]
    fn test_fraction() {
        let result = latex_to_omml("\\frac{a}{b}");
        assert!(result.contains("<m:f>"), "got: {}", result);
        assert!(result.contains("<m:num>"), "got: {}", result);
        assert!(result.contains("<m:den>"), "got: {}", result);
    }

    #[test]
    fn test_superscript() {
        let result = latex_to_omml("x^{2}");
        assert!(result.contains("<m:sSup>"), "got: {}", result);
    }

    #[test]
    fn test_subscript() {
        let result = latex_to_omml("a_{i}");
        assert!(result.contains("<m:sSub>"), "got: {}", result);
    }

    #[test]
    fn test_sqrt() {
        let result = latex_to_omml("\\sqrt{x}");
        assert!(result.contains("<m:rad>"), "got: {}", result);
    }

    #[test]
    fn test_operatorname() {
        let result = latex_to_omml("\\operatorname{Spec}");
        assert!(result.contains("Spec"), "got: {}", result);
        assert!(result.contains("<m:r>"), "got: {}", result);
    }

    #[test]
    fn test_complex_expression() {
        // E=mc^2\operatorname{Spec}(4{})
        let result = latex_to_omml("E=mc^2\\operatorname{Spec}(4{})");
        assert!(
            result.contains("<m:sSup>"),
            "should have superscript: {}",
            result
        );
        assert!(result.contains("Spec"), "should have Spec: {}", result);
        // The (4{}) is treated as text after the operator name
        assert!(result.contains("<m:t>(4"), "should have (4: {}", result);
    }

    #[test]
    fn test_function_with_limit() {
        let result = latex_to_omml("\\lim_{x \\to 0}");
        assert!(result.contains("<m:func>"), "should have func: {}", result);
        assert!(
            result.contains("<m:sSub>"),
            "should have subscript: {}",
            result
        );
    }

    #[test]
    fn test_nabla() {
        let result = latex_to_omml("\\nabla f");
        assert!(result.contains("∇"), "should have nabla: {}", result);
    }

    #[test]
    fn test_hat() {
        let result = latex_to_omml("\\hat{x}");
        assert!(result.contains("<m:acc>"), "should have accent: {}", result);
    }

    #[test]
    fn test_delimited() {
        let result = latex_to_omml("\\left( \\frac{a}{b} \\right)");
        assert!(
            result.contains("<m:d>"),
            "should have delimiter: {}",
            result
        );
        assert!(result.contains("<m:f>"), "should have fraction: {}", result);
    }

    #[test]
    fn test_matrix() {
        let result = latex_to_omml("\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}");
        assert!(
            result.contains("<m:d>"),
            "should have delimiter for pmatrix: {}",
            result
        );
    }

    #[test]
    fn test_cases() {
        let result = latex_to_omml("\\begin{cases}x&x>0\\\\0&x\\leq 0\\end{cases}");
        assert!(
            result.contains("<m:d>"),
            "should have delimiter for cases: {}",
            result
        );
    }

    #[test]
    fn test_color() {
        let result = latex_to_omml("\\textcolor{red}{x}");
        assert!(
            result.contains("FF0000"),
            "should have red color: {}",
            result
        );
    }

    // ═══ Comprehensive: Greek Letters ═══

    #[test]
    fn all_greek_lowercase() {
        let result = latex_to_omml("\\alpha \\beta \\gamma \\delta \\epsilon \\zeta \\eta \\theta \\iota \\kappa \\lambda \\mu \\nu \\xi \\pi \\rho \\sigma \\tau \\upsilon \\phi \\chi \\psi \\omega");
        for (cmd, sym) in &[
            ("alpha", "α"),
            ("beta", "β"),
            ("gamma", "γ"),
            ("delta", "δ"),
            ("epsilon", "ε"),
            ("theta", "θ"),
            ("pi", "π"),
            ("sigma", "σ"),
            ("omega", "ω"),
        ] {
            assert!(result.contains(sym), "{} missing: {}", cmd, result);
        }
    }

    #[test]
    fn uppercase_greek() {
        let result =
            latex_to_omml("\\Gamma \\Delta \\Theta \\Lambda \\Xi \\Pi \\Sigma \\Phi \\Psi \\Omega");
        for sym in &["Γ", "Δ", "Θ", "Λ", "Ξ", "Π", "Σ", "Φ", "Ψ", "Ω"] {
            assert!(result.contains(sym), "{} missing: {}", sym, result);
        }
    }

    // ═══ Comprehensive: Relation Symbols ═══

    #[test]
    fn relation_symbols() {
        let result = latex_to_omml("\\leq \\geq \\neq \\approx \\equiv \\sim \\propto \\cong");
        for (cmd, sym) in &[
            ("leq", "≤"),
            ("geq", "≥"),
            ("neq", "≠"),
            ("approx", "≈"),
            ("equiv", "≡"),
            ("sim", "∼"),
            ("propto", "∝"),
            ("cong", "≅"),
        ] {
            assert!(result.contains(sym), "{} missing: {}", cmd, result);
        }
    }

    // ═══ Comprehensive: Set Theory ═══

    #[test]
    fn set_theory() {
        let result = latex_to_omml(
            "\\in \\notin \\subset \\supset \\cup \\cap \\emptyset \\forall \\exists \\neg",
        );
        for (cmd, sym) in &[
            ("in", "∈"),
            ("notin", "∉"),
            ("subset", "⊂"),
            ("supset", "⊃"),
            ("cup", "∪"),
            ("cap", "∩"),
            ("emptyset", "∅"),
            ("forall", "∀"),
            ("exists", "∃"),
            ("neg", "¬"),
        ] {
            assert!(result.contains(sym), "{} missing: {}", cmd, result);
        }
    }

    // ═══ Comprehensive: Arrows ═══

    #[test]
    fn arrows() {
        let result = latex_to_omml("\\rightarrow \\leftarrow \\leftrightarrow \\Rightarrow \\Leftarrow \\Leftrightarrow \\mapsto");
        for (cmd, sym) in &[
            ("rightarrow", "→"),
            ("leftarrow", "←"),
            ("leftrightarrow", "↔"),
            ("Rightarrow", "⇒"),
            ("Leftarrow", "⇐"),
            ("Leftrightarrow", "⇔"),
            ("mapsto", "↦"),
        ] {
            assert!(result.contains(sym), "{} missing: {}", cmd, result);
        }
    }

    // ═══ Comprehensive: Operators ═══

    #[test]
    fn arithmetic_operators() {
        let result = latex_to_omml("\\pm \\mp \\times \\div \\cdot \\circ \\oplus \\otimes");
        for (cmd, sym) in &[
            ("pm", "±"),
            ("mp", "∓"),
            ("times", "×"),
            ("div", "÷"),
            ("cdot", "·"),
            ("circ", "∘"),
            ("oplus", "⊕"),
            ("otimes", "⊗"),
        ] {
            assert!(result.contains(sym), "{} missing: {}", cmd, result);
        }
    }

    #[test]
    fn calculus_symbols() {
        let result = latex_to_omml("\\partial \\nabla \\infty \\ell \\hbar \\aleph");
        for (cmd, sym) in &[
            ("partial", "∂"),
            ("nabla", "∇"),
            ("infty", "∞"),
            ("ell", "ℓ"),
            ("hbar", "ℏ"),
            ("aleph", "ℵ"),
        ] {
            assert!(result.contains(sym), "{} missing: {}", cmd, result);
        }
    }

    #[test]
    fn dot_symbols() {
        let result = latex_to_omml("\\ldots \\cdots \\vdots \\ddots");
        for (cmd, sym) in &[
            ("ldots", "…"),
            ("cdots", "⋯"),
            ("vdots", "⋮"),
            ("ddots", "⋱"),
        ] {
            assert!(result.contains(sym), "{} missing: {}", cmd, result);
        }
    }

    // ═══ Comprehensive: Colored Text ═══

    #[test]
    fn textcolor_red() {
        let r = latex_to_omml("\\textcolor{red}{x}");
        assert!(r.contains("FF0000"), "red missing: {}", r);
    }

    #[test]
    fn textcolor_blue() {
        let r = latex_to_omml("\\textcolor{blue}{y}");
        assert!(r.contains("0000FF"), "blue missing: {}", r);
    }

    #[test]
    fn textcolor_hex() {
        let r = latex_to_omml("\\textcolor{#FF8800}{z}");
        assert!(r.contains("FF8800"), "hex color missing: {}", r);
    }

    #[test]
    fn color_green() {
        let r = latex_to_omml("\\color{green}x+y");
        assert!(r.contains("00FF00"), "green missing: {}", r);
    }

    #[test]
    fn nested_color_and_frac() {
        let r = latex_to_omml("\\textcolor{red}{\\frac{a}{b}}");
        assert!(r.contains("FF0000"), "color missing: {}", r);
        assert!(r.contains("<m:f>"), "fraction missing: {}", r);
    }

    // ═══ Comprehensive: Font Styles ═══

    #[test]
    fn mathbf_bold() {
        let r = latex_to_omml("\\mathbf{x}");
        assert!(r.contains("<w:b/>"), "bold missing: {}", r);
    }

    #[test]
    fn boldsymbol() {
        let r = latex_to_omml("\\boldsymbol{\\alpha}");
        assert!(r.contains("<w:b/>"), "bold missing: {}", r);
        assert!(r.contains("α"), "alpha missing: {}", r);
    }

    #[test]
    fn text_command() {
        let r = latex_to_omml("\\text{Hello}");
        assert!(r.contains("Hello"), "text missing: {}", r);
        assert!(r.contains("Cambria Math"), "font missing: {}", r);
    }

    #[test]
    fn operatorname_upright() {
        let r = latex_to_omml("\\operatorname{Spec}");
        assert!(r.contains("Spec"), "Spec missing: {}", r);
        assert!(r.contains("Cambria Math"), "font missing: {}", r);
    }

    // ═══ Comprehensive: Complex Formulas ═══

    #[test]
    fn quadratic_formula() {
        let r = latex_to_omml("x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}");
        assert!(r.contains("<m:f>"), "fraction: {}", r);
        assert!(r.contains("<m:rad>"), "sqrt: {}", r);
        assert!(r.contains("<m:sSup>"), "superscript: {}", r);
        assert!(r.contains("±"), "pm: {}", r);
    }

    #[test]
    fn euler_identity() {
        let r = latex_to_omml("e^{i\\pi} + 1 = 0");
        assert!(r.contains("<m:sSup>"), "superscript: {}", r);
        assert!(r.contains("π"), "pi: {}", r);
    }

    #[test]
    fn limit_formula() {
        let r = latex_to_omml("\\lim_{x \\to 0} \\frac{\\sin x}{x}");
        assert!(r.contains("<m:func>"), "function: {}", r);
        assert!(r.contains("<m:sSub>"), "subscript: {}", r);
        assert!(r.contains("<m:f>"), "fraction: {}", r);
        assert!(r.contains("→"), "arrow: {}", r);
    }

    #[test]
    fn integral_formula() {
        let r = latex_to_omml("\\int_{0}^{\\infty} e^{-x^2} dx");
        assert!(r.contains("<m:nary>"), "nary: {}", r);
        assert!(r.contains("∫"), "integral: {}", r);
        assert!(r.contains("∞"), "infinity: {}", r);
    }

    #[test]
    fn accent_chain() {
        let r = latex_to_omml("\\hat{x} + \\vec{v} + \\bar{y} + \\dot{z} + \\tilde{w}");
        assert!(r.contains("<m:acc>"), "accent: {}", r);
        assert!(r.matches("<m:acc>").count() >= 5, "5 accents: {}", r);
    }

    #[test]
    fn mixed_color_and_symbols() {
        let r = latex_to_omml("\\textcolor{blue}{\\alpha} + \\beta^{2} = \\gamma");
        assert!(r.contains("0000FF"), "blue: {}", r);
        assert!(r.contains("α"), "alpha: {}", r);
        assert!(r.contains("<m:sSup>"), "superscript: {}", r);
        assert!(r.contains("β"), "beta: {}", r);
        assert!(r.contains("γ"), "gamma: {}", r);
    }

    #[test]
    fn user_formula_spec() {
        let r = latex_to_omml("E=mc^2\\operatorname{Spec}(4{})");
        assert!(r.contains("<m:sSup>"), "superscript: {}", r);
        assert!(r.contains("Spec"), "Spec: {}", r);
        assert!(r.contains("<m:t>"), "text runs: {}", r);
    }
}
