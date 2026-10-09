use latexsnipper_conversion::{parse_mathml_to_latex, parse_omml_to_latex};

#[test]
fn mathml_preserves_predefined_references_and_text_spaces() {
    let xml = "<math><mtext> a &amp; &lt; &gt; &quot; &apos; b </mtext></math>";
    assert_eq!(
        parse_mathml_to_latex(xml).unwrap(),
        "\\text{ a \\& < > \" ' b }"
    );
}

#[test]
fn omml_preserves_predefined_references_and_text_spaces() {
    let xml = "<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:r><m:t xml:space=\"preserve\"> a &amp; &lt; &gt; &quot; &apos; b </m:t></m:r></m:oMath>";
    assert_eq!(parse_omml_to_latex(xml).unwrap(), " a \\& < > \" ' b ");
}

#[test]
fn numeric_references_preserve_unicode_in_both_formula_readers() {
    let text = "&#20013; &#x1F600;";
    assert_eq!(
        parse_mathml_to_latex(&format!("<math><mtext>{text}</mtext></math>")).unwrap(),
        "\\text{中 😀}"
    );
    assert_eq!(
        parse_omml_to_latex(&format!("<oMath><r><t>{text}</t></r></oMath>")).unwrap(),
        "中 😀"
    );
}

#[test]
fn cdata_is_literal_text_and_is_not_entity_decoded() {
    let text = "<![CDATA[ a &amp; < b ]]>";
    assert_eq!(
        parse_mathml_to_latex(&format!("<math><mtext>{text}</mtext></math>")).unwrap(),
        "\\text{ a \\&amp; < b }"
    );
    assert_eq!(
        parse_omml_to_latex(&format!("<oMath><r><t>{text}</t></r></oMath>")).unwrap(),
        " a \\&amp; < b "
    );
}

#[test]
fn unknown_or_invalid_references_fail_instead_of_disappearing() {
    for reference in [
        "&unknown;",
        "&#xZZ;",
        "&#0;",
        "&#1;",
        "&#xFFFE;",
        "&#xD800;",
        "&#x110000;",
    ] {
        assert!(parse_mathml_to_latex(&format!("<math><mi>{reference}</mi></math>")).is_err());
        assert!(parse_omml_to_latex(&format!("<oMath><r><t>{reference}</t></r></oMath>")).is_err());
    }
}

#[test]
fn omml_layout_readback_preserves_text_but_not_container_indentation() {
    let xml = "<oMath>\n <r>\n <t xml:space=\"preserve\"> a &amp; &#20013; </t>\n </r>\n</oMath>";
    let layout = latexsnipper_conversion::omml_parser::parse_omml_to_layout(xml).unwrap();
    let latexsnipper_ast::FormulaNode::Symbol(symbol) = layout.root else {
        panic!("Expected one text symbol")
    };
    assert_eq!(symbol.latex, " a \\& 中 ");
    for text in ["&unknown;", "&#0;"] {
        assert!(
            latexsnipper_conversion::omml_parser::parse_omml_to_layout(&format!(
                "<oMath><r><t>{text}</t></r></oMath>"
            ))
            .is_err()
        );
    }
}

#[test]
fn numeric_math_symbols_keep_their_existing_operator_mapping() {
    assert_eq!(
        parse_mathml_to_latex("<math><mi>&#945;</mi><mo>&#x2264;</mo><mn>2</mn></math>").unwrap(),
        r"\alpha\leq 2"
    );
}

#[test]
fn indentation_is_not_formula_text_but_cell_text_spaces_survive() {
    let xml = "<math>\n <mtable>\n <mtr>\n <mtd><mtext>a &amp; b</mtext></mtd>\n <mtd><mn>2</mn></mtd>\n </mtr>\n </mtable>\n</math>";
    assert_eq!(
        parse_mathml_to_latex(xml).unwrap(),
        r"\begin{matrix} \text{a \& b} & 2 \end{matrix}"
    );
}
