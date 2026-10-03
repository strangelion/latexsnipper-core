use latexsnipper_conversion::{
    latex_ast::LatexNode, latex_parser::parse_latex, latex_to_typst::latex_ast_to_typst,
    parse_typst_to_latex, DocumentConverter, FormulaConversionMode, FormulaInputFormat,
    OutputFormat,
};

#[test]
fn substack_rows_remain_inside_sum_lower_limit() {
    let source = r"\sum_{\substack{i=1\\j=2}}^{n} x_{ij}";
    let typst = latex_ast_to_typst(&parse_latex(source));
    assert!(
        typst.contains("_(script(vec(delim: none, i = 1, j = 2)))"),
        "{typst}"
    );
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    assert!(xml.contains("<munderover>"), "{xml}");
    assert_eq!(xml.matches("<mtr>").count(), 2);
    assert!(xml.contains("scriptlevel=\"1\""));
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::OMML).unwrap();
    assert!(xml.contains("<m:nary>"), "{xml}");
    assert!(xml.contains("<m:sub><m:eqArr>"), "{xml}");
    assert_eq!(xml.matches("<m:eqArr>").count(), 1);
}

#[test]
fn nested_stacks_and_relation_commands_do_not_split_outer_rows() {
    let source = r"\substack{i\leq n\\\frac{\substack{a\\b}}{c}}";
    let ast = parse_latex(source);
    let LatexNode::Command { name, args } = &ast else {
        panic!("Expected stack")
    };
    assert_eq!(name, "substack");
    assert_eq!(args.len(), 2);
    assert!(latex_ast_to_typst(&args[0]).contains("lt.eq"));
    let typst = latex_ast_to_typst(&ast);
    assert_eq!(typst.matches("vec(delim: none,").count(), 2);
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    assert_eq!(xml.matches("<mtable").count(), 2);
    assert_eq!(xml.matches("<mtr>").count(), 4);
}

#[test]
fn standalone_stack_roundtrips_typst_and_latex_ast() {
    let source = r"\substack{i=1\\j=2}";
    let ast = parse_latex(source);
    let typst = latex_ast_to_typst(&ast);
    assert_eq!(parse_typst_to_latex(&typst).replace(' ', ""), source);
    assert_eq!(ast.to_string().replace(' ', ""), source);
}

#[test]
fn row_split_keeps_nested_environments_and_empty_rows() {
    let ast = parse_latex(r"\substack{\begin{matrix}a\\b\end{matrix}\\\\c}");
    let LatexNode::Command { args, .. } = ast else {
        panic!("Expected stack")
    };
    assert_eq!(args.len(), 3);
    assert!(matches!(args[0], LatexNode::Matrix { .. }));
    assert!(args[1].is_empty());
}

#[test]
fn whitespace_unicode_and_one_row_stack_keep_operands() {
    for source in ["\\substack\n\t{中文\\\\j=2}", r"\substack{x}"] {
        let ast = parse_latex(source);
        let LatexNode::Command { args, .. } = &ast else {
            panic!("Expected stack")
        };
        assert_eq!(args.len(), if source.contains("中文") { 2 } else { 1 });
        let typst = latex_ast_to_typst(&ast);
        assert!(typst.starts_with("script(vec(delim: none,"));
        let rebuilt = parse_typst_to_latex(&typst);
        assert!(rebuilt.starts_with(r"\substack{"), "{rebuilt}");
    }
}

#[test]
fn typst_stack_reconstruction_does_not_consume_trailing_or_incomplete_source() {
    for source in [
        "vec(delim: none, a, b)+z",
        "vec(delim: none, a, b",
        "vec(delim: none, a, b)^(2)",
    ] {
        assert_eq!(parse_typst_to_latex(source), source);
    }
}

#[test]
fn strict_omml_rejects_unverified_stack_layout_and_preserves_source() {
    for source in [r"\substack{a\\b}", r"\substack{", r"\substack"] {
        assert!(DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::Latex,
            OutputFormat::OMML,
            FormulaConversionMode::Strict,
        )
        .is_err());
        assert!(
            DocumentConverter::convert_latex_string(source, OutputFormat::Latex)
                .unwrap()
                .contains(source)
        );
    }
}
