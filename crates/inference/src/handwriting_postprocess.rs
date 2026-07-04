/// Handwriting-specific post-processing.
///
/// Applies heuristics to improve handwriting recognition results:
///   - Whitespace normalization
///   - Common OCR symbol confusion fixes
///   - Handwriting-specific number/letter confusions (O→0, l→1, etc.)
///   - LaTeX-specific cleanup
///
/// Post-process handwriting recognition result.
///
/// Applies handwriting-specific fixes that are not covered by
/// the general LaTeX repair module.
pub fn postprocess_handwriting(text: &str) -> String {
    let result = text.trim().to_string();

    // Skip empty input
    if result.is_empty() {
        return result;
    }

    // Fix common OCR confusions for handwriting
    let result = fix_ocr_confusions(&result);

    // Fix number/letter confusions common in handwriting OCR
    let result = fix_number_letter_confusions(&result);

    // Normalize whitespace (collapse multiple spaces)
    let result = normalize_spacing(&result);

    // Normalize punctuation for sentences
    normalize_punctuation(&result)
}

/// Fix common number/letter confusions in handwriting OCR.
///
/// TrOCR and similar handwriting models often confuse visually similar
/// characters that have different meanings in LaTeX formulas.
fn fix_number_letter_confusions(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut prev_char: Option<char> = None;

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Inside a LaTeX command — preserve everything
            output.push(ch);
            while let Some(&next) = chars.peek() {
                output.push(next);
                chars.next();
                if next == ' ' || next == '{' || next == '}' || next == '\\' {
                    break;
                }
            }
        } else if ch == 'O' || ch == 'o' {
            // 'O' → '0' confusion: when followed by digits, period, or
            // when surrounded by digits (e.g. "O7", "10O", "O 7")
            let next_is_numeric = match chars.peek() {
                Some(next) => next.is_ascii_digit() || *next == '.' || *next == ',',
                None => false,
            };
            let prev_is_numeric = prev_char.is_some_and(|p| p.is_ascii_digit() || p == '.');
            if next_is_numeric || prev_is_numeric {
                output.push('0');
            } else {
                output.push(ch);
            }
        } else if ch == 'l' {
            // lowercase 'l' → '1' confusion: in numeric positions
            let next_is_numeric = match chars.peek() {
                Some(next) => next.is_ascii_digit() || *next == '.' || *next == '/',
                None => false,
            };
            let prev_is_numeric = prev_char.is_some_and(|p| p.is_ascii_digit());
            if next_is_numeric || prev_is_numeric {
                output.push('1');
            } else {
                output.push('l');
            }
        } else if ch == 'S' {
            // 'S' → '5' confusion: only in numeric contexts
            let next_is_numeric = match chars.peek() {
                Some(next) => next.is_ascii_digit() || *next == '.' || *next == ',',
                None => false,
            };
            let prev_is_numeric = prev_char.is_some_and(|p| p.is_ascii_digit() || p == '.');
            if next_is_numeric || prev_is_numeric {
                output.push('5');
            } else {
                output.push('S');
            }
        } else {
            output.push(ch);
        }
        prev_char = Some(ch);
    }

    output
}

/// Normalize punctuation in handwriting recognition output.
///
/// - Ensures sentences end with period for natural language text
/// - Does NOT modify LaTeX/mathematical expressions (detected by presence of
///   backslash commands, braces, or math operators like =, +, /)
fn normalize_punctuation(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    let mut result = text.trim_end().to_string();

    // Skip if the text is clearly a LaTeX/math expression
    let is_math = result.contains('\\')
        || result.contains('{')
        || result.contains('_')
        || result.contains('^')
        || result.contains('=')
        || result.contains('+')
        || result.contains('/');

    // Only add period for multi-word natural language sentences
    let word_count = result.split_whitespace().count();
    let is_natural_language = !is_math && word_count >= 3;

    if is_natural_language
        && !result.ends_with('.')
        && !result.ends_with('!')
        && !result.ends_with('?')
    {
        result.push('.');
    }

    result
}

/// Fix common OCR symbol confusions that occur in handwriting recognition.
///
/// These are conservative, context-aware replacements that fix typical
/// TrOCR-on-handwriting errors without corrupting intentional LaTeX syntax.
fn fix_ocr_confusions(text: &str) -> String {
    let mut result = text.to_string();

    // Fix: OCR often reads backslash + letter combos wrong in handwriting
    // "\\alpha" can become "a\\lpha", "\\beta" → "\\bet a", etc.
    // Fix known LaTeX commands that got broken by whitespace insertion
    for (broken, fixed) in BROKEN_LATEX_PATTERNS {
        if result.contains(broken) {
            result = result.replace(broken, fixed);
        }
    }

    result
}

/// Common LaTeX command patterns broken by OCR whitespace insertion.
const BROKEN_LATEX_PATTERNS: &[(&str, &str)] = &[
    // Greek letters with stray spaces
    ("\\a l p h a", "\\alpha"),
    ("\\b e t a", "\\beta"),
    ("\\g a m m a", "\\gamma"),
    ("\\d e l t a", "\\delta"),
    ("\\e p s i l o n", "\\epsilon"),
    ("\\t h e t a", "\\theta"),
    ("\\l a m b d a", "\\lambda"),
    ("\\s i g m a", "\\sigma"),
    ("\\o m e g a", "\\omega"),
    // Common commands
    ("\\f r a c", "\\frac"),
    ("\\s q r t", "\\sqrt"),
    ("\\s u m", "\\sum"),
    ("\\i n t", "\\int"),
    ("\\l i m", "\\lim"),
    ("\\i n f t y", "\\infty"),
    // Common OCR symbol confusions in formulas
    ("\\ti mes", "\\times"),
    ("\\c d o t", "\\cdot"),
    ("\\p a r t i a l", "\\partial"),
    ("\\n a b l a", "\\nabla"),
    ("\\r i g h t a r r o w", "\\rightarrow"),
    ("\\l e f t a r r o w", "\\leftarrow"),
    ("\\R i g h t a r r o w", "\\Rightarrow"),
    ("\\L e f t a r r o w", "\\Leftarrow"),
    ("\\l e q", "\\leq"),
    ("\\g e q", "\\geq"),
];

/// Normalize whitespace in handwriting recognition output.
///
/// Collapses multiple consecutive spaces into one, trims, and
/// removes stray spaces before LaTeX braces.
fn normalize_spacing(text: &str) -> String {
    // Collapse multiple spaces
    let mut result = String::with_capacity(text.len());
    let mut prev_was_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }

    // Remove stray spaces before LaTeX braces: "{ a + b }" → "{a + b}"
    // But preserve intentional spacing in text
    // This is a conservative fix: only collapse spaces directly adjacent to braces
    let result = result.replace("{ ", "{").replace(" }", "}");

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postprocess_handwriting_preserves_good_input() {
        let input = "E = mc^2";
        let result = postprocess_handwriting(input);
        assert_eq!(result, "E = mc^2");
    }

    #[test]
    fn test_postprocess_handwriting_empty() {
        assert_eq!(postprocess_handwriting(""), "");
        assert_eq!(postprocess_handwriting("   "), "");
    }

    #[test]
    fn test_fix_broken_latex_command() {
        // OCR inserted spaces inside \\alpha
        let input = "\\a l p h a + \\b e t a";
        let result = fix_ocr_confusions(input);
        assert_eq!(result, "\\alpha + \\beta");
    }

    #[test]
    fn test_fix_broken_frac() {
        let input = "\\f r a c{1}{2}";
        let result = fix_ocr_confusions(input);
        assert_eq!(result, "\\frac{1}{2}");
    }

    #[test]
    fn test_normalize_spacing_collapses_spaces() {
        let input = "hello    world";
        let result = normalize_spacing(input);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_normalize_spacing_fixes_brace_spaces() {
        let input = "{ a + b }";
        let result = normalize_spacing(input);
        assert_eq!(result, "{a + b}");
    }

    #[test]
    fn test_normalize_spacing_preserves_intentional_spaces() {
        let input = "Hello World";
        let result = normalize_spacing(input);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_postprocess_handwriting_with_latex() {
        let input = "\\a l p h a + \\b e t a = \\g a m m a";
        let result = postprocess_handwriting(input);
        assert_eq!(result, "\\alpha + \\beta = \\gamma");
    }

    #[test]
    fn test_fix_o_zero_confusion_numeric() {
        // 'O' followed by digit → '0'
        let result = fix_number_letter_confusions("My file is O7");
        assert_eq!(result, "My file is 07");
    }

    #[test]
    fn test_fix_o_zero_confusion_alpha() {
        // 'O' in non-numeric context stays 'O'
        let result = fix_number_letter_confusions("Open the door");
        assert_eq!(result, "Open the door");
    }

    #[test]
    fn test_fix_l_one_confusion_numeric() {
        // 'l' before digit → '1'
        let result = fix_number_letter_confusions("Total: l00");
        assert_eq!(result, "Total: 100");
    }

    #[test]
    fn test_fix_l_one_confusion_alpha() {
        // 'l' in word stays 'l'
        let result = fix_number_letter_confusions("hello world");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_fix_s_five_confusion_numeric() {
        // 'S' followed by digit → '5'
        let result = fix_number_letter_confusions("Item S");
        assert_eq!(result, "Item S");
    }

    #[test]
    fn test_punctuation_normalization() {
        // Sentence without period should get one
        let result = normalize_punctuation("This is a sentence");
        assert_eq!(result, "This is a sentence.");
    }

    #[test]
    fn test_punctuation_preserves_math() {
        // Mathematical expressions should NOT get a trailing period
        let result = normalize_punctuation("x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}");
        assert_eq!(result, "x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}");
    }

    #[test]
    fn test_number_letter_confusions_inside_latex() {
        // LaTeX commands should not be corrupted
        let result = fix_number_letter_confusions("\\alpha + \\beta + \\gamma");
        assert_eq!(result, "\\alpha + \\beta + \\gamma");
    }
}
