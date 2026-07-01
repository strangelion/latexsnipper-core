/// Parse Typst math syntax into LaTeX string.
/// Typst uses `frac(a,b)`, `sqrt(x)`, `mat(a,b; c,d)`, `hat(x)`, etc.
pub fn parse_typst_to_latex(typst: &str) -> String {
    let s = typst.trim();
    convert_typst_expr(s)
}

fn convert_typst_expr(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }

    if let Some(inner) = s.strip_prefix("frac(") {
        if let Some((num, den)) = split_typst_call_args(inner) {
            return format!(
                "\\frac{{{}}}{{{}}}",
                convert_typst_expr(&num),
                convert_typst_expr(&den)
            );
        }
    }

    if let Some(inner) = s.strip_prefix("sqrt(") {
        if let Some((deg, rest)) = split_typst_sqrt_args(inner) {
            if deg.is_empty() {
                let content = rest.strip_suffix(')').unwrap_or(&rest);
                return format!("\\sqrt{{{}}}", convert_typst_expr(content));
            } else {
                let content_str = rest.strip_prefix(',').unwrap_or(&rest).trim();
                let content = content_str.strip_suffix(')').unwrap_or(content_str);
                return format!(
                    "\\sqrt[{}]{{{}}}",
                    convert_typst_expr(&deg),
                    convert_typst_expr(content)
                );
            }
        }
    }

    if let Some(inner) = s.strip_prefix("binom(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        if let Some((n, k)) = inner.split_once(',') {
            return format!(
                "\\binom{{{}}}{{{}}}",
                convert_typst_expr(n),
                convert_typst_expr(k)
            );
        }
    }

    if let Some(inner) = s.strip_prefix("vec(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let items: Vec<String> = split_typst_args(inner)
            .iter()
            .map(|a| convert_typst_expr(a))
            .collect();
        return format!(
            "\\begin{{pmatrix}} {} \\end{{pmatrix}}",
            items.join(" \\\\ ")
        );
    }

    if let Some(inner) = s.strip_prefix("mat(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let rows: Vec<String> = inner
            .split(';')
            .map(|row| {
                let cells: Vec<String> = split_typst_args(row)
                    .iter()
                    .map(|c| convert_typst_expr(c.trim()))
                    .collect();
                cells.join(" & ")
            })
            .collect();
        return format!("\\begin{{matrix}} {} \\end{{matrix}}", rows.join(" \\\\ "));
    }

    if let Some(inner) = s.strip_prefix("cases(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        let rows: Vec<String> = inner
            .split(';')
            .map(|row| convert_typst_expr(row.trim()))
            .collect();
        return format!("\\begin{{cases}} {} \\end{{cases}}", rows.join(" \\\\ "));
    }

    if let Some(inner) = s.strip_prefix("lr(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        return format!("\\left({}\\right)", convert_typst_expr(inner));
    }

    if let Some(inner) = s.strip_prefix("cancel(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner);
        return format!("\\cancel{{{}}}", convert_typst_expr(inner));
    }

    if let Some(inner) = s.strip_prefix("op(") {
        let inner = inner
            .strip_suffix(')')
            .unwrap_or(inner)
            .trim()
            .trim_matches('"');
        return format!("\\operatorname{{{}}}", inner);
    }

    if let Some(inner) = s.strip_prefix("bb(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner).trim();
        return format!("\\mathbb{{{}}}", inner);
    }

    if let Some(inner) = s.strip_prefix("bold(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner).trim();
        return format!("\\mathbf{{{}}}", convert_typst_expr(inner));
    }

    if let Some(inner) = s.strip_prefix("italic(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner).trim();
        return format!("\\mathit{{{}}}", convert_typst_expr(inner));
    }

    if let Some(inner) = s.strip_prefix("mono(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner).trim();
        return format!("\\mathtt{{{}}}", inner);
    }

    if let Some(inner) = s.strip_prefix("serif(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner).trim();
        return format!("\\mathrm{{{}}}", convert_typst_expr(inner));
    }

    if let Some(inner) = s.strip_prefix("sans(") {
        let inner = inner.strip_suffix(')').unwrap_or(inner).trim();
        return format!("\\mathsf{{{}}}", convert_typst_expr(inner));
    }

    let accents = [
        ("hat(", "\\hat{"),
        ("dot(", "\\dot{"),
        ("bar(", "\\overline{"),
        ("tilde(", "\\tilde{"),
        ("vec(", "\\vec{"),
        ("acute(", "\\acute{"),
        ("grave(", "\\grave{"),
        ("check(", "\\check{"),
        ("breve(", "\\breve{"),
    ];
    for (prefix, latex_cmd) in &accents {
        if let Some(inner) = s.strip_prefix(prefix) {
            if prefix == &"vec(" {
                // vec(a,b) is a matrix, vec(x) is accent
                if inner.contains(',') && !inner.contains(';') {
                    // Might be matrix vec, check if it has exactly one arg
                    if let Some(close) = inner.rfind(')') {
                        let args = &inner[..close];
                        if args.split(',').count() == 1 {
                            // Single arg accent
                            return format!("{}{}}}", latex_cmd, convert_typst_expr(args.trim()));
                        }
                        // Multi-arg is matrix - already handled above
                    }
                }
            }
            let inner = inner.strip_suffix(')').unwrap_or(inner);
            return format!("{}{}}}", latex_cmd, convert_typst_expr(inner));
        }
    }

    if s == "nothing" {
        return String::new();
    }
    if s == "dots" {
        return "\\ldots ".to_string();
    }
    if s == "dots.h" {
        return "\\cdots ".to_string();
    }
    if s == "dots.v" {
        return "\\vdots ".to_string();
    }
    if s == "dots.down" {
        return "\\ddots ".to_string();
    }
    if s == "infinity" {
        return "\\infty ".to_string();
    }
    if s == "alpha"
        || s == "beta"
        || s == "gamma"
        || s == "delta"
        || s == "epsilon"
        || s == "zeta"
        || s == "eta"
        || s == "theta"
        || s == "iota"
        || s == "kappa"
        || s == "lambda"
        || s == "mu"
        || s == "nu"
        || s == "xi"
        || s == "pi"
        || s == "rho"
        || s == "sigma"
        || s == "tau"
        || s == "upsilon"
        || s == "phi"
        || s == "chi"
        || s == "psi"
        || s == "omega"
    {
        return format!("\\{} ", s);
    }
    if s == "epsilon.alt" {
        return "\\epsilon ".to_string();
    }
    if s == "phi.alt" {
        return "\\varphi ".to_string();
    }
    if s == "theta.alt" {
        return "\\vartheta ".to_string();
    }
    if s == "pi.alt" {
        return "\\varpi ".to_string();
    }
    if s == "rho.alt" {
        return "\\varrho ".to_string();
    }
    if s == "sigma.alt" {
        return "\\varsigma ".to_string();
    }
    if s == "Gamma"
        || s == "Delta"
        || s == "Theta"
        || s == "Lambda"
        || s == "Xi"
        || s == "Pi"
        || s == "Sigma"
        || s == "Upsilon"
        || s == "Phi"
        || s == "Psi"
        || s == "Omega"
    {
        return format!("\\{} ", s);
    }

    if s == "lt" {
        return "<".to_string();
    }
    if s == "gt" {
        return ">".to_string();
    }
    if s == "leq" || s == "lt.eq" {
        return "\\leq ".to_string();
    }
    if s == "geq" || s == "gt.eq" {
        return "\\geq ".to_string();
    }
    if s == "neq" {
        return "\\neq ".to_string();
    }
    if s == "approx" {
        return "\\approx ".to_string();
    }
    if s == "equiv" {
        return "\\equiv ".to_string();
    }
    if s == "sim" {
        return "\\sim ".to_string();
    }
    if s == "propto" {
        return "\\propto ".to_string();
    }
    if s == "parallel" {
        return "\\parallel ".to_string();
    }
    if s == "perp" {
        return "\\perp ".to_string();
    }
    if s == "times" {
        return "\\times ".to_string();
    }
    if s == "div" {
        return "\\div ".to_string();
    }
    if s == "plus.minus" {
        return "\\pm ".to_string();
    }
    if s == "minus.plus" {
        return "\\mp ".to_string();
    }
    if s == "dot" {
        return "\\cdot ".to_string();
    }
    if s == "star" {
        return "\\star ".to_string();
    }
    if s == "ast" {
        return "\\ast ".to_string();
    }
    if s == "circ" {
        return "\\circ ".to_string();
    }
    if s == "bullet" {
        return "\\bullet ".to_string();
    }
    if s == "prop" {
        return "\\propto ".to_string();
    }
    if s == "in" {
        return "\\in ".to_string();
    }
    if s == "notin" {
        return "\\notin ".to_string();
    }
    if s == "subset" {
        return "\\subset ".to_string();
    }
    if s == "superset" {
        return "\\supset ".to_string();
    }
    if s == "subset.eq" {
        return "\\subseteq ".to_string();
    }
    if s == "superset.eq" {
        return "\\supseteq ".to_string();
    }
    if s == "union" {
        return "\\cup ".to_string();
    }
    if s == "intersect" {
        return "\\cap ".to_string();
    }
    if s == "emptyset" {
        return "\\emptyset ".to_string();
    }
    if s == "forall" {
        return "\\forall ".to_string();
    }
    if s == "exists" {
        return "\\exists ".to_string();
    }
    if s == "not" {
        return "\\neg ".to_string();
    }
    if s == "and" {
        return "\\wedge ".to_string();
    }
    if s == "or" {
        return "\\vee ".to_string();
    }
    if s == "therefore" {
        return "\\therefore ".to_string();
    }
    if s == "because" {
        return "\\because ".to_string();
    }

    if s == "arrow.r" || s == "rightarrow" {
        return "\\rightarrow ".to_string();
    }
    if s == "arrow.l" || s == "leftarrow" {
        return "\\leftarrow ".to_string();
    }
    if s == "arrow.l.r" || s == "leftrightarrow" {
        return "\\leftrightarrow ".to_string();
    }
    if s == "arrow.r.double" || s == "Rightarrow" {
        return "\\Rightarrow ".to_string();
    }
    if s == "arrow.l.double" || s == "Leftarrow" {
        return "\\Leftarrow ".to_string();
    }
    if s == "arrow.l.r.double" || s == "Leftrightarrow" {
        return "\\Leftrightarrow ".to_string();
    }
    if s == "arrow.r.long" {
        return "\\longrightarrow ".to_string();
    }
    if s == "arrow.l.long" {
        return "\\longleftarrow ".to_string();
    }
    if s == "arrow.r.harpoon" {
        return "\\rightharpoonup ".to_string();
    }
    if s == "arrow.l.harpoon" {
        return "\\leftharpoonup ".to_string();
    }
    if s == "arrow.t" || s == "uparrow" {
        return "\\uparrow ".to_string();
    }
    if s == "arrow.b" || s == "downarrow" {
        return "\\downarrow ".to_string();
    }

    if s == "sum" {
        return "\\sum ".to_string();
    }
    if s == "product" {
        return "\\prod ".to_string();
    }
    if s == "integral" {
        return "\\int ".to_string();
    }
    if s == "iint" {
        return "\\iint ".to_string();
    }
    if s == "iiint" {
        return "\\iiint ".to_string();
    }
    if s == "oint" {
        return "\\oint ".to_string();
    }
    if s == "partial" {
        return "\\partial ".to_string();
    }
    if s == "nabla" {
        return "\\nabla ".to_string();
    }
    if s == "series" {
        return "\\sum ".to_string();
    }

    if s == "sqrt" {
        return "\\sqrt".to_string();
    }

    if s.starts_with('"') && s.ends_with('"') {
        return format!("\\text{{{}}}", &s[1..s.len() - 1]);
    }

    if s.starts_with('\'') && s.ends_with('\'') {
        return format!("\\text{{{}}}", &s[1..s.len() - 1]);
    }

    s.to_string()
}

fn split_typst_call_args(inner: &str) -> Option<(String, String)> {
    let mut depth = 0;
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some((inner[..i].to_string(), String::new()));
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                let rest = &inner[i + 1..];
                let rest = rest.strip_suffix(')').unwrap_or(rest);
                return Some((inner[..i].to_string(), rest.to_string()));
            }
            _ => {}
        }
    }
    None
}

fn split_typst_sqrt_args(inner: &str) -> Option<(String, String)> {
    let mut depth = 0;
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                if depth == 0 {
                    return Some((String::new(), inner.to_string()));
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                let deg = inner[..i].trim().to_string();
                let rest = inner[i + 1..].to_string();
                return Some((deg, rest));
            }
            _ => {}
        }
    }
    Some((String::new(), inner.to_string()))
}

fn split_typst_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for ch in s.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction() {
        assert_eq!(parse_typst_to_latex("frac(a, b)"), "\\frac{a}{b}");
    }

    #[test]
    fn sqrt() {
        assert_eq!(parse_typst_to_latex("sqrt(x)"), "\\sqrt{x}");
    }

    #[test]
    fn sqrt_degree() {
        assert_eq!(parse_typst_to_latex("sqrt(3, x)"), "\\sqrt[3]{x}");
    }

    #[test]
    fn binom() {
        assert_eq!(parse_typst_to_latex("binom(n, k)"), "\\binom{n}{k}");
    }

    #[test]
    fn hat() {
        assert_eq!(parse_typst_to_latex("hat(x)"), "\\hat{x}");
    }

    #[test]
    fn dot() {
        assert_eq!(parse_typst_to_latex("dot(x)"), "\\dot{x}");
    }

    #[test]
    fn greek() {
        assert_eq!(parse_typst_to_latex("alpha"), "\\alpha ");
    }

    #[test]
    fn sum() {
        assert_eq!(parse_typst_to_latex("sum"), "\\sum ");
    }

    #[test]
    fn blackboard_bold() {
        assert_eq!(parse_typst_to_latex("bb(ℝ)"), "\\mathbb{ℝ}");
    }

    #[test]
    fn text_string() {
        assert_eq!(parse_typst_to_latex(r#""hello""#), "\\text{hello}");
    }

    #[test]
    fn arrows() {
        assert_eq!(parse_typst_to_latex("arrow.r"), "\\rightarrow ");
    }

    #[test]
    fn infinity() {
        assert_eq!(parse_typst_to_latex("infinity"), "\\infty ");
    }
}
