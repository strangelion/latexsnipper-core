use latexsnipper_conversion::{
    formula_source_probe::{inspect_formula_xml, ProbeError, SourceFormat, MAX_SOURCE_BYTES},
    svg_formula_source::{inspect_svg_formula_sources as inspect, SvgSourceError as Error, *},
};

fn svg(metadata: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:fs="urn:latexsnipper:formula-source:v1" xmlns:m="http://www.w3.org/1998/Math/MathML" xmlns:o="http://schemas.openxmlformats.org/officeDocument/2006/math"><metadata>{metadata}</metadata><rect width="10" height="10"/></svg>"#
    )
}
fn latex(source: &str) -> String {
    format!("<fs:source format=\"latex\">{source}</fs:source>")
}

#[test]
fn explicit_latex_preserves_whitespace_cdata_unicode_and_original_span() {
    let field = latex("  <![CDATA[\\text{中文}+x^2]]>&amp;y\n");
    let image = svg(&field);
    let report = inspect(&image).unwrap();
    assert_eq!(report.svg, image);
    assert_eq!(report.sources[0].source, "  \\text{中文}+x^2&y\n");
    assert_eq!(&image[report.sources[0].xml_span.clone()], field);
    assert!(!report.sources[0].namespace_materialized);
}

#[test]
fn inherited_namespaces_make_mathml_and_omml_independently_readable() {
    let image = svg(
        r#"<m:math><m:semantics><m:mi>x</m:mi><m:annotation encoding="application/x-tex">x&amp;y</m:annotation></m:semantics></m:math><o:oMath><o:r><o:t>x</o:t></o:r></o:oMath>"#,
    );
    let report = inspect(&image).unwrap();
    assert_eq!(report.sources.len(), 2);
    for source in &report.sources {
        assert!(source.namespace_materialized);
        assert_eq!(
            inspect_formula_xml(&source.source).unwrap().format,
            source.format
        );
    }
    let source = &report.sources[0];
    assert!(!image[source.xml_span.clone()].contains("xmlns"));
    assert_eq!(source.latex_annotations[0].source, "x&y");
    assert!(
        source.source[source.latex_annotations[0].xml_span.clone()].starts_with("<m:annotation")
    );
}

#[test]
fn local_namespace_override_prefix_variants_and_empty_xml_roots_work() {
    let image = r#"<?xml version="1.0" encoding="UTF-8"?><s:svg xmlns:s="http://www.w3.org/2000/svg" xmlns:m="urn:wrong"><s:metadata><math xmlns="http://www.w3.org/1998/Math/MathML"><mi>x</mi></math><m:math xmlns:m="http://www.w3.org/1998/Math/MathML"/></s:metadata></s:svg>"#;
    let report = inspect(image).unwrap();
    assert_eq!(report.sources.len(), 2);
    assert_eq!(
        inspect_formula_xml(&report.sources[0].source)
            .unwrap()
            .format,
        SourceFormat::Mathml
    );
    assert_eq!(
        inspect_formula_xml(&report.sources[1].source)
            .unwrap()
            .format,
        SourceFormat::Mathml
    );
}

#[test]
fn inherited_quoted_namespace_values_do_not_corrupt_materialized_xml() {
    let image = svg("<m:math><m:mi>x</m:mi></m:math>")
        .replace("<svg ", "<svg xmlns:unused='urn:quoted&amp;&quot;&apos;' ");
    let report = inspect(&image).unwrap();
    assert!(report.sources[0]
        .source
        .contains("urn:quoted&amp;&quot;&apos;"));
    inspect_formula_xml(&report.sources[0].source).unwrap();
}

#[test]
fn reports_conflicts_including_annotations_and_preserves_exact_duplicates() {
    let image = svg(&(latex("x") + &latex("x")));
    let report = inspect(&image).unwrap();
    assert_eq!(report.sources.len(), 2);
    assert!(report.conflicting_formats.is_empty());
    let image = svg(&(latex("x")
        + r#"<m:math><m:semantics><m:mi>y</m:mi><m:annotation encoding="application/x-tex">y</m:annotation></m:semantics></m:math>"#));
    assert_eq!(
        inspect(&image).unwrap().conflicting_formats,
        [SourceFormat::Latex]
    );
}

#[test]
fn ignores_descriptions_foreign_fields_nested_objects_and_xmp() {
    let image = svg(r#"<description xmlns="urn:other">\frac12</description><fs:source format="unknown">x</fs:source><m:math xmlns:m="urn:wrong"/><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description>\frac12</rdf:Description></rdf:RDF>"#)
        .replace("<rect", r#"<title>\frac12</title><desc>\sqrt{x}</desc><g><metadata><m:math/></metadata></g><rect"#);
    let report = inspect(&image).unwrap();
    assert!(report.sources.is_empty());
    assert_eq!(report.ignored_metadata_items, 4);
    assert!(inspect(r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#)
        .unwrap()
        .sources
        .is_empty());
}

#[test]
fn rejects_dtd_pi_unknown_entities_and_malformed_xml() {
    for image in [
        "<!DOCTYPE svg SYSTEM 'file:///private'>".to_owned() + &svg(""),
        "<?xml-stylesheet href='https://example.invalid'?>".to_owned() + &svg(""),
        svg(&latex("&unknown;")),
        svg(&latex("&#0;")),
        svg(&latex("\0")),
        svg(&latex("<m:mi>x</m:mi>")),
        svg(&latex("")),
        svg("<unbound:math/>"),
        svg("<m:math>").to_string(),
        svg("<!--bad--comment-->"),
        svg("") + &svg(""),
        " \n<?xml version='1.0'?>".to_owned() + &svg(""),
        "<?xml version='1.1'?>".to_owned() + &svg(""),
        "<?xml version='1.0' encoding='UTF-16'?>".to_owned() + &svg(""),
        svg("<m:math a='x' a='y'/>"),
        svg("<m:math xmlns:a='urn:same' xmlns:b='urn:same' a:x='1' b:x='2'/>"),
    ] {
        assert!(inspect(&image).is_err(), "accepted {image:?}");
    }
    assert_eq!(
        inspect(&svg(&latex("<m:mi/>"))).unwrap_err(),
        Error::InvalidFormula(ProbeError::UnsupportedAnnotationMarkup)
    );
    assert!(inspect("<svg/>").is_err());
}

#[test]
fn every_truncated_prefix_rejects_without_panicking() {
    let image = svg(&latex("x^2"));
    for end in 0..image.len() {
        assert!(inspect(&image[..end]).is_err(), "prefix {end}");
    }
}

#[test]
fn metadata_drawing_names_are_not_imported_as_visible_shapes() {
    let image = svg("<text>hidden metadata</text><rect width='99' height='99'/>");
    let parsed = latexsnipper_conversion::parse_svg(&image);
    assert_eq!(parsed.shapes.len(), 1);
    assert!(parsed.shapes[0].text.is_empty());
    assert_eq!(parsed.shapes[0].geometry.as_ref().unwrap().width, 10.0);
    assert!(parsed.diagnostics.is_empty());
    let parsed = latexsnipper_conversion::parse_svg("<svg><s:metadata xmlns:s='http://www.w3.org/2000/svg'><text>hidden</text></s:metadata><metadata/><text>visible</text></svg>");
    assert_eq!(parsed.shapes.len(), 1);
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn enforces_input_depth_event_field_source_and_namespace_limits() {
    assert_eq!(
        inspect(&"x".repeat(MAX_SVG_BYTES + 1)).unwrap_err(),
        Error::InputLimit
    );
    assert_eq!(
        inspect(&svg(&latex(&"x".repeat(MAX_SOURCE_BYTES + 1)))).unwrap_err(),
        Error::MetadataLimit
    );
    assert_eq!(
        inspect(&svg(&latex("x").repeat(MAX_FIELDS + 1))).unwrap_err(),
        Error::MetadataLimit
    );
    let image = svg("").replace(
        "<rect",
        &format!(
            "{}{}<rect",
            "<g>".repeat(MAX_DEPTH),
            "</g>".repeat(MAX_DEPTH)
        ),
    );
    assert_eq!(inspect(&image).unwrap_err(), Error::DepthLimit);
    let image = svg("").replace("<rect", &("<g/>".repeat(MAX_EVENTS) + "<rect"));
    assert_eq!(inspect(&image).unwrap_err(), Error::EventLimit);
    let image = svg("<m:math/>").replace(
        "<svg ",
        &("<svg ".to_owned()
            + &(0..65)
                .map(|i| format!("xmlns:p{i}='urn:p{i}' "))
                .collect::<String>()),
    );
    assert_eq!(inspect(&image).unwrap_err(), Error::NamespaceLimit);
    let image = svg("<m:math/>")
        .replace(
            "<svg ",
            &("<svg ".to_owned()
                + &(0..58)
                    .map(|i| format!("xmlns:p{i}='urn:p{i}' "))
                    .collect::<String>()),
        )
        .replace(
            "<metadata>",
            &("<metadata ".to_owned()
                + &(0..10)
                    .map(|i| format!("xmlns:q{i}='urn:q{i}' "))
                    .collect::<String>()
                + ">"),
        );
    assert_eq!(inspect(&image).unwrap_err(), Error::NamespaceLimit);
    let image = svg(&format!(
        "<m:math data='{}'/>",
        "x".repeat(MAX_SOURCE_BYTES)
    ));
    assert_eq!(inspect(&image).unwrap_err(), Error::MetadataLimit);
    let image = svg(&latex(&"x".repeat(MAX_SOURCE_BYTES - 100)).repeat(5));
    assert_eq!(inspect(&image).unwrap_err(), Error::MetadataLimit);
}

#[cfg(feature = "native")]
#[test]
fn native_import_keeps_original_svg_and_attaches_hash_bound_candidates() {
    use base64::Engine;
    use latexsnipper_ast::{AssetStorage, ImportOptions};
    use latexsnipper_conversion::importer::DocumentImporter;
    use sha2::{Digest, Sha256};
    let image = svg(&(latex("x") + &latex("y")));
    let document =
        DocumentImporter::from_bytes(image.as_bytes(), None, ImportOptions::default()).unwrap();
    let asset = &document.assets[0];
    match &asset.storage {
        AssetStorage::InlineBase64 { data } => assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap(),
            image.as_bytes()
        ),
        _ => panic!("source asset must stay inline"),
    }
    let metadata = &asset.metadata["formula_source_candidates_v1"];
    assert_eq!(metadata["carrier"], "svg");
    assert_eq!(
        metadata["carrierSha256"],
        format!("{:x}", Sha256::digest(image.as_bytes()))
    );
    assert_eq!(metadata["sources"][1]["source"], "y");
    assert!(document
        .diagnostics
        .iter()
        .any(|d| d.code == "W_SVG_FORMULA_SOURCE_CONFLICT"));
    let rejected = svg(&latex("<m:mi>x</m:mi>"));
    let document =
        DocumentImporter::from_bytes(rejected.as_bytes(), None, ImportOptions::default()).unwrap();
    assert!(!document.assets[0]
        .metadata
        .contains_key("formula_source_candidates_v1"));
    assert!(document
        .diagnostics
        .iter()
        .any(|d| d.code == "W_SVG_FORMULA_SOURCE_REJECTED"));
}
