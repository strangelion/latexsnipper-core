//! Bounded, non-executing interpretation of array column specifications.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArrayColumns {
    pub align: Vec<&'static str>,
    // Boundaries before, between, and after columns. Repeated bars are retained.
    pub rules: Vec<u8>,
}

pub(crate) fn parse_columns(source: &str) -> Result<ArrayColumns, String> {
    if source.len() > 4096 {
        return Err("Array column specification exceeds 4096 bytes".into());
    }
    let columns = parse_inner(source, 0)?;
    if columns.align.is_empty() {
        return Err("Array requires a nonempty column specification".into());
    }
    Ok(columns)
}

fn parse_inner(source: &str, depth: usize) -> Result<ArrayColumns, String> {
    if depth > 16 {
        return Err("Array column repetition nesting limit exceeded".into());
    }
    let chars: Vec<char> = source.chars().collect();
    let mut pos = 0;
    let mut result = ArrayColumns {
        align: Vec::new(),
        rules: vec![0],
    };
    while pos < chars.len() {
        skip_space(&chars, &mut pos);
        if pos == chars.len() {
            break;
        }
        match chars[pos] {
            'l' | 'c' | 'r' => {
                if result.align.len() == 128 {
                    return Err("Array exceeds 128 columns".into());
                }
                result.align.push(match chars[pos] {
                    'l' => "left",
                    'r' => "right",
                    _ => "center",
                });
                result.rules.push(0);
                pos += 1;
            }
            '|' => {
                let boundary = result.rules.last_mut().expect("one initial boundary");
                *boundary = boundary.checked_add(1).ok_or("Array rule count overflow")?;
                pos += 1;
            }
            '*' => {
                pos += 1;
                let count = group(&chars, &mut pos)?;
                let count = count.trim();
                if count.is_empty() || !count.bytes().all(|ch| ch.is_ascii_digit()) {
                    return Err("Array repeat count must be an unsigned integer".into());
                }
                let count: usize = count.parse().map_err(|_| "Array repeat count overflow")?;
                if count > 128 {
                    return Err("Array repeat count exceeds 128".into());
                }
                let nested = group(&chars, &mut pos)?;
                let nested = parse_inner(&nested, depth + 1)?;
                if nested.align.len().saturating_mul(count) > 128 - result.align.len() {
                    return Err("Array expanded column count exceeds 128".into());
                }
                for _ in 0..count {
                    let boundary = result.rules.last_mut().expect("one initial boundary");
                    *boundary = boundary
                        .checked_add(nested.rules[0])
                        .ok_or("Array rule count overflow")?;
                    result.align.extend_from_slice(&nested.align);
                    result.rules.extend_from_slice(&nested.rules[1..]);
                }
            }
            ch => {
                return Err(format!(
                "Unsupported array column modifier '{ch}'; original specification must be retained"
            ))
            }
        }
    }
    Ok(result)
}

fn skip_space(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() {
        if chars[*pos].is_whitespace() {
            *pos += 1;
        } else if chars[*pos] == '%' {
            while *pos < chars.len() && chars[*pos] != '\n' {
                *pos += 1;
            }
        } else {
            break;
        }
    }
}

fn group(chars: &[char], pos: &mut usize) -> Result<String, String> {
    skip_space(chars, pos);
    if chars.get(*pos) != Some(&'{') {
        return Err("Array repetition requires braced arguments".into());
    }
    *pos += 1;
    let start = *pos;
    let mut depth = 0;
    while *pos < chars.len() {
        match chars[*pos] {
            '\\' if *pos + 1 < chars.len() => {
                *pos += 2;
                continue;
            }
            '%' => {
                while *pos < chars.len() && chars[*pos] != '\n' {
                    *pos += 1;
                }
                continue;
            }
            '{' => depth += 1,
            '}' if depth == 0 => {
                let value = chars[start..*pos].iter().collect();
                *pos += 1;
                return Ok(value);
            }
            '}' => depth -= 1,
            _ => {}
        }
        *pos += 1;
    }
    Err("Unclosed array repetition argument".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alignments_repetitions_comments_and_rule_boundaries_are_retained() {
        let value = parse_columns("|l*{2}{cr|}% comment\nr").unwrap();
        assert_eq!(
            value.align,
            ["left", "center", "right", "center", "right", "right"]
        );
        assert_eq!(value.rules, [1, 0, 0, 1, 0, 1, 0]);
        assert_eq!(parse_columns("*{2}{|c|}").unwrap().rules, [1, 2, 1]);
    }
    #[test]
    fn modifiers_malformed_specs_and_expansion_bombs_are_rejected() {
        for source in [
            "",
            "p{2cm}",
            "@{}c",
            ">{\\bfseries}l",
            "*{-1}{c}",
            "*{999999999999999999999}{c}",
            "*{2}c",
            "*{2}{c",
            "*{128}{*{128}{c}}",
        ] {
            assert!(parse_columns(source).is_err(), "{source}");
        }
    }
}
