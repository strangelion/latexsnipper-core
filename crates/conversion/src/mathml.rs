use latexsnipper_ast::{Block, Document, Formula, FormulaSource, Inline};
use latexsnipper_foundation::Result;

use crate::converter::Converter;
use crate::latex_utils::*;

pub struct MathmlConverter;
impl Converter for MathmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Standard)
    }
    fn name(&self) -> &str {
        "mathml"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn mime_type(&self) -> &str {
        "application/mathml+xml"
    }
}

pub struct MathmlMmlConverter;
impl Converter for MathmlMmlConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Mml)
    }
    fn name(&self) -> &str {
        "mathml_mml"
    }
    fn extension(&self) -> &str {
        "mml"
    }
    fn mime_type(&self) -> &str {
        "application/mathml+xml"
    }
}

pub struct MathmlMConverter;
impl Converter for MathmlMConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::M)
    }
    fn name(&self) -> &str {
        "mathml_m"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn mime_type(&self) -> &str {
        "application/mathml+xml"
    }
}

pub struct MathmlAttrConverter;
impl Converter for MathmlAttrConverter {
    fn convert(&self, doc: &Document) -> Result<String> {
        convert_mathml(doc, MathmlMode::Attr)
    }
    fn name(&self) -> &str {
        "mathml_attr"
    }
    fn extension(&self) -> &str {
        "xml"
    }
    fn mime_type(&self) -> &str {
        "application/mathml+xml"
    }
}

enum MathmlMode {
    Standard,
    Mml,
    M,
    Attr,
}

fn convert_mathml(doc: &Document, mode: MathmlMode) -> Result<String> {
    let mut parts = Vec::new();
    for page in &doc.pages {
        for block in &page.blocks {
            match block {
                Block::Formula(f) => parts.push(convert_formula_to_mathml(&f.formula, &mode)),
                Block::Paragraph(p) => {
                    for inline in &p.inlines {
                        match inline {
                            Inline::Text(t) => {
                                parts.push(format!("<mtext>{}</mtext>", xml_escape(&t.text)))
                            }
                            Inline::Formula(f) => parts.push(convert_formula_to_mathml(f, &mode)),
                            Inline::Image(_) => {}
                            Inline::Footnote { content } => {
                                let inner = convert_inline_to_mathml(content, &mode);
                                parts.push(inner);
                            }
                            Inline::Label { .. } => {}
                            Inline::Reference { key, .. } => {
                                parts.push(format!("<mtext>({})</mtext>", key));
                            }
                            Inline::Citation { key, .. } => {
                                parts.push(format!("<mtext>[{}]</mtext>", key));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let content = parts.join("\n");
    match &mode {
        MathmlMode::Standard => Ok(format!(
            "<math xmlns=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</math>",
            content
        )),
        MathmlMode::Mml => Ok(format!(
            "<mml:math xmlns:mml=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</mml:math>",
            content
        )),
        MathmlMode::M => Ok(format!(
            "<m:math xmlns:m=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</m:math>",
            content
        )),
        MathmlMode::Attr => Ok(format!(
            "<math mathmode=\"inline\" xmlns=\"http://www.w3.org/1998/Math/MathML\">\n{}\n</math>",
            content
        )),
    }
}

fn convert_formula_to_mathml(f: &Formula, _mode: &MathmlMode) -> String {
    let content = match &f.source {
        FormulaSource::Latex(s) => latex_to_mathml(s),
        FormulaSource::MathML(s) => s.clone(),
        FormulaSource::Omml(s) => format!("<mrow><mi>{}</mi></mrow>", xml_escape(s)),
        FormulaSource::Typst(s) => latex_to_mathml(&typst_to_latex(s)),
    };
    if f.display_mode {
        format!("<displaymath>\n{}\n</displaymath>", content)
    } else {
        format!("<inlinemath>\n{}\n</inlinemath>", content)
    }
}

fn convert_inline_to_mathml(inline: &Inline, mode: &MathmlMode) -> String {
    match inline {
        Inline::Text(t) => format!("<mtext>{}</mtext>", xml_escape(&t.text)),
        Inline::Formula(f) => convert_formula_to_mathml(f, mode),
        _ => String::new(),
    }
}

fn latex_to_mathml(latex: &str) -> String {
    let latex = latex.trim();

    if let Some(rendered) = render_styled_sequence(latex) {
        return rendered;
    }

    // \textcolor{color}{content} → <mstyle mathcolor="color"><mrow>content</mrow></mstyle>
    if let Some(content) = latex.strip_prefix("\\textcolor{") {
        if let Some(close) = content.find('}') {
            let color = &content[..close];
            let rest = &content[close + 1..];
            let inner = rest
                .strip_prefix('{')
                .unwrap_or(rest)
                .strip_suffix('}')
                .unwrap_or(rest);
            let hex = mathml_color_name(color.trim());
            return format!(
                "<mstyle mathcolor=\"{}\"><mrow>{}</mrow></mstyle>",
                hex,
                latex_to_mathml(inner)
            );
        }
    }
    if let Some(content) = latex.strip_prefix("\\color{") {
        if let Some(close) = content.find('}') {
            let color = &content[..close];
            let hex = mathml_color_name(color.trim());
            let rest = &content[close + 1..];
            if rest.is_empty() {
                return format!("<mstyle mathcolor=\"{}\"/>", hex);
            }
            return format!(
                "<mstyle mathcolor=\"{}\"><mrow>{}</mrow></mstyle>",
                hex,
                latex_to_mathml(rest)
            );
        }
    }

    // \boldsymbol{...} → <mstyle fontweight="bold">
    if let Some(content) = latex.strip_prefix("\\boldsymbol{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<mstyle fontweight=\"bold\"><mrow>{}</mrow></mstyle>",
            latex_to_mathml(inner)
        );
    }
    // \mathbf{...} → <mstyle fontweight="bold">
    if let Some(content) = latex.strip_prefix("\\mathbf{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<mstyle fontweight=\"bold\"><mrow>{}</mrow></mstyle>",
            latex_to_mathml(inner)
        );
    }
    // \mathrm{...} → <mstyle fontfamily="serif">
    if let Some(content) = latex.strip_prefix("\\mathrm{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<mstyle fontfamily=\"serif\"><mrow>{}</mrow></mstyle>",
            latex_to_mathml(inner)
        );
    }
    if let Some(content) = latex.strip_prefix("\\text{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!("<mtext>{}</mtext>", xml_escape(inner));
    }
    // \mathbb{...} → <mi mathvariant="double-struck">
    if let Some(content) = latex.strip_prefix("\\mathbb{") {
        let inner = content.strip_suffix('}').unwrap_or(content);
        return format!(
            "<mi mathvariant=\"double-struck\">{}</mi>",
            latex_to_mathml(inner)
        );
    }
    for (command, variant) in [
        ("\\mathcal{", "script"),
        ("\\mathfrak{", "fraktur"),
        ("\\mathsf{", "sans-serif"),
        ("\\mathtt{", "monospace"),
    ] {
        if let Some(content) = latex.strip_prefix(command) {
            let inner = content.strip_suffix('}').unwrap_or(content);
            return format!(
                "<mstyle mathvariant=\"{}\"><mrow>{}</mrow></mstyle>",
                variant,
                latex_to_mathml(inner)
            );
        }
    }

    if let Some(inner) = latex.strip_prefix("\\frac") {
        if let Some((num, den)) = split_brace_pair(inner) {
            return format!(
                "<mfrac>\n  <mrow>{}</mrow>\n  <mrow>{}</mrow>\n</mfrac>",
                latex_to_mathml(num),
                latex_to_mathml(den)
            );
        }
    }

    if let Some(inner) = latex.strip_prefix("\\sqrt{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!("<msqrt><mrow>{}</mrow></msqrt>", latex_to_mathml(content));
    }

    if let Some(inner) = latex.strip_prefix("\\sqrt[") {
        if let Some((degree, rest)) = inner.split_once(']') {
            let content = rest
                .strip_prefix('{')
                .unwrap_or(rest)
                .strip_suffix('}')
                .unwrap_or(rest);
            return format!(
                "<mroot><mrow>{}</mrow><mrow>{}</mrow></mroot>",
                latex_to_mathml(content),
                latex_to_mathml(degree)
            );
        }
    }

    if let Some(rendered) = render_accent_mathml(latex) {
        return rendered;
    }

    if let Some(rendered) = render_left_right_mathml(latex) {
        return rendered;
    }

    // Matrix environments
    if let Some(inner) = extract_env(latex, "matrix") {
        return matrix_to_mathml(inner, None);
    }
    if let Some(inner) = extract_env(latex, "pmatrix") {
        return matrix_to_mathml(inner, Some(("(", ")")));
    }
    if let Some(inner) = extract_env(latex, "bmatrix") {
        return matrix_to_mathml(inner, Some(("[", "]")));
    }
    if let Some(inner) = extract_env(latex, "vmatrix") {
        return matrix_to_mathml(inner, Some(("|", "|")));
    }
    if let Some(inner) = extract_env(latex, "cases") {
        return cases_to_mathml(inner);
    }
    if let Some(inner) = extract_env(latex, "aligned") {
        return aligned_to_mathml(inner);
    }
    if let Some(inner) = extract_env(latex, "array") {
        return matrix_to_mathml(inner, None);
    }

    // \phantom
    if let Some(inner) = latex.strip_prefix("\\phantom{") {
        let content = inner.strip_suffix('}').unwrap_or(inner);
        return format!(
            "<mpadded width=\"0\" height=\"0\" depth=\"0\"><mrow>{}</mrow></mpadded>",
            latex_to_mathml(content)
        );
    }
    if let Some(rendered) = render_nary_limits(latex) {
        return rendered;
    }

    if let Some(rendered) = render_operator_limits(latex) {
        return rendered;
    }

    if let Some(parts) = split_math_sequence(latex) {
        let rendered = parts
            .iter()
            .map(|part| {
                if is_math_operator(part) {
                    map_symbol_mathml(part)
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("<mo>{}</mo>", xml_escape(part)))
                } else {
                    latex_to_mathml(part)
                }
            })
            .collect::<Vec<_>>()
            .join("");
        return format!("<mrow>{}</mrow>", rendered);
    }

    if let Some((base, sup)) = split_superscript(latex) {
        return format!(
            "<msup>\n  <mrow>{}</mrow>\n  <mrow>{}</mrow>\n</msup>",
            latex_to_mathml(base),
            latex_to_mathml(sup)
        );
    }

    if let Some((base, sub)) = split_subscript(latex) {
        return format!(
            "<msub>\n  <mrow>{}</mrow>\n  <mrow>{}</mrow>\n</msub>",
            latex_to_mathml(base),
            latex_to_mathml(sub)
        );
    }

    if let Some(sym) = map_symbol_mathml(latex) {
        return sym.to_string();
    }

    if latex.len() == 1 && latex.chars().next().unwrap().is_alphabetic() {
        format!("<mi>{}</mi>", latex)
    } else if latex.parse::<f64>().is_ok() {
        format!("<mn>{}</mn>", latex)
    } else {
        format!("<mi>{}</mi>", xml_escape(latex))
    }
}

fn render_operator_limits(latex: &str) -> Option<String> {
    let (command, mut pos) = [
        "\\limsup", "\\liminf", "\\lim", "\\max", "\\min", "\\sup", "\\inf", "\\log",
    ]
    .iter()
    .find_map(|command| {
        latex
            .strip_prefix(command)
            .map(|_| (*command, command.len()))
    })?;

    let name = command.trim_start_matches('\\');
    let mut lower = None;
    let mut upper = None;

    loop {
        pos = skip_ascii_whitespace(latex, pos);
        let marker = latex[pos..].chars().next();
        match marker {
            Some('_') => {
                let (value, next_pos) = read_script_argument(latex, pos + 1)?;
                lower = Some(value);
                pos = next_pos;
            }
            Some('^') => {
                let (value, next_pos) = read_script_argument(latex, pos + 1)?;
                upper = Some(value);
                pos = next_pos;
            }
            _ => break,
        }
    }

    if lower.is_none() && upper.is_none() {
        return None;
    }

    let core = match (lower, upper) {
        (Some(lower), Some(upper)) => format!(
            "<munderover><mi>{}</mi><mrow>{}</mrow><mrow>{}</mrow></munderover>",
            name,
            latex_to_mathml(&lower),
            latex_to_mathml(&upper)
        ),
        (Some(lower), None) => format!(
            "<munder><mi>{}</mi><mrow>{}</mrow></munder>",
            name,
            latex_to_mathml(&lower)
        ),
        (None, Some(upper)) => format!(
            "<mover><mi>{}</mi><mrow>{}</mrow></mover>",
            name,
            latex_to_mathml(&upper)
        ),
        (None, None) => unreachable!(),
    };

    let body = latex[pos..].trim();
    if body.is_empty() {
        Some(core)
    } else {
        Some(format!(
            "<mrow>{}<mrow>{}</mrow></mrow>",
            core,
            latex_to_mathml(body)
        ))
    }
}

fn render_nary_limits(latex: &str) -> Option<String> {
    let (command, mut pos) = [
        "\\sum", "\\prod", "\\coprod", "\\int", "\\iint", "\\iiint", "\\oint",
    ]
    .iter()
    .find_map(|command| {
        latex
            .strip_prefix(command)
            .map(|_| (*command, command.len()))
    })?;

    let op = map_symbol_mathml(command)?;
    let mut lower = None;
    let mut upper = None;

    loop {
        pos = skip_ascii_whitespace(latex, pos);
        let marker = latex[pos..].chars().next();
        match marker {
            Some('_') => {
                let (value, next_pos) = read_script_argument(latex, pos + 1)?;
                lower = Some(value);
                pos = next_pos;
            }
            Some('^') => {
                let (value, next_pos) = read_script_argument(latex, pos + 1)?;
                upper = Some(value);
                pos = next_pos;
            }
            _ => break,
        }
    }

    if lower.is_none() && upper.is_none() {
        return None;
    }

    let core = match (lower, upper) {
        (Some(lower), Some(upper)) => format!(
            "<munderover>{}<mrow>{}</mrow><mrow>{}</mrow></munderover>",
            op,
            latex_to_mathml(&lower),
            latex_to_mathml(&upper)
        ),
        (Some(lower), None) => format!(
            "<munder>{}<mrow>{}</mrow></munder>",
            op,
            latex_to_mathml(&lower)
        ),
        (None, Some(upper)) => format!(
            "<mover>{}<mrow>{}</mrow></mover>",
            op,
            latex_to_mathml(&upper)
        ),
        (None, None) => unreachable!(),
    };

    let body = latex[pos..].trim();
    if body.is_empty() {
        Some(core)
    } else {
        Some(format!(
            "<mrow>{}<mrow>{}</mrow></mrow>",
            core,
            latex_to_mathml(body)
        ))
    }
}

fn skip_ascii_whitespace(text: &str, mut pos: usize) -> usize {
    while pos < text.len() && text.as_bytes()[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

fn read_script_argument(text: &str, pos: usize) -> Option<(String, usize)> {
    let pos = skip_ascii_whitespace(text, pos);
    let rest = &text[pos..];
    if rest.starts_with('{') {
        let chars: Vec<char> = text.chars().collect();
        let char_start = text[..pos].chars().count();
        let mut depth = 0i32;
        for char_pos in char_start..chars.len() {
            match chars[char_pos] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let byte_end: usize =
                            chars[..char_pos].iter().map(|ch| ch.len_utf8()).sum();
                        let byte_after: usize =
                            chars[..=char_pos].iter().map(|ch| ch.len_utf8()).sum();
                        return Some((text[pos + 1..byte_end].to_string(), byte_after));
                    }
                }
                _ => {}
            }
        }
        return None;
    }

    if rest.starts_with('\\') {
        let mut end = pos + 1;
        while end < text.len() && text.as_bytes()[end].is_ascii_alphabetic() {
            end += 1;
        }
        if end == pos + 1 && end < text.len() {
            end += text[end..].chars().next()?.len_utf8();
        }
        return Some((text[pos..end].to_string(), end));
    }

    let end = pos + rest.chars().next()?.len_utf8();
    Some((text[pos..end].to_string(), end))
}

fn is_math_operator(text: &str) -> bool {
    matches!(
        text,
        "+" | "-"
            | "="
            | "<"
            | ">"
            | "/"
            | "*"
            | "|"
            | "("
            | ")"
            | "["
            | "]"
            | ","
            | "\\,"
            | "\\:"
            | "\\;"
            | "\\!"
            | "\\quad"
            | "\\qquad"
    ) || is_sequence_command_operator(text)
}

fn is_sequence_command_operator(text: &str) -> bool {
    matches!(
        text,
        "\\implies"
            | "\\Rightarrow"
            | "\\Leftarrow"
            | "\\Leftrightarrow"
            | "\\rightarrow"
            | "\\leftarrow"
            | "\\leftrightarrow"
            | "\\to"
            | "\\mapsto"
            | "\\leq"
            | "\\le"
            | "\\geq"
            | "\\ge"
            | "\\neq"
            | "\\ne"
            | "\\approx"
            | "\\equiv"
            | "\\sim"
    )
}

fn render_accent_mathml(latex: &str) -> Option<String> {
    let (command, accent) = [
        ("\\hat", "^"),
        ("\\widehat", "^"),
        ("\\bar", "\u{00AF}"),
        ("\\overline", "\u{00AF}"),
        ("\\vec", "\u{2192}"),
        ("\\overrightarrow", "\u{2192}"),
        ("\\overleftarrow", "\u{2190}"),
        ("\\dot", "."),
        ("\\ddot", ".."),
        ("\\tilde", "~"),
        ("\\widetilde", "~"),
        ("\\check", "\u{02C7}"),
    ]
    .iter()
    .find_map(|(command, accent)| latex.strip_prefix(command).map(|_| (*command, *accent)))?;
    let (inner, after_inner) = read_script_argument(latex, command.len())?;
    if skip_ascii_whitespace(latex, after_inner) != latex.len() {
        return None;
    }
    Some(format!(
        "<mover><mrow>{}</mrow><mo>{}</mo></mover>",
        latex_to_mathml(&inner),
        xml_escape(accent)
    ))
}

fn render_left_right_mathml(latex: &str) -> Option<String> {
    let rest = latex.strip_prefix("\\left")?;
    let mut chars = rest.chars();
    let left = chars.next()?;
    let content_start = "\\left".len() + left.len_utf8();
    let right_pos = latex[content_start..].rfind("\\right")? + content_start;
    let right_rest = &latex[right_pos + "\\right".len()..];
    let right = right_rest.chars().next()?;
    let mut pos = skip_ascii_whitespace(latex, right_pos + "\\right".len() + right.len_utf8());
    let content = &latex[content_start..right_pos];
    let open = if left == '.' {
        ""
    } else {
        &latex["\\left".len()..content_start]
    };
    let close_start = right_pos + "\\right".len();
    let close_end = close_start + right.len_utf8();
    let close = if right == '.' {
        ""
    } else {
        &latex[close_start..close_end]
    };
    let base = format!(
        "<mfenced open=\"{}\" close=\"{}\"><mrow>{}</mrow></mfenced>",
        xml_escape(open),
        xml_escape(close),
        latex_to_mathml(content)
    );

    let mut sub = None;
    let mut sup = None;
    while pos < latex.len() {
        match latex[pos..].chars().next()? {
            '_' => {
                let (value, next_pos) = read_script_argument(latex, pos + 1)?;
                sub = Some(value);
                pos = skip_ascii_whitespace(latex, next_pos);
            }
            '^' => {
                let (value, next_pos) = read_script_argument(latex, pos + 1)?;
                sup = Some(value);
                pos = skip_ascii_whitespace(latex, next_pos);
            }
            _ => return None,
        }
    }

    Some(match (sub, sup) {
        (Some(sub), Some(sup)) => format!(
            "<msubsup><mrow>{}</mrow><mrow>{}</mrow><mrow>{}</mrow></msubsup>",
            base,
            latex_to_mathml(&sub),
            latex_to_mathml(&sup)
        ),
        (Some(sub), None) => format!(
            "<msub><mrow>{}</mrow><mrow>{}</mrow></msub>",
            base,
            latex_to_mathml(&sub)
        ),
        (None, Some(sup)) => format!(
            "<msup><mrow>{}</mrow><mrow>{}</mrow></msup>",
            base,
            latex_to_mathml(&sup)
        ),
        (None, None) => base,
    })
}

fn split_math_sequence(latex: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let chars: Vec<char> = latex.chars().collect();
    let mut pos = 0usize;

    while pos < chars.len() {
        let ch = chars[pos];
        if ch == '\\' {
            let command_start = pos;
            current.push(ch);
            pos += 1;
            let name_start = pos;
            while pos < chars.len() && chars[pos].is_ascii_alphabetic() {
                current.push(chars[pos]);
                pos += 1;
            }
            if pos == name_start && pos < chars.len() {
                current.push(chars[pos]);
                pos += 1;
            }
            let command: String = chars[command_start..pos].iter().collect();
            if brace_depth == 0
                && bracket_depth == 0
                && (is_sequence_command_operator(&command)
                    || matches!(
                        command.as_str(),
                        "\\," | "\\:" | "\\;" | "\\!" | "\\quad" | "\\qquad"
                    ))
            {
                let before_command_len = current.len() - command.len();
                let before = current[..before_command_len].trim();
                if !before.is_empty() {
                    parts.push(before.to_string());
                }
                parts.push(command);
                current.clear();
            }
            continue;
        }

        match ch {
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' => {
                brace_depth -= 1;
                current.push(ch);
            }
            '[' => {
                bracket_depth += 1;
                current.push(ch);
            }
            ']' => {
                bracket_depth -= 1;
                current.push(ch);
            }
            '+' | '-' | '=' | '<' | '>' | '/' | '*' | '|' | '(' | ')' | ','
                if brace_depth == 0 && bracket_depth == 0 =>
            {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
                parts.push(ch.to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
        pos += 1;
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
    }

    if parts.iter().any(|part| is_math_operator(part)) {
        Some(parts)
    } else {
        None
    }
}

fn render_styled_sequence(latex: &str) -> Option<String> {
    let mut output = String::new();
    let mut plain = String::new();
    let chars: Vec<char> = latex.chars().collect();
    let mut pos = 0usize;
    let mut found_style = false;

    while pos < chars.len() {
        if chars[pos] != '\\' {
            plain.push(chars[pos]);
            pos += 1;
            continue;
        }

        let cmd_start = pos + 1;
        let mut cmd_end = cmd_start;
        while cmd_end < chars.len() && chars[cmd_end].is_ascii_alphabetic() {
            cmd_end += 1;
        }
        let command: String = chars[cmd_start..cmd_end].iter().collect();

        match command.as_str() {
            "textcolor" => {
                let Some((color, after_color)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                let Some((inner, after_inner)) = read_braced_group(&chars, after_color) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&format!(
                    "<mstyle mathcolor=\"{}\"><mrow>{}</mrow></mstyle>",
                    mathml_color_name(color.trim()),
                    latex_to_mathml(&inner)
                ));
                pos = after_inner;
                found_style = true;
            }
            "mathbf" | "boldsymbol" => {
                let Some((inner, after_inner)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&format!(
                    "<mstyle fontweight=\"bold\"><mrow>{}</mrow></mstyle>",
                    latex_to_mathml(&inner)
                ));
                pos = after_inner;
                found_style = true;
            }
            "mathit" => {
                let Some((inner, after_inner)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&format!(
                    "<mstyle fontstyle=\"italic\"><mrow>{}</mrow></mstyle>",
                    latex_to_mathml(&inner)
                ));
                pos = after_inner;
                found_style = true;
            }
            "mathrm" => {
                let Some((inner, after_inner)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&format!(
                    "<mstyle fontfamily=\"serif\"><mrow>{}</mrow></mstyle>",
                    latex_to_mathml(&inner)
                ));
                pos = after_inner;
                found_style = true;
            }
            "mathbb" => {
                let Some((inner, after_inner)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&format!(
                    "<mstyle mathvariant=\"double-struck\"><mrow>{}</mrow></mstyle>",
                    latex_to_mathml(&inner)
                ));
                pos = after_inner;
                found_style = true;
            }
            "mathcal" | "mathfrak" | "mathsf" | "mathtt" => {
                let Some((inner, after_inner)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                let variant = match command.as_str() {
                    "mathcal" => "script",
                    "mathfrak" => "fraktur",
                    "mathsf" => "sans-serif",
                    "mathtt" => "monospace",
                    _ => "normal",
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&format!(
                    "<mstyle mathvariant=\"{}\"><mrow>{}</mrow></mstyle>",
                    variant,
                    latex_to_mathml(&inner)
                ));
                pos = after_inner;
                found_style = true;
            }
            "text" | "textrm" | "textsf" | "texttt" => {
                let Some((inner, after_inner)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&format!("<mtext>{}</mtext>", xml_escape(&inner)));
                pos = after_inner;
                found_style = true;
            }
            "displaystyle" | "textstyle" | "scriptstyle" | "scriptscriptstyle" => {
                let (inner, after_inner) = read_braced_group(&chars, cmd_end)
                    .unwrap_or_else(|| (chars[cmd_end..].iter().collect(), chars.len()));
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&math_style_mathml(
                    command.as_str(),
                    &latex_to_mathml(&inner),
                ));
                pos = after_inner;
                found_style = true;
            }
            "phantom" | "vphantom" | "hphantom" | "boxed" | "tag" | "abs" | "norm" | "floor"
            | "ceil" => {
                let Some((inner, after_inner)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                let inner = latex_to_mathml(&inner);
                let rendered = match command.as_str() {
                    "phantom" => format!(
                        "<mpadded width=\"0\" height=\"0\" depth=\"0\"><mrow>{inner}</mrow></mpadded>"
                    ),
                    "vphantom" => {
                        format!("<mpadded width=\"0\"><mrow>{inner}</mrow></mpadded>")
                    }
                    "hphantom" => format!(
                        "<mpadded height=\"0\" depth=\"0\"><mrow>{inner}</mrow></mpadded>"
                    ),
                    "boxed" => {
                        format!("<menclosed notation=\"box\"><mrow>{inner}</mrow></menclosed>")
                    }
                    "tag" => format!(
                        "<mo lspace=\"0em\" rspace=\"0em\">(</mo><mrow>{inner}</mrow><mo lspace=\"0em\" rspace=\"0em\">)</mo>"
                    ),
                    "abs" => delimiter_mathml("|", "|", &inner),
                    "norm" => delimiter_mathml("‖", "‖", &inner),
                    "floor" => delimiter_mathml("⌊", "⌋", &inner),
                    "ceil" => delimiter_mathml("⌈", "⌉", &inner),
                    _ => unreachable!("command match is exhaustive"),
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&rendered);
                pos = after_inner;
                found_style = true;
            }
            "tiny" | "scriptsize" | "footnotesize" | "small" | "normalsize" | "large" | "Large"
            | "LARGE" | "huge" | "Huge" => {
                let Some((inner, after_inner)) = read_braced_group(&chars, cmd_end) else {
                    plain.push(chars[pos]);
                    pos += 1;
                    continue;
                };
                flush_mathml_plain(&mut output, &mut plain);
                output.push_str(&format!(
                    "<mstyle mathsize=\"{}\"><mrow>{}</mrow></mstyle>",
                    latex_size_to_mathml(command.as_str()),
                    latex_to_mathml(&inner)
                ));
                pos = after_inner;
                found_style = true;
            }
            _ => {
                plain.push(chars[pos]);
                pos += 1;
            }
        }
    }

    if !found_style {
        return None;
    }
    flush_mathml_plain(&mut output, &mut plain);
    Some(output)
}

fn read_braced_group(chars: &[char], mut pos: usize) -> Option<(String, usize)> {
    while pos < chars.len() && chars[pos].is_whitespace() {
        pos += 1;
    }
    if pos >= chars.len() || chars[pos] != '{' {
        return None;
    }

    let mut depth = 1i32;
    let start = pos + 1;
    pos += 1;
    while pos < chars.len() {
        match chars[pos] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let value = chars[start..pos].iter().collect();
                    return Some((value, pos + 1));
                }
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

fn flush_mathml_plain(output: &mut String, plain: &mut String) {
    let text = plain.trim();
    if !text.is_empty() {
        output.push_str(&latex_to_mathml(text));
    }
    plain.clear();
}

fn delimiter_mathml(open: &str, close: &str, inner: &str) -> String {
    format!(
        "<mo lspace=\"0em\" rspace=\"0em\">{open}</mo><mrow>{inner}</mrow><mo lspace=\"0em\" rspace=\"0em\">{close}</mo>"
    )
}

fn math_style_mathml(style: &str, inner: &str) -> String {
    let (displaystyle, scriptlevel) = match style {
        "displaystyle" => ("true", "0"),
        "textstyle" => ("false", "0"),
        "scriptstyle" => ("false", "1"),
        "scriptscriptstyle" => ("false", "2"),
        _ => unreachable!("caller only passes supported math styles"),
    };
    format!(
        "<mstyle displaystyle=\"{displaystyle}\" scriptlevel=\"{scriptlevel}\"><mrow>{inner}</mrow></mstyle>"
    )
}

fn latex_size_to_mathml(name: &str) -> &str {
    match name {
        "tiny" => "50%",
        "scriptsize" => "70%",
        "footnotesize" => "80%",
        "small" => "90%",
        "normalsize" => "100%",
        "large" => "120%",
        "Large" => "144%",
        "LARGE" => "173%",
        "huge" => "207%",
        "Huge" => "249%",
        _ => "100%",
    }
}

fn mathml_color_name(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "red" => "red".to_string(),
        "green" => "green".to_string(),
        "blue" => "blue".to_string(),
        "yellow" => "yellow".to_string(),
        "cyan" => "cyan".to_string(),
        "magenta" | "fuchsia" => "magenta".to_string(),
        "black" => "black".to_string(),
        "white" => "white".to_string(),
        "gray" | "grey" => "gray".to_string(),
        "orange" => "orange".to_string(),
        "purple" => "purple".to_string(),
        s if s.starts_with('#') && s.len() == 7 => s.to_string(),
        s if s.len() == 6 && s.chars().all(|c| c.is_ascii_hexdigit()) => format!("#{}", s),
        _ => "black".to_string(),
    }
}

fn map_symbol_mathml(latex: &str) -> Option<&str> {
    match latex {
        "\\alpha" | "alpha" => Some("<mi>\u{03B1}</mi>"),
        "\\beta" | "beta" => Some("<mi>\u{03B2}</mi>"),
        "\\gamma" | "gamma" => Some("<mi>\u{03B3}</mi>"),
        "\\delta" | "delta" => Some("<mi>\u{03B4}</mi>"),
        "\\theta" | "theta" => Some("<mi>\u{03B8}</mi>"),
        "\\lambda" | "lambda" => Some("<mi>\u{03BB}</mi>"),
        "\\sigma" | "sigma" => Some("<mi>\u{03C3}</mi>"),
        "\\omega" | "omega" => Some("<mi>\u{03C9}</mi>"),
        "\\pi" | "pi" => Some("<mi>\u{03C0}</mi>"),
        "\\infty" | "infinity" => Some("<mi>\u{221E}</mi>"),
        "\\pm" | "plus.minus" => Some("<mo>\u{00B1}</mo>"),
        "\\times" | "times" => Some("<mo>\u{00D7}</mo>"),
        "\\div" | "div" => Some("<mo>\u{00F7}</mo>"),
        "\\cdot" | "dot" => Some("<mo>\u{22C5}</mo>"),
        "\\leq" | "lt.eq" => Some("<mo>\u{2264}</mo>"),
        "\\geq" | "gt.eq" => Some("<mo>\u{2265}</mo>"),
        "\\neq" | "neq" => Some("<mo>\u{2260}</mo>"),
        "\\approx" | "approx" => Some("<mo>\u{2248}</mo>"),
        "\\rightarrow" | "\\to" | "rightarrow" => Some("<mo>\u{2192}</mo>"),
        "\\leftarrow" | "leftarrow" => Some("<mo>\u{2190}</mo>"),
        "\\leftrightarrow" | "leftrightarrow" => Some("<mo>\u{2194}</mo>"),
        "\\mapsto" | "mapsto" => Some("<mo>\u{21A6}</mo>"),
        "\\Rightarrow" => Some("<mo>\u{21D2}</mo>"),
        "\\Leftarrow" => Some("<mo>\u{21D0}</mo>"),
        "\\Leftrightarrow" => Some("<mo>\u{21D4}</mo>"),
        "\\implies" | "implies" => Some("<mo>\u{21D2}</mo>"),
        "\\sum" => Some("<mo>\u{2211}</mo>"),
        "\\prod" => Some("<mo>\u{220F}</mo>"),
        "\\coprod" => Some("<mo>\u{2210}</mo>"),
        "\\int" => Some("<mo>\u{222B}</mo>"),
        "\\iint" => Some("<mo>\u{222C}</mo>"),
        "\\iiint" => Some("<mo>\u{222D}</mo>"),
        "\\oint" => Some("<mo>\u{222E}</mo>"),
        "\\partial" => Some("<mo>\u{2202}</mo>"),
        "\\nabla" => Some("<mo>\u{2207}</mo>"),
        "\\forall" => Some("<mo>\u{2200}</mo>"),
        "\\exists" => Some("<mo>\u{2203}</mo>"),
        "\\neg" | "\\lnot" => Some("<mo>\u{00AC}</mo>"),
        "\\in" => Some("<mo>\u{2208}</mo>"),
        "\\notin" | "\\not\\in" => Some("<mo>\u{2209}</mo>"),
        "\\subset" => Some("<mo>\u{2282}</mo>"),
        "\\supset" => Some("<mo>\u{2283}</mo>"),
        "\\subseteq" => Some("<mo>\u{2286}</mo>"),
        "\\supseteq" => Some("<mo>\u{2287}</mo>"),
        "\\cup" => Some("<mo>\u{222A}</mo>"),
        "\\cap" => Some("<mo>\u{2229}</mo>"),
        "\\emptyset" => Some("<mo>\u{2205}</mo>"),
        "\\wedge" => Some("<mo>\u{2227}</mo>"),
        "\\vee" => Some("<mo>\u{2228}</mo>"),
        "\\oplus" => Some("<mo>\u{2295}</mo>"),
        "\\otimes" => Some("<mo>\u{2297}</mo>"),
        "\\perp" => Some("<mo>\u{22A5}</mo>"),
        "\\ldots" | "\\dots" => Some("<mo>\u{2026}</mo>"),
        "\\cdots" => Some("<mo>\u{22EF}</mo>"),
        // Math spacing
        "\\quad" | "quad" => Some("<mspace width=\"1em\"/>"),
        "\\qquad" | "qquad" => Some("<mspace width=\"2em\"/>"),
        "\\," => Some("<mspace width=\"thinmathspace\"/>"),
        "\\:" => Some("<mspace width=\"mediummathspace\"/>"),
        "\\;" => Some("<mspace width=\"thickmathspace\"/>"),
        "\\!" => Some("<mspace width=\"negativethinmathspace\"/>"),
        _ => None,
    }
}

fn matrix_to_mathml(content: &str, delimiters: Option<(&str, &str)>) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                format!(
                    "    <mtd><mrow>{}</mrow></mtd>",
                    latex_to_mathml(cell.trim())
                )
            })
            .collect();
        rows_xml.push(format!("  <mtr>\n{}\n  </mtr>", cells.join("\n")));
    }
    let table = format!("<mtable>\n{}\n</mtable>", rows_xml.join("\n"));
    match delimiters {
        Some((open, close)) => format!("<mrow><mo>{}</mo>{}</mrow><mo>{}</mo>", open, table, close),
        None => table,
    }
}

fn cases_to_mathml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| format!("<mtd><mrow>{}</mrow></mtd>", latex_to_mathml(cell.trim())))
            .collect();
        rows_xml.push(format!("  <mtr>{}</mtr>", cells.join("")));
    }
    format!(
        "<mrow><mo>{{</mo><mtable>\n{}\n</mtable><mo>}}</mo></mrow>",
        rows_xml.join("\n")
    )
}

fn aligned_to_mathml(content: &str) -> String {
    let rows = split_matrix_rows(content);
    let mut rows_xml = Vec::new();
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .map(|cell| {
                format!(
                    "    <mtd><mrow>{}</mrow></mtd>",
                    latex_to_mathml(cell.trim())
                )
            })
            .collect();
        rows_xml.push(format!("  <mtr>\n{}\n  </mtr>", cells.join("\n")));
    }
    format!("<mtable>\n{}\n</mtable>", rows_xml.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_layout_commands_preserve_trailing_expression() {
        let boxed = latex_to_mathml("\\boxed{x}+y");
        assert!(boxed.contains("<menclosed notation=\"box\">") && boxed.contains("<mi>y</mi>"));

        let phantom = latex_to_mathml("a+\\vphantom{b}+c");
        assert!(phantom.contains("<mpadded width=\"0\">") && phantom.contains("<mi>c</mi>"));
    }

    #[test]
    fn math_delimiter_macros_have_explicit_delimiters() {
        assert!(latex_to_mathml("\\abs{x}").contains(">|</mo>"));
        assert!(latex_to_mathml("\\norm{x}").contains(">‖</mo>"));
        assert!(latex_to_mathml("\\floor{x}").contains(">⌊</mo>"));
        assert!(latex_to_mathml("\\ceil{x}").contains(">⌈</mo>"));
    }

    #[test]
    fn math_style_declarations_apply_to_the_remaining_expression() {
        let display = latex_to_mathml("\\displaystyle \\sum_{i=1}^{n} x_i");
        assert!(display.starts_with("<mstyle displaystyle=\"true\" scriptlevel=\"0\">"));
        assert!(display.contains("∑"));

        let script = latex_to_mathml("\\scriptstyle{x}");
        assert!(script.contains("displaystyle=\"false\" scriptlevel=\"1\""));
    }

    #[test]
    fn accents_own_braced_and_unbraced_arguments() {
        for latex in ["\\vec v", "\\vec{v}", "\\overrightarrow{BC}"] {
            let mathml = latex_to_mathml(latex);
            assert!(mathml.contains("<mover>"), "{latex}: {mathml}");
            assert!(
                mathml.contains("<mi>v</mi>") || mathml.contains("BC"),
                "{latex}: {mathml}"
            );
            assert!(
                mathml.contains('→') && !mathml.contains("<mrow></mrow>"),
                "{latex}: {mathml}"
            );
        }
    }
}
