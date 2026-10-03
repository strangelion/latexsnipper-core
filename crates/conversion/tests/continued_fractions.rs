use latexsnipper_conversion::{
    latex_ast::LatexNode, latex_parser::parse_latex, latex_to_typst::latex_ast_to_typst,
    parse_mathml_to_latex, parse_typst_to_latex, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};

fn assert_well_formed(xml: &str) {
    let mut reader = quick_xml::Reader::from_str(xml);
    while !matches!(
        reader.read_event().expect("Export must be well-formed XML"),
        quick_xml::events::Event::Eof
    ) {}
}

#[test]
fn nested_continued_fractions_preserve_two_operands_per_level() {
    let source = r"\cfrac{1}{1+\cfrac{1}{x}}";
    let typst = latex_ast_to_typst(&parse_latex(source));
    assert_eq!(typst, "display(frac(1, 1 + display(frac(1, x))))");
    let rebuilt = parse_typst_to_latex(&typst);
    // Reconstruction may change insignificant spaces in this numeric fixture.
    assert_eq!(
        latex_ast_to_typst(&parse_latex(&rebuilt)).replace(' ', ""),
        typst.replace(' ', "")
    );
    let mathml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    let rebuilt = parse_mathml_to_latex(&mathml).unwrap();
    let roundtrip =
        DocumentConverter::convert_latex_string(&rebuilt, OutputFormat::MathML).unwrap();
    assert_eq!(roundtrip.matches("<mfrac>").count(), 2);
    assert_eq!(roundtrip.matches("displaystyle=\"true\"").count(), 2);
    for (format, element) in [
        (OutputFormat::MathML, "<mfrac>"),
        (OutputFormat::OMML, "<m:f>"),
    ] {
        let xml = DocumentConverter::convert_latex_string(source, format).unwrap();
        assert_well_formed(&xml);
        assert_eq!(xml.matches(element).count(), 2, "{xml}");
        assert!(!xml.contains("cfrac"), "{xml}");
    }
}

#[test]
fn alignment_is_retained_in_ast_source_and_mathml_not_as_a_third_operand() {
    for (option, expected) in [("l", "left"), ("r", "right")] {
        let source = format!("\\cfrac[{option}]{{a}}{{b}}");
        let ast = parse_latex(&source);
        let LatexNode::Command { name, args } = &ast else {
            panic!("Expected cfrac")
        };
        assert_eq!(name, "cfrac");
        assert_eq!(args.len(), 3);
        assert_eq!(args[2].to_string(), option);
        assert_eq!(ast.to_string(), source);
        assert_eq!(latex_ast_to_typst(&ast), "display(frac(a, b))");
        let xml = DocumentConverter::convert_latex_string(&source, OutputFormat::MathML).unwrap();
        assert_well_formed(&xml);
        assert!(
            xml.contains(&format!("<mfrac numalign=\"{expected}\">")),
            "{xml}"
        );
        let xml = DocumentConverter::convert_latex_string(&source, OutputFormat::OMML).unwrap();
        assert_well_formed(&xml);
        assert_eq!(xml.matches("<m:num>").count(), 1);
        assert_eq!(xml.matches("<m:den>").count(), 1);
    }
}

#[test]
fn scripts_and_siblings_remain_outside_continued_fraction() {
    let source = r"y+\cfrac[l]{a}{\tfrac{b}{c}}^{2}+z";
    let typst = latex_ast_to_typst(&parse_latex(source));
    assert!(
        typst.contains("display(frac(a, inline(frac(b, c))))^(2)"),
        "{typst}"
    );
    assert!(typst.ends_with("+ z"), "{typst}");
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    assert!(xml.contains("<msup>"), "{xml}");
    assert!(xml.contains("numalign=\"left\""));
    assert!(xml.contains("displaystyle=\"false\""));
}

#[test]
fn strict_mode_rejects_unverified_continued_layout_and_malformed_options() {
    for source in [
        r"\cfrac{a}{b}",
        r"\cfrac[l]{a}{b}",
        r"\cfrac[q]{a}{b}",
        r"\cfrac[l{a}{b}",
        r"\cfrac{a}",
    ] {
        assert!(DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::Latex,
            OutputFormat::OMML,
            FormulaConversionMode::Strict
        )
        .is_err());
        assert!(
            DocumentConverter::convert_latex_string(source, OutputFormat::Latex)
                .unwrap()
                .contains(source)
        );
    }
}

#[test]
fn optional_alignment_whitespace_and_untrusted_values_do_not_change_operands() {
    let ast = parse_latex("\\cfrac\n[l]\t{中文}\n{b}");
    assert_eq!(latex_ast_to_typst(&ast), "display(frac(中文, b))");
    let xml = DocumentConverter::convert_latex_string(
        r#"\cfrac[r" mathcolor="red]{a}{b}"#,
        OutputFormat::MathML,
    )
    .unwrap();
    assert!(xml.contains("<mfrac>"), "{xml}");
    assert!(!xml.contains("mathcolor="), "{xml}");
    assert!(!xml.contains("numalign="), "{xml}");
}
