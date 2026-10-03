use latexsnipper_conversion::{
    latex_ast::LatexNode, latex_parser::parse_latex, latex_to_typst::latex_ast_to_typst,
    parse_mathml_to_latex, parse_typst_to_latex, DocumentConverter, OutputFormat,
};

#[test]
fn styled_fraction_ast_consumes_exactly_two_operands() {
    for command in ["dfrac", "tfrac"] {
        let ast = parse_latex(&format!("\\{command}{{a}}{{b}}"));
        let LatexNode::Command { name, args } = ast else {
            panic!("Expected styled fraction")
        };
        assert_eq!(name, command);
        assert_eq!(args.len(), 2);
        assert_eq!(latex_ast_to_typst(&args[0]), "a");
        assert_eq!(latex_ast_to_typst(&args[1]), "b");
    }
}

#[test]
fn nested_fraction_styles_survive_mathml_and_typst_export() {
    let source = r"\dfrac{a}{\tfrac{b}{c}}";
    let typst = latex_ast_to_typst(&parse_latex(source));
    assert_eq!(typst, "display(frac(a, inline(frac(b, c))))");
    let rebuilt = parse_typst_to_latex(&typst);
    assert!(rebuilt.contains(r"\displaystyle"));
    assert!(rebuilt.contains(r"\textstyle"));
    assert_eq!(rebuilt.matches(r"\frac").count(), 2);
    assert_eq!(latex_ast_to_typst(&parse_latex(&rebuilt)), typst);
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    assert_eq!(xml.matches("<mfrac>").count(), 2);
    assert!(xml.contains("displaystyle=\"true\" scriptlevel=\"0\""));
    assert!(xml.contains("displaystyle=\"false\" scriptlevel=\"0\""));
    let rebuilt = parse_mathml_to_latex(&xml).unwrap();
    assert!(rebuilt.contains(r"\displaystyle"));
    assert!(rebuilt.contains(r"\textstyle"));
    assert_eq!(rebuilt.matches(r"\frac").count(), 2);
    let roundtrip =
        DocumentConverter::convert_latex_string(&rebuilt, OutputFormat::MathML).unwrap();
    assert_eq!(roundtrip.matches("<mfrac>").count(), 2);
    assert!(roundtrip.contains("displaystyle=\"true\""));
    assert!(roundtrip.contains("displaystyle=\"false\""));
}

#[test]
fn best_effort_omml_keeps_fraction_structure_without_claiming_style_fidelity() {
    let xml =
        DocumentConverter::convert_latex_string(r"\dfrac{a}{\tfrac{b}{c}}", OutputFormat::OMML)
            .unwrap();
    assert_eq!(xml.matches("<m:f>").count(), 2);
}

#[test]
fn adjacent_content_and_superscripts_remain_outside_fraction_operands() {
    let source = r"x+\dfrac{a}{b}^{2}+\tfrac{c}{d}+y";
    let typst = latex_ast_to_typst(&parse_latex(source));
    assert!(typst.contains("display(frac(a, b))^(2)"), "{typst}");
    assert!(typst.contains("inline(frac(c, d))"), "{typst}");
    assert!(typst.ends_with("+ y"), "{typst}");
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    assert_eq!(xml.matches("<mfrac>").count(), 2);
    assert!(xml.contains("<msup>"), "{xml}");
}
