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
    let decoded = if xml.contains("&lt;") || xml.contains("&#") {
        decode_entities(xml)
    } else {
        xml.to_string()
    };
    for pat in &[
        r"<m:oMathPara[\s>]",
        r"<m:oMath[\s>]",
        r"<\w+:oMathPara[\s>]",
        r"<\w+:oMath[\s>]",
        r"<oMathPara[\s>]",
        r"<oMath[\s>]",
    ] {
        if let Ok(re) = regex::Regex::new(pat) {
            if let Some(m) = re.find(&decoded) {
                let start = m.start();
                let tag = m
                    .as_str()
                    .trim()
                    .trim_end_matches('>')
                    .trim_end_matches(' ');
                let close = format!("</{}>", &tag[1..]);
                if let Some(end) = decoded[start..].find(&close) {
                    let end = start + end + close.len();
                    let mut result = decoded[start..end].to_string();
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
                text_buf.push_str(&e.unescape().unwrap_or_default());
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
            Err(e) => return Err(format!("OMML parse error: {}", e)),
            _ => {}
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
            // Extract run properties (color, bold, italic) from rPr
            let mut color = String::new();
            let mut bold = false;
            let mut italic = false;
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
                    }
                }
            }
            let text: String = children
                .iter()
                .filter(|(t, _)| t == "t")
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
                .concat();
            let mut result = text;
            if !color.is_empty() {
                result = format!("\\textcolor{{{}}}{{{}}}", color, result);
            }
            if bold {
                result = format!("\\mathbf{{{}}}", result);
            }
            if italic {
                result = format!("\\mathit{{{}}}", result);
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
        "d" => {
            let beg = get_child(children, "begChr");
            let end = get_child(children, "endChr");
            let beg = if beg.is_empty() { "(".to_string() } else { beg };
            let end = if end.is_empty() { ")".to_string() } else { end };
            let rows: Vec<String> = children
                .iter()
                .filter(|(t, _)| t == "e")
                .map(|(_, v)| v.clone())
                .collect();
            format!("{}{}{}", beg, rows.join(" \\\\ "), end)
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
            let chr = get_child(children, "chr");
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
        "m" | "mRow" => {
            let cells: Vec<String> = children
                .iter()
                .filter(|(t, _)| t == "e")
                .map(|(_, v)| v.clone())
                .collect();
            if cells.len() <= 1 {
                cells.first().cloned().unwrap_or_default()
            } else {
                cells.join("")
            }
        }
        "e" | "sub" | "sup" | "num" | "den" | "deg" | "limLow" | "limUpp" => children
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
            .join(""),
        "begChr" | "endChr" | "pos" | "chr" => children
            .iter()
            .map(|(_, v)| v.clone())
            .collect::<Vec<_>>()
            .join(""),
        // Run properties: extract color/bold/italic for parent <m:r>
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

fn map_nary(chr: &str) -> &str {
    match chr {
        "\u{222B}" | "\u{222E}" => "\\int",
        "\u{222C}" => "\\iint",
        "\u{222D}" => "\\iiint",
        "\u{2211}" => "\\sum",
        "\u{220F}" => "\\prod",
        "\u{2210}" => "\\coprod",
        "\u{2202}" => "\\partial",
        "\u{2207}" => "\\nabla",
        _ => "\\int",
    }
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
        "\u{20D7}" => format!("\\vec{{{}}}", content),
        _ => format!("\\hat{{{}}}", content),
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
