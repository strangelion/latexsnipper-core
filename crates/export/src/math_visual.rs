/// Convert common LaTeX math syntax into a readable Unicode visual fallback.
///
/// This is intentionally not a semantic serializer. The original source is
/// retained separately by visual exporters for provenance.
pub(crate) fn visual_math_text(latex: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = latex.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\\' {
            let start = index + 1;
            index = start;
            while index < chars.len() && chars[index].is_ascii_alphabetic() {
                index += 1;
            }
            let command: String = chars[start..index].iter().collect();
            match command.as_str() {
                "frac" => {
                    if let (Some((numerator, next)), Some((denominator, end))) =
                        (read_group(&chars, index), read_group_after(&chars, index))
                    {
                        output.push('(');
                        output.push_str(&visual_math_text(&numerator));
                        output.push_str(")/(");
                        output.push_str(&visual_math_text(&denominator));
                        output.push(')');
                        index = end.max(next);
                    } else {
                        output.push_str("frac");
                    }
                }
                "sqrt" => {
                    if let Some((value, end)) = read_group(&chars, index) {
                        output.push_str("√(");
                        output.push_str(&visual_math_text(&value));
                        output.push(')');
                        index = end;
                    } else {
                        output.push('√');
                    }
                }
                _ => output.push_str(command_symbol(&command)),
            }
            if command.is_empty() && index < chars.len() {
                output.push(chars[index]);
                index += 1;
            }
            continue;
        }

        match chars[index] {
            '{' | '}' | '$' => {}
            '^' => output.push('˄'),
            '_' => output.push('˅'),
            value => output.push(value),
        }
        index += 1;
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn read_group_after(chars: &[char], index: usize) -> Option<(String, usize)> {
    let (_, next) = read_group(chars, index)?;
    read_group(chars, next)
}

fn read_group(chars: &[char], mut index: usize) -> Option<(String, usize)> {
    while index < chars.len() && chars[index].is_whitespace() {
        index += 1;
    }
    if chars.get(index) != Some(&'{') {
        return None;
    }
    let start = index + 1;
    let mut depth = 1usize;
    index = start;
    while index < chars.len() {
        match chars[index] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((chars[start..index].iter().collect(), index + 1));
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn command_symbol(command: &str) -> &str {
    match command {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "theta" => "θ",
        "lambda" => "λ",
        "mu" => "μ",
        "pi" => "π",
        "sigma" => "σ",
        "phi" => "φ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Omega" => "Ω",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "infty" => "∞",
        "times" => "×",
        "cdot" => "·",
        "pm" => "±",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "in" => "∈",
        "notin" => "∉",
        "partial" => "∂",
        "nabla" => "∇",
        "text" | "mathrm" | "mathbf" | "mathit" | "operatorname" => "",
        unknown => unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_structured_math_without_exposing_latex_commands() {
        let rendered = visual_math_text(r"\frac{\alpha+1}{\sqrt{x}} \leq \infty");
        assert_eq!(rendered, "(α+1)/(√(x)) ≤ ∞");
        assert!(!rendered.contains('\\'));
    }
}
