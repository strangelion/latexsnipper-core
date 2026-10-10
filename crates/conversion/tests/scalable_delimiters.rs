use latexsnipper_conversion::{
    latex_ast::LatexNode,
    latex_parser::parse_latex,
    omml::{latex_to_omml, validate_omml_latex},
    parse_mathml_to_latex, parse_omml_to_latex, DocumentConverter, OutputFormat,
};
use latexsnipper_syntax::latex::{
    read_scalable_delimiter, scalable_delimiter_glyph, validate_latex_structure,
};

fn render(source: &str, target: OutputFormat) -> String {
    DocumentConverter::convert_latex_string(source, target).unwrap()
}

#[test]
fn common_named_delimiters_are_tokens_not_body_text() {
    for (left, right, open, close) in [
        (r"\langle", r"\rangle", "⟨", "⟩"),
        (r"\{", r"\}", "{", "}"),
        (r"\lfloor", r"\rfloor", "⌊", "⌋"),
        (r"\lceil", r"\rceil", "⌈", "⌉"),
        (r"\lVert", r"\rVert", "‖", "‖"),
        (r"\vert", "|", "|", "|"),
        (r"\backslash", "/", "\\", "/"),
        (r"\uparrow", r"\Downarrow", "↑", "⇓"),
        (".", r"\rbrace", "", "}"),
        (".", ".", "", ""),
        ("[", ")", "[", ")"),
    ] {
        let source = format!("\\left{left} x\\right{right}");
        validate_omml_latex(&source).unwrap();
        let LatexNode::Delimited {
            left: actual_left,
            content,
            right: actual_right,
        } = parse_latex(&source)
        else {
            panic!("Delimiter was not structured: {source}");
        };
        assert_eq!(actual_left, left);
        assert_eq!(actual_right, right);
        assert_eq!(scalable_delimiter_glyph(left), Some(open));
        assert_eq!(scalable_delimiter_glyph(right), Some(close));
        assert!(
            content.iter().all(|node| !node.to_string().contains(left)),
            "{source}"
        );
        let omml = latex_to_omml(&source);
        assert!(
            omml.contains(&format!("<m:begChr m:val=\"{open}\"/>")),
            "{omml}"
        );
        assert!(
            omml.contains(&format!("<m:endChr m:val=\"{close}\"/>")),
            "{omml}"
        );
        let mathml = render(&source, OutputFormat::MathML);
        assert!(
            mathml.contains(&format!("<mfenced open=\"{open}\" close=\"{close}\"")),
            "{mathml}"
        );
    }
}

#[test]
fn nested_delimiters_and_outer_scripts_keep_ownership_and_tail() {
    let source = r"\left\langle\frac{a}{\left[b\right]}\right\rangle_i^2+z";
    validate_omml_latex(source).unwrap();
    let omml = latex_to_omml(source);
    assert_eq!(omml.matches("<m:d>").count(), 2, "{omml}");
    assert!(omml.contains("<m:sSubSup><m:e><m:d>"), "{omml}");
    assert!(omml.contains("<m:den><m:d>"), "{omml}");
    assert!(omml.contains("<m:t>z</m:t>"), "{omml}");
    let mathml = render(source, OutputFormat::MathML);
    assert_eq!(mathml.matches("<mfenced ").count(), 2, "{mathml}");
    assert!(mathml.contains("<msubsup>"), "{mathml}");
    assert!(mathml.contains("<mi>z</mi>"), "{mathml}");
    let typst = render(source, OutputFormat::Typst);
    assert_eq!(typst.matches("lr(").count(), 2, "{typst}");
    assert!(
        typst.contains("chevron.l") && typst.contains("chevron.r"),
        "{typst}"
    );
    let canonical = parse_latex(source).to_string();
    validate_omml_latex(&canonical).unwrap();
    assert_eq!(latex_to_omml(&canonical), omml, "{canonical}");
}

#[test]
fn comments_spaces_and_exact_command_boundaries_cannot_close_the_wrong_pair() {
    for ending in ["\n", "\r\n", "\r"] {
        let source = format!("\\left% FAKE \\right){ending}\\langle x% FAKE \\right){ending}+\\left[y\\right]\\right% FAKE ]{ending}\\rangle+z");
        validate_omml_latex(&source).unwrap();
        let omml = latex_to_omml(&source);
        assert_eq!(omml.matches("<m:d>").count(), 2, "{omml}");
        assert!(!omml.contains("FAKE"));
        let token = read_scalable_delimiter(&format!("% comment{ending}\\langle"), 0)
            .map(|(token, _)| token.to_string());
        assert_eq!(token.as_deref(), Some(r"\langle"));
    }
    assert_eq!(read_scalable_delimiter("中文", 1), None);
    assert!(validate_latex_structure(r"\leftarrow x\rightarrow y").is_empty());
    assert!(validate_omml_latex(r"\left(x\rightward y").is_err());
}

#[test]
fn malformed_or_cross_scope_delimiters_are_not_strictly_replaceable() {
    for source in [
        r"\left",
        r"\left(x",
        r"\right)",
        r"\left{x\right}",
        r"\left\unknown x\right)",
        r"\left( {x\right)}",
        r"{\left(x}\right)",
        r"\begin{matrix}\left(x\end{matrix}\right)",
        r"{\left(x}{y\right)}",
        r"$\left(x$\right)",
        r"\middle|x",
        r"\left(x\middle|y\right)",
    ] {
        assert!(validate_omml_latex(source).is_err(), "Accepted: {source}");
    }
    for source in [r"\left\unknown x\right)", r"\left(x"] {
        assert_eq!(parse_latex(source).to_string(), source);
        assert!(latex_to_omml(source).contains(source), "{source}");
    }
    let source = format!("{}x{}", r"\left(".repeat(130), r"\right)".repeat(130));
    assert!(validate_omml_latex(&source).is_err());
    assert_eq!(parse_latex(&source).to_string(), source);
}

#[test]
fn xml_roundtrip_retains_common_fence_glyphs_and_invisible_sides() {
    for source in [
        r"\left\langle x\right\rangle",
        r"\left\{x\right\}",
        r"\left.\frac{a}{b}\right|",
        r"\left.x\right.",
        r"\left(\left[x\right]+y\right)",
    ] {
        let expected = latex_to_omml(source);
        for restored in [
            parse_omml_to_latex(&expected).unwrap(),
            parse_mathml_to_latex(&render(source, OutputFormat::MathML)).unwrap(),
        ] {
            validate_omml_latex(&restored)
                .unwrap_or_else(|error| panic!("{source} -> {restored}: {error}"));
            assert_eq!(latex_to_omml(&restored), expected, "{source} -> {restored}");
        }
    }
    assert_eq!(
        parse_mathml_to_latex("<math><mfenced open=\"\" close=\"\"/></math>").unwrap(),
        r"\left.\right."
    );
    assert_eq!(
        parse_mathml_to_latex("<math><mfenced><mi>a</mi><mi>b</mi></mfenced></math>").unwrap(),
        r"\left(a,b\right)"
    );
    assert!(
        parse_mathml_to_latex("<math><mfenced open=\"unknown\"><mi>x</mi></mfenced></math>")
            .is_err()
    );
    assert!(parse_mathml_to_latex("<math><mfenced open=\"unknown\"/></math>").is_err());
    assert!(
        parse_mathml_to_latex("<math><mfenced separators=\",;\"><mi>x</mi></mfenced></math>")
            .is_err()
    );
    assert!(parse_mathml_to_latex(
        "<math><mfenced xmlns:q=\"urn:test\" q:open=\"[\"><mi>x</mi></mfenced></math>"
    )
    .is_err());
    assert!(parse_omml_to_latex("<oMath><d><dPr><begChr val=\"(\"/><begChr val=\"[\"/></dPr><e><r><t>x</t></r></e></d></oMath>").is_err());
    for source in [
        "<math><mfenced>x</mfenced></math>",
        "<math><mfenced>x<mi>y</mi></mfenced></math>",
        "<math><mfenced><![CDATA[x]]></mfenced></math>",
    ] {
        assert!(parse_mathml_to_latex(source).is_err());
    }
}

#[test]
fn simultaneous_scripts_differ_from_explicitly_grouped_script_bases() {
    for source in [r"x_i^2", r"x^2_i", r"\left(x\right)_i^2"] {
        let omml = latex_to_omml(source);
        assert_eq!(omml.matches("<m:sSubSup>").count(), 1, "{omml}");
        assert_eq!(latex_to_omml(&parse_latex(source).to_string()), omml);
    }
    for source in [r"{x_i}^2", r"{x^2}_i"] {
        let omml = latex_to_omml(source);
        assert!(!omml.contains("<m:sSubSup>"), "{omml}");
        assert!(
            omml.contains("<m:sSup>") && omml.contains("<m:sSub>"),
            "{omml}"
        );
        assert_eq!(latex_to_omml(&parse_latex(source).to_string()), omml);
    }
}

#[test]
fn math_space_commands_survive_delimiter_canonical_source() {
    let source = r"\left(x\,y\;z\right)";
    let canonical = parse_latex(source).to_string();
    assert!(canonical.contains(r"\,"), "{canonical}");
    assert!(canonical.contains(r"\;"), "{canonical}");
    assert_eq!(latex_to_omml(source), latex_to_omml(&canonical));
    let mathml = render(source, OutputFormat::MathML);
    assert!(mathml.contains("thinmathspace"), "{mathml}");
    assert!(mathml.contains("thickmathspace"), "{mathml}");
    assert!(!mathml.contains("<mo>,</mo>"), "{mathml}");
}
