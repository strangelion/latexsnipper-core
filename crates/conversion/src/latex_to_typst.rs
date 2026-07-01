//! Convert LaTeX AST to Typst format.
//! Based on Typst official math documentation.

use crate::latex_ast::LatexNode;

/// Convert a LaTeX AST node to Typst string.
pub fn latex_ast_to_typst(node: &LatexNode) -> String {
    match node {
        LatexNode::Text(s) => s.clone(),
        LatexNode::Sequence(nodes) => {
            // Smart spacing: don't add spaces between certain elements
            let mut result = String::new();
            for (i, n) in nodes.iter().enumerate() {
                let converted = latex_ast_to_typst(n);
                if i > 0 && !converted.is_empty() {
                    // Don't add space before superscript/subscript
                    if !converted.starts_with('^') && !converted.starts_with('_') {
                        // Check if previous ends with space
                        if !result.ends_with(' ') {
                            result.push(' ');
                        }
                    }
                }
                result.push_str(&converted);
            }
            result
        }
        LatexNode::Group(nodes) => {
            nodes.iter().map(|n| latex_ast_to_typst(n)).collect::<Vec<_>>().join(" ")
        }
        LatexNode::Fraction { num, den } => {
            format!("frac({}, {})", latex_ast_to_typst(num), latex_ast_to_typst(den))
        }
        LatexNode::SquareRoot { index, content } => {
            match index {
                Some(idx) => format!("root({}, {})", latex_ast_to_typst(idx), latex_ast_to_typst(content)),
                None => format!("sqrt({})", latex_ast_to_typst(content)),
            }
        }
        LatexNode::Superscript { base, exp } => {
            let base_str = latex_ast_to_typst(base);
            if base_str.is_empty() {
                format!("^({})", latex_ast_to_typst(exp))
            } else {
                format!("{}^({})", base_str, latex_ast_to_typst(exp))
            }
        }
        LatexNode::Subscript { base, sub } => {
            let base_str = latex_ast_to_typst(base);
            if base_str.is_empty() {
                format!("_({})", latex_ast_to_typst(sub))
            } else {
                format!("{}_({})", base_str, latex_ast_to_typst(sub))
            }
        }
        LatexNode::Math { content, display } => {
            let inner = content.iter().map(|n| latex_ast_to_typst(n)).collect::<Vec<_>>().join(" ");
            if *display {
                format!("$ {} $", inner)
            } else {
                format!("${}$", inner)
            }
        }
        LatexNode::Delimited { left, content, right } => {
            let inner = content.iter().map(|n| latex_ast_to_typst(n)).collect::<Vec<_>>().join(" ");
            let typst_left = convert_delimiter(left);
            let typst_right = convert_delimiter(right);
            format!("lr({}{}{})", typst_left, inner, typst_right)
        }
        LatexNode::Operator(op) => convert_operator(op),
        LatexNode::Relation(rel) => convert_relation(rel),
        LatexNode::Greek(g) => convert_greek(g),
        LatexNode::Symbol(s) => convert_symbol(s),
        LatexNode::FontModifier { font, content } => {
            let inner = latex_ast_to_typst(content);
            convert_font_modifier(font, &inner)
        }
        LatexNode::Matrix { env, rows } => {
            let typst_fn = match env.as_str() {
                "pmatrix" | "bmatrix" | "Bmatrix" => "mat",
                "vmatrix" | "Vmatrix" => "mat",
                "array" => "mat",
                "cases" => "cases",
                _ => "mat",
            };
            let cell_rows: Vec<String> = rows.iter().map(|row| {
                let cells: Vec<String> = row.iter().map(|cell| latex_ast_to_typst(cell)).collect();
                cells.join(", ")
            }).collect();
            format!("{}({})", typst_fn, cell_rows.join("; "))
        }
        LatexNode::Cases(rows) => {
            let cases: Vec<String> = rows.iter().map(|row| {
                row.iter().map(|cell| latex_ast_to_typst(cell)).collect::<Vec<_>>().join(" ")
            }).collect();
            format!("cases({})", cases.join(", "))
        }
        LatexNode::Command { name, args } => {
            let arg_str: Vec<String> = args.iter().map(|a| latex_ast_to_typst(a)).collect();
            convert_command(name, &arg_str, args)
        }
    }
}

/// Convert delimiter to Typst syntax.
fn convert_delimiter(s: &str) -> &str {
    match s {
        "(" | ")" => s,
        "[" | "]" => s,
        "{" | "}" => s,
        "|" | "|" => "|",
        "||" => "||",
        "." => ".",  // invisible delimiter
        _ => s,
    }
}

/// Convert LaTeX command to Typst.
fn convert_command(name: &str, arg_str: &[String], args: &[LatexNode]) -> String {
    match name {
        // Text commands
        "text" | "textbf" | "textit" => {
            if let Some(arg) = args.first() {
                let text = latex_ast_to_typst(arg);
                format!("\"{}\"", text)
            } else {
                String::new()
            }
        }
        // Binomial
        "binom" => {
            if arg_str.len() >= 2 {
                format!("binom({}, {})", arg_str[0], arg_str[1])
            } else {
                arg_str.join(", ")
            }
        }
        // Over/under braces
        "overbrace" => {
            if let Some(arg) = args.first() {
                format!("overbrace({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        "underbrace" => {
            if let Some(arg) = args.first() {
                format!("underbrace({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        "overbracket" => {
            if let Some(arg) = args.first() {
                format!("overbracket({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        "underbracket" => {
            if let Some(arg) = args.first() {
                format!("underbracket({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        // Cancel
        "cancel" => {
            if let Some(arg) = args.first() {
                format!("cancel({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        "bcancel" => {
            if let Some(arg) = args.first() {
                format!("cancel({}, inverted: true)", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        "xcancel" => {
            if let Some(arg) = args.first() {
                format!("cancel({}, cross: true)", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        // Phantom
        "phantom" => {
            if let Some(arg) = args.first() {
                format!("phantom({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        // Boxed
        "boxed" => {
            if let Some(arg) = args.first() {
                format!("box({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        // Underline/overline
        "underline" => {
            if let Some(arg) = args.first() {
                format!("underline({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        "overline" => {
            if let Some(arg) = args.first() {
                format!("overline({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        // Overset/underset
        "overset" => {
            if arg_str.len() >= 2 {
                format!("overset({}, {})", arg_str[0], arg_str[1])
            } else {
                arg_str.join(", ")
            }
        }
        "underset" => {
            if arg_str.len() >= 2 {
                format!("underset({}, {})", arg_str[0], arg_str[1])
            } else {
                arg_str.join(", ")
            }
        }
        // sqrt with optional nth root
        "sqrt" => {
            if let Some(arg) = args.first() {
                format!("sqrt({})", latex_ast_to_typst(arg))
            } else {
                String::new()
            }
        }
        // Default: pass through as function call
        _ => {
            if arg_str.is_empty() {
                name.to_string()
            } else {
                format!("{}({})", name, arg_str.join(", "))
            }
        }
    }
}

/// Convert font modifier to Typst.
fn convert_font_modifier(font: &str, inner: &str) -> String {
    match font {
        "mathbb" => format!("bb({})", inner),
        "mathbf" => format!("bold({})", inner),
        "mathit" => format!("italic({})", inner),
        "mathsf" => format!("sans({})", inner),
        "mathtt" => format!("mono({})", inner),
        "mathcal" => format!("cal({})", inner),
        "mathfrak" => format!("frak({})", inner),
        "mathrm" | "mathnormal" => inner.to_string(),
        "bar" | "overline" => format!("overline({})", inner),
        "hat" | "widehat" => format!("hat({})", inner),
        "tilde" | "widetilde" => format!("tilde({})", inner),
        "vec" => format!("vec({})", inner),
        "dot" => format!("dot({})", inner),
        "ddot" => format!("dot.double({})", inner),
        "breve" => format!("breve({})", inner),
        "check" => format!("check({})", inner),
        "acute" => format!("acute({})", inner),
        "grave" => format!("grave({})", inner),
        _ => inner.to_string(),
    }
}

/// Convert LaTeX operator to Typst.
fn convert_operator(op: &str) -> String {
    match op {
        // Integrals
        "int" => "integral".to_string(),
        "iint" => "integral.double".to_string(),
        "iiint" => "integral.triple".to_string(),
        "oint" => "integral.cont".to_string(),
        // Sums and products
        "sum" => "sum".to_string(),
        "prod" => "product".to_string(),
        "coprod" => "product.co".to_string(),
        // Limits
        "lim" => "limit".to_string(),
        "limsup" => "limit.sup".to_string(),
        "liminf" => "limit.inf".to_string(),
        // Extrema
        "max" => "max".to_string(),
        "min" => "min".to_string(),
        "sup" => "sup".to_string(),
        "inf" => "inf".to_string(),
        // Trig functions
        "sin" => "sin".to_string(),
        "cos" => "cos".to_string(),
        "tan" => "tan".to_string(),
        "cot" => "cot".to_string(),
        "sec" => "sec".to_string(),
        "csc" => "csc".to_string(),
        "arcsin" => "arcsin".to_string(),
        "arccos" => "arccos".to_string(),
        "arctan" => "arctan".to_string(),
        "sinh" => "sinh".to_string(),
        "cosh" => "cosh".to_string(),
        "tanh" => "tanh".to_string(),
        "coth" => "coth".to_string(),
        // Logarithms
        "log" => "log".to_string(),
        "ln" => "ln".to_string(),
        "exp" => "exp".to_string(),
        // Other functions
        "det" => "det".to_string(),
        "gcd" => "gcd".to_string(),
        "lcm" => "lcm".to_string(),
        "Pr" => "Pr".to_string(),
        "arg" => "arg".to_string(),
        "dim" => "dim".to_string(),
        "ker" => "ker".to_string(),
        "hom" => "hom".to_string(),
        "deg" => "deg".to_string(),
        "mod" => "mod".to_string(),
        _ => op.to_string(),
    }
}

/// Convert LaTeX relation to Typst.
fn convert_relation(rel: &str) -> String {
    match rel {
        "leq" | "le" => "lt.eq".to_string(),
        "geq" | "ge" => "gt.eq".to_string(),
        "neq" | "ne" => "neq".to_string(),
        "approx" => "approx".to_string(),
        "equiv" => "equiv".to_string(),
        "sim" => "tilde".to_string(),
        "simeq" => "tilde.equiv".to_string(),
        "cong" => "tilde.equiv".to_string(),
        "propto" => "propto".to_string(),
        "ll" => "lt.double".to_string(),
        "gg" => "gt.double".to_string(),
        "lll" => "lt.triple".to_string(),
        "ggg" => "gt.triple".to_string(),
        "prec" => "prec".to_string(),
        "succ" => "succ".to_string(),
        "preceq" => "prec.eq".to_string(),
        "succeq" => "succ.eq".to_string(),
        "lesssim" => "lt.tilde".to_string(),
        "gtrsim" => "gt.tilde".to_string(),
        "lessgtr" => "lt.gt".to_string(),
        "gtrless" => "gt.lt".to_string(),
        "doteq" => "dot.eq".to_string(),
        "asymp" => "bowtie".to_string(),
        "vdash" => "tack.r".to_string(),
        "dashv" => "tack.l".to_string(),
        "Vdash" => "tack.r.double".to_string(),
        "vDash" => "tack.r.double".to_string(),
        "models" => "tack.r.double".to_string(),
        "perp" => "perp".to_string(),
        "parallel" => "parallel".to_string(),
        "nparallel" => "parallel.not".to_string(),
        "mid" => "divides".to_string(),
        "nmid" => "divides.not".to_string(),
        _ => rel.to_string(),
    }
}

/// Convert LaTeX Greek letter to Typst.
fn convert_greek(g: &str) -> String {
    match g {
        // Lowercase
        "alpha" => "alpha".to_string(),
        "beta" => "beta".to_string(),
        "gamma" => "gamma".to_string(),
        "delta" => "delta".to_string(),
        "epsilon" => "epsilon.alt".to_string(),
        "varepsilon" => "epsilon".to_string(),
        "zeta" => "zeta".to_string(),
        "eta" => "eta".to_string(),
        "theta" => "theta".to_string(),
        "vartheta" => "theta.alt".to_string(),
        "iota" => "iota".to_string(),
        "kappa" => "kappa".to_string(),
        "varkappa" => "kappa.alt".to_string(),
        "lambda" => "lambda".to_string(),
        "mu" => "mu".to_string(),
        "nu" => "nu".to_string(),
        "xi" => "xi".to_string(),
        "pi" => "pi".to_string(),
        "varpi" => "pi.alt".to_string(),
        "rho" => "rho".to_string(),
        "varrho" => "rho.alt".to_string(),
        "sigma" => "sigma".to_string(),
        "varsigma" => "sigma.alt".to_string(),
        "tau" => "tau".to_string(),
        "upsilon" => "upsilon".to_string(),
        "phi" => "phi".to_string(),
        "varphi" => "phi.alt".to_string(),
        "chi" => "chi".to_string(),
        "psi" => "psi".to_string(),
        "omega" => "omega".to_string(),
        // Uppercase
        "Gamma" => "Gamma".to_string(),
        "Delta" => "Delta".to_string(),
        "Theta" => "Theta".to_string(),
        "Lambda" => "Lambda".to_string(),
        "Xi" => "Xi".to_string(),
        "Pi" => "Pi".to_string(),
        "Sigma" => "Sigma".to_string(),
        "Upsilon" => "Upsilon".to_string(),
        "Phi" => "Phi".to_string(),
        "Psi" => "Psi".to_string(),
        "Omega" => "Omega".to_string(),
        _ => g.to_string(),
    }
}

/// Convert LaTeX symbol to Typst.
fn convert_symbol(s: &str) -> String {
    match s {
        // Infinity and calculus
        "infty" => "infinity".to_string(),
        "partial" => "partial".to_string(),
        "nabla" => "nabla".to_string(),

        // Logic
        "forall" => "forall".to_string(),
        "exists" => "exists".to_string(),
        "nexists" => "exists.not".to_string(),
        "neg" | "lnot" => "not".to_string(),
        "land" => "and".to_string(),
        "lor" => "or".to_string(),

        // Set theory
        "in" => "in".to_string(),
        "notin" => "in.not".to_string(),
        "ni" => "in.rev".to_string(),
        "subset" => "subset".to_string(),
        "supset" => "supset".to_string(),
        "subseteq" => "subset.eq".to_string(),
        "supseteq" => "supset.eq".to_string(),
        "subsetneq" => "subset.neq".to_string(),
        "supsetneq" => "supset.neq".to_string(),
        "sqsubset" => "subset.sq".to_string(),
        "sqsupset" => "supset.sq".to_string(),
        "sqsubseteq" => "subset.sq.eq".to_string(),
        "sqsupseteq" => "supset.sq.eq".to_string(),
        "cup" => "union".to_string(),
        "cap" => "intersect".to_string(),
        "cupplus" => "union.plus".to_string(),
        "setminus" => "without".to_string(),
        "emptyset" | "varnothing" => "empty".to_string(),

        // Big operators
        "bigcup" => "union.big".to_string(),
        "bigcap" => "intersect.big".to_string(),
        "biguplus" => "union.plus.big".to_string(),
        "bigoplus" => "plus.circle.big".to_string(),
        "bigotimes" => "times.circle.big".to_string(),
        "bigsqcup" => "union.sq.big".to_string(),

        // Arithmetic operators
        "pm" => "plus.minus".to_string(),
        "mp" => "minus.plus".to_string(),
        "times" => "times".to_string(),
        "div" => "div".to_string(),
        "cdot" => "dot".to_string(),
        "ast" => "asterisk".to_string(),
        "star" => "star".to_string(),
        "circ" => "circle".to_string(),
        "bullet" => "bullet".to_string(),
        "diamond" => "diamond".to_string(),
        "oplus" => "plus.circle".to_string(),
        "otimes" => "times.circle".to_string(),
        "odot" => "dot.circle".to_string(),
        "oslash" => "slash.circle".to_string(),
        "boxminus" => "minus.box".to_string(),
        "boxplus" => "plus.box".to_string(),
        "boxtimes" => "times.box".to_string(),

        // Delimiters
        "lfloor" => "floor.l".to_string(),
        "rfloor" => "floor.r".to_string(),
        "lceil" => "ceil.l".to_string(),
        "rceil" => "ceil.r".to_string(),
        "langle" => "angle.l".to_string(),
        "rangle" => "angle.r".to_string(),
        "lvert" => "abs.l".to_string(),
        "rvert" => "abs.r".to_string(),
        "lVert" => "norm.l".to_string(),
        "rVert" => "norm.r".to_string(),
        "lbrace" => "brace.l".to_string(),
        "rbrace" => "brace.r".to_string(),

        // Spacing
        "quad" => "quad".to_string(),
        "qquad" => "qquad".to_string(),
        "," => "thin".to_string(),
        ";" => "hair".to_string(),
        ":" => "med".to_string(),
        "!" => "negthin".to_string(),

        // Dots
        "ldots" | "dots" => "dots.low".to_string(),
        "cdots" => "dots.h".to_string(),
        "vdots" => "dots.v".to_string(),
        "ddots" => "dots.down".to_string(),
        "dotsc" => "dots".to_string(),
        "dotsb" => "dots.h".to_string(),
        "dotsm" => "dots.h".to_string(),

        // Special symbols
        "prime" => "prime".to_string(),
        "dagger" => "dagger".to_string(),
        "ddagger" => "dagger.double".to_string(),
        "copyright" => "copyright".to_string(),
        "pounds" => "pounds".to_string(),
        "yen" => "yen".to_string(),
        "S" => "section.sign".to_string(),
        "P" => "pilcrow".to_string(),
        "hbar" => "planck.reduce".to_string(),
        "ell" => "ell".to_string(),
        "Re" => "Re".to_string(),
        "Im" => "Im".to_string(),
        "aleph" => "alef".to_string(),
        "beth" => "bet".to_string(),
        "gimel" => "gimel".to_string(),

        // Therefore/Because
        "therefore" => "therefore".to_string(),
        "because" => "because".to_string(),

        // Arrows (if used as symbols)
        "rightarrow" | "to" => "arrow.r".to_string(),
        "leftarrow" => "arrow.l".to_string(),
        "leftrightarrow" => "arrow.l.r".to_string(),
        "Rightarrow" => "arrow.r.double".to_string(),
        "Leftarrow" => "arrow.l.double".to_string(),
        "Leftrightarrow" => "arrow.l.r.double".to_string(),
        "uparrow" => "arrow.t".to_string(),
        "downarrow" => "arrow.b".to_string(),
        "updownarrow" => "arrow.t.b".to_string(),
        "Uparrow" => "arrow.t.double".to_string(),
        "Downarrow" => "arrow.b.double".to_string(),
        "nearrow" => "arrow.tr".to_string(),
        "searrow" => "arrow.br".to_string(),
        "swarrow" => "arrow.bl".to_string(),
        "nwarrow" => "arrow.tl".to_string(),
        "mapsto" => "arrow.r.bar".to_string(),
        "hookrightarrow" => "arrow.r.hook".to_string(),
        "hookleftarrow" => "arrow.l.hook".to_string(),
        "rightarrowtail" => "arrow.r.tail".to_string(),
        "leftarrowtail" => "arrow.l.tail".to_string(),
        "twoheadrightarrow" => "arrow.r.twoheaded".to_string(),
        "twoheadleftarrow" => "arrow.l.twoheaded".to_string(),
        "rightleftharpoons" => "harpoons.arrow.r.l".to_string(),
        "leftrightharpoons" => "harpoons.arrow.l.r".to_string(),
        "dashrightarrow" => "arrow.r.dashed".to_string(),
        "dashleftarrow" => "arrow.l.dashed".to_string(),

        _ => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::latex_parser::parse_latex;

    #[test]
    fn test_fraction() {
        let node = parse_latex("\\frac{a}{b}");
        let result = latex_ast_to_typst(&node);
        assert_eq!(result, "frac(a, b)");
    }

    #[test]
    fn test_sqrt() {
        let node = parse_latex("\\sqrt{x}");
        let result = latex_ast_to_typst(&node);
        assert_eq!(result, "sqrt(x)");
    }

    #[test]
    fn test_superscript() {
        let node = parse_latex("x^{2}");
        let result = latex_ast_to_typst(&node);
        assert_eq!(result, "x^(2)");
    }

    #[test]
    fn test_greek() {
        let node = parse_latex("\\alpha");
        let result = latex_ast_to_typst(&node);
        assert_eq!(result, "alpha");
    }

    #[test]
    fn test_complex() {
        let node = parse_latex("\\frac{a}{b} + \\sqrt{c}");
        let result = latex_ast_to_typst(&node);
        assert!(result.contains("frac(a, b)"));
        assert!(result.contains("sqrt(c)"));
    }

    #[test]
    fn test_binom() {
        let node = parse_latex("\\binom{n}{k}");
        let result = latex_ast_to_typst(&node);
        assert_eq!(result, "binom(n, k)");
    }

    #[test]
    fn test_integral() {
        let node = parse_latex("\\int_{0}^{\\infty} e^{-x^2} dx");
        let result = latex_ast_to_typst(&node);
        assert!(result.contains("integral"));
        assert!(result.contains("infinity"));
    }

    #[test]
    fn test_relations() {
        assert_eq!(convert_relation("leq"), "lt.eq");
        assert_eq!(convert_relation("geq"), "gt.eq");
        assert_eq!(convert_relation("neq"), "neq");
        assert_eq!(convert_relation("approx"), "approx");
    }

    #[test]
    fn test_symbols() {
        assert_eq!(convert_symbol("infty"), "infinity");
        assert_eq!(convert_symbol("partial"), "partial");
        assert_eq!(convert_symbol("forall"), "forall");
        assert_eq!(convert_symbol("exists"), "exists");
        assert_eq!(convert_symbol("in"), "in");
        assert_eq!(convert_symbol("subset"), "subset");
        assert_eq!(convert_symbol("cup"), "union");
        assert_eq!(convert_symbol("cap"), "intersect");
    }

    #[test]
    fn test_greek_letters() {
        assert_eq!(convert_greek("alpha"), "alpha");
        assert_eq!(convert_greek("beta"), "beta");
        assert_eq!(convert_greek("Gamma"), "Gamma");
        assert_eq!(convert_greek("Delta"), "Delta");
    }

    #[test]
    fn test_operators() {
        assert_eq!(convert_operator("int"), "integral");
        assert_eq!(convert_operator("sum"), "sum");
        assert_eq!(convert_operator("prod"), "product");
        assert_eq!(convert_operator("lim"), "limit");
        assert_eq!(convert_operator("sin"), "sin");
        assert_eq!(convert_operator("log"), "log");
    }

    #[test]
    fn test_delimiters() {
        assert_eq!(convert_delimiter("("), "(");
        assert_eq!(convert_delimiter(")"), ")");
        assert_eq!(convert_delimiter("["), "[");
        assert_eq!(convert_delimiter("|"), "|");
    }
}
