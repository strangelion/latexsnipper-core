use latexsnipper_conversion::{DocumentConverter, OutputFormat};

fn xml(source: &str) -> String {
    DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap()
}

#[test]
fn mathml_matrix_export_keeps_prefix_and_suffix_expressions() {
    let result = xml(r"x+\begin{matrix}1&2\\3&4\end{matrix}+y");
    assert!(result.contains("<mi>x</mi>"), "{result}");
    assert!(result.contains("<mi>y</mi>"), "{result}");
    assert_eq!(result.matches("<mtr>").count(), 2);
    assert_eq!(result.matches("<mtd>").count(), 4);
}

#[test]
fn nested_mathml_matrices_keep_both_tables_and_outer_cells() {
    let result = xml(r"\begin{pmatrix}\begin{matrix}1&2\\3&4\end{matrix}&5\end{pmatrix}");
    assert_eq!(result.matches("<mtable>").count(), 2, "{result}");
    assert_eq!(result.matches("<mtr>").count(), 3, "{result}");
    assert_eq!(result.matches("<mtd>").count(), 6, "{result}");
    assert!(result.contains("<mn>5</mn>"));
    assert!(result.contains("<mo>(</mo>") && result.contains("<mo>)</mo>"));
}

#[test]
fn adjacent_and_scripted_matrices_keep_all_operands() {
    let result = xml(r"\begin{matrix}1\end{matrix}^{2}+\begin{matrix}3\end{matrix}_{4}");
    assert_eq!(result.matches("<mtable>").count(), 2, "{result}");
    assert_eq!(result.matches("<msup>").count(), 1, "{result}");
    assert_eq!(result.matches("<msub>").count(), 1, "{result}");
    for operand in ["1", "2", "3", "4"] {
        assert!(result.contains(&format!("<mn>{operand}</mn>")));
    }
}

#[test]
fn all_matrix_fences_are_inside_the_same_mathml_row() {
    for (env, open, close) in [
        ("pmatrix", "(", ")"),
        ("bmatrix", "[", "]"),
        ("Bmatrix", "{", "}"),
        ("vmatrix", "|", "|"),
        ("Vmatrix", "‖", "‖"),
    ] {
        let result = xml(&format!("\\begin{{{env}}}1\\end{{{env}}}"));
        let row = format!("<mrow><mo>{open}</mo><mtable>");
        assert!(result.contains(&row), "{result}");
        assert!(
            result.contains(&format!("</mtable><mo>{close}</mo></mrow>")),
            "{result}"
        );
    }
}

#[test]
fn cases_have_only_a_left_brace_and_retain_surrounding_content() {
    let result = xml(r"f=\begin{cases}1&x>0\\0&x\leq0\end{cases}+z");
    assert_eq!(result.matches("<mtr>").count(), 2);
    assert_eq!(result.matches("<mo>{</mo>").count(), 1);
    assert_eq!(result.matches("<mo>}</mo>").count(), 0);
    assert!(
        result.contains("<mi>f</mi>") && result.contains("<mi>z</mi>"),
        "{result}"
    );
}

#[test]
fn matrix_and_cases_ast_source_serialization_retains_actual_operands() {
    for source in [
        r"\begin{pmatrix}\begin{matrix}1&2\\3&4\end{matrix}&5\end{pmatrix}",
        r"\begin{cases}\frac{1}{2}&x>0\\0&x\leq0\end{cases}",
    ] {
        let ast = latexsnipper_conversion::latex_parser::parse_latex(source);
        let rebuilt = ast.to_string();
        assert!(!rebuilt.contains("..."), "{rebuilt}");
        assert_eq!(rebuilt.replace(' ', ""), source);
        let repeated = latexsnipper_conversion::latex_parser::parse_latex(&rebuilt).to_string();
        assert_eq!(repeated, rebuilt);
    }
}

#[test]
fn fraction_with_nested_matrix_keeps_every_numerator_operand() {
    let result = xml(r"\frac{\begin{matrix}1&2\\3&4\end{matrix}}{5}+z");
    assert_eq!(result.matches("<mfrac>").count(), 1, "{result}");
    assert_eq!(result.matches("<mtr>").count(), 2, "{result}");
    for operand in ["1", "2", "3", "4", "5"] {
        assert!(result.contains(&format!("<mn>{operand}</mn>")), "{result}");
    }
    assert!(result.contains("<mi>z</mi>"));
}

#[test]
fn root_with_a_matrix_does_not_consume_the_following_expression() {
    for (index, tag) in [("", "msqrt"), ("[3]", "mroot")] {
        let result = xml(&format!(
            "\\sqrt{index}{{\\begin{{matrix}}1&2\\\\3&4\\end{{matrix}}}}+z"
        ));
        assert!(result.contains(&format!("<{tag}>")), "{result}");
        assert_eq!(result.matches("<mtr>").count(), 2, "{result}");
        assert_eq!(result.matches("<mtd>").count(), 4, "{result}");
        assert!(result.contains("<mi>z</mi>"), "{result}");
    }
}

#[test]
fn matrix_in_a_script_preserves_the_script_base_and_tail() {
    for (marker, tag) in [("^", "msup"), ("_", "msub")] {
        let result = xml(&format!(
            "x{marker}{{\\begin{{matrix}}1&2\\end{{matrix}}}}+y"
        ));
        assert!(result.contains(&format!("<{tag}>")), "{result}");
        assert!(
            result.contains("<mi>x</mi>") && result.contains("<mi>y</mi>"),
            "{result}"
        );
        assert_eq!(result.matches("<mtd>").count(), 2, "{result}");
    }
}

#[test]
fn matrix_in_a_root_index_retains_radicand_and_tail() {
    let result = xml(r"\sqrt[\begin{matrix}1&2\end{matrix}]{3}+z");
    assert!(result.contains("<mroot>"), "{result}");
    assert_eq!(result.matches("<mtd>").count(), 2, "{result}");
    assert!(
        result.contains("<mn>3</mn>") && result.contains("<mi>z</mi>"),
        "{result}"
    );
}
