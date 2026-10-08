use latexsnipper_conversion::{DocumentConverter, OutputFormat};

fn xml(source: &str) -> String {
    DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap()
}

#[test]
fn known_commands_in_sequences_remain_separate_glyphs_and_operands() {
    for (command, glyph) in [
        ("alpha", 'α'),
        ("Gamma", 'Γ'),
        ("Delta", 'Δ'),
        ("Theta", 'Θ'),
        ("Lambda", 'Λ'),
        ("mu", 'μ'),
        ("rho", 'ρ'),
        ("varphi", 'φ'),
        ("Omega", 'Ω'),
        ("pm", '±'),
        ("times", '×'),
        ("div", '÷'),
        ("partial", '∂'),
        ("nabla", '∇'),
    ] {
        let result = xml(&format!("p+\\{command} x+q"));
        assert!(result.contains(glyph), "{command}: {result}");
        for operand in ['p', 'x', 'q'] {
            assert!(result.contains(&format!(">{operand}<")), "{result}");
        }
        assert!(
            !result.contains(&format!("\\{command}")),
            "literal macro survived: {result}"
        );
    }
}

#[test]
fn nested_mixed_symbols_keep_fraction_root_and_matrix_structure() {
    let result = xml("p+\\frac{\\alpha x}{\\sqrt[3]{\\beta y}}+\\begin{matrix}\\Gamma z&\\mu t\\\\\\rho u&\\Omega v\\end{matrix}+q");
    assert_eq!(result.matches("<mfrac>").count(), 1);
    assert_eq!(result.matches("<mroot>").count(), 1);
    assert_eq!(result.matches("<mtr>").count(), 2);
    assert_eq!(result.matches("<mtd>").count(), 4);
    for glyph in ['α', 'β', 'Γ', 'μ', 'ρ', 'Ω'] {
        assert!(result.contains(glyph), "{result}");
    }
    for operand in ['p', 'x', 'y', 'z', 't', 'u', 'v', 'q'] {
        assert!(result.contains(&format!(">{operand}<")), "{result}");
    }
}

#[test]
fn unknown_macros_styles_and_large_operator_limits_keep_existing_paths() {
    let unknown = xml("\\alpha x\\mysymbol y");
    assert!(
        unknown.contains("mysymbol") && unknown.contains('y'),
        "{unknown}"
    );
    let sum = xml("\\sum_{i=1}^{n}\\alpha x_i");
    assert!(
        sum.contains("<munderover>") && sum.contains('∑') && sum.contains('α'),
        "{sum}"
    );
    let style = xml("\\textcolor{red}{\\alpha x}+y");
    assert!(
        style.contains("mathcolor=\"red\"") && style.contains('α') && style.contains(">y<"),
        "{style}"
    );
}

#[test]
fn punctuation_decimals_and_function_calls_are_not_one_identifier() {
    let result = xml("\\alpha f(x)+12.5\\pm y");
    assert!(
        result.contains("<mi>f</mi>")
            && result.contains("<mo>(</mo>")
            && result.contains("<mo>)</mo>"),
        "{result}"
    );
    assert!(
        result.contains("<mn>12.5</mn>") && result.contains('±') && result.contains(">y<"),
        "{result}"
    );
}

#[test]
fn mixed_symbol_matrices_and_cases_keep_their_visible_fences() {
    for (env, open, close) in [
        ("pmatrix", "(", ")"),
        ("bmatrix", "[", "]"),
        ("vmatrix", "|", "|"),
    ] {
        let result = xml(&format!("\\begin{{{env}}}\\alpha x&\\beta y\\end{{{env}}}"));
        assert!(
            result.contains(&format!("<mo>{open}</mo>"))
                && result.contains(&format!("<mo>{close}</mo>")),
            "{result}"
        );
        assert_eq!(result.matches("<mtd>").count(), 2);
        assert!(
            result.contains('α')
                && result.contains('β')
                && result.contains(">x<")
                && result.contains(">y<"),
            "{result}"
        );
    }
    let cases = xml("\\begin{cases}\\alpha x&x>0\\\\\\beta y&x<0\\end{cases}");
    assert!(
        cases.contains("<mo>{</mo>") && !cases.contains("<mo>}</mo>"),
        "{cases}"
    );
    assert_eq!(cases.matches("<mtr>").count(), 2);
}
