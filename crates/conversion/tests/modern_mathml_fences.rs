use latexsnipper_conversion::{
    latex_parser::parse_latex,
    omml::{latex_to_omml, validate_omml_latex},
    parse_mathml_to_latex, DocumentConverter, OutputFormat,
};

fn render(source: &str) -> String {
    DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap()
}

fn operator(glyph: &str, form: &str) -> String {
    let separator = if form == "infix" { "true" } else { "false" };
    format!("<mo fence=\"true\" stretchy=\"true\" symmetric=\"true\" form=\"{form}\" separator=\"{separator}\" lspace=\"0em\" rspace=\"0em\">{glyph}</mo>")
}

#[test]
fn modern_fences_roundtrip_nested_scripts_empty_and_asymmetric_sides() {
    for source in [
        r"\left\{x\middle|\frac{a}{b}\right\}",
        r"\left(\left[\frac{a}{b}\right]\middle\Vert c\right)_i^2+z",
        r"\left.\frac{a}{b}\right|",
        r"\left.x\right.",
        r"\left[x\right)",
        r"\left(\middle|\middle|x\middle|\right)",
        r"\left\langle\alpha x\right\rangle+y",
        r"\left(\begin{array}{lr}a&b\\c&d\end{array}\right)",
        r"\left(x\middle|\Biggl[\frac{a}{b}\Biggr]\right)",
    ] {
        let xml = render(source);
        assert!(!xml.contains("mfenced"), "{xml}");
        assert!(xml.contains("form=\"prefix\" separator=\"false\""), "{xml}");
        assert!(
            xml.contains("form=\"postfix\" separator=\"false\""),
            "{xml}"
        );
        let restored = parse_mathml_to_latex(&xml).unwrap();
        assert_eq!(
            latex_to_omml(&restored),
            latex_to_omml(source),
            "{source} -> {restored}"
        );
    }
}

#[test]
fn mixed_middle_glyphs_are_supported_in_mathml_without_widening_omml() {
    for source in [
        r"\left(x\middle|\frac{a}{b}\middle\Vert z\right)",
        r"\left\{x\middle. y\middle\langle z\right\}",
        r"\left(x\middle|\left[y\middle\Vert z\right]\middle\lfloor t\right)",
    ] {
        assert!(validate_omml_latex(source).is_err());
        let xml = render(source);
        assert!(!xml.contains("\\middle"), "{xml}");
        assert!(!xml.contains("mfenced"), "{xml}");
        let restored = parse_mathml_to_latex(&xml).unwrap();
        assert_eq!(
            parse_latex(&restored).to_string(),
            parse_latex(source).to_string(),
            "{restored}"
        );
        assert!(latex_to_omml(source).contains(&parse_latex(source).to_string()));
    }
}

#[test]
fn legacy_mfenced_and_plain_dictionary_operators_remain_readable() {
    let legacy = "<math><mfenced open=\"[\" close=\"]\" separators=\"|\"><mi>x</mi><mfrac><mi>a</mi><mi>b</mi></mfrac></mfenced></math>";
    let source = parse_mathml_to_latex(legacy).unwrap();
    assert_eq!(source, r"\left[x\middle|\frac{a}{b}\right]");
    assert!(!render(&source).contains("mfenced"));
    assert_eq!(
        parse_mathml_to_latex("<math><mrow><mo>(</mo><mi>x</mi><mo>)</mo></mrow></math>").unwrap(),
        "(x)"
    );
    let left = operator("(", "prefix");
    let right = operator(")", "postfix");
    assert_eq!(
        parse_mathml_to_latex(&format!("{left}<mi>x</mi>{right}")).unwrap(),
        r"\left(x\right)"
    );
}

#[test]
fn fence_markers_cannot_cross_rows_operands_or_raw_text() {
    let left = operator("(", "prefix");
    let middle = operator("|", "infix");
    let right = operator(")", "postfix");
    for xml in [
        format!("<math>{left}<mi>x</mi></math>"),
        format!("<math>{right}</math>"),
        format!("<math>{middle}<mi>x</mi></math>"),
        format!("<math><mrow>{left}<mi>x</mi></mrow>{right}</math>"),
        format!("<math><mfrac>{left}<mi>x</mi>{right}</mfrac></math>"),
        format!("<math><msup>{left}<mi>2</mi></msup>{right}</math>"),
        format!("<math><mrow>raw{left}<mi>x</mi>{right}</mrow></math>"),
        format!("<math><mrow>{left}<mi>x</mi>{right}raw</mrow></math>"),
        format!("<math><mrow>{left}<mi>x</mi>{right}"),
        "<math><mi>x</mi></math><mrow>".to_string(),
        "<math><mi>x</mi></math>raw".to_string(),
    ] {
        assert!(parse_mathml_to_latex(&xml).is_err(), "{xml}");
    }
    let xml = format!(
        "<math><mrow>{left}<mi>x</mi>{right}<mo>+</mo>{left}<mi>y</mi>{right}</mrow></math>"
    );
    assert_eq!(
        parse_mathml_to_latex(&xml).unwrap(),
        r"\left(x\right)+\left(y\right)"
    );
}

#[test]
fn unknown_qualified_or_styled_modern_fences_are_not_silently_flattened() {
    let xml = render(r"\left(x\right)");
    for altered in [
        xml.replace("rspace=\"0em\"", "rspace=\"1em\""),
        xml.replace("symmetric=\"true\"", "symmetric=\"false\""),
        xml.replace("form=\"prefix\"", "form=\"unknown\""),
        xml.replace("<mo ", "<mo mathvariant=\"bold\" "),
        xml.replace("<mo ", "<mo xmlns:q=\"urn:fixture\" q:separator=\"false\" "),
        xml.replace(">(</mo>", ">unknown</mo>"),
    ] {
        assert!(parse_mathml_to_latex(&altered).is_err(), "{altered}");
    }
    let empty_left = operator("", "prefix").replace("></mo>", "/>");
    let empty_right = operator("", "postfix").replace("></mo>", "/>");
    assert_eq!(
        parse_mathml_to_latex(&format!("<math>{empty_left}<mi>x</mi>{empty_right}</math>"))
            .unwrap(),
        r"\left.x\right."
    );
}

#[test]
fn matrix_and_delimiter_macro_sides_share_one_parent_row() {
    for source in [
        r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        r"\abs{\frac{a}{b}}",
        r"\norm{x}",
        r"\floor{x}",
        r"\ceil{x}",
    ] {
        let xml = render(source);
        assert!(xml.contains("form=\"prefix\""), "{xml}");
        assert!(xml.contains("form=\"postfix\""), "{xml}");
        let restored = parse_mathml_to_latex(&xml).unwrap();
        validate_omml_latex(&restored).unwrap();
    }
}

#[test]
fn inline_and_prefixed_exports_use_mathml_elements_and_namespaces() {
    use latexsnipper_conversion::{
        mathml::{MathmlAttrConverter, MathmlConverter, MathmlMConverter, MathmlMmlConverter},
        Converter,
    };
    use latexsnipper_syntax::{latex::LatexParser, Parser};
    use quick_xml::{events::Event, name::ResolveResult, NsReader};
    let document = LatexParser
        .parse(r"$\left(x\middle|\frac{a}{b}\right)$")
        .unwrap();
    for output in [
        MathmlConverter.convert(&document).unwrap(),
        MathmlMConverter.convert(&document).unwrap(),
        MathmlMmlConverter.convert(&document).unwrap(),
        MathmlAttrConverter.convert(&document).unwrap(),
    ] {
        assert!(!output.contains("inlinemath"), "{output}");
        assert!(!output.contains("mfenced"), "{output}");
        assert!(output.contains("displaystyle=\"false\""), "{output}");
        let mut reader = NsReader::from_str(&output);
        loop {
            let (namespace, event) = reader.read_resolved_event().unwrap();
            match event {
                Event::Start(_) | Event::Empty(_) => assert!(
                    matches!(namespace, ResolveResult::Bound(value) if value.as_ref() == b"http://www.w3.org/1998/Math/MathML"),
                    "{output}"
                ),
                Event::Eof => break,
                _ => {}
            }
        }
        assert!(parse_mathml_to_latex(&output)
            .unwrap()
            .contains(r"\middle|"));
    }
}
