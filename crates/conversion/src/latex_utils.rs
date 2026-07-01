//! Shared LaTeX parsing utilities for all converters.

/// Parse LaTeX brace pairs: {content1}{content2} or content1}{content2}
pub fn split_brace_pair(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let mut depth = 0i32;
    let mut first_end = None;

    for (i, b) in s.bytes().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    first_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let end = first_end?;
    let first = if s.starts_with('{') {
        &s[1..end]
    } else {
        &s[..end]
    };
    let rest = &s[end + 1..];
    let rest = rest.trim_start();

    let second = if rest.starts_with('{') {
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
        rest.find('}').map(|i| &rest[..i]).unwrap_or(rest)
    };

    Some((first, second))
}

/// Split superscript: a^{b} → (a, b)
pub fn split_superscript(s: &str) -> Option<(&str, &str)> {
    let pos = s.find("^{")?;
    let base = &s[..pos];
    let after = &s[pos + 2..];
    let end = after.find('}')?;
    Some((base, &after[..end]))
}

/// Split subscript: a_{b} → (a, b)
pub fn split_subscript(s: &str) -> Option<(&str, &str)> {
    let pos = s.find("_{")?;
    let base = &s[..pos];
    let after = &s[pos + 2..];
    let end = after.find('}')?;
    Some((base, &after[..end]))
}

/// Extract content from \begin{env}...\end{env}
pub fn extract_env<'a>(latex: &'a str, env: &str) -> Option<&'a str> {
    let begin_tag = format!("\\begin{{{}}}", env);
    let end_tag = format!("\\end{{{}}}", env);
    let start = latex.find(&begin_tag)?;
    let after_begin = &latex[start + begin_tag.len()..];
    let end = after_begin.find(&end_tag)?;
    Some(after_begin[..end].trim())
}

/// Split matrix rows by \\ separator
pub fn split_matrix_rows(content: &str) -> Vec<Vec<&str>> {
    content
        .split('\\')
        .filter(|s| !s.trim().is_empty() && s.trim() != "\\")
        .map(|row| row.split('&').filter(|s| !s.trim().is_empty()).collect())
        .filter(|row: &Vec<&str>| !row.is_empty())
        .collect()
}

/// Convert Typst to approximate LaTeX
pub fn typst_to_latex(typst: &str) -> String {
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

/// Escape XML special characters
pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Map LaTeX symbols to Unicode
pub fn map_symbol_unicode(latex: &str) -> Option<&str> {
    match latex {
        "\\alpha" | "alpha" => Some("α"),
        "\\beta" | "beta" => Some("β"),
        "\\gamma" | "gamma" => Some("γ"),
        "\\delta" | "delta" => Some("δ"),
        "\\theta" | "theta" => Some("θ"),
        "\\lambda" | "lambda" => Some("λ"),
        "\\sigma" | "sigma" => Some("σ"),
        "\\omega" | "omega" => Some("ω"),
        "\\pi" | "pi" => Some("π"),
        "\\infty" | "infinity" => Some("∞"),
        "\\pm" | "plus.minus" => Some("±"),
        "\\times" | "times" => Some("×"),
        "\\div" | "div" => Some("÷"),
        "\\cdot" | "dot" => Some("·"),
        "\\leq" | "lt.eq" => Some("≤"),
        "\\geq" | "gt.eq" => Some("≥"),
        "\\neq" | "neq" => Some("≠"),
        "\\approx" | "approx" => Some("≈"),
        "\\rightarrow" | "rightarrow" => Some("→"),
        "\\leftarrow" | "leftarrow" => Some("←"),
        _ => None,
    }
}

/// Map large operators to Unicode
pub fn map_large_op(latex: &str) -> Option<&str> {
    match latex {
        "\\sum" => Some("∑"),
        "\\prod" => Some("∏"),
        "\\int" => Some("∫"),
        "\\iint" => Some("∬"),
        "\\iiint" => Some("∭"),
        "\\oint" => Some("∮"),
        "\\bigcup" => Some("⋃"),
        "\\bigcap" => Some("⋂"),
        _ => None,
    }
}
