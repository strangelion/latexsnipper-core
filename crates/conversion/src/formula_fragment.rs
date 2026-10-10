//! Bounded standalone formula projection; no document or math-mode wrapper.
use latexsnipper_foundation::{Result, SnipperError};

// Only accept the shape produced by one standalone display formula. This is
// not a general TeX parser or a renderer safety gate: execution stays outside
// this bridge. Comments and escaped control symbols must not change boundaries.
pub fn latex_display_to_fragment(display: &str) -> Result<String> {
    if display.len() > 64 * 1024 + 8 {
        return Err(SnipperError::LimitExceeded(
            "formula fragment exceeds 64 KiB".into(),
        ));
    }
    let source = display
        .trim()
        .strip_prefix("\\[")
        .and_then(|value| value.strip_suffix("\\]"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| fragment_error("formula fragment requires exactly one display formula"))?;
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut trailing_comment = false;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                    index += 1;
                }
                trailing_comment = index == bytes.len();
            }
            b'$' => {
                return Err(fragment_error(
                    "formula fragment contains nested math delimiters",
                ))
            }
            b'\\' => {
                index += 1;
                if index == bytes.len() {
                    return Err(fragment_error(
                        "formula fragment ends in an incomplete command",
                    ));
                }
                if matches!(bytes[index], b'[' | b']' | b'(' | b')') {
                    return Err(fragment_error(
                        "formula fragment contains nested math delimiters",
                    ));
                }
                let start = index;
                if bytes[index].is_ascii_alphabetic() {
                    while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
                        index += 1;
                    }
                    let command = &source[start..index];
                    if matches!(
                        command,
                        "documentclass"
                            | "documentstyle"
                            | "usepackage"
                            | "RequirePackage"
                            | "verb"
                            | "catcode"
                    ) {
                        return Err(fragment_error(
                            "formula fragment contains document or lexical commands",
                        ));
                    }
                    if matches!(command, "begin" | "end") {
                        let mut rest = &source[index..];
                        loop {
                            rest = rest.trim_start();
                            if let Some(comment) = rest.strip_prefix('%') {
                                rest = comment
                                    .find(['\n', '\r'])
                                    .map_or("", |offset| &comment[offset..]);
                            } else {
                                break;
                            }
                        }
                        let environment = rest
                            .strip_prefix('{')
                            .and_then(|rest| rest.split_once('}'))
                            .map(|(name, _)| name.trim());
                        if matches!(
                            environment,
                            Some(
                                "document"
                                    | "math"
                                    | "displaymath"
                                    | "equation"
                                    | "equation*"
                                    | "align"
                                    | "align*"
                                    | "gather"
                                    | "gather*"
                                    | "multline"
                                    | "multline*"
                                    | "eqnarray"
                                    | "eqnarray*"
                            )
                        ) {
                            return Err(fragment_error(
                                "formula fragment contains a document environment",
                            ));
                        }
                    }
                } else {
                    // Consume one UTF-8 control symbol, including escaped '%'/'$'.
                    index += source[index..].chars().next().unwrap().len_utf8();
                }
            }
            _ => index += 1,
        }
    }
    // A trailing comment would swallow the synthetic outer closing delimiter.
    // The source trim above intentionally retains comments; add a line boundary.
    Ok(if trailing_comment {
        format!("{source}\n")
    } else {
        source.to_string()
    })
}

fn fragment_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Conversion(message.into())
}
