use latexsnipper_conversion::formula_source_probe::{
    inspect_formula_xml, probe_text_metadata, ProbeError, SourceFormat, TextMetadata,
    MAX_LATEX_ANNOTATIONS, MAX_SOURCE_BYTES, MAX_XML_DEPTH, MAX_XML_EVENTS,
};

const MATH: &str = "http://www.w3.org/1998/Math/MathML";
const OMML: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";

#[test]
fn explicit_text_fields_preserve_source_without_guessing_descriptions() {
    let latex = "  \\frac{a}{b}\n";
    match probe_text_metadata("LaTeX", latex).unwrap().unwrap() {
        TextMetadata::Latex(source) => assert!(std::ptr::eq(source, latex)),
        _ => panic!("expected declared LaTeX"),
    }
    for key in [
        "description",
        "AlternativeText",
        "title",
        "not-latex",
        " LaTeX",
    ] {
        assert!(probe_text_metadata(key, latex).unwrap().is_none());
    }
    assert_eq!(
        probe_text_metadata("latex", " ").unwrap_err(),
        ProbeError::EmptySource
    );
    assert_eq!(
        probe_text_metadata("latex", "x\0").unwrap_err(),
        ProbeError::InvalidText
    );
    assert_eq!(
        probe_text_metadata("latex", &"x".repeat(MAX_SOURCE_BYTES + 1)).unwrap_err(),
        ProbeError::InputLimit
    );
}

#[test]
fn namespaces_and_declared_formats_are_checked_before_handoff() {
    let mathml = format!("<q:math xmlns:q='{MATH}'><q:mi>x</q:mi></q:math>");
    let omml = format!("<q:oMath xmlns:q='{OMML}'><q:r><q:t>x</q:t></q:r></q:oMath>");
    for (xml, format, key) in [
        (&mathml, SourceFormat::Mathml, "mathml"),
        (&omml, SourceFormat::Omml, "omml"),
    ] {
        let report = inspect_formula_xml(xml).unwrap();
        assert_eq!(report.format, format);
        assert_eq!(report.source.as_ptr(), xml.as_ptr());
        assert!(matches!(
            probe_text_metadata(key, xml).unwrap(),
            Some(TextMetadata::Xml(_))
        ));
    }
    assert_eq!(
        probe_text_metadata("omml", &mathml).unwrap_err(),
        ProbeError::FormatMismatch
    );
    for xml in [
        "<math><mi>x</mi></math>",
        "<math xmlns='wrong'/>",
        "<html><math/></html>",
        "<svg/>",
    ] {
        assert_eq!(
            inspect_formula_xml(xml).unwrap_err(),
            ProbeError::UnsupportedRoot
        );
    }
    assert_eq!(
        inspect_formula_xml("<m:math/>").unwrap_err(),
        ProbeError::InvalidXml
    );
}

#[test]
fn mathml_annotations_keep_xml_and_decode_tex_spelling_without_choosing_conflicts() {
    let xml = format!("<math xmlns='{MATH}'><semantics><mi>x</mi><annotation encoding='application/x-tex'>  x&lt;y &amp; z </annotation><annotation encoding='application/x-latex'><![CDATA[\\frac{{a}}{{b}}]]></annotation><annotation encoding='other'>hint</annotation></semantics></math>");
    let report = inspect_formula_xml(&xml).unwrap();
    assert_eq!(report.source, xml);
    assert_eq!(report.latex_annotations.len(), 2);
    assert_eq!(report.latex_annotations[0].source, "  x<y & z ");
    assert_eq!(report.latex_annotations[1].source, "\\frac{a}{b}");
    assert!(xml[report.latex_annotations[0].xml_span.clone()].starts_with("<annotation "));
    assert!(xml[report.latex_annotations[0].xml_span.clone()].ends_with("</annotation>"));
    assert!(report.conflicting_latex_annotations);
    assert_eq!(report.ignored_annotations, 1);
}

#[test]
fn foreign_annotations_and_nested_text_markup_are_not_misreported_as_tex() {
    let shadowed = format!("<math xmlns='{MATH}'><semantics><annotation xmlns='foreign' encoding='application/x-tex'>wrong</annotation></semantics></math>");
    assert!(inspect_formula_xml(&shadowed)
        .unwrap()
        .latex_annotations
        .is_empty());
    let nested = format!("<math xmlns='{MATH}'><semantics><annotation encoding='text/x-tex'><mi>x</mi></annotation></semantics></math>");
    assert_eq!(
        inspect_formula_xml(&nested).unwrap_err(),
        ProbeError::UnsupportedAnnotationMarkup
    );
    let duplicated = format!("<math xmlns='{MATH}'><semantics><annotation encoding='text/x-tex'>x</annotation><annotation encoding='text/x-tex'>x</annotation></semantics></math>");
    assert!(
        !inspect_formula_xml(&duplicated)
            .unwrap()
            .conflicting_latex_annotations
    );
}

#[test]
fn malformed_xml_entities_and_external_processing_are_rejected() {
    for xml in [
        format!("<!DOCTYPE math [<!ENTITY source SYSTEM 'file:///unavailable'>]><math xmlns='{MATH}'>&source;</math>"),
        format!("<?source load?><math xmlns='{MATH}'/>"),
    ] {
        assert_eq!(inspect_formula_xml(&xml).unwrap_err(), ProbeError::ForbiddenXmlConstruct);
    }
    for xml in [
        format!("<math xmlns='{MATH}'>&unknown;</math>"),
        format!("<math xmlns='{MATH}'><mi>x</math>"),
        format!("<math xmlns='{MATH}'/><math xmlns='{MATH}'/>"),
        format!("<math xmlns='{MATH}' xmlns='{MATH}'/>"),
        format!("<math xmlns='{MATH}'><mi a:x='1'/></math>"),
        format!("<?xml version='1.0' encoding='UTF-16'?><math xmlns='{MATH}'/>"),
        format!("<?xml version='1.1'?><math xmlns='{MATH}'/>"),
        format!(" \n<?xml version='1.0'?><math xmlns='{MATH}'/>"),
        format!("\u{a0}<math xmlns='{MATH}'/>"),
        format!("<math xmlns='{MATH}' invalid='&#0;'/>"),
        format!("<math xmlns='{MATH}'><!-- invalid -- comment --></math>"),
        format!("<math xmlns='{MATH}'>&#0;</math>"),
    ] {
        assert_eq!(
            inspect_formula_xml(&xml).unwrap_err(),
            ProbeError::InvalidXml
        );
    }
}

#[test]
fn recognized_source_can_be_handed_to_existing_finite_converters() {
    let mathml = format!("<math xmlns='{MATH}'><mfrac><mi>a</mi><mi>b</mi></mfrac></math>");
    let omml = format!("<m:oMath xmlns:m='{OMML}'><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>");
    let math = inspect_formula_xml(&mathml).unwrap();
    let office = inspect_formula_xml(&omml).unwrap();
    assert_eq!(
        latexsnipper_conversion::parse_mathml_to_latex(math.source).unwrap(),
        "\\frac{a}{b}"
    );
    assert_eq!(
        latexsnipper_conversion::parse_omml_to_latex(office.source).unwrap(),
        "\\frac{a}{b}"
    );
    let utf8 = format!("<?xml version='1.0' encoding='utf-8'?><math xmlns='{MATH}'/>");
    assert!(inspect_formula_xml(&utf8).is_ok());
}

#[test]
fn xml_work_and_annotation_budgets_are_bounded() {
    let nested = |depth: usize| {
        format!(
            "<math xmlns='{MATH}'>{}{}</math>",
            "<mrow>".repeat(depth - 1),
            "</mrow>".repeat(depth - 1)
        )
    };
    assert!(inspect_formula_xml(&nested(MAX_XML_DEPTH)).is_ok());
    assert_eq!(
        inspect_formula_xml(&nested(MAX_XML_DEPTH + 1)).unwrap_err(),
        ProbeError::DepthLimit
    );
    let many = format!(
        "<math xmlns='{MATH}'>{}</math>",
        "<mi/>".repeat(MAX_XML_EVENTS)
    );
    assert_eq!(
        inspect_formula_xml(&many).unwrap_err(),
        ProbeError::EventLimit
    );
    let annotations = |count| {
        format!(
            "<math xmlns='{MATH}'><semantics>{}</semantics></math>",
            "<annotation encoding='text/x-tex'>x</annotation>".repeat(count)
        )
    };
    assert_eq!(
        inspect_formula_xml(&annotations(MAX_LATEX_ANNOTATIONS))
            .unwrap()
            .latex_annotations
            .len(),
        MAX_LATEX_ANNOTATIONS
    );
    assert_eq!(
        inspect_formula_xml(&annotations(MAX_LATEX_ANNOTATIONS + 1)).unwrap_err(),
        ProbeError::AnnotationLimit
    );
    let empty = format!(
        "<math xmlns='{MATH}'><semantics>{}</semantics></math>",
        "<annotation encoding='text/x-tex'/>".repeat(MAX_LATEX_ANNOTATIONS + 1)
    );
    assert_eq!(
        inspect_formula_xml(&empty).unwrap_err(),
        ProbeError::AnnotationLimit
    );
    assert_eq!(
        inspect_formula_xml(&" ".repeat(MAX_SOURCE_BYTES + 1)).unwrap_err(),
        ProbeError::InputLimit
    );
}
