/// Parse Typst math syntax into LaTeX string.
/// Typst uses `frac(a,b)`, `sqrt(x)`, `mat(a,b; c,d)`, `hat(x)`, etc.
/// This finite projection never executes code. Unsupported shapes retain their
/// original source; fallible conversion/import/layout adapters report errors.
pub fn parse_typst_to_latex(typst: &str) -> String {
    crate::typst_math::parse_checked(typst).unwrap_or_else(|_| typst.trim().to_string())
}

pub(crate) fn symbol_atom(s: &str) -> String {
    if matches!(
        s,
        "min" | "max" | "lim" | "sin" | "cos" | "tan" | "log" | "ln" | "exp" | "det" | "gcd"
    ) {
        return format!("\\{s} ");
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

    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return format!("\\text{{{}}}", &s[1..s.len() - 1]);
    }

    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return format!("\\text{{{}}}", &s[1..s.len() - 1]);
    }

    s.to_string()
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
    fn distinguishes_vector_accent_from_vector_literal() {
        assert_eq!(parse_typst_to_latex("vec(v)"), "\\vec{v}");
        assert_eq!(
            parse_typst_to_latex("vec(a, b)"),
            "\\begin{pmatrix} a \\\\ b \\end{pmatrix}"
        );
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
    fn lone_quote_is_not_sliced_as_a_string() {
        assert_eq!(parse_typst_to_latex("\"  "), "\"");
        assert_eq!(parse_typst_to_latex("'  "), "'");
    }

    #[test]
    fn arrows() {
        assert_eq!(parse_typst_to_latex("arrow(v)"), "\\vec{v}");
        assert_eq!(parse_typst_to_latex("$ arrow(v) $"), "\\vec{v}");
        assert_eq!(parse_typst_to_latex("arrow(B C)"), "\\vec{B C}");
        assert_eq!(parse_typst_to_latex("arrow.r"), "\\rightarrow ");
    }

    #[test]
    fn infinity() {
        assert_eq!(parse_typst_to_latex("infinity"), "\\infty ");
    }
}
