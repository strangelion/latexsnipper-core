use latexsnipper_conversion::{
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};

#[test]
fn all_directions_and_modes_are_projected_for_both_targets() {
    for target in [
        CapabilityTarget::Native,
        CapabilityTarget::Wasm32UnknownUnknown,
    ] {
        let routes = CapabilityRegistry::formula_conversions(target);
        assert_eq!(
            routes.len(),
            FormulaInputFormat::all().len() * OutputFormat::all().len() * 2
        );
        let exports = CapabilityRegistry::for_target(target);
        for route in routes {
            assert_eq!(route.target, target);
            assert!(exports.iter().any(|entry| entry.format == route.output));
            assert_eq!(route.available, route.unavailable_reason.is_none());
            assert!(!route.limitations.is_empty());
            if route.mode == FormulaConversionMode::Strict && route.available {
                assert_eq!(route.input, FormulaInputFormat::Latex);
                assert_eq!(route.output, "omml");
            }
        }
    }
}

#[test]
fn best_effort_routes_execute_existing_parsers_and_exporters() {
    let samples = [
        (FormulaInputFormat::Latex, r"\frac{a}{b}"),
        (FormulaInputFormat::Mathml, "<math><mfrac><mi>a</mi><mi>b</mi></mfrac></math>"),
        (FormulaInputFormat::Omml, "<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath>"),
        (FormulaInputFormat::Typst, "frac(a, b)"),
        (FormulaInputFormat::Markdown, "A fraction $\\frac{a}{b}$"),
    ];
    for (input, content) in samples {
        for &output in OutputFormat::all() {
            let actual = DocumentConverter::convert_formula_string(
                content,
                input,
                output,
                FormulaConversionMode::BestEffort,
            )
            .unwrap_or_else(|error| panic!("{} -> {}: {error}", input.name(), output.name()));
            let expected = match input {
                FormulaInputFormat::Latex => {
                    DocumentConverter::convert_latex_string(content, output)
                }
                FormulaInputFormat::Mathml => {
                    DocumentConverter::convert_mathml_string(content, output)
                }
                FormulaInputFormat::Omml => DocumentConverter::convert_omml_string(content, output),
                FormulaInputFormat::Typst => {
                    DocumentConverter::convert_typst_string(content, output)
                }
                FormulaInputFormat::Markdown => {
                    DocumentConverter::convert_markdown_string(content, output)
                }
                _ => unreachable!(),
            }
            .unwrap();
            assert!(!actual.is_empty());
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn unavailable_modes_and_inputs_are_rejected_instead_of_relabelled() {
    for &input in FormulaInputFormat::all() {
        for &output in OutputFormat::all() {
            for mode in [
                FormulaConversionMode::Strict,
                FormulaConversionMode::BestEffort,
            ] {
                let route = CapabilityRegistry::formula_conversion(
                    input,
                    output,
                    mode,
                    CapabilityTarget::Native,
                );
                if !route.available {
                    let error = DocumentConverter::convert_formula_string("x", input, output, mode)
                        .unwrap_err();
                    assert!(error
                        .to_string()
                        .contains(route.unavailable_reason.unwrap()));
                }
            }
        }
    }
    for label in [
        "unicode-math",
        "ascii-math",
        "mtef",
        "mathtype",
        "ole",
        "vsto",
    ] {
        assert!(CapabilityRegistry::resolve_export(label).is_none());
    }
}

#[test]
fn strict_latex_omml_rejects_missing_syntax_without_changing_legacy_conversion() {
    let convert = |content| {
        DocumentConverter::convert_formula_string(
            content,
            FormulaInputFormat::Latex,
            OutputFormat::OMML,
            FormulaConversionMode::Strict,
        )
    };
    assert!(convert(r"\frac{a}{b}").unwrap().contains("<m:f>"));
    for source in [
        r"\unknownmacro+x",
        r"\frac{a}",
        r"\begin{unknown}x\end{unknown}",
        r"x^{",
        " ",
    ] {
        assert!(convert(source).is_err(), "{source}");
    }
    assert!(
        DocumentConverter::convert_latex_string(r"\unknownmacro+x", OutputFormat::Latex).is_ok()
    );
}

#[test]
fn serialized_capability_explains_reconstruction_and_strict_limits() {
    let route = CapabilityRegistry::formula_conversion(
        FormulaInputFormat::Mathml,
        OutputFormat::OMML,
        FormulaConversionMode::BestEffort,
        CapabilityTarget::Wasm32UnknownUnknown,
    );
    let json = serde_json::to_value(route).unwrap();
    assert_eq!(json["input"], "mathml");
    assert_eq!(json["output"], "omml");
    assert_eq!(json["mode"], "best-effort");
    assert_eq!(json["path"], "reconstructed-latex");
    assert_eq!(json["target"], "wasm32-unknown-unknown");
    assert_eq!(json["available"], true);
    assert!(json["limitations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "no-complete-round-trip-guarantee"));
}
