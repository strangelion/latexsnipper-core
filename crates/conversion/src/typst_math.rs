//! Finite, non-executing Typst math projection with explicit operand boundaries.
use latexsnipper_ast::formula_layout::join_latex_fragments;

type ParseResult = Result<String, String>;

fn unsupported() -> String {
    "Unsupported or malformed Typst math projection; retain the original source".into()
}

pub(crate) fn parse_checked(source: &str) -> ParseResult {
    if source.len() > 64 * 1024 {
        return Err("Typst math projection exceeds 64 KiB".into());
    }
    let source = source.trim();
    let source = source
        .strip_prefix('$')
        .and_then(|value| value.strip_suffix('$'))
        .unwrap_or(source);
    expression(source, 0, false)
}

fn expression(source: &str, depth: usize, in_fence: bool) -> ParseResult {
    if depth > 64 {
        return Err("Typst math projection exceeds nesting budget".into());
    }
    let source = source.trim();
    let mut fragments = Vec::new();
    let mut index = 0;
    while index < source.len() {
        let ch = source[index..].chars().next().ok_or_else(unsupported)?;
        if ch.is_whitespace() {
            let tail = source[index..].trim_start();
            index = source.len() - tail.len();
            if !tail.is_empty()
                && fragments.last().is_some_and(|value: &String| {
                    value.bytes().all(|ch| ch.is_ascii_alphanumeric())
                })
                && source[index..atom_end(source, index)?]
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric())
            {
                fragments.push(" ".into());
            }
            continue;
        }
        let end = atom_end(source, index)?;
        let atom = &source[index..end];
        let mut converted = convert_atom(atom, depth + 1, in_fence)?;
        index = end;
        let mut scripts = [false; 2];
        loop {
            let tail = source[index..].trim_start();
            let Some(marker @ ('_' | '^')) = tail.chars().next() else {
                break;
            };
            let slot = usize::from(marker == '^');
            if scripts[slot] {
                return Err(unsupported());
            }
            if !scripts.iter().any(|present| *present) && atom.starts_with('(') {
                converted = format!(
                    "{{{}}}",
                    expression(&atom[1..atom.len() - 1], depth + 1, false)?
                );
            }
            scripts[slot] = true;
            index = source.len() - tail.len() + 1;
            let operand = source[index..].trim_start();
            index = source.len() - operand.len();
            let operand_end = atom_end(source, index)?;
            let operand = &source[index..operand_end];
            if !operand
                .chars()
                .next()
                .is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '(' | '"'))
            {
                return Err(unsupported());
            }
            let value = if operand.starts_with('(') {
                expression(&operand[1..operand.len() - 1], depth + 1, false)?
            } else {
                convert_atom(operand, depth + 1, false)?
            };
            converted.push_str(&format!("{marker}{{{value}}}"));
            index = operand_end;
        }
        fragments.push(converted);
    }
    Ok(join_latex_fragments(fragments))
}

fn atom_end(source: &str, start: usize) -> Result<usize, String> {
    let ch = source[start..].chars().next().ok_or_else(unsupported)?;
    match ch {
        '(' | '[' => group_end(source, start),
        '"' => string_end(source, start),
        '\\' if source[start..].starts_with(r"\,") || source[start..].starts_with(r"\;") => {
            Ok(start + 2)
        }
        '#' | '$' | ')' | '_' | '^' | ']' | '{' | '}' | '\\' => Err(unsupported()),
        ch if ch.is_ascii_alphanumeric() => {
            let mut end = start + ch.len_utf8();
            while end < source.len() {
                let next = source[end..].chars().next().ok_or_else(unsupported)?;
                if !(next.is_ascii_alphanumeric() || next == '.') {
                    break;
                }
                end += next.len_utf8();
            }
            if source[end..].starts_with('(') {
                group_end(source, end)
            } else {
                Ok(end)
            }
        }
        _ => Ok(start + ch.len_utf8()),
    }
}

fn string_end(source: &str, start: usize) -> Result<usize, String> {
    let mut escaped = false;
    for (offset, ch) in source[start + 1..].char_indices() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Ok(start + offset + 2);
        }
    }
    Err(unsupported())
}

fn group_end(source: &str, start: usize) -> Result<usize, String> {
    let mut stack = Vec::new();
    let mut index = start;
    while index < source.len() {
        let ch = source[index..].chars().next().ok_or_else(unsupported)?;
        match ch {
            '"' => {
                index = string_end(source, index)?;
                continue;
            }
            '(' | '[' | '{' => {
                stack.push(ch);
                if stack.len() > 64 {
                    return Err(unsupported());
                }
            }
            ')' | ']' | '}' => {
                let open = stack.pop().ok_or_else(unsupported)?;
                if !matches!((open, ch), ('(', ')') | ('[', ']') | ('{', '}')) {
                    return Err(unsupported());
                }
                if stack.is_empty() {
                    return Ok(index + ch.len_utf8());
                }
            }
            _ => {}
        }
        index += ch.len_utf8();
    }
    Err(unsupported())
}

fn split_args(source: &str, separator: char) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let (mut index, mut start) = (0, 0);
    while index < source.len() {
        let ch = source[index..].chars().next().ok_or_else(unsupported)?;
        if ch == '\\' && (source[index..].starts_with(r"\,") || source[index..].starts_with(r"\;"))
        {
            index += 2;
            continue;
        }
        if matches!(ch, '(' | '[' | '{') {
            index = group_end(source, index)?;
            continue;
        }
        if ch == '"' {
            index = string_end(source, index)?;
            continue;
        }
        if ch == separator {
            parts.push(source[start..index].trim());
            start = index + ch.len_utf8();
        }
        index += ch.len_utf8();
    }
    parts.push(source[start..].trim());
    Ok(parts)
}

fn convert_atom(atom: &str, depth: usize, in_fence: bool) -> ParseResult {
    if depth > 64 {
        return Err(unsupported());
    }
    if matches!(atom, r"\," | r"\;") {
        return Ok(atom[1..].into());
    }
    if atom.starts_with('"') {
        let text: String = serde_json::from_str(atom).map_err(|_| unsupported())?;
        return Ok(format!(
            "\\text{{{}}}",
            crate::latex_utils::escape_text_symbols(&text)
        ));
    }
    if atom.starts_with('(') {
        return Ok(format!(
            "({})",
            expression(&atom[1..atom.len() - 1], depth, in_fence)?
        ));
    }
    if atom.starts_with('[') {
        return Ok(format!(
            "[{}]",
            expression(&atom[1..atom.len() - 1], depth, in_fence)?
        ));
    }
    let Some((name, tail)) = atom.split_once('(') else {
        return Ok(if "%&#".contains(atom) {
            format!("\\{atom}")
        } else {
            crate::typst_parser::symbol_atom(atom)
        });
    };
    let body = tail.strip_suffix(')').ok_or_else(unsupported)?;
    if name == "class" {
        return fixed_delimiter(body);
    }
    if name == "lr" {
        return scalable_fence(body, depth);
    }
    if name == "mid" {
        if !in_fence {
            return Err(unsupported());
        }
        return Ok(format!(
            "\\middle{}",
            delimiter_token(body.trim()).ok_or_else(unsupported)?
        ));
    }
    let args = split_args(body, ',')?;
    for (index, arg) in args.iter().enumerate() {
        if split_args(arg, ':')?.len() > 1
            && !(name == "vec" && index == 0 && *arg == "delim: none")
        {
            return Err(unsupported());
        }
    }
    let convert = |source: &str| expression(source, depth, false);
    match (name, args.as_slice()) {
        (_, _) if name.len() == 1 && name.as_bytes()[0].is_ascii_alphabetic() => {
            Ok(format!("{name}({})", convert(body)?))
        }
        ("limits" | "scripts", [operator @ ("min" | "max")]) => Ok(format!(
            "\\{operator}\\{}",
            if name == "limits" {
                "limits"
            } else {
                "nolimits"
            }
        )),
        ("frac", [a, b]) => Ok(format!("\\frac{{{}}}{{{}}}", convert(a)?, convert(b)?)),
        ("binom", [a, b]) => Ok(format!("\\binom{{{}}}{{{}}}", convert(a)?, convert(b)?)),
        ("overbrace" | "underbrace", [body, label]) => Ok(format!(
            "\\{name}{{{}}}{}{{{}}}",
            convert(body)?,
            if name == "overbrace" { '^' } else { '_' },
            convert(label)?
        )),
        ("overset" | "underset", [label, body]) => Ok(format!(
            "\\{name}{{{}}}{{{}}}",
            convert(label)?,
            convert(body)?
        )),
        ("sqrt", [value]) => Ok(format!("\\sqrt{{{}}}", convert(value)?)),
        ("sqrt" | "root", [index, value]) => Ok(format!(
            "\\sqrt[{}]{{{}}}",
            convert(index)?,
            convert(value)?
        )),
        ("op", [value]) => {
            let text: String = serde_json::from_str(value).map_err(|_| unsupported())?;
            Ok(format!(
                "\\operatorname{{{}}}",
                crate::latex_utils::escape_text_symbols(&text)
            ))
        }
        ("vec", ["delim: none", rows @ ..]) if !rows.is_empty() => {
            let rows = rows
                .iter()
                .map(|row| convert(row))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("\\substack{{{}}}", rows.join("\\\\")))
        }
        ("vec", rows) if rows.len() > 1 => {
            let rows = rows
                .iter()
                .map(|row| convert(row))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!(
                "\\begin{{pmatrix}} {} \\end{{pmatrix}}",
                rows.join(" \\\\ ")
            ))
        }
        ("cases", _) => {
            let rows = if split_args(body, ';')?.len() > 1 {
                split_args(body, ';')?
            } else {
                args
            };
            let rows = rows
                .iter()
                .map(|row| {
                    case_cells(row)?
                        .iter()
                        .map(|cell| convert(cell))
                        .collect::<Result<Vec<_>, _>>()
                        .map(|cells| cells.join(" & "))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!(
                "\\begin{{cases}} {} \\end{{cases}}",
                rows.join(" \\\\ ")
            ))
        }
        ("mat", _) => {
            let rows = split_args(body, ';')?
                .iter()
                .map(|row| {
                    split_args(row, ',')?
                        .iter()
                        .map(|cell| convert(cell))
                        .collect::<Result<Vec<_>, _>>()
                        .map(|cells| cells.join(" & "))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!(
                "\\begin{{matrix}} {} \\end{{matrix}}",
                rows.join(" \\\\ ")
            ))
        }
        (_, [value]) => {
            let command = match name {
                "arrow" | "vec" => "vec",
                "hat" => "hat",
                "dot" => "dot",
                "dot.double" => "ddot",
                "bar" | "overline" => "overline",
                "underline" => "underline",
                "tilde" => "tilde",
                "acute" => "acute",
                "grave" => "grave",
                "check" => "check",
                "breve" => "breve",
                "cancel" => "cancel",
                "overbrace" => "overbrace",
                "underbrace" => "underbrace",
                "overbracket" => "overbracket",
                "underbracket" => "underbracket",
                "phantom" => "phantom",
                "box" => "boxed",
                "sin" => "sin",
                "cos" => "cos",
                "tan" => "tan",
                "log" => "log",
                "ln" => "ln",
                "exp" => "exp",
                "abs" => "abs",
                "norm" => "norm",
                "floor" => "floor",
                "ceil" => "ceil",
                "cal" => "mathcal",
                "frak" => "mathfrak",
                "bb" => "mathbb",
                "bold" => "mathbf",
                "italic" => "mathit",
                "mono" => "mathtt",
                "serif" | "upright" => "mathrm",
                "sans" => "mathsf",
                "display" => "displaystyle",
                "inline" => "textstyle",
                "script" => "scriptstyle",
                "sscript" => "scriptscriptstyle",
                _ => return Err(unsupported()),
            };
            let mut value = convert(value)?;
            if name == "upright"
                && value
                    .bytes()
                    .all(|ch| ch.is_ascii_alphabetic() || ch.is_ascii_whitespace())
            {
                // Canonical export separates TeX letters to avoid a Typst name.
                value.retain(|ch| !ch.is_ascii_whitespace());
            }
            if name == "script" && value.starts_with("\\substack{") {
                return Ok(value);
            }
            if matches!(name, "display" | "inline" | "script" | "sscript") {
                Ok(format!("{{\\{command} {value}}}"))
            } else {
                Ok(format!("\\{command}{{{value}}}"))
            }
        }
        _ => Err(unsupported()),
    }
}

fn case_cells(row: &str) -> Result<Vec<&str>, String> {
    let parts = split_args(row, '&')?;
    if parts.len() == 1 {
        return Ok(parts);
    }
    if parts[0].is_empty() && parts.len() % 2 == 0 {
        let mut cells = vec![parts[1]];
        for pair in parts[2..].as_chunks::<2>().0 {
            if !pair[0].is_empty() {
                return Err(unsupported());
            }
            let tail = pair[1].strip_prefix("quad").ok_or_else(unsupported)?;
            if tail.chars().next().is_some_and(|ch| !ch.is_whitespace()) {
                return Err(unsupported());
            }
            cells.push(tail.trim_start());
        }
        return Ok(cells);
    }
    // Basic alignment points retain columns, including one trailing empty cell.
    // Empty alternating blocks cannot be guessed into a TeX column spec.
    if parts.len() == 2 || parts.iter().all(|part| !part.is_empty()) {
        return Ok(parts);
    }
    Err(unsupported())
}

fn fixed_delimiter(body: &str) -> ParseResult {
    let args = split_args(body, ',')?;
    let [class, sized] = args.as_slice() else {
        return Err(unsupported());
    };
    let suffix = match *class {
        "\"normal\"" => "",
        "\"opening\"" => "l",
        "\"relation\"" => "m",
        "\"closing\"" => "r",
        _ => return Err(unsupported()),
    };
    let body = sized
        .strip_prefix("lr(")
        .and_then(|value| value.strip_suffix(')'))
        .ok_or_else(unsupported)?;
    let parts = split_args(body, ',')?;
    let [size, glyph] = parts.as_slice() else {
        return Err(unsupported());
    };
    let size = size
        .strip_prefix("size:")
        .map(str::trim)
        .ok_or_else(unsupported)?;
    let tier = match size {
        "#1.2em" => "big",
        "#1.8em" => "Big",
        "#2.4em" => "bigg",
        "#3em" => "Bigg",
        _ => return Err(unsupported()),
    };
    let token = if *glyph == "#box()" {
        ".".to_string()
    } else {
        delimiter_token(glyph).ok_or_else(unsupported)?
    };
    Ok(format!("\\{tier}{suffix}{token}"))
}

const DELIMITERS: &[(&str, &str)] = &[
    ("paren.l", "("),
    ("paren.r", ")"),
    ("bracket.l", "["),
    ("bracket.r", "]"),
    ("brace.l", "{"),
    ("brace.r", "}"),
    ("chevron.l", "⟨"),
    ("chevron.r", "⟩"),
    ("floor.l", "⌊"),
    ("floor.r", "⌋"),
    ("ceil.l", "⌈"),
    ("ceil.r", "⌉"),
    ("bar.v.double", "‖"),
    ("backslash", "\\"),
    ("slash", "/"),
    ("arrow.t.double", "⇑"),
    ("arrow.b.double", "⇓"),
    ("arrow.t.b.double", "⇕"),
    ("arrow.t.b", "↕"),
    ("arrow.t", "↑"),
    ("arrow.b", "↓"),
    ("|", "|"),
    ("(", "("),
    (")", ")"),
    ("[", "["),
    ("]", "]"),
];

fn delimiter_token(atom: &str) -> Option<String> {
    DELIMITERS
        .iter()
        .find(|(name, _)| *name == atom)
        .map(|(_, glyph)| crate::latex_utils::delimiter_source_token(glyph))
}

fn scalable_fence(body: &str, depth: usize) -> ParseResult {
    if split_args(body, ',')?.len() != 1 {
        return Err(unsupported());
    }
    let mut body = body.trim();
    let boundary = |ch: char| !(ch.is_ascii_alphanumeric() || ch == '.');
    let left = DELIMITERS.iter().find_map(|(name, _)| {
        body.strip_prefix(name)
            .filter(|tail| name.len() == 1 || tail.chars().next().is_none_or(boundary))
            .map(|tail| (*name, tail))
    });
    let left_token = if let Some((name, tail)) = left {
        body = tail.trim();
        delimiter_token(name).unwrap()
    } else {
        ".".into()
    };
    let right = DELIMITERS.iter().find_map(|(name, _)| {
        body.strip_suffix(name)
            .filter(|head| name.len() == 1 || head.chars().last().is_none_or(boundary))
            .map(|head| (*name, head))
    });
    let right_token = if let Some((name, head)) = right {
        body = head.trim();
        delimiter_token(name).unwrap()
    } else {
        ".".into()
    };
    let content = expression(body, depth, left.is_some() || right.is_some())?;
    if left.is_none() && right.is_none() {
        return Ok(content);
    }
    Ok(join_latex_fragments([
        format!("\\left{left_token}"),
        content,
        format!("\\right{right_token}"),
    ]))
}
