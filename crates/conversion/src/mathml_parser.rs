use quick_xml::events::Event;
use quick_xml::Reader;

/// Parse MathML XML string into a LaTeX string.
pub fn parse_mathml_to_latex(xml: &str) -> Result<String, String> {
    let cleaned = strip_xml_declaration(xml);
    let mut reader = Reader::from_str(&cleaned);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<(String, Vec<String>, String)> = Vec::new();
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = local_tag(e.name().as_ref());
                let mut attrs = String::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|error| error.to_string())?;
                    if attr.key.as_ref() == b"xmlns" || attr.key.as_ref().starts_with(b"xmlns:") {
                        continue;
                    }
                    let key = local_tag(attr.key.as_ref());
                    if tag == "mfenced"
                        && matches!(key.as_str(), "open" | "close" | "separators")
                        && attr.key.as_ref().contains(&b':')
                    {
                        return Err("Qualified MathML fence attributes are not supported; retain the original XML".into());
                    }
                    if key == "xmlns" || key.starts_with("xmlns:") {
                        continue;
                    }
                    let val = if matches!(
                        key.as_str(),
                        "columnalign"
                            | "columnlines"
                            | "notation"
                            | "open"
                            | "close"
                            | "separators"
                    ) {
                        attr.decoded_and_normalized_value(
                            quick_xml::XmlVersion::Implicit1_0,
                            reader.decoder(),
                        )
                        .map_err(|error| error.to_string())?
                        .into_owned()
                    } else {
                        String::from_utf8_lossy(&attr.value).to_string()
                    };
                    if tag == "mfenced"
                        && matches!(key.as_str(), "open" | "close")
                        && latexsnipper_syntax::latex::scalable_delimiter_glyph(
                            &crate::latex_utils::delimiter_source_token(&val),
                        ) != Some(val.as_str())
                    {
                        return Err(
                            "Unsupported MathML fence glyph; retain the original XML".into()
                        );
                    }
                    if tag == "mfenced" && key == "separators" && val.chars().count() > 1 {
                        return Err("Multiple MathML fence separators are not supported; retain the original XML".into());
                    }
                    // Keep multi-valued layout attributes intact inside the legacy
                    // tokenized attribute representation, without changing style keys.
                    let val = if matches!(key.as_str(), "columnalign" | "columnlines" | "notation")
                    {
                        val.split_whitespace().collect::<Vec<_>>().join(",")
                    } else {
                        val
                    };
                    if !attrs.is_empty() {
                        attrs.push(' ');
                    }
                    attrs.push_str(&format!("{}={}", key, val));
                }
                stack.push((tag, Vec::new(), attrs));
                current_text.clear();
            }
            Ok(Event::Text(e)) => {
                let t = e.decode().map_err(|error| error.to_string())?;
                if stack.last().is_some_and(|(tag, _, _)| tag == "mfenced") && !t.trim().is_empty()
                {
                    return Err(
                        "MathML fence content must use token elements; retain the original XML"
                            .into(),
                    );
                }
                current_text.push_str(&t);
            }
            Ok(Event::GeneralRef(e)) => {
                if stack.last().is_some_and(|(tag, _, _)| tag == "mfenced") {
                    return Err(
                        "MathML fence content must use token elements; retain the original XML"
                            .into(),
                    );
                }
                current_text.push_str(&crate::xml_util::decode_xml_reference(&e)?);
            }
            Ok(Event::CData(e)) => {
                if stack.last().is_some_and(|(tag, _, _)| tag == "mfenced") {
                    return Err(
                        "MathML fence content must use token elements; retain the original XML"
                            .into(),
                    );
                }
                current_text.push_str(&e.decode().map_err(|error| error.to_string())?);
            }
            Ok(Event::Empty(e)) => {
                let tag = local_tag(e.name().as_ref());
                let text = extract_text_attrs(&e);
                let attrs = if tag == "mfenced" {
                    let mut fields = Vec::new();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|error| error.to_string())?;
                        let key = local_tag(attr.key.as_ref());
                        if matches!(key.as_str(), "open" | "close" | "separators")
                            && attr.key.as_ref().contains(&b':')
                        {
                            return Err("Qualified MathML fence attributes are not supported; retain the original XML".into());
                        }
                        if !matches!(key.as_str(), "open" | "close" | "separators") {
                            continue;
                        }
                        let value = attr
                            .decoded_and_normalized_value(
                                quick_xml::XmlVersion::Implicit1_0,
                                reader.decoder(),
                            )
                            .map_err(|error| error.to_string())?;
                        if matches!(key.as_str(), "open" | "close")
                            && latexsnipper_syntax::latex::scalable_delimiter_glyph(
                                &crate::latex_utils::delimiter_source_token(&value),
                            ) != Some(value.as_ref())
                        {
                            return Err(
                                "Unsupported MathML fence glyph; retain the original XML".into()
                            );
                        }
                        if key == "separators" && value.chars().count() > 1 {
                            return Err("Multiple MathML fence separators are not supported; retain the original XML".into());
                        }
                        fields.push(format!("{key}={value}"));
                    }
                    fields.join(" ")
                } else {
                    String::new()
                };
                let node = build_mathml_node(&tag, &text, &[], &attrs);
                if let Some((_, ref mut parent, _)) = stack.last_mut() {
                    parent.push(node);
                } else {
                    return Ok(node);
                }
            }
            Ok(Event::End(_)) => {
                if let Some((tag, children, attrs)) = stack.pop() {
                    let text = if !children.is_empty() {
                        collect_text(&children)
                    } else {
                        current_text.clone()
                    };
                    let text = if matches!(tag.as_str(), "mtext" | "ms") {
                        text.as_str()
                    } else {
                        text.trim()
                    };
                    let node = build_mathml_node(&tag, text, &children, &attrs);
                    current_text.clear();
                    if let Some((_, ref mut parent, _)) = stack.last_mut() {
                        parent.push(node);
                    } else {
                        return Ok(node);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("MathML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    if let Some((tag, children, attrs)) = stack.pop() {
        let text = collect_text(&children);
        Ok(build_mathml_node(&tag, &text, &children, &attrs))
    } else {
        Err("Empty MathML document".to_string())
    }
}

fn strip_xml_declaration(xml: &str) -> String {
    let mut s = xml.to_string();
    if let Some(pos) = s.find("<?xml") {
        if let Some(end) = s[pos..].find("?>") {
            s.replace_range(..pos + end + 2, "");
        }
    }
    s
}

fn local_tag(name: &[u8]) -> String {
    let raw = String::from_utf8_lossy(name).to_string();
    if let Some(idx) = raw.find(':') {
        raw[idx + 1..].to_string()
    } else {
        raw
    }
}

fn extract_text_attrs(e: &quick_xml::events::BytesStart) -> String {
    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref());
        if key == "alttext" || key == "open" || key == "close" {
            return String::from_utf8_lossy(&attr.value).to_string();
        }
    }
    String::new()
}

fn collect_text(children: &[String]) -> String {
    children.concat()
}

fn build_mathml_node(tag: &str, text: &str, children: &[String], attrs: &str) -> String {
    match tag {
        "mfenced" => {
            let attribute = |key: &str| {
                attrs
                    .split_whitespace()
                    .find_map(|part| part.strip_prefix(key))
            };
            let left = attribute("open=").unwrap_or("(");
            let right = attribute("close=").unwrap_or(")");
            let separator =
                crate::latex_utils::escape_text_symbols(attribute("separators=").unwrap_or(","));
            crate::latex_utils::delimited_source(left, &children.join(&separator), right)
        }
        "math" | "mrow" | "style" | "semantics" | "annotation-xml" | "none" => {
            if children.is_empty() {
                text.to_string()
            } else {
                children.join("")
            }
        }

        "mstyle" => {
            let inner = if children.is_empty() {
                text.to_string()
            } else {
                children.join("")
            };
            let mut color = String::new();
            let mut bold = false;
            let mut italic = false;
            // Check attrs string "mathcolor=red fontweight=bold"
            for part in attrs.split_whitespace() {
                if let Some(v) = part.strip_prefix("mathcolor=") {
                    color = v.to_string();
                }
                if part.starts_with("fontweight=") && part.contains("bold") {
                    bold = true;
                }
                if part.starts_with("fontstyle=") && part.contains("italic") {
                    italic = true;
                }
            }
            let mut result = inner;
            if !color.is_empty() {
                result = format!("\\textcolor{{{}}}{{{}}}", color, result);
            }
            if bold {
                result = format!("\\mathbf{{{}}}", result);
            }
            if italic {
                result = format!("\\mathit{{{}}}", result);
            }
            let display = attrs
                .split_whitespace()
                .find_map(|part| part.strip_prefix("displaystyle="));
            let level = attrs
                .split_whitespace()
                .find_map(|part| part.strip_prefix("scriptlevel="));
            if level == Some("0") {
                match display {
                    Some("true") => result = format!("{{\\displaystyle {result}}}"),
                    Some("false") => result = format!("{{\\textstyle {result}}}"),
                    _ => {}
                }
            }
            result
        }

        "mi" => {
            if let Some(latex) = reverse_mi_map(text) {
                latex
            } else if is_greek(text) {
                format!("\\{} ", text)
            } else if text.len() == 1 && text.chars().next().is_some_and(|c| c.is_alphabetic()) {
                text.to_string()
            } else {
                format!(
                    "\\mathrm{{{}}}",
                    crate::latex_utils::escape_text_symbols(text)
                )
            }
        }
        "mn" => crate::latex_utils::escape_text_symbols(text),
        "mo" => map_operator(text),
        "mtext" | "ms" => format!(
            "\\text{{{}}}",
            crate::latex_utils::escape_text_symbols(text)
        ),
        "mspace" => "\\quad".to_string(),

        "mfrac" => {
            if children.len() >= 2 {
                format!("\\frac{{{}}}{{{}}}", children[0], children[1])
            } else {
                text.to_string()
            }
        }
        "msqrt" => {
            let inner = children.join("");
            format!("\\sqrt{{{}}}", inner)
        }
        "mroot" => {
            if children.len() >= 2 {
                format!("\\sqrt[{}]{{{}}}", children[1], children[0])
            } else {
                format!("\\sqrt{{{}}}", children.join(""))
            }
        }

        "msup" => {
            if children.len() >= 2 {
                format!("{{{}}}^{{{}}}", children[0], children[1])
            } else {
                text.to_string()
            }
        }
        "msub" => {
            if children.len() >= 2 {
                format!("{{{}}}_{{{}}}", children[0], children[1])
            } else {
                text.to_string()
            }
        }
        "msubsup" => {
            if children.len() >= 3 {
                format!(
                    "{{{}}}_{{{}}}^{{{}}}",
                    children[0], children[1], children[2]
                )
            } else {
                text.to_string()
            }
        }
        "mmultiscripts" => {
            let base = children.first().map(|s| s.as_str()).unwrap_or("");
            let mut sub = String::new();
            let mut sup = String::new();
            let mut i = 1;
            while i < children.len() {
                if children[i] == *"\\mpscripts" || children[i].is_empty() {
                    i += 1;
                    continue;
                }
                if sub.is_empty() {
                    sub = children[i].clone();
                } else if sup.is_empty() {
                    sup = children[i].clone();
                }
                i += 1;
            }
            if !sub.is_empty() && !sup.is_empty() {
                format!("{{{}}}_{{{}}}^{{{}}}", base, sub, sup)
            } else if !sup.is_empty() {
                format!("{{{}}}^{{{}}}", base, sup)
            } else if !sub.is_empty() {
                format!("{{{}}}_{{{}}}", base, sub)
            } else {
                base.to_string()
            }
        }

        "mover" => {
            if children.len() >= 2 {
                let base = &children[0];
                let accent = &children[1];
                map_over_accent(base, accent)
            } else {
                text.to_string()
            }
        }
        "munder" => {
            if children.len() >= 2 {
                let base = &children[0];
                let under = &children[1];
                format!("\\underset{{{}}}{{{}}}", under, base)
            } else {
                text.to_string()
            }
        }
        "munderover" => {
            if children.len() >= 3 {
                format!(
                    "\\underset{{{}}}{{\\overset{{{}}}{{{}}}}}",
                    children[1], children[2], children[0]
                )
            } else {
                text.to_string()
            }
        }

        "mtable" => {
            let align = attrs
                .split_whitespace()
                .find_map(|part| part.strip_prefix("columnalign="))
                .or_else(|| {
                    attrs
                        .split_whitespace()
                        .any(|part| part.starts_with("columnlines="))
                        .then_some("center")
                });
            if let Some(align) = align {
                let align: Vec<_> = align.split(',').collect();
                let width = children
                    .iter()
                    .map(|row| {
                        crate::latex_utils::split_matrix_rows(row)
                            .first()
                            .map_or(0, Vec::len)
                    })
                    .max()
                    .unwrap_or(0)
                    .max(align.len());
                if width > 128
                    || align.is_empty()
                    || align
                        .iter()
                        .any(|value| !matches!(*value, "left" | "center" | "right"))
                {
                    return format!("\\unsupportedmathmlcolumns{{{}}}", children.join(" \\\\ "));
                }
                let lines = attrs
                    .split_whitespace()
                    .find_map(|part| part.strip_prefix("columnlines="))
                    .unwrap_or("");
                let lines: Vec<_> = if lines.is_empty() {
                    Vec::new()
                } else {
                    lines.split(',').collect()
                };
                if lines
                    .iter()
                    .any(|value| !matches!(*value, "none" | "solid"))
                {
                    return format!("\\unsupportedmathmlcolumns{{{}}}", children.join(" \\\\ "));
                }
                let mut columns = String::new();
                for index in 0..width {
                    let value = align
                        .get(index)
                        .or(align.last())
                        .copied()
                        .unwrap_or("center");
                    columns.push(match value {
                        "left" => 'l',
                        "right" => 'r',
                        _ => 'c',
                    });
                    if index + 1 < width
                        && lines.get(index).or(lines.last()).copied() == Some("solid")
                    {
                        columns.push('|');
                    }
                }
                format!(
                    "\\begin{{array}}{{{columns}}}{}\\end{{array}}",
                    children.join(" \\\\ ")
                )
            } else {
                matrix_to_latex(children)
            }
        }
        "mtr" => children.join(" & "),
        "mtd" => children.join(""),

        "mopen" | "mclose" => text.to_string(),
        "mpadded" => children.join(""),
        "mphantom" => {
            format!("\\phantom{{{}}}", children.join(""))
        }
        "menclose" => {
            let inner = children.join("");
            let notation = attrs
                .split_whitespace()
                .find_map(|part| part.strip_prefix("notation="))
                .unwrap_or("");
            let notation: Vec<_> = notation
                .split(',')
                .filter(|value| !value.is_empty())
                .collect();
            if !notation.is_empty()
                && notation
                    .iter()
                    .all(|value| matches!(*value, "left" | "right"))
            {
                if let crate::latex_ast::LatexNode::Array { column_spec, rows } =
                    crate::latex_parser::parse_latex(&inner)
                {
                    let spec = format!(
                        "{}{}{}",
                        if notation.contains(&"left") { "|" } else { "" },
                        column_spec,
                        if notation.contains(&"right") { "|" } else { "" }
                    );
                    return crate::latex_ast::LatexNode::Array {
                        column_spec: spec,
                        rows,
                    }
                    .to_string();
                }
            }
            // Check for notation attribute
            let notation = attrs
                .split_whitespace()
                .find(|p| p.starts_with("notation="))
                .and_then(|p| p.strip_prefix("notation="));
            match notation {
                Some("updiagonalstrike") | Some("downdiagonalstrike") => {
                    format!("\\cancel{{{}}}", inner)
                }
                Some("horizontalstrike") => {
                    format!("\\cancel{{{}}}", inner)
                }
                Some("verticalstrike") => {
                    format!("\\cancel{{{}}}", inner)
                }
                Some("madruwb") => {
                    // Box notation
                    format!("\\boxed{{{}}}", inner)
                }
                _ => inner,
            }
        }
        "mlabeledtr" => children.join(" & "),

        "mprescripts" => "\\mpscripts".to_string(),
        "mglyph" => text.to_string(),

        _ => children.join(""),
    }
}

fn reverse_mi_map(text: &str) -> Option<String> {
    match text {
        "\u{03B1}" => Some("\\alpha".to_string()),
        "\u{03B2}" => Some("\\beta".to_string()),
        "\u{03B3}" => Some("\\gamma".to_string()),
        "\u{03B4}" => Some("\\delta".to_string()),
        "\u{03B5}" => Some("\\epsilon".to_string()),
        "\u{03B6}" => Some("\\zeta".to_string()),
        "\u{03B7}" => Some("\\eta".to_string()),
        "\u{03B8}" => Some("\\theta".to_string()),
        "\u{03B9}" => Some("\\iota".to_string()),
        "\u{03BA}" => Some("\\kappa".to_string()),
        "\u{03BB}" => Some("\\lambda".to_string()),
        "\u{03BC}" => Some("\\mu".to_string()),
        "\u{03BD}" => Some("\\nu".to_string()),
        "\u{03BE}" => Some("\\xi".to_string()),
        "\u{03C0}" => Some("\\pi".to_string()),
        "\u{03C1}" => Some("\\rho".to_string()),
        "\u{03C3}" => Some("\\sigma".to_string()),
        "\u{03C4}" => Some("\\tau".to_string()),
        "\u{03C5}" => Some("\\upsilon".to_string()),
        "\u{03C6}" => Some("\\phi".to_string()),
        "\u{03C7}" => Some("\\chi".to_string()),
        "\u{03C8}" => Some("\\psi".to_string()),
        "\u{03C9}" => Some("\\omega".to_string()),
        "\u{0391}" => Some("\\Alpha".to_string()),
        "\u{0392}" => Some("\\Beta".to_string()),
        "\u{0393}" => Some("\\Gamma".to_string()),
        "\u{0394}" => Some("\\Delta".to_string()),
        "\u{0398}" => Some("\\Theta".to_string()),
        "\u{039B}" => Some("\\Lambda".to_string()),
        "\u{039E}" => Some("\\Xi".to_string()),
        "\u{03A0}" => Some("\\Pi".to_string()),
        "\u{03A3}" => Some("\\Sigma".to_string()),
        "\u{03A6}" => Some("\\Phi".to_string()),
        "\u{03A8}" => Some("\\Psi".to_string()),
        "\u{03A9}" => Some("\\Omega".to_string()),
        "\u{221E}" => Some("\\infty".to_string()),
        "\u{2202}" => Some("\\partial".to_string()),
        "\u{2207}" => Some("\\nabla".to_string()),
        "\u{2200}" => Some("\\forall".to_string()),
        "\u{2203}" => Some("\\exists".to_string()),
        "\u{2205}" => Some("\\emptyset".to_string()),
        "\u{2208}" => Some("\\in".to_string()),
        "\u{2209}" => Some("\\notin".to_string()),
        "\u{2260}" => Some("\\neq".to_string()),
        "\u{2264}" => Some("\\leq".to_string()),
        "\u{2265}" => Some("\\geq".to_string()),
        "\u{2248}" => Some("\\approx".to_string()),
        "\u{2261}" => Some("\\equiv".to_string()),
        "\u{223C}" => Some("\\sim".to_string()),
        "\u{2192}" => Some("\\rightarrow".to_string()),
        "\u{2190}" => Some("\\leftarrow".to_string()),
        "\u{2194}" => Some("\\leftrightarrow".to_string()),
        "\u{21D2}" => Some("\\Rightarrow".to_string()),
        "\u{00B1}" => Some("\\pm".to_string()),
        "\u{00D7}" => Some("\\times".to_string()),
        "\u{00F7}" => Some("\\div".to_string()),
        "\u{22C5}" => Some("\\cdot".to_string()),
        "\u{222A}" => Some("\\cup".to_string()),
        "\u{2229}" => Some("\\cap".to_string()),
        "\u{2216}" => Some("\\setminus".to_string()),
        "\u{2282}" => Some("\\subset".to_string()),
        "\u{2283}" => Some("\\supset".to_string()),
        "\u{2286}" => Some("\\subseteq".to_string()),
        "\u{2287}" => Some("\\supseteq".to_string()),
        "\u{2227}" => Some("\\wedge".to_string()),
        "\u{2228}" => Some("\\vee".to_string()),
        "\u{00AC}" => Some("\\neg".to_string()),
        "\u{2211}" => Some("\\sum".to_string()),
        "\u{220F}" => Some("\\prod".to_string()),
        "\u{2210}" => Some("\\coprod".to_string()),
        "\u{222B}" => Some("\\int".to_string()),
        "\u{222C}" => Some("\\iint".to_string()),
        "\u{222D}" => Some("\\iiint".to_string()),
        "\u{222E}" => Some("\\oint".to_string()),
        _ => None,
    }
}

fn is_greek(s: &str) -> bool {
    matches!(
        s,
        "alpha"
            | "beta"
            | "gamma"
            | "delta"
            | "epsilon"
            | "varepsilon"
            | "zeta"
            | "eta"
            | "theta"
            | "vartheta"
            | "iota"
            | "kappa"
            | "lambda"
            | "mu"
            | "nu"
            | "xi"
            | "pi"
            | "varpi"
            | "rho"
            | "varrho"
            | "sigma"
            | "varsigma"
            | "tau"
            | "upsilon"
            | "phi"
            | "varphi"
            | "chi"
            | "psi"
            | "omega"
            | "Gamma"
            | "Delta"
            | "Theta"
            | "Lambda"
            | "Xi"
            | "Pi"
            | "Sigma"
            | "Upsilon"
            | "Phi"
            | "Psi"
            | "Omega"
    )
}

fn map_operator(text: &str) -> String {
    match text {
        "%" | "&" | "#" | "_" | "$" | "{" | "}" | "\\" | "^" | "~" => {
            crate::latex_utils::escape_text_symbols(text)
        }
        "+" | "\u{2212}" | "\u{2B0}" => text.to_string(),
        "\u{00D7}" | "\u{2717}" => "\\times ".to_string(),
        "\u{00F7}" | "\u{2215}" => "\\div ".to_string(),
        "\u{22C5}" | "\u{00B7}" => "\\cdot ".to_string(),
        "\u{2264}" => "\\leq ".to_string(),
        "\u{2265}" => "\\geq ".to_string(),
        "\u{2260}" => "\\neq ".to_string(),
        "\u{2248}" => "\\approx ".to_string(),
        "\u{221E}" => "\\infty ".to_string(),
        "\u{20D6}" | "\u{20D7}" => text.to_string(),
        "\u{2190}" | "\u{2192}" | "\u{2194}" | "\u{21D2}" | "\u{21D4}" => {
            let sym = match text {
                "\u{2190}" => "\\leftarrow ",
                "\u{2192}" => "\\rightarrow ",
                "\u{2194}" => "\\leftrightarrow ",
                "\u{21D2}" => "\\Rightarrow ",
                "\u{21D4}" => "\\Leftrightarrow ",
                _ => "",
            };
            sym.to_string()
        }
        "\u{222B}" => "\\int ".to_string(),
        "\u{222C}" => "\\iint ".to_string(),
        "\u{222D}" => "\\iiint ".to_string(),
        "\u{2211}" => "\\sum ".to_string(),
        "\u{220F}" => "\\prod ".to_string(),
        "\u{2210}" => "\\coprod ".to_string(),
        "\u{2202}" => "\\partial ".to_string(),
        "\u{2207}" => "\\nabla ".to_string(),
        "\u{2261}" => "\\equiv ".to_string(),
        "\u{223C}" => "\\sim ".to_string(),
        "\u{2245}" => "\\cong ".to_string(),
        "\u{227A}" => "\\prec ".to_string(),
        "\u{227B}" => "\\succ ".to_string(),
        "\u{226A}" => "\\ll ".to_string(),
        "\u{226B}" => "\\gg ".to_string(),
        "\u{2208}" => "\\in ".to_string(),
        "\u{2209}" => "\\notin ".to_string(),
        "\u{2282}" => "\\subset ".to_string(),
        "\u{2283}" => "\\supset ".to_string(),
        "\u{2286}" => "\\subseteq ".to_string(),
        "\u{2287}" => "\\supseteq ".to_string(),
        "\u{222A}" => "\\cup ".to_string(),
        "\u{2229}" => "\\cap ".to_string(),
        "\u{2200}" => "\\forall ".to_string(),
        "\u{2203}" => "\\exists ".to_string(),
        "\u{00AC}" => "\\neg ".to_string(),
        "\u{2227}" => "\\wedge ".to_string(),
        "\u{2228}" => "\\vee ".to_string(),
        "\u{2234}" => "\\therefore ".to_string(),
        "\u{2235}" => "\\because ".to_string(),
        "(" | ")" | "[" | "]" | "||" => text.to_string(),
        _ => {
            if text.len() == 1 {
                text.to_string()
            } else {
                format!("\\operatorname{{{}}}", text)
            }
        }
    }
}

fn map_over_accent(base: &str, accent: &str) -> String {
    match accent {
        "\u{0302}" | "\u{02C6}" => format!("\\hat{{{}}}", base),
        "\u{0304}" | "\u{02C9}" => format!("\\bar{{{}}}", base),
        "\u{0305}" | "\u{2015}" => format!("\\overline{{{}}}", base),
        "\u{0307}" => format!("\\dot{{{}}}", base),
        "\u{0308}" => format!("\\ddot{{{}}}", base),
        "\u{030C}" | "\u{02C7}" => format!("\\check{{{}}}", base),
        "\u{0303}" | "\u{02DC}" => format!("\\tilde{{{}}}", base),
        "\u{20D7}" | "\u{2192}" | "\\rightarrow" | "\\rightarrow " => {
            format!("\\vec{{{}}}", base)
        }
        "\u{20D6}" | "\u{2190}" | "\\leftarrow" | "\\leftarrow " => {
            format!("\\overleftarrow{{{}}}", base)
        }
        "\u{20E5}" => format!("\\cancel{{{}}}", base),
        _ => format!("\\overline{{{}}}", base),
    }
}

fn matrix_to_latex(rows: &[String]) -> String {
    // Each child is already one XML row; never infer cells from its LaTeX text.
    let body = rows.join(" \\\\ ");
    format!("\\begin{{matrix}} {body} \\end{{matrix}}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_text() {
        let xml = r#"<math><mi>x</mi></math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert_eq!(result, "x");
    }

    #[test]
    fn fraction() {
        let xml = r#"<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert_eq!(result, "\\frac{a}{b}");
    }

    #[test]
    fn superscript() {
        let xml = r#"<math><msup><mi>x</mi><mn>2</mn></msup></math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert_eq!(result, "{x}^{2}");
    }

    #[test]
    fn subscript() {
        let xml = r#"<math><msub><mi>x</mi><mi>i</mi></msub></math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert_eq!(result, "{x}_{i}");
    }

    #[test]
    fn square_root() {
        let xml = r#"<math><msqrt><mi>x</mi></msqrt></math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert_eq!(result, "\\sqrt{x}");
    }

    #[test]
    fn root_with_degree() {
        let xml = r#"<math><mroot><mi>x</mi><mn>3</mn></mroot></math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert_eq!(result, "\\sqrt[3]{x}");
    }

    #[test]
    fn arrow_accents_roundtrip_without_symbol_loss() {
        for (arrow, command) in [
            ("\u{2192}", "\\vec{v}"),
            ("\u{2190}", "\\overleftarrow{v}"),
            ("\u{20D7}", "\\vec{v}"),
        ] {
            let xml = format!("<math><mover><mi>v</mi><mo>{arrow}</mo></mover></math>");
            assert_eq!(parse_mathml_to_latex(&xml).unwrap(), command, "{arrow}");
        }
    }

    #[test]
    fn complex_formula() {
        let xml = r#"<math><mrow><mi>E</mi><mo>=</mo><mi>m</mi><msup><mi>c</mi><mn>2</mn></msup></mrow></math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert!(result.contains("E"));
        assert!(result.contains("m"));
        assert!(result.contains("c"));
        assert!(result.contains("2"));
    }

    #[test]
    fn greek_letter() {
        let xml = r#"<math><mi>alpha</mi></math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert_eq!(result, "\\alpha ");
    }

    #[test]
    fn named_namespace() {
        let xml = r#"<mml:math xmlns:mml="http://www.w3.org/1998/Math/MathML"><mml:mi>x</mml:mi></mml:math>"#;
        let result = parse_mathml_to_latex(xml).unwrap();
        assert_eq!(result, "x");
    }
}
