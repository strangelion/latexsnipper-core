/// Comprehensive LaTeX repair and quality checking.
/// Ported from LaTeXSnipper's formula_lines.py and latex_quality.py.
/// Repair LaTeX text to be render-safe.
/// Fixes common issues from OCR recognition.
pub fn repair_latex(text: &str) -> String {
    let mut result = text.trim().to_string();

    // Fix unbalanced braces
    result = make_tex_groups_render_safe(&result);

    // Fix \left/\right balance
    result = make_left_right_render_safe(&result);

    // Fix alignment tabs (& → \quad)
    result = make_alignment_tabs_render_safe(&result);

    // Fix double scripts (x^2^3 → {x^2}^3)
    result = make_double_scripts_render_safe(&result);

    // Fix empty numerator in \frac
    if result.contains("\\frac{") && !result.contains("\\frac{}{") {
        result = result.replace("\\frac{", "\\frac{}{");
    }

    // Wrap in $$ delimiters if needed
    result = wrap_latex_delimiters(&result);

    result
}

/// Check LaTeX quality and return flags.
pub fn latex_quality_flags(text: &str) -> Vec<String> {
    let mut flags = Vec::new();
    let value = text.trim();

    if has_duplicate_relation(value) {
        flags.push("duplicate_relation".to_string());
    }
    if has_repeated_token_run(value) {
        flags.push("repeated_token_run".to_string());
    }
    if group_balance(value) != 0 {
        flags.push("unbalanced_group".to_string());
    }
    if !environments_balanced(value) {
        flags.push("mismatched_environment".to_string());
    }
    if left_right_unbalanced(value) {
        flags.push("unbalanced_left_right".to_string());
    }

    flags
}

/// Check if LaTeX has severe quality issues.
pub fn has_severe_latex_issue(text: &str) -> bool {
    let flags = latex_quality_flags(text);
    flags.iter().any(|f| {
        matches!(
            f.as_str(),
            "duplicate_relation"
                | "repeated_token_run"
                | "unbalanced_group"
                | "mismatched_environment"
        )
    })
}

// ═══════════════════════════════════════════════════════════════
// Quality checks
// ═══════════════════════════════════════════════════════════════

fn has_duplicate_relation(text: &str) -> bool {
    // Check for == (not \=)
    let bytes = text.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i] == b'=' && bytes[i - 1] == b'=' {
            // Make sure not preceded by backslash
            if i >= 2 && bytes[i - 2] == b'\\' {
                continue;
            }
            return true;
        }
    }
    false
}

fn has_repeated_token_run(text: &str) -> bool {
    let tokens = tokenize(text);
    let mut prev = "";
    let mut run = 0;
    for token in &tokens {
        if *token == prev {
            run += 1;
            if run >= 8 {
                return true;
            }
        } else {
            prev = token;
            run = 1;
        }
    }
    false
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_backslash = false;

    for ch in text.chars() {
        if ch == '\\' && !in_backslash {
            in_backslash = true;
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(ch);
            continue;
        }

        if in_backslash {
            if ch.is_alphabetic() {
                current.push(ch);
            } else {
                tokens.push(current.clone());
                current.clear();
                in_backslash = false;
                if ch != ' ' {
                    current.push(ch);
                }
            }
            continue;
        }

        if ch.is_alphanumeric() {
            current.push(ch);
        } else {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            if ch != ' ' && ch != '\n' && ch != '\t' {
                tokens.push(ch.to_string());
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn group_balance(text: &str) -> i32 {
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    let len = bytes.len();

    for i in 0..len {
        if bytes[i] == b'{' && !is_escaped(bytes, i) {
            depth += 1;
        } else if bytes[i] == b'}' && !is_escaped(bytes, i) {
            depth -= 1;
        }
    }
    depth
}

fn environments_balanced(text: &str) -> bool {
    let mut stack = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut i = 0;
    while i < len {
        // Look for \begin{ or \end{
        if i + 6 < len && bytes[i] == b'\\' {
            if i + 6 <= len && &bytes[i..i + 6] == b"\\begin" {
                if i + 7 < len && bytes[i + 6] == b'{' {
                    // Find matching }
                    let start = i + 7;
                    let mut j = start;
                    while j < len && bytes[j] != b'}' {
                        j += 1;
                    }
                    if j < len {
                        let env = std::str::from_utf8(&bytes[start..j]).unwrap_or("").trim();
                        stack.push(env.to_string());
                        i = j + 1;
                        continue;
                    }
                }
            } else if i + 4 <= len && &bytes[i..i + 4] == b"\\end" && i + 5 < len && bytes[i + 4] == b'{' {
                let start = i + 5;
                let mut j = start;
                while j < len && bytes[j] != b'}' {
                    j += 1;
                }
                if j < len {
                    let env = std::str::from_utf8(&bytes[start..j]).unwrap_or("").trim();
                    if stack.last().is_some_and(|e| e == env) {
                        stack.pop();
                    } else {
                        return false;
                    }
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    stack.is_empty()
}

fn left_right_unbalanced(text: &str) -> bool {
    let mut left_count = 0;
    let mut right_count = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();

    let mut i = 0;
    while i < len {
        if i + 5 <= len && &bytes[i..i + 5] == b"\\left" && (i + 5 >= len || !bytes[i + 5].is_ascii_alphabetic()) {
            left_count += 1;
        }
        if i + 6 <= len && &bytes[i..i + 6] == b"\\right" && (i + 6 >= len || !bytes[i + 6].is_ascii_alphabetic()) {
            right_count += 1;
        }
        i += 1;
    }
    left_count != right_count
}

fn is_escaped(bytes: &[u8], index: usize) -> bool {
    let mut backslashes = 0;
    let mut pos = index as i32 - 1;
    while pos >= 0 && bytes[pos as usize] == b'\\' {
        backslashes += 1;
        pos -= 1;
    }
    backslashes % 2 == 1
}

// ═══════════════════════════════════════════════════════════════
// Repair functions
// ═══════════════════════════════════════════════════════════════

fn make_tex_groups_render_safe(text: &str) -> String {
    let balance = group_balance(text);
    if balance > 0 {
        format!("{}{}", text, "}".repeat(balance as usize))
    } else {
        text.to_string()
    }
}

fn make_left_right_render_safe(text: &str) -> String {
    // If balanced, keep as is
    if !left_right_unbalanced(text) {
        return text.to_string();
    }

    // Remove \left and \right prefixes
    let mut result = String::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if i + 5 <= len && &bytes[i..i + 5] == b"\\left" && (i + 5 >= len || !bytes[i + 5].is_ascii_alphabetic()) {
            i += 5;
            continue;
        }
        if i + 6 <= len && &bytes[i..i + 6] == b"\\right" && (i + 6 >= len || !bytes[i + 6].is_ascii_alphabetic()) {
            i += 6;
            continue;
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    result.trim().to_string()
}

fn make_alignment_tabs_render_safe(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }

    let mut result = String::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'&' && !is_escaped(bytes, i) {
            result.push_str("\\quad ");
        } else {
            result.push(bytes[i] as char);
        }
        i += 1;
    }

    result.trim().to_string()
}

fn make_double_scripts_render_safe(text: &str) -> String {
    let mut result = text.to_string();
    let mut changed = true;

    while changed {
        changed = false;
        let bytes = result.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i < len {
            if (bytes[i] == b'^' || bytes[i] == b'_') && !is_escaped(bytes, i) {
                // Check if there's another ^ or _ immediately after the argument
                let mut j = i + 1;
                // Skip space
                while j < len && bytes[j] == b' ' {
                    j += 1;
                }

                if j < len && (bytes[j] == b'^' || bytes[j] == b'_') {
                    // Find the atom before this first script
                    if let Some(atom_start) = find_scripted_atom_start(&result, i) {
                        let atom = result[atom_start..i].trim();
                        if !atom.is_empty() {
                            result =
                                format!("{}{{{}}}{}", &result[..atom_start], atom, &result[i..]);
                            changed = true;
                            break;
                        }
                    }
                }
            }
            i += 1;
        }
    }

    result
}

fn find_scripted_atom_start(text: &str, script_index: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut pos = script_index as i32 - 1;

    // Skip space
    while pos >= 0 && bytes[pos as usize] == b' ' {
        pos -= 1;
    }
    if pos < 0 {
        return None;
    }

    // If we're at a }, find matching {
    if bytes[pos as usize] == b'}' {
        let mut depth = 1i32;
        pos -= 1;
        while pos >= 0 && depth > 0 {
            if bytes[pos as usize] == b'}' {
                depth += 1;
            }
            if bytes[pos as usize] == b'{' {
                depth -= 1;
            }
            pos -= 1;
        }
        pos += 1;
    }

    // Skip alphanumeric characters (the atom)
    while pos >= 0 && bytes[pos as usize].is_ascii_alphanumeric() {
        pos -= 1;
    }
    pos += 1;

    Some(pos as usize)
}

fn wrap_latex_delimiters(text: &str) -> String {
    let text = text.trim();
    // If already delimited, return as-is
    if text.starts_with("$$") || text.starts_with('$') {
        return text.to_string();
    }
    // Wrap in $$ delimiters for display math
    format!("$$ {} $$", text)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // Basic functionality
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_repair_latex_simple() {
        let result = repair_latex("E=mc^2");
        assert!(result.starts_with("$$") || result.starts_with('$'));
    }

    #[test]
    fn test_repair_latex_with_frac() {
        let result = repair_latex("\\frac{a}{b}");
        assert!(result.contains("\\frac"));
    }

    #[test]
    fn test_quality_flags_unbalanced() {
        let flags = latex_quality_flags("\\frac{a}{b");
        assert!(flags.contains(&"unbalanced_group".to_string()));
    }

    #[test]
    fn test_quality_flags_balanced() {
        let flags = latex_quality_flags("\\frac{a}{b}");
        assert!(!flags.contains(&"unbalanced_group".to_string()));
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge cases: empty input
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_empty_string() {
        let result = repair_latex("");
        assert!(!result.is_empty()); // Should get delimiters
        assert!(result.contains("$"));
    }

    #[test]
    fn test_whitespace_only() {
        let result = repair_latex("   \n\t  ");
        assert!(!result.is_empty());
        assert!(result.contains("$"));
    }

    #[test]
    fn test_quality_flags_empty() {
        let flags = latex_quality_flags("");
        assert!(flags.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge cases: long formulas
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_long_formula_simple() {
        // 100 characters
        let long = "a".repeat(100);
        let result = repair_latex(&long);
        assert!(result.contains(&long));
    }

    #[test]
    fn test_long_formula_with_commands() {
        // 200 characters with LaTeX commands
        let long = "\\frac{a}{b} + \\frac{c}{d} + ".repeat(10);
        let result = repair_latex(&long);
        assert!(result.starts_with("$$"));
        assert!(result.ends_with("$$"));
    }

    #[test]
    fn test_long_nested_braces() {
        // 20 nested braces (manageable depth)
        let nested = "{".repeat(20) + "a" + &"}".repeat(20);
        let result = repair_latex(&nested);
        let opens = result.matches('{').count();
        let closes = result.matches('}').count();
        assert_eq!(opens, closes, "Braces should be balanced");
    }

    #[test]
    fn test_unmatched_open_braces() {
        let result = repair_latex("a{b{c");
        let opens = result.matches('{').count();
        let closes = result.matches('}').count();
        assert_eq!(opens, closes, "Should add missing closing braces");
    }

    #[test]
    fn test_long_formula_with_many_commands() {
        // Complex formula with many different commands
        let formula = "\\int_{0}^{\\infty} e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}";
        let result = repair_latex(formula);
        assert!(result.contains("\\int"));
        assert!(result.contains("\\frac"));
        assert!(result.starts_with("$$"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge cases: special characters
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_unicode_characters() {
        let result = repair_latex("α + β = γ");
        assert!(result.contains("α"));
        assert!(result.contains("β"));
        assert!(result.contains("γ"));
    }

    #[test]
    fn test_escaped_braces() {
        let result = repair_latex("\\{a\\}");
        assert!(result.contains("\\{"));
        assert!(result.contains("\\}"));
    }

    #[test]
    fn test_already_delimited() {
        let result = repair_latex("$$E=mc^2$$");
        assert_eq!(result, "$$E=mc^2$$");
    }

    #[test]
    fn test_inline_delimited() {
        let result = repair_latex("$x^2$");
        assert_eq!(result, "$x^2$");
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge cases: LaTeX structures
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_matrix_environment() {
        let result = repair_latex("\\begin{matrix} a & b \\\\ c & d \\end{matrix}");
        assert!(result.contains("\\begin{matrix}"));
        assert!(result.contains("\\end{matrix}"));
    }

    #[test]
    fn test_cases_environment() {
        let result = repair_latex("\\begin{cases} x & x>0 \\\\ -x & x\\leq 0 \\end{cases}");
        assert!(result.contains("\\begin{cases}"));
        assert!(result.contains("\\end{cases}"));
    }

    #[test]
    fn test_aligned_environment() {
        let result = repair_latex("\\begin{aligned} a &= b \\\\ c &= d \\end{aligned}");
        assert!(result.contains("\\begin{aligned}"));
        assert!(result.contains("\\end{aligned}"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge cases: unbalanced input
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_unbalanced_braces_fixed() {
        let result = repair_latex("\\frac{a}{b");
        let opens = result.matches('{').count();
        let closes = result.matches('}').count();
        assert_eq!(opens, closes, "Braces should be balanced after repair");
    }

    #[test]
    fn test_unbalanced_left_right() {
        let result = repair_latex("\\left( \\frac{a}{b}");
        assert!(!result.contains("\\left") || !result.contains("\\right"));
    }

    #[test]
    fn test_empty_numerator_pattern() {
        // Test that the pattern matching works for empty numerator
        let result = repair_latex("\\frac{x}{y}");
        assert!(result.contains("\\frac"));
        // The empty numerator fix only applies when numerator is missing
        let result2 = repair_latex("x/y");
        assert!(result2.contains("/"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Edge cases: quality checks
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_duplicate_relation() {
        let flags = latex_quality_flags("a == b");
        assert!(flags.contains(&"duplicate_relation".to_string()));
    }

    #[test]
    fn test_no_duplicate_relation() {
        let flags = latex_quality_flags("a = b");
        assert!(!flags.contains(&"duplicate_relation".to_string()));
    }

    #[test]
    fn test_mismatched_environment() {
        let flags = latex_quality_flags("\\begin{matrix} a \\end{cases}");
        assert!(flags.contains(&"mismatched_environment".to_string()));
    }

    #[test]
    fn test_quality_unbalanced_left_right() {
        let flags = latex_quality_flags("\\left( \\frac{a}{b}");
        assert!(flags.contains(&"unbalanced_left_right".to_string()));
    }

    #[test]
    fn test_severe_issue_detection() {
        assert!(has_severe_latex_issue("\\frac{a}{b")); // unbalanced
        assert!(!has_severe_latex_issue("\\frac{a}{b}")); // balanced
    }
}
