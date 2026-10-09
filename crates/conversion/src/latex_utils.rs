//! Shared LaTeX parsing utilities for all converters.

/// Remove default-catcode comments while retaining escaped control symbols.
pub(crate) fn without_comments(source: &str) -> std::borrow::Cow<'_, str> {
    if !source.contains('%') {
        return std::borrow::Cow::Borrowed(source);
    }
    let mut result = String::new();
    let mut chars = source.chars().peekable();
    let mut control_word = false;
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            result.push(ch);
            if let Some(next) = chars.next() {
                result.push(next);
                control_word = next.is_ascii_alphabetic();
            }
        } else if ch == '%' {
            while chars.peek().is_some_and(|ch| !matches!(ch, '\n' | '\r')) {
                chars.next();
            }
            if chars.next() == Some('\r') && chars.peek() == Some(&'\n') {
                chars.next();
            }
            while chars.peek().is_some_and(|ch| matches!(ch, ' ' | '\t')) {
                chars.next();
            }
            // A comment ends a control word even when it removes the line ending.
            if control_word && chars.peek().is_some_and(char::is_ascii_alphabetic) {
                result.push(' ');
            }
            control_word = false;
        } else {
            result.push(ch);
            control_word &= ch.is_ascii_alphabetic();
        }
    }
    std::borrow::Cow::Owned(result)
}

pub(crate) fn decode_text_symbols(source: &str) -> String {
    let mut result = String::new();
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek().is_some_and(|ch| "%&#_${}".contains(*ch)) {
            result.push(chars.next().expect("peeked control symbol"));
        } else if ch == '\\' {
            let mut decoded = false;
            for (command, symbol) in [
                ("backslash{}", '\\'),
                ("textasciicircum{}", '^'),
                ("textasciitilde{}", '~'),
            ] {
                if chars.clone().take(command.len()).eq(command.chars()) {
                    for _ in 0..command.len() {
                        chars.next();
                    }
                    result.push(symbol);
                    decoded = true;
                    break;
                }
            }
            if !decoded {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// Protect visible XML characters from becoming TeX comments, groups or separators.
pub(crate) fn escape_text_symbols(source: &str) -> String {
    let mut result = String::new();
    for ch in source.chars() {
        if let Some(command) = match ch {
            '\\' => Some("\\backslash{}"),
            '^' => Some("\\textasciicircum{}"),
            '~' => Some("\\textasciitilde{}"),
            _ => None,
        } {
            result.push_str(command);
            continue;
        }
        if "%&#_${}".contains(ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

pub(crate) fn literal_command_symbol(command: &str) -> Option<&'static str> {
    match command {
        "%" => Some("%"),
        "&" => Some("&"),
        "#" => Some("#"),
        "_" => Some("_"),
        "$" => Some("$"),
        "{" => Some("{"),
        "}" => Some("}"),
        "backslash" => Some("\\"),
        "textasciicircum" => Some("^"),
        "textasciitilde" => Some("~"),
        _ => None,
    }
}

/// Split stack rows without splitting grouped operands or nested environments.
pub(crate) fn split_stack_rows(source: &str) -> Vec<&str> {
    split_math_top_level(source, true)
}

fn split_matrix_cells(source: &str) -> Vec<&str> {
    split_math_top_level(source, false)
}

fn split_math_top_level(source: &str, split_rows: bool) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut rows = Vec::new();
    let (mut pos, mut start, mut braces, mut environments) = (0, 0, 0usize, 0usize);
    while pos < bytes.len() {
        match bytes[pos] {
            b'%' => {
                while pos < bytes.len() && !matches!(bytes[pos], b'\r' | b'\n') {
                    pos += 1;
                }
            }
            b'\\' if bytes.get(pos + 1) == Some(&b'\\') => {
                if split_rows && braces == 0 && environments == 0 {
                    rows.push(source[start..pos].trim());
                    start = pos + 2;
                }
                pos += 2;
            }
            b'&' if !split_rows && braces == 0 && environments == 0 => {
                rows.push(source[start..pos].trim());
                start = pos + 1;
                pos += 1;
            }
            b'\\' => {
                pos += 1;
                let command_start = pos;
                while pos < bytes.len() && bytes[pos].is_ascii_alphabetic() {
                    pos += 1;
                }
                match &source[command_start..pos] {
                    "begin" => environments += 1,
                    "end" => environments = environments.saturating_sub(1),
                    "" if pos < bytes.len() => pos += 1,
                    _ => {}
                }
            }
            b'{' => {
                braces += 1;
                pos += 1;
            }
            b'}' => {
                braces = braces.saturating_sub(1);
                pos += 1;
            }
            _ => pos += 1,
        }
    }
    rows.push(source[start..].trim());
    rows
}

/// Parse LaTeX brace pairs: {content1}{content2} or content1}{content2}
/// Correctly handles nested commands like \frac{\frac{a}{b}}{c}.
pub fn split_brace_pair(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut depth = 0i32;
    let mut first_end = None;
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    first_end = Some(i);
                    break;
                }
                i += 1;
            }
            b'\\' => {
                // Skip LaTeX command name (e.g. \frac, \sqrt, \text)
                i += 1;
                while i < len && bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                // If followed by optional [args], skip them
                if i < len && bytes[i] == b'[' {
                    let mut d = 1i32;
                    i += 1;
                    while i < len && d > 0 {
                        match bytes[i] {
                            b'[' => d += 1,
                            b']' => d -= 1,
                            _ => {}
                        }
                        i += 1;
                    }
                }
            }
            _ => {
                i += 1;
            }
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
        let rb = rest.as_bytes();
        let rlen = rb.len();
        let mut j = 0;
        while j < rlen {
            match rb[j] {
                b'{' => {
                    d += 1;
                    j += 1;
                }
                b'}' => {
                    d -= 1;
                    if d == 0 {
                        close = Some(j);
                        break;
                    }
                    j += 1;
                }
                b'\\' => {
                    j += 1;
                    while j < rlen && rb[j].is_ascii_alphabetic() {
                        j += 1;
                    }
                }
                _ => {
                    j += 1;
                }
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
    split_script(s, '^')
}

/// Split subscript: a_{b} → (a, b)
pub fn split_subscript(s: &str) -> Option<(&str, &str)> {
    split_script(s, '_')
}

fn split_script(s: &str, marker: char) -> Option<(&str, &str)> {
    let pos = s.find(marker)?;
    let base = &s[..pos];
    let after_pos = pos + marker.len_utf8();
    let rest = &s[after_pos..];
    if rest.is_empty() {
        return None;
    }

    if let Some(after_open) = rest.strip_prefix('{') {
        let mut depth = 1i32;
        for (idx, ch) in after_open.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((base, &after_open[..idx]));
                    }
                }
                _ => {}
            }
        }
        return None;
    }

    if let Some(after_command) = rest.strip_prefix('\\') {
        let end = after_command
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_alphabetic())
            .last()
            .map(|(idx, ch)| idx + ch.len_utf8())
            .unwrap_or_else(|| {
                after_command
                    .chars()
                    .next()
                    .map(char::len_utf8)
                    .unwrap_or(0)
            });
        if end == 0 {
            None
        } else {
            Some((base, &rest[..1 + end]))
        }
    } else {
        let end = rest.chars().next()?.len_utf8();
        Some((base, &rest[..end]))
    }
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
    if content.trim().is_empty() {
        return Vec::new();
    }
    let mut rows = split_stack_rows(content);
    // A final row terminator is not an additional blank row; interior blank rows are retained.
    if rows.last().is_some_and(|row| row.is_empty()) {
        rows.pop();
    }
    rows.into_iter().map(split_matrix_cells).collect()
}

/// Convert Typst to approximate LaTeX
pub fn typst_to_latex(typst: &str) -> String {
    crate::typst_parser::parse_typst_to_latex(typst)
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
        "\\coprod" => Some("∐"),
        "\\int" => Some("∫"),
        "\\iint" => Some("∬"),
        "\\iiint" => Some("∭"),
        "\\oint" => Some("∮"),
        "\\bigcup" => Some("⋃"),
        "\\bigcap" => Some("⋂"),
        _ => None,
    }
}
