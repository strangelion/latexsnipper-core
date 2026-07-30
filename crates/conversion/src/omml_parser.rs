use quick_xml::events::Event;
use quick_xml::Reader;

/// Parse OMML XML string into a LaTeX string.
pub fn parse_omml_to_latex(xml: &str) -> Result<String, String> {
    let math_xml = extract_o_math(xml).unwrap_or_else(|| xml.to_string());
    let cleaned = strip_xml_declaration(&math_xml);
    parse_inner(&cleaned)
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

fn decode_entities(xml: &str) -> String {
    let mut r = xml
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"");
    if let Ok(re) = regex::Regex::new(r"&#x([0-9a-fA-F]+);") {
        r = re
            .replace_all(&r, |c: &regex::Captures| {
                c.get(1)
                    .and_then(|h| u32::from_str_radix(h.as_str(), 16).ok())
                    .and_then(char::from_u32)
                    .map(|ch| ch.to_string())
                    .unwrap_or_default()
            })
            .to_string();
    }
    if let Ok(re) = regex::Regex::new(r"&#(\d+);") {
        r = re
            .replace_all(&r, |c: &regex::Captures| {
                c.get(1)
                    .and_then(|d| d.as_str().parse::<u32>().ok())
                    .and_then(char::from_u32)
                    .map(|ch| ch.to_string())
                    .unwrap_or_default()
            })
            .to_string();
    }
    r
}

fn extract_o_math(xml: &str) -> Option<String> {
    find_o_math_fragment(xml).or_else(|| {
        if xml.contains("&lt;") || xml.contains("&#") {
            find_o_math_fragment(&decode_entities(xml))
        } else {
            None
        }
    })
}

fn find_o_math_fragment(xml: &str) -> Option<String> {
    for pat in &[
        r"<m:oMathPara[\s>]",
        r"<m:oMath[\s>]",
        r"<\w+:oMathPara[\s>]",
        r"<\w+:oMath[\s>]",
        r"<oMathPara[\s>]",
        r"<oMath[\s>]",
    ] {
        if let Ok(re) = regex::Regex::new(pat) {
            if let Some(m) = re.find(xml) {
                let start = m.start();
                let tag = m
                    .as_str()
                    .trim()
                    .trim_end_matches('>')
                    .trim_end_matches(' ');
                let close = format!("</{}>", &tag[1..]);
                if let Some(end) = xml[start..].find(&close) {
                    let end = start + end + close.len();
                    let mut result = xml[start..end].to_string();
                    if !result.contains("xmlns:m=") {
                        if let Some(gt) = result.find('>') {
                            result.insert_str(gt, r#" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math""#);
                        }
                    }
                    return Some(result);
                }
            }
        }
    }
    None
}

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name).to_string();
    s.split(':').next_back().unwrap_or(&s).to_string()
}

fn parse_inner(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut text_buf = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = local(e.name().as_ref());
                stack.push((tag, Vec::new()));
                text_buf.clear();
            }
            Ok(Event::Text(e)) => {
                text_buf
                    .push_str(&crate::xml_util::decode_and_unescape_text(&e).unwrap_or_default());
            }
            Ok(Event::Empty(e)) => {
                let tag = local(e.name().as_ref());
                if tag.starts_with("xmlns") {
                    continue;
                }
                let val = e
                    .attributes()
                    .flatten()
                    .find(|a| local(a.key.as_ref()) == "val")
                    .map(|a| String::from_utf8_lossy(&a.value).to_string())
                    .unwrap_or_default();
                let val = if tag == "spacing" {
                    match val.as_str() {
                        "1" => "\\quad ".to_owned(),
                        "2" => "\\qquad ".to_owned(),
                        "3" => "\\,".to_owned(),
                        "4" => "\\:".to_owned(),
                        "5" => "\\;".to_owned(),
                        "6" => "\\!".to_owned(),
                        _ => String::new(),
                    }
                } else {
                    val
                };
                if let Some((_, ref mut parent)) = stack.last_mut() {
                    parent.push((tag, val));
                }
            }
            Ok(Event::End(_)) => {
                if let Some((tag, tagged_children)) = stack.pop() {
                    let text = text_buf.clone();
                    text_buf.clear();

                    let result = build_latex(&tag, &tagged_children, &text);

                    if let Some((_, ref mut parent)) = stack.last_mut() {
                        parent.push((tag, result));
                    } else {
                        return Ok(result);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::Comment(_))
            | Ok(Event::CData(_))
            | Ok(Event::Decl(_))
            | Ok(Event::PI(_))
            | Ok(Event::DocType(_))
            | Ok(Event::GeneralRef(_)) => continue,
            Err(e) => return Err(format!("OMML parse error: {}", e)),
        }
        buf.clear();
    }
    Err("Empty OMML document".to_string())
}

fn build_latex(tag: &str, children: &[(String, String)], _text: &str) -> String {
    match tag {
        "oMathPara" | "oMath" => children
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
            .join(""),
        "r" => {
            let cells: Vec<String> = children
                .iter()
                .filter(|(t, _)| t == "e")
                .map(|(_, v)| v.clone())
                .collect();
            if !cells.is_empty() {
                return cells.join(" & ");
            }

            // Extract run properties from rPr and apply them to all nested run content.
            let mut color = String::new();
            let mut bold = false;
            let mut italic = false;
            let mut normal_text = false;
            let mut size = String::new();
            let mut sty = String::new();
            for (tag, val) in children {
                if tag == "rPr" {
                    for part in val.split(',') {
                        if let Some(c) = part.strip_prefix("color=") {
                            color = c.to_string();
                        }
                        if part == "b=1" {
                            bold = true;
                        }
                        if part == "i=1" {
                            italic = true;
                        }
                        if part == "nor=1" {
                            normal_text = true;
                        }
                        if let Some(s) = part.strip_prefix("sz=") {
                            size = s.to_string();
                        }
                        if let Some(s) = part.strip_prefix("sty=") {
                            sty = s.to_string();
                        }
                    }
                }
            }
            let text: String = children
                .iter()
                .filter(|(t, _)| t != "rPr")
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
                .concat();
            let mut result = text;
            // Apply math style (m:sty) — overrides individual bold/italic
            if !sty.is_empty() {
                result = match sty.as_str() {
                    "b" => format!("\\mathbf{{{}}}", result),
                    "d" => format!("\\mathbb{{{}}}", result),
                    "c" => format!("\\mathcal{{{}}}", result),
                    "f" => format!("\\mathfrak{{{}}}", result),
                    "i" => format!("\\mathit{{{}}}", result),
                    "s" => format!("\\mathsf{{{}}}", result),
                    "t" => format!("\\mathtt{{{}}}", result),
                    _ => result,
                };
            } else {
                if italic {
                    result = format!("\\mathit{{{}}}", result);
                }
                if bold {
                    result = format!("\\mathbf{{{}}}", result);
                }
                if normal_text {
                    result = format!("\\mathrm{{{}}}", result);
                }
            }
            if !size.is_empty() {
                result = format!("\\{}{{{}}}", half_points_to_latex_size(&size), result);
            }
            if !color.is_empty() {
                result = format!("\\textcolor{{{}}}{{{}}}", color, result);
            }
            result
        }
        "t" => _text.to_string(),
        "f" => {
            let (num, den) = get_two(children);
            format!("\\frac{{{}}}{{{}}}", num, den)
        }
        "sSup" => {
            let (base, sup) = get_two(children);
            format!("{{{}}}^{{{}}}", base, sup)
        }
        "sSub" => {
            let (base, sub) = get_two(children);
            format!("{{{}}}_{{{}}}", base, sub)
        }
        "sSubSup" => {
            let (base, sub, sup) = get_three(children);
            format!("{{{}}}_{{{}}}^{{{}}}", base, sub, sup)
        }
        "rad" => {
            let content = get_child(children, "e");
            let deg = get_child(children, "deg");
            if deg.is_empty() {
                format!("\\sqrt{{{}}}", content)
            } else {
                format!("\\sqrt[{}]{{{}}}", deg, content)
            }
        }
        "nary" => {
            let chr = {
                let c = get_child(children, "chr");
                if c.is_empty() {
                    get_child(children, "naryPr")
                } else {
                    c
                }
            };
            let op = map_nary(&chr);
            let sub = get_child(children, "sub");
            let sup = get_child(children, "sup");
            let body = get_child(children, "e");
            if sub.is_empty() && sup.is_empty() {
                format!("{}{}", op, body)
            } else if sub.is_empty() {
                format!("{}^{{{}}}{{{}}}", op, sup, body)
            } else if sup.is_empty() {
                format!("{}_{{{}}}{{{}}}", op, sub, body)
            } else {
                format!("{}^{{{}}}_{{{}}}{{{}}}", op, sup, sub, body)
            }
        }
        "naryPr" => get_child(children, "chr"),
        "func" => {
            let name = get_child(children, "fName");
            let arg = get_child(children, "e");
            let cmd = map_function_name(&name);
            if arg.is_empty() {
                cmd
            } else {
                format!("{}{{{}}}", cmd, arg)
            }
        }
        "fName" => children
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
            .join(""),
        "d" => {
            let (beg, end) = get_delimiter_chars(children);
            let (beg, end) = if beg.is_empty() && end.is_empty() {
                ("(".to_string(), ")".to_string())
            } else {
                (beg, end)
            };
            let rows: Vec<String> = children
                .iter()
                .filter(|(t, _)| t == "r")
                .map(|(_, v)| v.clone())
                .collect();
            if !rows.is_empty() {
                return format_delimited_rows(&beg, &end, &rows);
            }

            let content: Vec<String> = children
                .iter()
                .filter(|(t, _)| t == "e")
                .map(|(_, v)| v.clone())
                .collect();
            let joined = content.join(" \\\\ ");
            if beg == "{" && end.is_empty() && (joined.contains(" & ") || joined.contains("\\\\")) {
                return format!("\\begin{{cases}} {} \\end{{cases}}", joined);
            }
            if let Some(env) = matrix_env_from_delimiters(&beg, &end) {
                if joined.contains(" & ") || joined.contains("\\\\") {
                    return format!("\\begin{{{}}} {} \\end{{{}}}", env, joined, env);
                }
            }
            format!("{}{}{}", beg, joined, end)
        }
        "bar" => {
            let pos = get_child(children, "pos");
            let content = get_child(children, "e");
            if pos == "top" {
                format!("\\overline{{{}}}", content)
            } else {
                format!("\\underline{{{}}}", content)
            }
        }
        "acc" => {
            let chr = {
                let c = get_child(children, "chr");
                if c.is_empty() {
                    get_child(children, "accPr")
                } else {
                    c
                }
            };
            let content = get_child(children, "e");
            map_accent(&chr, &content)
        }
        "groupChr" => {
            let chr = get_child(children, "chr");
            let pos = get_child(children, "pos");
            let content = get_child(children, "e");
            if chr == "\u{23DF}" || chr == "\u{23DE}" {
                if pos == "bot" {
                    format!("\\underbrace{{{}}}", content)
                } else {
                    format!("\\overbrace{{{}}}", content)
                }
            } else {
                content
            }
        }
        "eqArr" => {
            let rows: Vec<String> = children
                .iter()
                .filter(|(t, _)| t == "e")
                .map(|(_, v)| v.clone())
                .collect();
            format!(
                "\\begin{{aligned}} {} \\end{{aligned}}",
                rows.join(" \\\\ ")
            )
        }
        "limLow" => {
            let base = normalize_limit_base(&get_child(children, "e"));
            let limit = normalize_limit_script(&get_child(children, "lim"));
            format!("{}_{{{}}}", base, limit)
        }
        "limUpp" => {
            let base = normalize_limit_base(&get_child(children, "e"));
            let limit = normalize_limit_script(&get_child(children, "lim"));
            format!("{}^{{{}}}", base, limit)
        }
        "m" => {
            let rows: Vec<String> = children
                .iter()
                .filter(|(t, _)| t == "mr" || t == "mRow")
                .map(|(_, v)| v.clone())
                .collect();
            if !rows.is_empty() {
                rows.join(" \\\\ ")
            } else {
                children
                    .iter()
                    .filter(|(t, _)| t == "e")
                    .map(|(_, v)| v.clone())
                    .collect::<Vec<_>>()
                    .join(" & ")
            }
        }
        "mr" | "mRow" => {
            let cells: Vec<String> = children
                .iter()
                .filter(|(t, _)| t == "e")
                .map(|(_, v)| v.clone())
                .collect();
            if cells.len() <= 1 {
                cells.first().cloned().unwrap_or_default()
            } else {
                cells.join(" & ")
            }
        }
        "e" | "sub" | "sup" | "num" | "den" | "deg" | "lim" => children
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
            .join(""),
        "begChr" | "endChr" | "pos" | "chr" => children
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
            .join(""),
        "dPr" => {
            let beg = get_child(children, "begChr");
            let end = get_child(children, "endChr");
            format!("beg={},end={}", beg, end)
        }
        // Run properties: extract color/bold/italic/style for parent <m:r>
        "rPr" | "w:rPr" => {
            let mut parts = Vec::new();
            for (tag, val) in children {
                if tag == "color" || tag == "w:color" {
                    parts.push(format!("color={}", val));
                }
                if tag == "b" || tag == "w:b" {
                    parts.push("b=1".to_string());
                }
                if tag == "i" || tag == "w:i" {
                    parts.push("i=1".to_string());
                }
                if tag == "nor" {
                    parts.push("nor=1".to_string());
                }
                if tag == "sz" || tag == "w:sz" {
                    parts.push(format!("sz={}", val));
                }
                if tag == "sty" && !val.is_empty() {
                    parts.push(format!("sty={}", val));
                }
                // Handle nested w:rPr inside rPr
                if (tag == "w:rPr" || tag == "rPr") && !val.is_empty() {
                    for part in val.split(',') {
                        if !part.is_empty() {
                            parts.push(part.to_string());
                        }
                    }
                }
            }
            parts.join(",")
        }
        _ => children
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn half_points_to_latex_size(value: &str) -> &'static str {
    let Ok(size) = value.parse::<u16>() else {
        return "normalsize";
    };
    match size {
        0..=12 => "tiny",
        13..=15 => "scriptsize",
        16..=17 => "footnotesize",
        18..=19 => "small",
        20..=23 => "normalsize",
        24..=28 => "large",
        29..=34 => "Large",
        35..=40 => "LARGE",
        41..=49 => "huge",
        _ => "Huge",
    }
}

fn is_content_tag(t: &str) -> bool {
    matches!(t, "e" | "sub" | "sup" | "num" | "den" | "deg")
}

fn get_two(children: &[(String, String)]) -> (String, String) {
    let content: Vec<&String> = children
        .iter()
        .filter(|(t, _)| is_content_tag(t))
        .map(|(_, v)| v)
        .collect();
    (
        content.first().map(|s| s.to_string()).unwrap_or_default(),
        content.get(1).map(|s| s.to_string()).unwrap_or_default(),
    )
}

fn get_three(children: &[(String, String)]) -> (String, String, String) {
    let content: Vec<&String> = children
        .iter()
        .filter(|(t, _)| is_content_tag(t))
        .map(|(_, v)| v)
        .collect();
    (
        content.first().map(|s| s.to_string()).unwrap_or_default(),
        content.get(1).map(|s| s.to_string()).unwrap_or_default(),
        content.get(2).map(|s| s.to_string()).unwrap_or_default(),
    )
}

fn get_child(children: &[(String, String)], tag: &str) -> String {
    children
        .iter()
        .find(|(t, _)| t == tag)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn get_delimiter_chars(children: &[(String, String)]) -> (String, String) {
    let beg = get_child(children, "begChr");
    let end = get_child(children, "endChr");
    if !beg.is_empty() || !end.is_empty() {
        return (beg, end);
    }

    let dpr = get_child(children, "dPr");
    if let Some(rest) = dpr.strip_prefix("beg=") {
        if let Some((beg, end_part)) = rest.split_once(",end=") {
            return (beg.to_string(), end_part.to_string());
        }
    }
    let mut chars = dpr.chars();
    let Some(first) = chars.next() else {
        return (String::new(), String::new());
    };
    let Some(last) = chars.next_back() else {
        return (first.to_string(), String::new());
    };
    (first.to_string(), last.to_string())
}

fn format_delimited_rows(beg: &str, end: &str, rows: &[String]) -> String {
    if beg == "{" && (end == "}" || end.is_empty()) && rows.iter().any(|row| row.contains(" & ")) {
        return format!("\\begin{{cases}} {} \\end{{cases}}", rows.join(" \\\\ "));
    }

    if let Some(env) = matrix_env_from_delimiters(beg, end) {
        return format!(
            "\\begin{{{}}} {} \\end{{{}}}",
            env,
            rows.join(" \\\\ "),
            env
        );
    }

    format!("{}{}{}", beg, rows.join(" \\\\ "), end)
}

fn matrix_env_from_delimiters(beg: &str, end: &str) -> Option<&'static str> {
    match (beg, end) {
        ("(", ")") => Some("pmatrix"),
        ("[", "]") => Some("bmatrix"),
        ("{", "}") => Some("Bmatrix"),
        ("|", "|") => Some("vmatrix"),
        _ => None,
    }
}

fn map_nary(chr: &str) -> &str {
    match chr {
        "\u{222B}" => "\\int",
        "\u{222E}" => "\\oint",
        "\u{222C}" => "\\iint",
        "\u{222D}" => "\\iiint",
        "\u{2211}" => "\\sum",
        "\u{220F}" => "\\prod",
        "\u{2210}" => "\\coprod",
        "\u{22C3}" => "\\bigcup",
        "\u{22C2}" => "\\bigcap",
        "\u{2202}" => "\\partial",
        "\u{2207}" => "\\nabla",
        _ => "\\int",
    }
}

fn map_function_name(name: &str) -> String {
    let trimmed = name.trim();
    let normalized = trimmed
        .strip_prefix("\\mathrm{")
        .and_then(|value| value.strip_suffix('}'))
        .unwrap_or(trimmed);
    match normalized {
        "lim" | "limsup" | "liminf" | "max" | "min" | "sup" | "inf" | "log" | "ln" | "sin"
        | "cos" | "tan" | "cot" | "sec" | "csc" | "arcsin" | "arccos" | "arctan" | "sinh"
        | "cosh" | "tanh" | "det" | "gcd" => format!("\\{}", normalized),
        other if !other.is_empty() => format!("\\operatorname{{{}}}", other),
        _ => String::new(),
    }
}

fn normalize_limit_base(base: &str) -> String {
    let trimmed = base.trim();
    let name = trimmed
        .strip_prefix("\\mathrm{")
        .and_then(|value| value.strip_suffix('}'))
        .unwrap_or(trimmed);
    match name {
        "lim" | "limsup" | "liminf" | "max" | "min" | "sup" | "inf" => {
            format!("\\{}", name)
        }
        _ => base.to_string(),
    }
}

fn normalize_limit_script(script: &str) -> String {
    script
        .replace('\u{2192}', "\\to ")
        .replace('\u{21D2}', "\\implies ")
}

fn map_accent(chr: &str, content: &str) -> String {
    match chr {
        "\u{0302}" | "\u{02C6}" => format!("\\hat{{{}}}", content),
        "\u{0304}" | "\u{02C9}" => format!("\\bar{{{}}}", content),
        "\u{0305}" => format!("\\overline{{{}}}", content),
        "\u{0307}" => format!("\\dot{{{}}}", content),
        "\u{0308}" => format!("\\ddot{{{}}}", content),
        "\u{030C}" | "\u{02C7}" => format!("\\check{{{}}}", content),
        "\u{0303}" | "\u{02DC}" => format!("\\tilde{{{}}}", content),
        "\u{20D7}" | "\u{2192}" => format!("\\vec{{{}}}", content),
        _ => format!("\\hat{{{}}}", content),
    }
}

#[cfg(test)]
mod structural_tests {
    use super::*;

    #[test]
    fn nary_limit_keeps_nested_fraction() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:naryPr><m:chr m:val="∑"/></m:naryPr><m:sub><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:sub><m:sup><m:r><m:t>n</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert!(
            result.contains("\\sum") && result.contains("\\frac{a}{b}"),
            "nested nary limit was lost: {}",
            result
        );
    }

    #[test]
    fn lower_limit_preserves_operator_and_relation_commands() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:limLow><m:e><m:r><m:rPr><m:nor/></m:rPr><m:t>lim</m:t></m:r></m:e><m:lim><m:r><m:t>x→1</m:t></m:r></m:lim></m:limLow></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert_eq!(result, "\\lim_{x\\to 1}");
    }

    #[test]
    fn upper_limit_preserves_known_operator_command() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:limUpp><m:e><m:r><m:rPr><m:nor/></m:rPr><m:t>sup</m:t></m:r></m:e><m:lim><m:r><m:t>n</m:t></m:r></m:lim></m:limUpp></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert_eq!(result, "\\sup^{n}");
    }

    #[test]
    fn function_name_normal_run_roundtrips_to_command() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:func><m:fName><m:r><m:rPr><m:nor/></m:rPr><m:t>sin</m:t></m:r></m:fName><m:e><m:r><m:t>x</m:t></m:r></m:e></m:func></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert_eq!(result, "\\sin{x}");
    }

    #[test]
    fn delimited_rows_become_matrix_environment() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:d><m:dPr><m:begChr m:val="("/><m:endChr m:val=")"/></m:dPr><m:r><m:e><m:r><m:t>a</m:t></m:r></m:e><m:e><m:r><m:t>b</m:t></m:r></m:e></m:r><m:r><m:e><m:r><m:t>c</m:t></m:r></m:e><m:e><m:r><m:t>d</m:t></m:r></m:e></m:r></m:d></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert!(
            result.contains("\\begin{pmatrix}")
                && result.contains("a & b")
                && result.contains("c & d"),
            "matrix row or column boundaries were lost: {}",
            result
        );
    }

    #[test]
    fn delimited_brace_rows_become_cases_environment() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:d><m:dPr><m:begChr m:val="{"/><m:endChr m:val="}"/></m:dPr><m:r><m:e><m:r><m:t>x</m:t></m:r></m:e><m:e><m:r><m:t>a</m:t></m:r></m:e><m:e><m:r><m:t>b</m:t></m:r></m:e></m:r><m:r><m:e><m:r><m:t>y</m:t></m:r></m:e><m:e><m:r><m:t>c</m:t></m:r></m:e><m:e><m:r><m:t>d</m:t></m:r></m:e></m:r></m:d></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert!(
            result.contains("\\begin{cases}")
                && result.contains("x & a & b")
                && result.contains("y & c & d"),
            "cases row or extra column boundaries were lost: {}",
            result
        );
    }
}

/// Parse OMML XML directly into a FormulaLayout (Math IR).
///
/// This bypasses the LaTeX intermediate format, providing direct OMML -> FormulaLayout
/// conversion for higher-fidelity round-trips.
pub fn parse_omml_to_layout(xml: &str) -> Result<latexsnipper_ast::FormulaLayout, String> {
    let math_xml = extract_o_math(xml).unwrap_or_else(|| xml.to_string());
    let cleaned = strip_xml_declaration(&math_xml);
    let root = parse_omml_node_to_layout(&cleaned)?;
    let symbol_count = count_symbols(&root);
    Ok(latexsnipper_ast::FormulaLayout {
        root,
        symbol_count,
        semantic_annotations: Vec::new(),
    })
}

fn parse_omml_node_to_layout(xml: &str) -> Result<latexsnipper_ast::FormulaNode, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut stack: Vec<(String, Vec<latexsnipper_ast::FormulaNode>)> = Vec::new();
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = local(e.name().as_ref()).to_lowercase();
                stack.push((tag, Vec::new()));
                current_text.clear();
            }
            Ok(Event::Text(e)) => {
                let t = crate::xml_util::decode_and_unescape_text(&e).unwrap_or_default();
                current_text.push_str(&t);
            }
            Ok(Event::Empty(_e)) => {
                let text = String::new();
                let node = make_symbol_from_text(&text);
                if let Some((_, ref mut children)) = stack.last_mut() {
                    children.push(node);
                }
            }
            Ok(Event::End(_)) => {
                if let Some((tag, children)) = stack.pop() {
                    let node = build_omml_layout_node(&tag, &children, &current_text);
                    if let Some((_, ref mut parent)) = stack.last_mut() {
                        parent.push(node);
                    } else {
                        return Ok(node);
                    }
                }
                current_text.clear();
            }
            Ok(Event::Eof) => break,
            Ok(Event::Comment(_))
            | Ok(Event::CData(_))
            | Ok(Event::Decl(_))
            | Ok(Event::PI(_))
            | Ok(Event::DocType(_))
            | Ok(Event::GeneralRef(_)) => continue,
            Err(e) => return Err(format!("XML parse error: {}", e)),
        }
    }

    stack
        .pop()
        .map(|(_, children)| {
            if children.len() == 1 {
                children.into_iter().next().unwrap()
            } else {
                latexsnipper_ast::FormulaNode::Group(children)
            }
        })
        .ok_or_else(|| "Empty OMML document".to_string())
}

fn build_omml_layout_node(
    tag: &str,
    children: &[latexsnipper_ast::FormulaNode],
    text: &str,
) -> latexsnipper_ast::FormulaNode {
    use latexsnipper_ast::{CommandInfo, FormulaNode, SymbolCategory, SymbolInfo};

    match tag {
        "r" => {
            if !text.is_empty() {
                FormulaNode::Symbol(SymbolInfo {
                    latex: text.to_string(),
                    category: SymbolCategory::Letter,
                    rect: None,
                    confidence: 1.0,
                })
            } else if children.len() == 1 {
                children[0].clone()
            } else {
                FormulaNode::Group(children.to_vec())
            }
        }
        "f" => {
            let (num, den) = get_two_children(children);
            FormulaNode::Fraction {
                num: Box::new(num),
                den: Box::new(den),
            }
        }
        "rad" => {
            // `radPr` and a hidden/empty `deg` are represented by empty nodes
            // in this lightweight layout parser. Filter those wrappers so an
            // explicit degree is preserved and the radicand remains last.
            let meaningful: Vec<_> = children
                .iter()
                .filter(|child| !is_empty_layout_node(child))
                .cloned()
                .collect();
            let (index, radicand) = match meaningful.as_slice() {
                [] => (None, FormulaNode::Text(String::new())),
                [radicand] => (None, radicand.clone()),
                [index, rest @ ..] => (Some(Box::new(index.clone())), rest.last().unwrap().clone()),
            };
            FormulaNode::SquareRoot {
                index,
                content: Box::new(radicand),
            }
        }
        "nary" => {
            let chr = find_nary_chr(children);
            let (_lower, _upper, body) = get_three_children(children);
            FormulaNode::Command(CommandInfo {
                name: chr,
                args: vec![body],
            })
        }
        "sSup" => {
            let (base, exp) = get_two_children(children);
            FormulaNode::Superscript {
                base: Box::new(base),
                exp: Box::new(exp),
            }
        }
        "sSub" => {
            let (base, sub) = get_two_children(children);
            FormulaNode::Subscript {
                base: Box::new(base),
                sub: Box::new(sub),
            }
        }
        "oMath" | "oMathPara" => {
            if children.len() == 1 {
                children[0].clone()
            } else {
                FormulaNode::Group(children.to_vec())
            }
        }
        _ => {
            if children.is_empty() && !text.is_empty() {
                FormulaNode::Symbol(SymbolInfo {
                    latex: text.to_string(),
                    category: SymbolCategory::Letter,
                    rect: None,
                    confidence: 1.0,
                })
            } else if children.len() == 1 {
                children[0].clone()
            } else {
                FormulaNode::Group(children.to_vec())
            }
        }
    }
}

fn get_two_children(
    children: &[latexsnipper_ast::FormulaNode],
) -> (latexsnipper_ast::FormulaNode, latexsnipper_ast::FormulaNode) {
    let empty = || latexsnipper_ast::FormulaNode::Text(String::new());
    match children.len() {
        0 => (empty(), empty()),
        1 => (children[0].clone(), empty()),
        _ => (children[0].clone(), children[1].clone()),
    }
}

fn get_three_children(
    children: &[latexsnipper_ast::FormulaNode],
) -> (
    latexsnipper_ast::FormulaNode,
    latexsnipper_ast::FormulaNode,
    latexsnipper_ast::FormulaNode,
) {
    let empty = || latexsnipper_ast::FormulaNode::Text(String::new());
    match children.len() {
        0 => (empty(), empty(), empty()),
        1 => (empty(), empty(), children[0].clone()),
        2 => (children[0].clone(), empty(), children[1].clone()),
        _ => (
            children[0].clone(),
            children[1].clone(),
            children[2].clone(),
        ),
    }
}

fn find_nary_chr(children: &[latexsnipper_ast::FormulaNode]) -> String {
    for child in children {
        if let latexsnipper_ast::FormulaNode::Group(group) = child {
            for node in group {
                if let latexsnipper_ast::FormulaNode::Symbol(s) = node {
                    if s.latex.len() == 1 && !s.latex.starts_with('\\') {
                        return s.latex.clone();
                    }
                }
            }
        }
    }
    "∑".to_string()
}

fn make_symbol_from_text(text: &str) -> latexsnipper_ast::FormulaNode {
    use latexsnipper_ast::{SymbolCategory, SymbolInfo};
    latexsnipper_ast::FormulaNode::Symbol(SymbolInfo {
        latex: text.to_string(),
        category: SymbolCategory::Letter,
        rect: None,
        confidence: 1.0,
    })
}

fn is_empty_layout_node(node: &latexsnipper_ast::FormulaNode) -> bool {
    use latexsnipper_ast::FormulaNode;
    match node {
        FormulaNode::Symbol(symbol) => symbol.latex.is_empty(),
        FormulaNode::Text(text) => text.is_empty(),
        FormulaNode::Group(children) => children.iter().all(is_empty_layout_node),
        _ => false,
    }
}

fn count_symbols(node: &latexsnipper_ast::FormulaNode) -> usize {
    use latexsnipper_ast::FormulaNode;
    match node {
        FormulaNode::Symbol(_) => 1,
        FormulaNode::Command(c) => c
            .args
            .iter()
            .map(count_symbols)
            .sum::<usize>()
            .saturating_add(1),
        FormulaNode::Group(children) => children.iter().map(count_symbols).sum(),
        FormulaNode::Environment(env) => env.content.iter().flatten().map(count_symbols).sum(),
        FormulaNode::Superscript { base, exp } => count_symbols(base) + count_symbols(exp),
        FormulaNode::Subscript { base, sub } => count_symbols(base) + count_symbols(sub),
        FormulaNode::Fraction { num, den } => count_symbols(num) + count_symbols(den),
        FormulaNode::SquareRoot { index, content } => {
            index.as_deref().map(count_symbols).unwrap_or_default() + count_symbols(content)
        }
        FormulaNode::Text(t) => t.chars().count(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:f><m:fPr/><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert_eq!(result, "\\frac{a}{b}");
    }

    #[test]
    fn superscript() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:sSup><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert_eq!(result, "{x}^{2}");
    }

    #[test]
    fn subscript() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:sSub><m:e><m:r><m:t>x</m:t></m:r></m:e><m:sub><m:r><m:t>i</m:t></m:r></m:sub></m:sSub></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert_eq!(result, "{x}_{i}");
    }

    #[test]
    fn sqrt() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:rad><m:radPr><m:degHide m:val="1"/></m:radPr><m:deg/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:rad></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert_eq!(result, "\\sqrt{x}");
    }

    #[test]
    fn sqrt_with_degree() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:rad><m:radPr/><m:deg><m:r><m:t>3</m:t></m:r></m:deg><m:e><m:r><m:t>x</m:t></m:r></m:e></m:rad></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert_eq!(result, "\\sqrt[3]{x}");
    }

    #[test]
    fn layout_preserves_square_root_degree() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:rad><m:radPr/><m:deg><m:r><m:t>3</m:t></m:r></m:deg><m:e><m:r><m:t>x</m:t></m:r></m:e></m:rad></m:oMath>"#;
        let layout = parse_omml_to_layout(xml).unwrap();

        assert_eq!(layout.canonical_latex(), "\\sqrt[3]{x}");
        assert_eq!(layout.symbol_count, 2);
    }

    #[test]
    fn emc2() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>E</m:t></m:r><m:r><m:t>=</m:t></m:r><m:r><m:t>m</m:t></m:r><m:r><m:t>c</m:t></m:r><m:sSup><m:e><m:r><m:t></m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert!(result.contains("E"));
        assert!(result.contains("="));
        assert!(result.contains("m"));
        assert!(result.contains("c"));
    }

    #[test]
    fn from_word_doc() {
        let xml = r#"<?xml version="1.0" standalone="yes"?>
<?mso-application progid="Word.Document"?>
<w:wordDocument xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
<m:oMathPara><m:oMath><m:r><m:t>E</m:t></m:r><m:r><m:t>=</m:t></m:r><m:r><m:t>m</m:t></m:r><m:r><m:t>c</m:t></m:r><m:sSup><m:e><m:r><m:t></m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:oMath></m:oMathPara>
</w:wordDocument>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert!(result.contains("E"));
        assert!(result.contains("="));
        assert!(result.contains("m"));
        assert!(result.contains("c"));
    }

    #[test]
    fn sum() {
        let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:naryPr><m:chr m:val="∑"/></m:naryPr><m:sub><m:r><m:t>i=1</m:t></m:r></m:sub><m:sup><m:r><m:t>n</m:t></m:r></m:sup><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#;
        let result = parse_omml_to_latex(xml).unwrap();
        assert!(result.contains("\\sum"));
    }
}
