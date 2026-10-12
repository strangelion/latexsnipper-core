use latexsnipper_conversion::{
    parse_typst_to_latex, DocumentConverter, FormulaConversionMode, FormulaInputFormat,
    OutputFormat,
};

fn fragment(source: &str) -> String {
    DocumentConverter::convert_formula_fragment(
        source,
        FormulaInputFormat::Typst,
        FormulaConversionMode::BestEffort,
    )
    .unwrap()
}

fn render(source: &str, format: OutputFormat) -> String {
    DocumentConverter::convert_latex_string(source, format).unwrap()
}

#[test]
fn fixed_typst_projection_recovers_every_tier_class_and_token() {
    for tier in ["big", "Big", "bigg", "Bigg"] {
        for suffix in ["", "l", "m", "r"] {
            for token in [
                "(",
                ")",
                "[",
                "]",
                r"\{",
                r"\}",
                r"\langle",
                r"\rangle",
                r"\lfloor",
                r"\rfloor",
                r"\lceil",
                r"\rceil",
                r"\vert",
                r"\Vert",
                r"\backslash",
                "/",
                r"\uparrow",
                r"\Downarrow",
                ".",
            ] {
                let source = format!("\\{tier}{suffix}{token}");
                let typst = render(&source, OutputFormat::Typst);
                let restored = fragment(&typst);
                assert_eq!(
                    render(&restored, OutputFormat::MathML),
                    render(&source, OutputFormat::MathML),
                    "{source} -> {typst} -> {restored}"
                );
            }
        }
    }
}

#[test]
fn mixed_fixed_atoms_keep_fraction_scripts_and_following_content() {
    for source in [
        r"\Bigl(\frac{a}{b}\Bigm|x\Bigr)_i^2+z",
        r"\frac{\Bigl[x\Bigr]}{\sqrt[3]{y}}+z",
        r"\bigl(\frac{\Bigl[x\Bigr]}{y}\bigr)+z",
        r"\Bigg._i^2+x",
        r"{\Bigl)x_i}^2+z",
        r"\Bigg\langle x+\Bigg\lfloor x+\Bigg\Vert x",
    ] {
        let typst = render(source, OutputFormat::Typst);
        let restored = fragment(&typst);
        assert_eq!(
            render(&restored, OutputFormat::MathML),
            render(source, OutputFormat::MathML),
            "{source} -> {typst} -> {restored}"
        );
        assert!(!restored.contains("class("), "{restored}");
    }
}

#[test]
fn automatic_fences_and_middles_surround_fixed_atoms_without_changing_scope() {
    for source in [
        r"\left(x\middle|\biggl[\frac{a}{b}\biggr]\right)_i^2+z",
        r"\left\langle\frac{\Bigl[x\Bigr]}{y}\right\rangle+z",
        r"\left.\Bigl(x\Bigr)\right|+z",
        r"\left(x\middle|y\middle\Vert z\right)",
        r"\left(\left[x\right]+y\right)",
    ] {
        let typst = render(source, OutputFormat::Typst);
        let restored = fragment(&typst);
        assert_eq!(
            render(&restored, OutputFormat::MathML),
            render(source, OutputFormat::MathML),
            "{source} -> {typst} -> {restored}"
        );
    }
}

#[test]
fn adjacent_calls_nested_arguments_and_quoted_separators_do_not_swallow_tails() {
    assert_eq!(fragment("frac(a,b)+sqrt(x)+z"), r"\frac{a}{b}+\sqrt{x}+z");
    assert_eq!(
        fragment("frac(frac(a,b),root(3,x))+z"),
        r"\frac{\frac{a}{b}}{\sqrt[3]{x}}+z"
    );
    assert_eq!(
        fragment(r#"frac("a,b",x)+"%_#""#),
        r"\frac{\text{a,b}}{x}+\text{\%\_\#}"
    );
    assert_eq!(
        fragment("binom(frac(a,b),sqrt(x))+z"),
        r"\binom{\frac{a}{b}}{\sqrt{x}}+z"
    );
    let plain = fragment("x_(i)^(2)+z");
    let grouped = fragment("(x_(i))^(2)+z");
    assert!(render(&plain, OutputFormat::MathML).contains("<msubsup>"));
    assert!(!render(&grouped, OutputFormat::MathML).contains("<msubsup>"));
}

#[test]
fn literal_brackets_punctuation_and_upright_keep_operand_boundaries() {
    for source in [
        "[X,Y]",
        r"\nabla_{[X,Y]}Z+z",
        r"\frac{[a,b];}{\mathrm{pH}}+z",
        r"\mathbb{E}[X^2]-\mathbb{E}[X]^2",
    ] {
        let typst = render(source, OutputFormat::Typst);
        let restored = fragment(&typst);
        assert_eq!(
            render(&restored, OutputFormat::MathML),
            render(source, OutputFormat::MathML),
            "{source} -> {typst} -> {restored}"
        );
    }
    assert_eq!(fragment(r"frac(a\,b,c\;d)+z"), r"\frac{a,b}{c;d}+z");
    for source in ["[x", "x]", "[x)+z", "[#read(\"secret\")]", r"frac(a\q,b)"] {
        assert!(DocumentConverter::convert_typst_string(source, OutputFormat::MathML).is_err());
    }
}

#[test]
fn unsupported_projections_are_errors_in_adapters_and_preserved_in_legacy_parser() {
    for source in [
        r#"class("opening", lr(size: #2em, paren.l))"#,
        r#"class("opening", lr(size: #1.2em, unknown))"#,
        r#"class("opening", lr(size: #1.2em, paren.l), extra: true)"#,
        r#"class("mystery", lr(size: #1.2em, paren.l))"#,
        r#"class("normal", #box())"#,
        r#"class("opening", lr(size: #1.2em, #box(width: 1cm)))"#,
        r#"frac(class("opening", lr(size: #2em, paren.l)),x)+z"#,
        "lr(size: #1.2em, paren.l)",
        "mid(|)",
        "vec(a,delim: none)",
        "vec(delim: unknown,a,b)",
        "mat(delim: \"[\",a,b)",
        "sqrt(x,extra: true)",
        "lr(x mid(|) y)",
        "lr((frac(a,mid(|))))",
        "lr((x_(mid(|))))",
        "frac(a,b)+#read(\"secret\")",
        "frac(a,b",
        "frac(a,b))+z",
        "frac(a,b,c)+z",
        "sqrt(x)^",
        "x^-1",
        "x^+y",
        "x_i_j",
        "x^2^3",
        "frac(a,b)+unknown(x)",
        "#let x=1",
        "$x$ $y$",
        "frac(\"unclosed,x)",
    ] {
        assert_eq!(parse_typst_to_latex(source), source, "{source}");
        assert!(
            DocumentConverter::convert_typst_string(source, OutputFormat::MathML).is_err(),
            "{source}"
        );
        assert!(
            DocumentConverter::convert_formula_fragment(
                source,
                FormulaInputFormat::Typst,
                FormulaConversionMode::BestEffort
            )
            .is_err(),
            "{source}"
        );
    }
    for source in [
        "x".repeat(64 * 1024 + 1),
        format!("{}x{}", "sqrt(".repeat(65), ")".repeat(65)),
    ] {
        assert_eq!(parse_typst_to_latex(&source), source);
        assert!(DocumentConverter::convert_typst_string(&source, OutputFormat::MathML).is_err());
    }
}

#[test]
fn extremum_projection_remains_an_operator_with_explicit_script_placement() {
    assert!(fragment("min_(i) x").contains(r"\min"));
    assert!(fragment("max_(i) x").contains(r"\max"));
    assert_eq!(fragment("limits(min)_(i)^(n)"), r"\min\limits_{i}^{n}");
    assert_eq!(fragment("scripts(max)_(i)"), r"\max\nolimits_{i}");
}

#[cfg(feature = "native")]
#[test]
fn unified_import_uses_checked_projection_and_does_not_accept_unknown_code() {
    use latexsnipper_ast::{ImportOptions, InputFormat};
    use latexsnipper_conversion::DocumentImporter;
    let doc = DocumentImporter::from_bytes(
        b"frac(a,b)+z",
        Some(InputFormat::Typst),
        ImportOptions::default(),
    )
    .unwrap();
    assert_eq!(
        DocumentConverter::new(OutputFormat::MathML)
            .convert(&doc)
            .unwrap(),
        render(r"\frac{a}{b}+z", OutputFormat::MathML)
    );
    for source in [
        "#read(\"secret\")",
        r#"class("normal", #box())"#,
        "frac(a,b)+unknown(x)",
    ] {
        assert!(DocumentImporter::from_bytes(
            source.as_bytes(),
            Some(InputFormat::Typst),
            ImportOptions::default()
        )
        .is_err());
    }
}

#[test]
fn cases_keep_branch_count_and_nested_matrices_keep_cell_boundaries() {
    let source = "cases(frac(a,b),sqrt(x))";
    let restored = fragment(source);
    assert_eq!(
        restored,
        r"\begin{cases} \frac{a}{b} \\ \sqrt{x} \end{cases}"
    );
    let source = "mat(frac(a,b),mat(c,d; e,f); x,y)";
    assert_eq!(
        fragment(source),
        r"\begin{matrix} \frac{a}{b} & \begin{matrix} c & d \\ e & f \end{matrix} \\ x & y \end{matrix}"
    );
}

#[cfg(feature = "formula-layout")]
#[test]
fn layout_adapter_rejects_unsupported_typst_instead_of_parsing_it_as_latex() {
    use latexsnipper_ast::FormulaSource;
    use latexsnipper_conversion::parse_formula_source_to_layout;
    let source = FormulaSource::Typst(r#"class("opening", lr(size: #2em, paren.l))"#.into());
    assert!(parse_formula_source_to_layout(&source).is_err());
}
