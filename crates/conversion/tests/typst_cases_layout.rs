use latexsnipper_conversion::{
    latex_ast::LatexNode, latex_parser::parse_latex, latex_to_typst::latex_ast_to_typst,
    DocumentConverter, FormulaConversionMode, FormulaInputFormat, OutputFormat,
};

fn render(source: &str, format: OutputFormat) -> String {
    DocumentConverter::convert_latex_string(source, format).unwrap()
}

fn read(source: &str) -> String {
    DocumentConverter::convert_formula_fragment(
        source,
        FormulaInputFormat::Typst,
        FormulaConversionMode::BestEffort,
    )
    .unwrap()
}

const SOURCES: &[&str] = &[
    r"\begin{cases}x^2&x>0\\0&x\leq0\end{cases}",
    r"\begin{cases}\frac{1}{x}&x>0&\text{first}\\12345&x\leq0&\text{second}\end{cases}",
    r"\begin{cases}&x>0\\x&\end{cases}",
    r"\begin{cases}x&x>0\\0\end{cases}",
    r"\begin{cases}\begin{cases}x&x>0\\0&x\leq0\end{cases}&x>1\\\frac{a}{b}&\text{otherwise}\end{cases}",
    r"\begin{cases}x_i^2&\text{a,b;c\&d}\\\Bigl(\frac{a}{b}\Bigr)&\text{otherwise}\end{cases}_i^2+z",
    r"\begin{cases}\begin{matrix}a&b\\c&d\end{matrix}&\text{if}\end{cases}",
];

#[test]
fn generated_cases_keep_column_markers_and_mathml_operand_structure() {
    for source in SOURCES {
        let typst = render(source, OutputFormat::Typst);
        assert!(typst.contains("cases(& "), "{typst}");
        assert!(typst.contains(" && quad "), "{typst}");
        let restored = read(&typst);
        assert_eq!(
            render(&restored, OutputFormat::MathML),
            render(source, OutputFormat::MathML),
            "{source} -> {typst} -> {restored}"
        );
    }
}

#[test]
fn legacy_matrix_cases_ast_uses_the_same_projection_as_cases_node() {
    let LatexNode::Cases(rows) = parse_latex(SOURCES[0]) else {
        panic!("cases AST expected")
    };
    assert_eq!(
        latex_ast_to_typst(&LatexNode::Matrix {
            env: "cases".into(),
            rows: rows.clone()
        }),
        latex_ast_to_typst(&LatexNode::Cases(rows))
    );
}

#[test]
fn alignment_split_does_not_split_nested_calls_quotes_or_literal_ampersands() {
    assert_eq!(
        read(r#"cases(& frac(a,b) && quad "a&b", & 0 && quad "else")"#),
        r"\begin{cases} \frac{a}{b} & \text{a\&b} \\ 0 & \text{else} \end{cases}"
    );
    assert_eq!(
        read("cases(x & x>0, 0 & x lt.eq 0)"),
        r"\begin{cases} x & x>0 \\ 0 & x\leq 0 \end{cases}"
    );
    assert_eq!(
        read("cases(& x && quad)"),
        r"\begin{cases} x &  \end{cases}"
    );
    assert_eq!(read("cases(x)"), r"\begin{cases} x \end{cases}");
    assert_eq!(read("cases(x &)"), r"\begin{cases} x &  \end{cases}");
    // Stack/vector formats retain their existing operand grammar.
    assert_eq!(read("vec(delim: none,a,b)+z"), r"\substack{a\\b}+z");
}

#[test]
fn noncanonical_alternating_alignment_and_unsupported_options_are_not_guessed() {
    for source in [
        "cases(& x && y)",
        "cases(& x && quadruple y)",
        "cases(& x & quad y)",
        "cases(x && y)",
        "cases(gap: #1em,x)",
        "cases(reverse: true,x)",
        "cases(& frac(a,b) && quad #read(\"secret\"))",
        "cases(& frac(a,b) && quad mid(|))",
    ] {
        assert!(
            DocumentConverter::convert_typst_string(source, OutputFormat::MathML).is_err(),
            "{source}"
        );
    }
}

#[test]
#[ignore = "requires an explicitly available local Typst compiler and visual QA"]
fn generated_cases_compile_and_preserve_cells_after_readback() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/typst-cases-layout");
    std::fs::create_dir_all(&directory).unwrap();
    let pairs = SOURCES
        .iter()
        .map(|source| {
            let first = render(source, OutputFormat::Typst);
            let second = render(&read(&first), OutputFormat::Typst);
            assert_eq!(first, second, "{source}");
            format!("{first}\n\n{second}")
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let input = directory.join("generated.typ");
    let output = directory.join("generated.png");
    std::fs::write(
        &input,
        format!("#set page(width: 18cm, height: auto, margin: 1cm)\n{pairs}"),
    )
    .unwrap();
    let compiler =
        std::env::var_os("LATEXSNIPPER_TYPST_TEST_BIN").unwrap_or_else(|| "typst".into());
    let result = std::process::Command::new(compiler)
        .arg("compile")
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    println!("Cases evidence: {}", output.display());
}
