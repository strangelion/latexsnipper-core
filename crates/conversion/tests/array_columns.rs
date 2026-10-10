use latexsnipper_conversion::{
    latex_ast::LatexNode,
    latex_parser::parse_latex,
    omml::{latex_to_omml, validate_omml_latex},
    parse_mathml_to_latex, parse_omml_to_latex, DocumentConverter, OutputFormat,
};

#[test]
fn array_spec_is_not_a_mathematical_cell_and_source_projection_retains_it() {
    let source = r"\begin{array}{lcr}a&b&c\\d&e&f\end{array}";
    let node = parse_latex(source);
    if let LatexNode::Array { column_spec, rows } = &node {
        assert_eq!(column_spec, "lcr");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][0].to_string(), "a");
    } else {
        panic!("{node:?}");
    }
    assert_eq!(node.to_string(), source);
    validate_omml_latex(source).unwrap();
    let output = latex_to_omml(source);
    for align in ["left", "center", "right"] {
        assert!(
            output.contains(&format!("<m:mcJc m:val=\"{align}\"/>")),
            "{output}"
        );
    }
    assert_eq!(output.matches("<m:e>").count(), 6);
    let restored = parse_omml_to_latex(&format!("<m:oMath xmlns:m='http://schemas.openxmlformats.org/officeDocument/2006/math'>{output}</m:oMath>")).unwrap();
    assert_eq!(parse_latex(&restored).to_string(), source);
}

#[test]
fn repeated_and_nested_arrays_keep_column_layout_and_empty_cells() {
    let source = r"\begin{array}{*{2}{cr}}1&2&3&4\\5&6\end{array}";
    validate_omml_latex(source).unwrap();
    let output = latex_to_omml(source);
    assert_eq!(output.matches("<m:e>").count(), 8);
    assert_eq!(output.matches("m:val=\"right\"").count(), 2);
    assert!(output.contains("<m:e></m:e><m:e></m:e>"));
    let nested = latex_to_omml(
        r"x+\begin{array}{lc}\begin{array}{r}1\\2\end{array}&\frac{a}{b}\end{array}+y",
    );
    assert_eq!(nested.matches("<m:m>").count(), 2);
    for text in ["x", "y", "a", "b"] {
        assert!(nested.contains(&format!("<m:t>{text}</m:t>")));
    }
}

#[test]
fn column_spec_comments_and_expanded_repeat_boundaries_are_lexical() {
    let source = "\\begin{array}% before\n{*{2}{l% fake } closing\nc}}a&b&c&d\\end{array}";
    validate_omml_latex(source).unwrap();
    let output = latex_to_omml(source);
    assert_eq!(output.matches("<m:mc>").count(), 4);
    assert!(!output.contains("fake"));
}

#[test]
fn mathml_array_alignment_and_single_rules_roundtrip_without_column_text() {
    let source = r"\begin{array}{|l|cr|}1&2&3\\4&5&6\end{array}";
    let output = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    assert!(
        output.contains("columnalign=\"left center right\""),
        "{output}"
    );
    assert!(output.contains("columnlines=\"solid none\""), "{output}");
    assert!(output.contains("notation=\"left right\""), "{output}");
    let restored = parse_mathml_to_latex(&output).unwrap();
    assert_eq!(parse_latex(&restored).to_string(), source);
}

#[test]
fn unsupported_layout_and_oversized_specs_fail_strict_and_preserve_source() {
    for source in [
        r"\begin{array}{|c|}x\end{array}",
        r"\begin{array}{p{2cm}}x\end{array}",
        r"\begin{array}{>{\bfseries}l}x\end{array}",
        r"\begin{array}{c}x&y\end{array}",
        r"\begin{array}{*{128}{*{128}{c}}}x\end{array}",
    ] {
        assert!(validate_omml_latex(source).is_err(), "{source}");
        let output = latex_to_omml(source);
        assert!(output.contains("\\begin{array}"), "{output}");
        assert!(!output.contains("<m:m>"), "{output}");
    }
}

#[test]
fn omml_column_groups_decode_attributes_defaults_and_bounds() {
    let wrap = |properties: &str| {
        format!(
        "<m:oMath xmlns:m='http://schemas.openxmlformats.org/officeDocument/2006/math'><m:m><m:mPr><m:mcs>{properties}</m:mcs></m:mPr><m:mr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:mr></m:m></m:oMath>"
    )
    };
    let xml = wrap("<m:mc><m:mcPr><m:count m:val='&#50;'></m:count><m:mcJc m:val='left'></m:mcJc></m:mcPr></m:mc><m:mc><m:mcPr/></m:mc><m:mc/>");
    assert_eq!(
        parse_omml_to_latex(&xml).unwrap(),
        r"\begin{array}{llcc}x\end{array}"
    );
    for properties in [
        "<m:mc><m:mcPr><m:count m:val='0'/></m:mcPr></m:mc>",
        "<m:mc><m:mcPr><m:count m:val='129'/></m:mcPr></m:mc>",
        "<m:mc><m:mcPr><m:mcJc m:val='justify'/></m:mcPr></m:mc>",
        "<m:mc><m:mcPr><m:count m:val='128'/></m:mcPr></m:mc><m:mc/>",
        "<m:mc><m:mcPr><m:count m:val='1' m:val='2'/></m:mcPr></m:mc>",
        "<m:mc><m:mcPr><m:mcJc m:val='left'/><m:mcJc m:val='right'/></m:mcPr></m:mc>",
    ] {
        assert!(
            parse_omml_to_latex(&wrap(properties)).is_err(),
            "{properties}"
        );
    }
}

#[test]
fn mathml_columns_repeat_defaults_and_reject_unknown_layout() {
    let xml = "<math xmlns='http://www.w3.org/1998/Math/MathML'><mtable columnalign='left right'><mtr><mtd><mi>a</mi></mtd><mtd><mi>b</mi></mtd><mtd><mi>c</mi></mtd></mtr></mtable></math>";
    let source = parse_mathml_to_latex(xml).unwrap();
    assert_eq!(
        parse_latex(&source).to_string(),
        r"\begin{array}{lrr}a&b&c\end{array}"
    );
    let source = parse_mathml_to_latex("<math><mtable columnlines='solid'><mtr><mtd><mi>a</mi></mtd><mtd><mi>b</mi></mtd></mtr></mtable></math>").unwrap();
    assert_eq!(
        parse_latex(&source).to_string(),
        r"\begin{array}{c|c}a&b\end{array}"
    );
    let source = parse_mathml_to_latex(&xml.replace("left right", "le&#102;t&#32;right")).unwrap();
    assert_eq!(
        parse_latex(&source).to_string(),
        r"\begin{array}{lrr}a&b&c\end{array}"
    );
    for attrs in [
        "columnalign='decimalpoint'",
        "columnalign='left' columnlines='dashed'",
    ] {
        let source = parse_mathml_to_latex(&format!(
            "<math><mtable {attrs}><mtr><mtd><mi>x</mi></mtd></mtr></mtable></math>"
        ))
        .unwrap();
        assert!(source.contains("unsupportedmathmlcolumns"), "{source}");
        assert!(validate_omml_latex(&source).is_err());
    }
    assert!(parse_mathml_to_latex("<math><mtable columnalign='left' columnalign='right'><mtr><mtd><mi>x</mi></mtd></mtr></mtable></math>").is_err());
}

#[test]
fn omml_outer_delimiters_do_not_erase_explicit_array_columns() {
    let source = r"\left(\begin{array}{lr}a&b\\c&d\end{array}\right)";
    let output = latex_to_omml(source);
    let restored = parse_omml_to_latex(&format!("<m:oMath>{output}</m:oMath>")).unwrap();
    assert!(restored.contains(r"\begin{array}{lr}"), "{restored}");
    assert!(restored.contains(r"\left("), "{restored}");
}

#[test]
#[ignore = "requires an explicitly available local Typst compiler"]
fn exported_arrays_compile_with_local_typst() {
    let compiler =
        std::env::var_os("LATEXSNIPPER_TYPST_TEST_BIN").unwrap_or_else(|| "typst".into());
    let version = std::process::Command::new(&compiler)
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target")
        .join(format!("array-column-typst-{id}"));
    std::fs::create_dir_all(&directory).unwrap();
    let sources = [
        r"\begin{array}{lcr}a&bb&ccc\\dddd&e&f\end{array}",
        r"\begin{array}{|l|cr|}1&2&3\\4&5\end{array}",
        r"x+\begin{array}{lr}\begin{array}{c}1\\2\end{array}&\frac{a}{b}\end{array}+y",
        r"\frac{\begin{array}{lr}a&b\end{array}}{c}",
        r"\begin{array}{>{\bfseries}p{2cm}}x\end{array}",
        r"\begin{array}{lc}\text{total}&\operatorname{rank}A\end{array}",
        r"A^{\overrightarrow{BC}}",
        "\\begin{array}{lc}50\\%&\\text{a\\%b\\&c}\\\\% FAKE & \\end{array}\n\\alpha x&\\frac{a}{b}\\end{array}",
        r"\text{literal \backslash{}frac\%+x\textasciicircum{}y\textasciitilde{}z}",
        r"\left\langle\frac{a}{\left[b\right]}\right\rangle_i^2+z",
        r"\left.\frac{a}{b}\right|",
        r"\left\{\left\lfloor x\right\rfloor\right\}",
        r"\left\lVert x\right\rVert_i^2",
        r"\left\backslash x\right/",
        r"\left\uparrow x\right\Downarrow",
        r"{x_i}^2+x_i^2+\left(x\,y\right)",
        r"\langle x\rangle+\lVert y\rVert+\lvert z\rvert",
    ];
    let output = sources
        .iter()
        .map(|source| DocumentConverter::convert_latex_string(source, OutputFormat::Typst).unwrap())
        .collect::<Vec<_>>()
        .join("\n\n");
    let input = directory.join("arrays.typ");
    let png = directory.join("arrays.png");
    std::fs::write(
        &input,
        format!("#set page(width: 18cm, height: auto, margin: 1cm)\n{output}"),
    )
    .unwrap();
    let compiled = std::process::Command::new(compiler)
        .arg("compile")
        .arg(&input)
        .arg(&png)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}\n{output}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    println!(
        "{}\nRendered evidence: {}",
        String::from_utf8_lossy(&version.stdout).trim(),
        png.display()
    );
}
