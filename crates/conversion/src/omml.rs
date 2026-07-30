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
                                Inline::Footnote { content: _ } => {
                                    // Footnotes in OMML are represented as text markers
                                    parts.push("[^footnote]".to_string());
                                }
                                Inline::Label { .. } => {}
                                Inline::Reference { key, .. } => {
                                    parts.push(format!("({})", key));
                                }
                                Inline::Citation { key, .. } => {
                                    parts.push(format!("[{}]", key));
                                }
                                _ => {}
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
        format!(
            "<m:oMathPara>\n<m:oMath>{}\n</m:oMath>\n</m:oMathPara>",
            content
        )
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
    if let Some(nary) = normalize_nary_head(node) {
        return nary_to_omml(&nary);
    }

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
            // Math spacing commands — output OMML <m:spacing> element
            if let Some(spacing_val) = map_omml_spacing(name) {
                return format!("<m:spacing m:val=\"{}\"/>", spacing_val);
            }
            if let Some(sym) = map_symbol_unicode(&format!("\\{}", name)) {
                wrap_mtext(sym)
            } else if let Some(sym) = map_omml_symbol(&format!("\\{}", name)) {
                wrap_mtext(sym)
            } else {
                wrap_mtext(name)
            }
        }

        LatexNode::Operator(name) => {
            // Functions like \lim, \log, \sin, etc. A self-closing expression
            // is valid here and must not be rewritten as an n-ary placeholder.
            format!(
                "<m:func>\n  <m:fName><m:r><m:t>{}</m:t></m:r></m:fName>\n  <m:e/>\n</m:func>",
                name
            )
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
            let base_omml = ast_to_omml(base);
            format!(
                "<m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup>",
                base_omml,
                ast_to_omml(exp)
            )
        }

        LatexNode::Subscript { base, sub } => {
            // Non-large operators like \lim → use m:func with sub
            let is_func = matches!(base.as_ref(), LatexNode::Operator(_));
            if is_func {
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
                "mathbf" | "boldsymbol" | "bm" => wrap_omml_runs_with_math_style(&inner, "b"),
                "mathbb" => wrap_omml_runs_with_math_style(&inner, "d"),
                "mathcal" => wrap_omml_runs_with_math_style(&inner, "c"),
                "mathfrak" => wrap_omml_runs_with_math_style(&inner, "f"),
                "mathit" | "mathnormal" => wrap_omml_runs_with_math_style(&inner, "i"),
                "mathsf" => wrap_omml_runs_with_math_style(&inner, "s"),
                "mathtt" => wrap_omml_runs_with_math_style(&inner, "t"),
                "mathrm" => wrap_normal_mtext(&extract_text_from_omml(&inner)),
                _ => inner,
            }
        }

        LatexNode::OperatorName { args, .. } => {
            // \operatorname{Spec} — render as upright text
            let name = args.first().map(|a| extract_text_from_omml(&ast_to_omml(a))).unwrap_or_default();
            format!(
                "<m:r><m:t>{} </m:t></m:r>",
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

        LatexNode::Overset { top, base } => {
            // Word OMML doesn't have a perfect overset equivalent.
            // Use m:bar with position=top, which draws a bar above the base.
            // For text stacking, we use sSup on the base's m:e.
            let base_str = ast_to_omml(base);
            let top_str = ast_to_omml(top);
            format!(
                "<m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup>",
                base_str, top_str
            )
        }

        LatexNode::Underset { bottom, base } => {
            let base_str = ast_to_omml(base);
            let bottom_str = ast_to_omml(bottom);
            format!(
                "<m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub>",
                base_str, bottom_str
            )
        }

        LatexNode::XArrow { direction, above, below } => {
            let sym = if direction == "rightarrow" { "\u{2192}" } else { "\u{2190}" };
            let arrow_text = wrap_mtext(sym);
            match (above, below) {
                (Some(a), Some(b)) => {
                    let above_str = ast_to_omml(a);
                    let below_str = ast_to_omml(b);
                    format!(
                        "<m:sSubSup><m:e>{}</m:e><m:sub>{}</m:sub><m:sup>{}</m:sup></m:sSubSup>",
                        arrow_text, below_str, above_str
                    )
                }
                (Some(a), None) => {
                    let above_str = ast_to_omml(a);
                    format!(
                        "<m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup>",
                        arrow_text, above_str
                    )
                }
                (None, Some(b)) => {
                    let below_str = ast_to_omml(b);
                    format!(
                        "<m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub>",
                        arrow_text, below_str
                    )
                }
                (None, None) => arrow_text,
            }
        }

        LatexNode::Delimited { left, content, right } => {
            format!(
                "<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
                xml_escape(left),
                xml_escape(right),
                sequence_to_omml(content)
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
                    wrap_normal_mtext(&text)
                }
                "underline" => {
                    if let Some(arg) = args.first() {
                        let inner = ast_to_omml(arg);
                        wrap_with_underline(&inner)
                    } else {
                        String::new()
                    }
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
                "vphantom" | "hphantom" => {
                    "<m:r><m:t></m:t></m:r>".to_string()
                }
                "boxed" => {
                    if let Some(arg) = args.first() {
                        let inner = ast_to_omml(arg);
                        format!("<m:borderBox><m:e>{}</m:e></m:borderBox>", inner)
                    } else {
                        String::new()
                    }
                }
                // OMML has no standalone display-style element. Preserve the
                // expression rather than emitting an invalid run property.
                "displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle" => {
                    if let Some(arg) = args.first() {
                        ast_to_omml(arg)
                    } else {
                        String::new()
                    }
                }
                "tag" => {
                    if let Some(arg) = args.first() {
                        let text = extract_text_from_args(std::slice::from_ref(arg));
                        wrap_normal_mtext(&format!("({text})"))
                    } else {
                        String::new()
                    }
                }
                "abs" => {
                    if let Some(arg) = args.first() {
                        let inner = ast_to_omml(arg);
                        format!(
                            "<m:d>\n  <m:dPr><m:begChr m:val=\"|\"/><m:endChr m:val=\"|\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
                            inner
                        )
                    } else {
                        String::new()
                    }
                }
                "norm" => {
                    if let Some(arg) = args.first() {
                        let inner = ast_to_omml(arg);
                        format!(
                            "<m:d>\n  <m:dPr><m:begChr m:val=\"‖\"/><m:endChr m:val=\"‖\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
                            inner
                        )
                    } else {
                        String::new()
                    }
                }
                "floor" => {
                    if let Some(arg) = args.first() {
                        let inner = ast_to_omml(arg);
                        format!(
                            "<m:d>\n  <m:dPr><m:begChr m:val=\"⌊\"/><m:endChr m:val=\"⌋\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
                            inner
                        )
                    } else {
                        String::new()
                    }
                }
                "ceil" => {
                    if let Some(arg) = args.first() {
                        let inner = ast_to_omml(arg);
                        format!(
                            "<m:d>\n  <m:dPr><m:begChr m:val=\"⌈\"/><m:endChr m:val=\"⌉\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
                            inner
                        )
                    } else {
                        String::new()
                    }
                }
                _ => {
                    // Unknown command — render arguments as sequence
                    let parts: Vec<String> = args.iter().map(ast_to_omml).collect();
                    parts.join("")
                }
            }
        }

        LatexNode::DescriptionItem { label, content } => {
            let mut result = String::new();
            // Add label in bold if present
            if let Some(label_node) = label {
                let label_omml = ast_to_omml(label_node);
                result.push_str(&wrap_with_bold(&label_omml));
            }
            // Add content
            let content_omml: Vec<String> = content.iter().map(ast_to_omml).collect();
            result.push_str(&content_omml.join(""));
            result
        }

        LatexNode::Description(items) => {
            let parts: Vec<String> = items.iter().map(ast_to_omml).collect();
            parts.join("")
        }

        LatexNode::Footnote { content } => {
            let inner = ast_to_omml(content);
            format!("[^{}]", inner)
        }

        LatexNode::Label { .. } => {
            // Labels are not rendered
            String::new()
        }

        LatexNode::Reference { key, eq_ref } => {
            if *eq_ref {
                format!("(?{})", key)
            } else {
                format!("({})", key)
            }
        }

        LatexNode::Citation { key, .. } => {
            format!("[{}]", key)
        }

        LatexNode::Bibliography { .. } => {
            // Bibliography is not rendered in OMML
            String::new()
        }

        LatexNode::TableOfContents => {
            wrap_mtext("目录")
        }

        LatexNode::Theorem { name, content } => {
            let inner = ast_to_omml(content);
            format!(
                "<m:r><m:t>{}.</m:t></m:r> {}",
                name, inner
            )
        }

        LatexNode::Proof { content } => {
            let inner = ast_to_omml(content);
            format!(
                "<m:r><m:t>Proof.</m:t></m:r> {} □",
                inner
            )
        }

        LatexNode::Minipage { content, .. } => {
            ast_to_omml(content)
        }

        LatexNode::Float { content, .. } => {
            ast_to_omml(content)
        }

        LatexNode::Group(nodes) => {
            sequence_to_omml(nodes)
        }

        LatexNode::Math { content, .. } => {
            sequence_to_omml(content)
        }

        LatexNode::Sequence(nodes) => sequence_to_omml(nodes),
    }
}

// ── Helper functions ──

#[derive(Clone, Copy)]
enum NaryOperator {
    Integral,
    DoubleIntegral,
    TripleIntegral,
    ContourIntegral,
    Sum,
    Product,
    Coproduct,
    Union,
    Intersection,
}

impl NaryOperator {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "int" => Some(Self::Integral),
            "iint" => Some(Self::DoubleIntegral),
            "iiint" => Some(Self::TripleIntegral),
            "oint" => Some(Self::ContourIntegral),
            "sum" => Some(Self::Sum),
            "prod" => Some(Self::Product),
            "coprod" => Some(Self::Coproduct),
            "bigcup" => Some(Self::Union),
            "bigcap" => Some(Self::Intersection),
            _ => None,
        }
    }

    fn symbol(self) -> &'static str {
        match self {
            Self::Integral => "∫",
            Self::DoubleIntegral => "∬",
            Self::TripleIntegral => "∭",
            Self::ContourIntegral => "∮",
            Self::Sum => "∑",
            Self::Product => "∏",
            Self::Coproduct => "∐",
            Self::Union => "⋃",
            Self::Intersection => "⋂",
        }
    }

    fn limit_location(self) -> &'static str {
        match self {
            Self::Integral
            | Self::DoubleIntegral
            | Self::TripleIntegral
            | Self::ContourIntegral => "subSup",
            Self::Sum | Self::Product | Self::Coproduct | Self::Union | Self::Intersection => {
                "undOvr"
            }
        }
    }
}

struct OmmlNary<'a> {
    operator: NaryOperator,
    sub: Option<&'a LatexNode>,
    sup: Option<&'a LatexNode>,
    body: Vec<&'a LatexNode>,
}

fn normalize_nary_head(node: &LatexNode) -> Option<OmmlNary<'_>> {
    match node {
        LatexNode::Operator(name) => Some(OmmlNary {
            operator: NaryOperator::from_name(name)?,
            sub: None,
            sup: None,
            body: Vec::new(),
        }),
        LatexNode::Subscript { base, sub } => {
            if let LatexNode::Superscript { base: inner, exp } = base.as_ref() {
                let mut nary = normalize_nary_head(inner)?;
                nary.sub = Some(sub);
                nary.sup = Some(exp);
                Some(nary)
            } else {
                let mut nary = normalize_nary_head(base)?;
                nary.sub = Some(sub);
                Some(nary)
            }
        }
        LatexNode::Superscript { base, exp } => {
            if let LatexNode::Subscript { base: inner, sub } = base.as_ref() {
                let mut nary = normalize_nary_head(inner)?;
                nary.sub = Some(sub);
                nary.sup = Some(exp);
                Some(nary)
            } else {
                let mut nary = normalize_nary_head(base)?;
                nary.sup = Some(exp);
                Some(nary)
            }
        }
        LatexNode::Group(nodes) if nodes.len() == 1 => normalize_nary_head(&nodes[0]),
        _ => None,
    }
}

fn sequence_to_omml(nodes: &[LatexNode]) -> String {
    let mut output = String::new();
    let mut segment_start = 0;

    for (index, node) in nodes.iter().enumerate() {
        if is_nary_operand_boundary(node) {
            output.push_str(&nary_segment_to_omml(&nodes[segment_start..index]));
            output.push_str(&ast_to_omml(node));
            segment_start = index + 1;
        }
    }

    output.push_str(&nary_segment_to_omml(&nodes[segment_start..]));
    output
}

fn nary_segment_to_omml(nodes: &[LatexNode]) -> String {
    for (index, node) in nodes.iter().enumerate() {
        if let Some(mut nary) = normalize_nary_head(node) {
            let mut output = nodes[..index].iter().map(ast_to_omml).collect::<String>();
            nary.body.extend(nodes[index + 1..].iter());
            output.push_str(&nary_to_omml(&nary));
            return output;
        }
    }

    nodes.iter().map(ast_to_omml).collect()
}

fn is_nary_operand_boundary(node: &LatexNode) -> bool {
    match node {
        LatexNode::Relation(_) => true,
        LatexNode::Text(text) => matches!(text.trim(), "=" | "<" | ">"),
        _ => false,
    }
}

fn nary_to_omml(nary: &OmmlNary<'_>) -> String {
    let mut properties = format!(
        "<m:chr m:val=\"{}\"/><m:limLoc m:val=\"{}\"/><m:grow m:val=\"1\"/>",
        nary.operator.symbol(),
        nary.operator.limit_location()
    );
    if nary.sub.is_none() {
        properties.push_str("<m:subHide m:val=\"1\"/>");
    }
    if nary.sup.is_none() {
        properties.push_str("<m:supHide m:val=\"1\"/>");
    }

    let sub = nary.sub.map(ast_to_omml).unwrap_or_default();
    let sup = nary.sup.map(ast_to_omml).unwrap_or_default();
    let body = if nary.body.is_empty() {
        "<m:r><m:t></m:t></m:r>".to_string()
    } else {
        let owned: Vec<LatexNode> = nary.body.iter().map(|node| (*node).clone()).collect();
        sequence_to_omml(&owned)
    };

    format!(
        "<m:nary><m:naryPr>{properties}</m:naryPr><m:sub>{sub}</m:sub><m:sup>{sup}</m:sup><m:e>{body}</m:e></m:nary>"
    )
}

fn wrap_mtext(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    // Strip XML control characters (U+0000-U+001F) except \t, \n, \r
    let escaped: String = escaped
        .chars()
        .filter(|&c| c.is_ascii_graphic() || c.is_ascii_whitespace() || !c.is_ascii())
        .collect();
    format!("<m:r><m:t>{}</m:t></m:r>", escaped)
}

fn wrap_normal_mtext(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;");
    let escaped: String = escaped
        .chars()
        .filter(|&c| c.is_ascii_graphic() || c.is_ascii_whitespace() || !c.is_ascii())
        .collect();
    format!("<m:r><m:rPr><m:nor/></m:rPr><m:t>{}</m:t></m:r>", escaped)
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
    wrap_omml_runs_with_word_rpr(omml_content, &color_tag)
}

/// Apply a math style to every text run inside an OMML fragment without flattening structures.
fn wrap_omml_runs_with_math_style(omml_content: &str, style: &str) -> String {
    let style_tag = format!("<m:sty m:val=\"{}\"/>", style);
    if omml_content.contains(&style_tag) {
        return omml_content.to_string();
    }

    omml_content
        .replace("<m:r><m:rPr>", &format!("<m:r><m:rPr>{}", style_tag))
        .replace(
            "<m:r><m:t>",
            &format!("<m:r><m:rPr>{}</m:rPr><m:t>", style_tag),
        )
}

fn wrap_with_bold(omml_content: &str) -> String {
    omml_content.to_string()
}

fn wrap_with_underline(omml_content: &str) -> String {
    omml_content.to_string()
}

fn wrap_with_size(omml_content: &str, half_points: u16) -> String {
    let size_tag = format!("<w:sz w:val=\"{}\"/>", half_points);
    wrap_omml_runs_with_word_rpr(omml_content, &size_tag)
}

fn wrap_omml_runs_with_word_rpr(omml_content: &str, word_rpr_tag: &str) -> String {
    if omml_content.contains(word_rpr_tag) {
        return omml_content.to_string();
    }

    let word_rpr = format!("<w:rPr>{}</w:rPr>", word_rpr_tag);
    omml_content
        .replace("<m:r><m:rPr>", &format!("<m:r><m:rPr>{}", word_rpr))
        .replace(
            "<m:r><m:t>",
            &format!("<m:r><m:rPr>{}</m:rPr><m:t>", word_rpr),
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

    let mut rows_xml = Vec::new();
    for row in rows {
        let cell_xml: Vec<String> = row
            .iter()
            .map(|cell| format!("    <m:e>{}</m:e>", ast_to_omml(cell)))
            .collect();
        rows_xml.push(format!(
            "  <m:mr>\n{}\n  </m:mr>",
            if cell_xml.is_empty() {
                "    <m:e></m:e>".to_string()
            } else {
                cell_xml.join("\n")
            }
        ));
    }

    let matrix = format!("<m:m>\n{}\n</m:m>", rows_xml.join("\n"));
    if open.is_empty() && close.is_empty() {
        matrix
    } else {
        format!(
            "<m:d>\n  <m:dPr><m:begChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>\n  <m:e>{}</m:e>\n</m:d>",
            xml_escape(open),
            xml_escape(close),
            matrix
        )
    }
}

fn cases_to_omml(rows: &[Vec<LatexNode>]) -> String {
    let mut rows_xml = Vec::new();
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| format!("    <m:e>{}</m:e>", ast_to_omml(cell)))
            .collect();
        rows_xml.push(format!(
            "  <m:mr>\n{}\n  </m:mr>",
            if cells.is_empty() {
                "    <m:e></m:e>".to_string()
            } else {
                cells.join("\n")
            }
        ));
    }
    format!(
        "<m:d>\n  <m:dPr><m:begChr m:val=\"{{\"/><m:endChr m:val=\"\"/></m:dPr>\n  <m:e><m:m>\n{}\n  </m:m></m:e>\n</m:d>",
        rows_xml.join("\n")
    )
}

/// Map LaTeX spacing commands to OMML <m:spacing> values.
/// OMML spacing values: 0=zero, 1=1em(quad), 2=2em(qquad),
/// 3=3/18em(thin), 4=4/18em(medium), 5=5/18em(thick), 6=-3/18em(neg thin).
fn map_omml_spacing(name: &str) -> Option<&str> {
    match name {
        "quad" => Some("1"),
        "qquad" => Some("2"),
        "," => Some("3"),
        ":" => Some("4"),
        ";" => Some("5"),
        "!" => Some("6"),
        _ => None,
    }
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
        "\\implies" | "\\implies " => Some("\u{21D2}"),
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
        s if s.len() == 6
            && !s.chars().any(|c| c.is_ascii_alphabetic())
            && s.chars().all(|c| c.is_ascii_hexdigit()) =>
        {
            s.to_uppercase()
        }
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
        // If ?> not found (e.g. truncated XML), remove <?xml line
        // by scanning to newline or end
        if s[pos..].starts_with("<?xml") && s[pos..].contains("?>") {
            // already handled above
        } else {
            // Remove from <?xml to first newline or end
            let end = s[pos..].find('\n').map(|n| pos + n + 1).unwrap_or(s.len());
            s.replace_range(..end, "");
        }
    }

    // Fix XSLT tag typos
    s = s.replace("<m:eqAr>", "<m:eqArr>");
    s = s.replace("</m:eqAr>", "</m:eqArr>");

    // Remove mml namespace prefix remnants
    s = s.replace(" xmlns:mml=\"http://www.w3.org/1998/Math/MathML\"", "");

    // Bare run fallback disabled — was adding invalid <w:rPr> inside <m:rPr>
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
        // No formatting needed
    }

    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::events::Event;
    use quick_xml::Reader;
    use serde::Deserialize;

    #[derive(Debug)]
    struct NarySnapshot {
        operator: String,
        body: String,
    }

    #[derive(Debug)]
    struct ActiveNary {
        snapshot_index: usize,
        element_depth: usize,
        body_depth: Option<usize>,
    }

    fn local_tag(name: &[u8]) -> String {
        let name = std::str::from_utf8(name).expect("fixture tags must be UTF-8");
        name.rsplit(':').next().unwrap_or(name).to_string()
    }

    fn nary_snapshots(xml: &str) -> Vec<NarySnapshot> {
        let mut reader = Reader::from_str(xml);
        let mut buffer = Vec::new();
        let mut stack = Vec::<String>::new();
        let mut active = Vec::<ActiveNary>::new();
        let mut snapshots = Vec::<NarySnapshot>::new();

        loop {
            match reader.read_event_into(&mut buffer) {
                Ok(Event::Start(event)) => {
                    let tag = local_tag(event.name().as_ref());
                    if tag == "nary" {
                        snapshots.push(NarySnapshot {
                            operator: String::new(),
                            body: String::new(),
                        });
                        active.push(ActiveNary {
                            snapshot_index: snapshots.len() - 1,
                            element_depth: stack.len() + 1,
                            body_depth: None,
                        });
                    } else if tag == "e" {
                        if let Some(nary) = active.last_mut() {
                            if stack.len() == nary.element_depth {
                                nary.body_depth = Some(stack.len() + 1);
                            }
                        }
                    }
                    stack.push(tag);
                }
                Ok(Event::Empty(event)) => {
                    if local_tag(event.name().as_ref()) == "chr" {
                        if let Some(nary) = active.last() {
                            if !snapshots[nary.snapshot_index].operator.is_empty() {
                                buffer.clear();
                                continue;
                            }
                            let value = event
                                .attributes()
                                .flatten()
                                .find(|attribute| local_tag(attribute.key.as_ref()) == "val")
                                .map(|attribute| {
                                    String::from_utf8_lossy(attribute.value.as_ref()).into_owned()
                                })
                                .unwrap_or_default();
                            snapshots[nary.snapshot_index].operator = value;
                        }
                    }
                }
                Ok(Event::Text(event)) => {
                    let text =
                        crate::xml_util::decode_and_unescape_text(&event).unwrap_or_default();
                    for nary in &active {
                        if nary
                            .body_depth
                            .is_some_and(|body_depth| stack.len() >= body_depth)
                        {
                            snapshots[nary.snapshot_index].body.push_str(&text);
                        }
                    }
                }
                Ok(Event::End(event)) => {
                    let tag = local_tag(event.name().as_ref());
                    if tag == "e" {
                        if let Some(nary) = active.last_mut() {
                            if nary.body_depth == Some(stack.len()) {
                                nary.body_depth = None;
                            }
                        }
                    } else if tag == "nary" {
                        active.pop();
                    }
                    stack.pop();
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => panic!("invalid generated OMML: {error}: {xml}"),
            }
            buffer.clear();
        }

        snapshots
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct NaryFixture {
        schema_version: u32,
        cases: Vec<NaryFixtureCase>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct NaryFixtureCase {
        latex: String,
        expected_fragment: String,
        expected_operator: String,
        expected_body_tokens: Vec<String>,
        expected_nary_count: usize,
    }

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
    fn integral_operand_is_inside_nary_e() {
        let result = latex_to_omml("\\int_{0}^{1} x\\,dx");
        let naries = nary_snapshots(&result);
        assert_eq!(naries.len(), 1, "{result}");
        assert_eq!(naries[0].operator, "∫");
        assert!(naries[0].body.contains('x'), "{result}");
        assert!(naries[0].body.contains("dx"), "{result}");
    }

    #[test]
    fn debug_arrow_limits() {
        let result = latex_to_omml("x \\to^{a}_{b}");
        eprintln!("=== arrow with limits ===\n{}", result);
    }

    #[test]
    fn debug_xrightarrow() {
        let result = latex_to_omml("\\xrightarrow{abc}");
        eprintln!("=== xrightarrow with text ===\n{}", result);
    }

    #[test]
    fn debug_xrightarrow_both() {
        let result = latex_to_omml("\\xrightarrow[below]{above}");
        eprintln!("=== xrightarrow with both ===\n{}", result);
    }

    #[test]
    fn debug_overset() {
        let result = latex_to_omml("\\overset{*}{x}");
        eprintln!("=== overset ===\n{}", result);
    }

    #[test]
    fn test_simple_text() {
        let result = latex_to_omml("hello");
        assert!(result.contains("<m:t>hello</m:t>"), "got: {}", result);
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
    fn cases_keeps_extra_columns() {
        let result = latex_to_omml("\\begin{cases}x&a&b\\\\y&c&d\\end{cases}");
        for value in ["a", "b", "c", "d"] {
            assert!(
                result.contains(&format!("<m:t>{}</m:t>", value)),
                "cases column was dropped: {}",
                result
            );
        }
    }

    #[test]
    fn test_color() {
        let result = latex_to_omml("\\textcolor{red}{x}");
        assert!(
            result.contains("<m:t>x</m:t>"),
            "color test should contain x: {}",
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
    // Note: Color support via <w:rPr> was removed — it produced invalid OMML
    // (Word namespace nested inside Math namespace). Tests check content only.

    #[test]
    fn textcolor_red() {
        let r = latex_to_omml("\\textcolor{red}{x}");
        assert!(r.contains("<m:t>x</m:t>"), "red test: {}", r);
    }

    #[test]
    fn textcolor_blue() {
        let r = latex_to_omml("\\textcolor{blue}{y}");
        assert!(r.contains("<m:t>y</m:t>"), "blue test: {}", r);
    }

    #[test]
    fn textcolor_hex() {
        let r = latex_to_omml("\\textcolor{#FF8800}{z}");
        assert!(r.contains("<m:t>z</m:t>"), "hex test: {}", r);
    }

    #[test]
    fn color_green() {
        let r = latex_to_omml("\\color{green}x+y");
        assert!(
            r.contains("<w:color w:val=\"00FF00\"/>"),
            "green color missing: {}",
            r
        );
        assert!(r.contains("<m:t>x</m:t>"), "green x missing: {}", r);
        assert!(r.contains("<m:t>+</m:t>"), "green plus missing: {}", r);
        assert!(r.contains("<m:t>y</m:t>"), "green y missing: {}", r);
    }

    #[test]
    fn nested_color_and_frac() {
        let r = latex_to_omml("\\textcolor{red}{\\frac{a}{b}}");
        assert!(r.contains("<m:f>"), "fraction missing: {}", r);
        assert!(r.contains("<m:t>a</m:t>"), "numerator missing: {}", r);
    }

    // ═══ Comprehensive: Font Styles ═══
    // Bold/italic/font via <w:rPr> removed — Word applies default math
    // formatting (Cambria Math, italic) automatically.

    #[test]
    fn mathbf_bold() {
        let r = latex_to_omml("\\mathbf{x}");
        assert!(r.contains("<m:t>x</m:t>"), "mathbf test: {}", r);
    }

    #[test]
    fn mathbf_keeps_nested_fraction_structure() {
        let r = latex_to_omml("\\mathbf{\\frac{a}{b}}");
        assert!(r.contains("<m:f>"), "fraction was flattened: {}", r);
        assert!(
            r.contains("<m:sty m:val=\"b\"/>"),
            "bold math style missing from nested runs: {}",
            r
        );
    }

    #[test]
    fn boldsymbol() {
        let r = latex_to_omml("\\boldsymbol{\\alpha}");
        assert!(r.contains("α"), "alpha missing: {}", r);
    }

    #[test]
    fn text_command() {
        let r = latex_to_omml("\\text{Hello}");
        assert!(r.contains("Hello"), "text missing: {}", r);
    }

    #[test]
    fn operatorname_upright() {
        let r = latex_to_omml("\\operatorname{Spec}");
        assert!(r.contains("Spec"), "Spec missing: {}", r);
    }

    // ═══ Pure AST-level "ast_to_omml" tests (no DocumentConverter pipeline) ═══

    use crate::latex_parser::parse_latex;

    #[test]
    fn ast_omml_fraction() {
        let ast = parse_latex("\\frac{a}{b}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:f>"), "frac: {}", omml);
        assert!(omml.contains("<m:num>"), "num: {}", omml);
        assert!(omml.contains("<m:den>"), "den: {}", omml);
    }

    #[test]
    fn ast_omml_superscript() {
        let ast = parse_latex("x^{2}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:sSup>"), "sup: {}", omml);
    }

    #[test]
    fn ast_omml_subscript() {
        let ast = parse_latex("x_{i}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:sSub>"), "sub: {}", omml);
    }

    #[test]
    fn ast_omml_sqrt() {
        let ast = parse_latex("\\sqrt{x}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:rad>"), "rad: {}", omml);
    }

    #[test]
    fn ast_omml_integral() {
        let ast = parse_latex("\\int_{0}^{\\infty} f(x) dx");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:nary>"), "nary: {}", omml);
    }

    #[test]
    fn ast_omml_nary_limit_keeps_nested_fraction() {
        let ast = parse_latex("\\sum_{\\frac{a}{b}}^{n} x");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:nary>"), "nary: {}", omml);
        assert!(
            omml.contains("<m:sub><m:f>"),
            "nested fraction in nary subscript was flattened: {}",
            omml
        );
    }

    #[test]
    fn ast_omml_xrightarrow() {
        let ast = parse_latex("\\xrightarrow{abc}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:sSup>"), "sup for arrow: {}", omml);
        assert!(omml.contains("→"), "arrow symbol: {}", omml);
    }

    #[test]
    fn ast_omml_xrightarrow_both() {
        let ast = parse_latex("\\xrightarrow[below]{above}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:sSubSup>"), "subsup: {}", omml);
    }

    #[test]
    fn ast_omml_overset() {
        let ast = parse_latex("\\overset{*}{x}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:sSup>"), "sup: {}", omml);
    }

    #[test]
    fn ast_omml_underset() {
        let ast = parse_latex("\\underset{n}{x}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("<m:sSub>"), "sub: {}", omml);
    }

    #[test]
    fn ast_omml_text_with_spaces() {
        let ast = parse_latex("\\text{Hello World}");
        let omml = ast_to_omml(&ast);
        assert!(omml.contains("Hello"), "Hello: {}", omml);
        assert!(omml.contains("World"), "World: {}", omml);
    }

    #[test]
    fn ast_omml_no_self_closing_e_in_nary() {
        let inputs = &[
            "\\sum_{i=0}^{n} x_i",
            "\\int_{0}^{1}",
            "\\sum",
            "\\prod_{i=1}^{n} a_i",
        ];
        for latex in inputs {
            let ast = parse_latex(latex);
            let omml = ast_to_omml(&ast);
            assert!(
                !omml.contains("<m:e/>"),
                "nary with self-closing <m:e/> (Word box!) for {}: {}",
                latex,
                omml
            );
        }
    }

    #[test]
    fn canonical_nary_fixture_owns_every_operand() {
        let fixture: NaryFixture = serde_json::from_str(include_str!(
            "../../../contracts/fixtures/omml-nary-canonical-v1.json"
        ))
        .expect("canonical n-ary fixture must be valid");
        assert_eq!(fixture.schema_version, 1);

        for case in fixture.cases {
            let omml = latex_to_omml(&case.latex);
            assert!(
                omml.contains(&case.expected_fragment),
                "canonical properties differ for {}: {}",
                case.latex,
                omml
            );
            let snapshots = nary_snapshots(&omml);
            assert_eq!(
                snapshots.len(),
                case.expected_nary_count,
                "n-ary count differs for {}: {}",
                case.latex,
                omml
            );
            assert_eq!(
                snapshots[0].operator, case.expected_operator,
                "{}",
                case.latex
            );
            for token in case.expected_body_tokens {
                assert!(
                    snapshots[0].body.contains(&token),
                    "{token:?} is not owned by the outer n-ary body for {}: {}",
                    case.latex,
                    omml
                );
            }
        }
    }

    #[test]
    fn gaussian_integral_has_complete_nary_operand() {
        let omml = latex_to_omml("\\int_{-\\infty}^{\\infty}e^{-x^2}\\,dx");
        let snapshots = nary_snapshots(&omml);
        assert_eq!(snapshots.len(), 1, "{omml}");
        for token in ["e", "x", "2", "dx"] {
            assert!(snapshots[0].body.contains(token), "{token}: {omml}");
        }
    }

    #[test]
    fn double_integral_nests_nary_operands() {
        let omml = latex_to_omml("\\int_0^1\\int_0^1 f(x,y)\\,dx\\,dy");
        let snapshots = nary_snapshots(&omml);
        assert_eq!(snapshots.len(), 2, "{omml}");
        assert!(snapshots[0].body.contains("dx"), "{omml}");
        assert!(snapshots[0].body.contains("dy"), "{omml}");
        assert!(snapshots[1].body.contains("dx"), "{omml}");
    }

    #[test]
    fn relation_after_integral_stays_outside_operand() {
        let omml = latex_to_omml("\\int_0^1 f(x)\\,dx = 1");
        let snapshots = nary_snapshots(&omml);
        assert_eq!(snapshots.len(), 1, "{omml}");
        assert!(snapshots[0].body.contains("dx"), "{omml}");
        assert!(!snapshots[0].body.contains('1'), "{omml}");
        let nary_end = omml.find("</m:nary>").expect("n-ary closing tag");
        assert!(omml[nary_end..].contains("<m:t>=</m:t>"), "{omml}");
        assert!(omml[nary_end..].contains("<m:t> 1</m:t>"), "{omml}");
    }

    #[test]
    fn vector_accent_owns_unbraced_argument() {
        let omml = latex_to_omml("x^{\\vec v}");
        assert!(omml.contains("<m:acc>"), "{omml}");
        assert!(
            omml.contains("<m:e><m:r><m:t>v</m:t></m:r></m:e>"),
            "{omml}"
        );
        assert!(!omml.contains("<m:e></m:e>"), "{omml}");
        let superscript = omml
            .split("<m:sup>")
            .nth(1)
            .and_then(|value| value.split("</m:sup>").next())
            .expect("superscript body");
        assert!(superscript.contains("<m:acc>"), "{omml}");
        assert!(superscript.contains("<m:t>v</m:t>"), "{omml}");
    }

    #[test]
    fn vector_accents_remain_inside_integral_limits() {
        let omml = latex_to_omml("\\int_{\\vec a}^{\\vec b} f(x)\\,dx");
        let nary = nary_snapshots(&omml);
        assert_eq!(nary.len(), 1, "{omml}");
        assert!(omml.contains("<m:sub><m:acc>"), "{omml}");
        assert!(omml.contains("<m:sup><m:acc>"), "{omml}");
        assert!(omml.contains("<m:t>a</m:t>"), "{omml}");
        assert!(omml.contains("<m:t>b</m:t>"), "{omml}");
        assert!(!omml.contains("<m:e></m:e>"), "{omml}");
    }

    #[test]
    fn long_arrow_commands_preserve_their_arguments() {
        for (latex, character) in [
            ("A^{\\overrightarrow{BC}}", "\u{20D7}"),
            ("A^{\\overleftarrow{BC}}", "\u{20D6}"),
        ] {
            let omml = latex_to_omml(latex);
            assert!(omml.contains("<m:acc>"), "{latex}: {omml}");
            assert!(
                omml.contains(&format!("<m:chr m:val=\"{character}\"/>")),
                "{latex}: {omml}"
            );
            assert!(omml.contains("<m:t>BC</m:t>"), "{latex}: {omml}");
            assert!(!omml.contains("<m:e></m:e>"), "{latex}: {omml}");
        }
    }

    #[test]
    fn nary_roundtrip_preserves_body() {
        let omml = latex_to_omml("I=\\int_{0}^{\\infty}x\\,dx");
        let wrapped = format!("<m:oMath>{omml}</m:oMath>");
        let latex = crate::omml_parser::parse_omml_to_latex(&wrapped)
            .expect("generated n-ary OMML must parse");
        assert!(latex.contains("\\int"), "{latex}");
        assert!(latex.contains('x'), "{latex}");
        assert!(latex.contains("dx"), "{latex}");
    }

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

    #[test]
    fn nary_no_self_closing_e() {
        // <m:e/> in nary causes Word to render a box instead of the operator.
        // All nary <m:e> elements must have content inside (even just a space).
        let cases = vec![
            "\\sum_{i=0}^{n} x_i",
            "\\int_{0}^{\\infty} f(x) dx",
            "\\prod_{i=1}^{n} a_i",
            "\\sum_{i=0}^{n}",
            "\\sum^{n}",
            "\\sum_{i}",
            "\\int_{0}^{1}",
            "\\int f(x) dx",
            "\\sum",
        ];
        for latex in &cases {
            let r = latex_to_omml(latex);
            // Every <m:e> inside nary must have content, not be self-closing
            let _nary_count = r.matches("<m:nary").count();
            let self_close_e_count = r.matches("<m:e/>").count();
            assert_eq!(
                self_close_e_count, 0,
                "nary output for {} has {} self-closing <m:e/>, Word will render boxes: {}",
                latex, self_close_e_count, r
            );
        }
    }

    #[test]
    fn layout_and_delimiter_commands_emit_valid_structures() {
        let boxed = latex_to_omml("\\boxed{x}");
        assert!(boxed.contains("<m:borderBox><m:e>"), "boxed: {boxed}");

        let tag = latex_to_omml("E=mc^2\\tag{1}");
        assert!(tag.contains("<m:t>(1)</m:t>"), "tag: {tag}");
        assert!(!tag.contains("<m:eqNum>"), "tag: {tag}");

        for (latex, open, close) in [
            ("\\abs{x}", "|", "|"),
            ("\\norm{x}", "‖", "‖"),
            ("\\floor{x}", "⌊", "⌋"),
            ("\\ceil{x}", "⌈", "⌉"),
        ] {
            let output = latex_to_omml(latex);
            assert!(output.contains(&format!("m:begChr m:val=\"{open}\"")));
            assert!(output.contains(&format!("m:endChr m:val=\"{close}\"")));
        }
    }
}
