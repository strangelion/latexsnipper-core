use latexsnipper_conversion::{
    DocumentConverter, FormulaConversionMode, FormulaInputFormat, OutputFormat,
};

#[test]
fn bare_formula_is_explicit_and_legacy_latex_remains_a_document() {
    let mode = FormulaConversionMode::BestEffort;
    let source = "frac(a,b)";
    let fragment =
        DocumentConverter::convert_formula_fragment(source, FormulaInputFormat::Typst, mode)
            .unwrap();
    assert_eq!(fragment, "\\frac{a}{b}");
    let document = DocumentConverter::convert_formula_string(
        source,
        FormulaInputFormat::Typst,
        OutputFormat::Latex,
        mode,
    )
    .unwrap();
    assert!(document.contains("\\documentclass") && document.contains("\\begin{document}"));
    assert!(DocumentConverter::convert_formula_fragment(
        source,
        FormulaInputFormat::Typst,
        FormulaConversionMode::Strict
    )
    .is_err());
    assert_eq!(
        DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::Typst,
            OutputFormat::MarkdownInline,
            mode
        )
        .unwrap(),
        "$\\frac{a}{b}$"
    );
    assert_eq!(
        DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::Typst,
            OutputFormat::MarkdownBlock,
            mode
        )
        .unwrap(),
        "$$\n\\frac{a}{b}\n$$"
    );
}

#[test]
fn fragment_shape_budget_and_comments_do_not_enable_document_splicing() {
    use latexsnipper_conversion::formula_fragment::latex_display_to_fragment;
    for input in [
        "prefix \\[x\\]",
        "\\[x\\]tail",
        "\\[x\\]\\[y\\]",
        "\\[$x$\\]",
        "\\[\\begin % comment\n{document}x\\end{document}\\]",
        "\\[\\begin{align}x&=y\\end{align}\\]",
        "\\[\\begin{doc% comment\nument}x\\end{document}\\]",
        "\\[\\begin{doc ument}x\\end{document}\\]",
        "\\[\\documentclass{article}x\\]",
        "\\[\\verb|x|\\]",
    ] {
        assert!(latex_display_to_fragment(input).is_err(), "{input}");
    }
    assert!(latex_display_to_fragment(&format!("\\[{}\\]", "x".repeat(70 * 1024))).is_err());
    assert_eq!(
        latex_display_to_fragment("\\[x % final comment\n\\]").unwrap(),
        "x % final comment\n"
    );
    assert_eq!(
        latex_display_to_fragment("\\[\\$+\\%+\\begin{aligned}x&=y\\end{aligned}\\]").unwrap(),
        "\\$+\\%+\\begin{aligned}x&=y\\end{aligned}"
    );
}

#[test]
fn already_bare_source_validation_never_rewrites_or_implicitly_strips_a_wrapper() {
    use latexsnipper_conversion::formula_fragment::validate_latex_fragment_source;
    for source in [
        "",
        "  ",
        "% only a comment\n",
        "x % unfinished",
        "x\\",
        "$x$",
        "$$x$$",
        r"\[x\]",
        r"\(x\)",
        r"\documentclass{article}x",
        "\\begin{equ% comment\nation*}x\\end{equation*}",
        r"\begin{doc ument}x",
    ] {
        assert!(validate_latex_fragment_source(source).is_err(), "{source}");
    }
    for source in [
        "  α+1  ",
        "x % retained\n ",
        r"\text{cost \$5}",
        r"\frac{}{}",
        r"\begin{aligned}x&=y\end{aligned}",
    ] {
        assert!(validate_latex_fragment_source(source).is_ok(), "{source}");
    }
    assert!(validate_latex_fragment_source(&"x".repeat(65_536)).is_ok());
    assert!(validate_latex_fragment_source(&"x".repeat(65_537)).is_err());
}
