use quick_xml::events::Event;
use quick_xml::Reader;

/// Parse MathML XML string into a LaTeX string.
pub fn parse_mathml_to_latex(xml: &str) -> Result<String, String> {
    let cleaned = strip_xml_declaration(xml);
    let mut reader = Reader::from_str(&cleaned);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = local_tag(e.name().as_ref());
                stack.push((tag, Vec::new()));
                current_text.clear();
            }
            Ok(Event::Text(e)) => {
                let t = e.unescape().unwrap_or_default().to_string();
                current_text.push_str(&t);
            }
            Ok(Event::Empty(e)) => {
                let tag = local_tag(e.name().as_ref());
                let text = extract_text_attrs(&e);
                let node = build_mathml_node(&tag, &text, &[]);
                if let Some((_, ref mut parent)) = stack.last_mut() {
                    parent.push(node);
                } else {
                    return Ok(node);
                }
            }
            Ok(Event::End(_)) => {
                if let Some((tag, children)) = stack.pop() {
                    let text = if current_text.is_empty() {
                        collect_text(&children)
                    } else {
                        current_text.clone()
                    };
                    let node = build_mathml_node(&tag, &text, &children);
                    current_text.clear();
                    if let Some((_, ref mut parent)) = stack.last_mut() {
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

    if let Some((tag, children)) = stack.pop() {
        let text = collect_text(&children);
        Ok(build_mathml_node(&tag, &text, &children))
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

fn build_mathml_node(tag: &str, text: &str, children: &[String]) -> String {
    match tag {
        "math" | "mrow" | "style" | "semantics" | "annotation-xml" | "none" => {
            if children.is_empty() {
                text.to_string()
            } else {
                children.join("")
            }
        }

        "mi" => {
            if let Some(latex) = reverse_mi_map(text) {
                latex
            } else if is_greek(text) {
                format!("\\{} ", text)
            } else if text.len() == 1 && text.chars().next().is_some_and(|c| c.is_alphabetic()) {
                text.to_string()
            } else {
                format!("\\mathrm{{{}}}", text)
            }
        }
        "mn" => text.to_string(),
        "mo" => map_operator(text),
        "mtext" | "ms" => format!("\\text{{{}}}", text),
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
            let rows = extract_matrix_rows(children);
            matrix_to_latex(&rows, "")
        }
        "mtr" => children.join(" & "),
        "mtd" => children.join(""),

        "mopen" | "mclose" => text.to_string(),
        "mpadded" => children.join(""),
        "mphantom" => {
            format!("\\phantom{{{}}}", children.join(""))
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
        "+" | "\u{2212}" | "\u{2B0}" => text.to_string(),
        "\u{00D7}" | "\u{2717}" => "\\times ".to_string(),
        "\u{00F7}" | "\u{2215}" => "\\div ".to_string(),
        "\u{22C5}" | "\u{00B7}" => "\\cdot ".to_string(),
        "\u{2264}" => "\\leq ".to_string(),
        "\u{2265}" => "\\geq ".to_string(),
        "\u{2260}" => "\\neq ".to_string(),
        "\u{2248}" => "\\approx ".to_string(),
        "\u{221E}" => "\\infty ".to_string(),
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
        "(" | ")" | "[" | "]" | "{" | "}" | "||" => text.to_string(),
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
        "\u{20D7}" => format!("\\vec{{{}}}", base),
        "\u{20E5}" => format!("\\cancel{{{}}}", base),
        _ => format!("\\overline{{{}}}", base),
    }
}

fn extract_matrix_rows(children: &[String]) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut current_row = Vec::new();
    for child in children {
        if child.starts_with("\\tr") || child.contains("&") {
            if !current_row.is_empty() {
                rows.push(current_row.clone());
                current_row.clear();
            }
        } else if !child.is_empty() {
            let cells: Vec<String> = child.split(" & ").map(|s| s.to_string()).collect();
            if cells.len() > 1 {
                rows.push(cells);
            } else {
                current_row.push(child.clone());
            }
        }
    }
    if !current_row.is_empty() {
        rows.push(current_row);
    }
    rows
}

fn matrix_to_latex(rows: &[Vec<String>], env: &str) -> String {
    let env_name = if env.is_empty() { "matrix" } else { env };
    let body = rows
        .iter()
        .map(|row| row.join(" & "))
        .collect::<Vec<_>>()
        .join(" \\\\ ");
    format!("\\begin{{{}}} {} \\end{{{}}}", env_name, body, env_name)
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
