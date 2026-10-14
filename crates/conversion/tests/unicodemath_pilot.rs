use latexsnipper_conversion::{
    asciimath_pilot::parse_asciimath,
    latex_ast::LatexNode,
    unicodemath_pilot::{parse_unicodemath, write_unicodemath, UnicodeMathErrorKind},
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};

fn canonical(source: &str) -> String {
    let ast = parse_unicodemath(source).unwrap_or_else(|error| panic!("{source:?}: {error}"));
    write_unicodemath(&ast).unwrap_or_else(|error| panic!("{source:?}: {error}"))
}

#[test]
fn placeholder_masks_preserve_empty_slots_without_inventing_a_limit_value() {
    use latexsnipper_conversion::FormulaOutputTarget;
    for (source, expected) in [
        ("∑8▒x", "∑8▒〖x〗"),
        ("∫16▒x", "∫16▒〖x〗"),
        ("∫24▒x", "∫24▒〖x〗"),
        ("∫9▒x", "∫9▒〖x〗"),
        ("∑26▒x", "∑26▒〖x〗"),
        ("∑8_i▒x", "〖∑〗_(i)▒〖x〗"),
        ("∑24_i▒x", "〖∑16〗_(i)▒〖x〗"),
        ("〖∑_i〗_()▒x", "〖∑〗_(i)▒〖x〗"),
        ("∫_()^()▒x", "∫24▒〖x〗"),
    ] {
        assert_eq!(canonical(source), expected, "{source}");
        assert_eq!(canonical(expected), expected, "{source}");
        for (output, input) in [
            (OutputFormat::OMML, FormulaInputFormat::Omml),
            (OutputFormat::MathML, FormulaInputFormat::Mathml),
        ] {
            let xml = DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::UnicodeMath,
                output,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            let restored = DocumentConverter::convert_formula_to_target(
                &xml,
                input,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert_eq!(restored, expected, "{source}: {xml}");
            assert!(
                !xml.contains("<m:t>0</m:t>") && !xml.contains("<mn>0</mn>"),
                "{xml}"
            );
        }
        let display = DocumentConverter::convert_formula_string(
            expected,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::LatexDisplay,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        let bare =
            latexsnipper_conversion::formula_fragment::latex_display_to_fragment(&display).unwrap();
        assert_eq!(
            DocumentConverter::convert_formula_to_target(
                &bare,
                FormulaInputFormat::Latex,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .unwrap(),
            expected
        );
    }
    let xml = DocumentConverter::convert_formula_string(
        "∫24▒x",
        FormulaInputFormat::UnicodeMath,
        OutputFormat::OMML,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    assert!(xml.contains("<m:subHide m:val=\"0\"/>") && xml.contains("<m:supHide m:val=\"0\"/>"));
    assert!(xml.contains("<m:sub></m:sub>") && xml.contains("<m:sup></m:sup>"));
    for source in ["∫88▒x", "∫89▒x", "∫90▒x", "■(∫72▒x&∑82▒y)"] {
        let expected = canonical(if source == "∫90▒x" {
            "∫88▒x"
        } else {
            source
        });
        let xml = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(
            DocumentConverter::convert_formula_to_target(
                &xml,
                FormulaInputFormat::Omml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .unwrap(),
            expected,
            "{xml}"
        );
    }
}

#[test]
fn finite_placement_placeholder_and_growth_masks_compose_without_budget_bypass() {
    for placement in [0, 1, 2] {
        for placeholders in [0, 8, 16, 24] {
            for growth in [0, 64, 128] {
                let flag = placement + placeholders + growth;
                let lower = if placeholders == 0 { "_i" } else { "" };
                let source = format!("∫{flag}{lower}▒x");
                assert_eq!(canonical(&canonical(&source)), canonical(&source));
                let xml = DocumentConverter::convert_formula_string(
                    &source,
                    FormulaInputFormat::UnicodeMath,
                    OutputFormat::OMML,
                    FormulaConversionMode::BestEffort,
                )
                .unwrap();
                assert!(
                    xml.contains(if growth == 64 {
                        "<m:grow m:val=\"0\"/>"
                    } else {
                        "<m:grow m:val=\"1\"/>"
                    }),
                    "{source}: {xml}"
                );
                if placeholders & 8 != 0 {
                    assert!(xml.contains("<m:subHide m:val=\"0\"/>"), "{xml}");
                }
                if placeholders & 16 != 0 {
                    assert!(xml.contains("<m:supHide m:val=\"0\"/>"), "{xml}");
                }
            }
        }
    }
    let deep = format!("{}x", "∫88▒".repeat(80));
    assert_eq!(
        parse_unicodemath(&deep).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
}

#[test]
fn native_empty_limit_visibility_is_not_ignored_or_reinterpreted_as_a_value() {
    use latexsnipper_conversion::FormulaOutputTarget;
    let prefix = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:naryPr><m:chr m:val="∫"/><m:grow m:val="1"/>"#;
    let suffix =
        r#"</m:naryPr><m:sub/><m:sup/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#;
    for (properties, expected) in [
        ("", "∫24▒x"),
        (r#"<m:subHide m:val="1"/><m:supHide m:val="true"/>"#, "∫▒x"),
        (r#"<m:subHide m:val="0"/><m:supHide/>"#, "∫8▒x"),
        (r#"<m:subHide/><m:supHide m:val="false"/>"#, "∫16▒x"),
        (
            r#"<m:subHide m:val="false"></m:subHide><m:supHide/>"#,
            "∫8▒x",
        ),
    ] {
        let xml = format!("{prefix}{properties}{suffix}");
        assert_eq!(
            DocumentConverter::convert_formula_to_target(
                &xml,
                FormulaInputFormat::Omml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .unwrap(),
            canonical(expected),
            "{xml}"
        );
    }
    for properties in [
        r#"<m:subHide m:val="maybe"/>"#,
        r#"<m:supHide m:val=""/>"#,
        r#"<m:subHide m:val="0"/><m:subHide/>"#,
        "<m:supHide>0</m:supHide>",
        r#"<m:subHide m:val="0"><m:r/></m:subHide>"#,
    ] {
        assert!(
            DocumentConverter::convert_formula_to_target(
                &format!("{prefix}{properties}{suffix}"),
                FormulaInputFormat::Omml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .is_err(),
            "{properties}"
        );
    }
    for (slot, hide) in [("sub", "subHide"), ("sup", "supHide")] {
        let xml = format!("{prefix}<m:{hide} m:val=\"1\"/></m:naryPr><m:{slot}><m:r><m:t>k</m:t></m:r></m:{slot}><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>");
        let error = DocumentConverter::convert_formula_to_target(
            &xml,
            FormulaInputFormat::Omml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("Hidden nonempty"), "{error}");
        assert!(DocumentConverter::convert_formula_string(
            &xml,
            FormulaInputFormat::Omml,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort
        )
        .is_err());
    }
    for source in ["x_()", "sin_() x", "∫▒()", "∫_()x", "〖∑_i〗_j▒x"] {
        assert!(parse_unicodemath(source).is_err(), "{source}");
    }
}

#[test]
fn growth_masks_use_structured_omml_without_fabricating_a_tex_macro() {
    use latexsnipper_conversion::FormulaOutputTarget;
    assert_eq!(canonical("∫64"), "∫64");
    assert_eq!(canonical("∫65_0▒x"), "〖∫65〗_(0)▒〖x〗");
    let diagnostic = parse_unicodemath("∫64▒x").unwrap().to_string();
    assert!(diagnostic.contains("requires structured output"));
    assert!(!diagnostic.contains("LatexSnipperInternal"));
    for source in [
        "∫64_0^1▒x+b",
        "∫65_0^1▒x+b",
        "∑66_i^n▒x+b",
        "∫64▒x",
        "∫64",
        "∫64▒∑66_i^n▒x",
        "■(∫64▒x&∑66_i^n▒y)",
        "(∫64▒x)/y",
        "√(∫64▒x)",
    ] {
        let written = canonical(source);
        assert_eq!(canonical(&written), written);
        let xml = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(xml.contains("<m:grow m:val=\"0\"/>"), "{xml}");
        assert!(!xml.contains("LatexSnipperInternal"));
        let restored = DocumentConverter::convert_formula_to_target(
            &xml,
            FormulaInputFormat::Omml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(restored, written, "{source}: {xml}");
        let native_roundtrip = DocumentConverter::convert_formula_string(
            &xml,
            FormulaInputFormat::Omml,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(
            native_roundtrip.contains("<m:grow m:val=\"0\"/>"),
            "{native_roundtrip}"
        );
        for output in [OutputFormat::LatexDisplay, OutputFormat::Typst] {
            let error = DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::UnicodeMath,
                output,
                FormulaConversionMode::BestEffort,
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains("growth"), "{error}");
        }
        assert!(DocumentConverter::convert_formula_to_target(
            source,
            FormulaInputFormat::UnicodeMath,
            FormulaOutputTarget::AsciiMath,
            FormulaConversionMode::BestEffort
        )
        .is_err());
    }
    for (source, normalized) in [
        ("∫128_0^1▒x", "∫_0^1▒x"),
        ("∫129_0^1▒x", "∫1_0^1▒x"),
        ("∑130_i^n▒x", "∑2_i^n▒x"),
    ] {
        assert_eq!(canonical(&canonical(source)), canonical(source));
        let xml = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(xml.contains("<m:grow m:val=\"1\"/>"));
        assert_eq!(
            DocumentConverter::convert_formula_to_target(
                &xml,
                FormulaInputFormat::Omml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .unwrap(),
            canonical(normalized)
        );
    }
    for source in [
        "∫192▒x",
        "∑194_i▒x",
        "∑96_i▒x",
        "∑160_i▒x",
        "∑131_i▒x",
        "∑256_i▒x",
        "∫999999999999▒x",
    ] {
        assert_eq!(
            parse_unicodemath(source).unwrap_err().kind,
            UnicodeMathErrorKind::UnsupportedSyntax
        );
    }
}

#[test]
fn mathml_growth_roundtrip_preserves_style_slots_and_owned_operands() {
    use latexsnipper_conversion::FormulaOutputTarget;
    for glyph in ['∫', '∬', '∭', '∮', '∑', '∏', '∐', '⋃', '⋂'] {
        for growth in [64, 128] {
            let source = format!("{glyph}{growth}_0^1▒x");
            let xml = DocumentConverter::convert_formula_string(
                &source,
                FormulaInputFormat::UnicodeMath,
                OutputFormat::MathML,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert_eq!(
                DocumentConverter::convert_formula_to_target(
                    &xml,
                    FormulaInputFormat::Mathml,
                    FormulaOutputTarget::UnicodeMath,
                    FormulaConversionMode::BestEffort
                )
                .unwrap(),
                canonical(&source)
            );
        }
    }
    for source in [
        "∫64",
        "∫128",
        "∫64_0^1▒x+b",
        "∑130_i^n▒x+b",
        "∫89▒x",
        "∑154▒x",
        "∫64▒∑130_i^n▒x",
        "■(∫64▒x&∑130_i^n▒y)",
        "(∫64▒x)/y",
        "√(∫128▒x)",
    ] {
        let xml = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::MathML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(xml.contains("stretchy="), "{xml}");
        assert!(!xml.contains("LatexSnipperInternal"), "{xml}");
        let restored = DocumentConverter::convert_formula_to_target(
            &xml,
            FormulaInputFormat::Mathml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(restored, canonical(source), "{source}: {xml}");
        let rebuilt = DocumentConverter::convert_formula_string(
            &xml,
            FormulaInputFormat::Mathml,
            OutputFormat::MathML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(
            DocumentConverter::convert_formula_to_target(
                &rebuilt,
                FormulaInputFormat::Mathml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .unwrap(),
            canonical(source),
            "{source}"
        );
        let omml = DocumentConverter::convert_formula_string(
            &xml,
            FormulaInputFormat::Mathml,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(omml.contains("<m:grow m:val="), "{omml}");
        for target in [OutputFormat::Latex, OutputFormat::Typst] {
            assert!(DocumentConverter::convert_formula_string(
                &xml,
                FormulaInputFormat::Mathml,
                target,
                FormulaConversionMode::BestEffort
            )
            .unwrap_err()
            .to_string()
            .contains("growth"));
        }
    }
    for placement in [0, 1, 2] {
        for placeholders in [0, 8, 16, 24] {
            for growth in [64, 128] {
                let flag = placement + placeholders + growth;
                let source = if placeholders == 0 {
                    format!("∫{flag}_0^1▒x")
                } else {
                    format!("∫{flag}▒x")
                };
                let xml = DocumentConverter::convert_formula_string(
                    &source,
                    FormulaInputFormat::UnicodeMath,
                    OutputFormat::MathML,
                    FormulaConversionMode::BestEffort,
                )
                .unwrap();
                assert!(
                    xml.contains(if growth == 64 {
                        "stretchy=\"false\""
                    } else {
                        "stretchy=\"true\""
                    }),
                    "{xml}"
                );
                let restored = DocumentConverter::convert_formula_to_target(
                    &xml,
                    FormulaInputFormat::Mathml,
                    FormulaOutputTarget::UnicodeMath,
                    FormulaConversionMode::BestEffort,
                )
                .unwrap();
                // Same-as-default integral placement has no distinct XML encoding.
                let expected = if placement == 2 {
                    source.replacen(&format!("∫{flag}"), &format!("∫{}", flag - 2), 1)
                } else {
                    source.clone()
                };
                assert_eq!(restored, canonical(&expected), "{source}: {xml}");
            }
        }
    }
}

#[test]
fn mathml_growth_rejects_unsupported_properties_and_reserved_bridge_text() {
    use latexsnipper_conversion::FormulaOutputTarget;
    for xml in [
        format!(
            "<math>{}<mo stretchy=\"true\">∫</mo>{}</math>",
            "<mrow>".repeat(65),
            "</mrow>".repeat(65)
        ),
        format!(
            "<math><mtext>{}</mtext><mo stretchy=\"true\">∫</mo></math>",
            "x".repeat(65_536)
        ),
    ] {
        assert!(DocumentConverter::convert_formula_to_target(
            &xml,
            FormulaInputFormat::Mathml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort
        )
        .is_err());
    }
    for operator in [
        "<mo stretchy=\"yes\">∫</mo>",
        "<mo stretchy=\"\">∫</mo>",
        "<mo stretchy=\"true\" largeop=\"false\">∫</mo>",
        "<mo stretchy=\"true\" movablelimits=\"yes\">∫</mo>",
        "<mo stretchy=\"true\" largeop=\"true\">⨁</mo>",
        "<mo stretchy=\"true\" largeop=\"true\">∫∫</mo>",
        "<mo stretchy=\"true\" style=\"height:3em\">∫</mo>",
        "<mo stretchy=\"true\" minsize=\"2em\">∫</mo>",
        "<mo stretchy=\"true\" stretchy=\"false\">∫</mo>",
        "<mo xmlns:a=\"urn:test\" a:stretchy=\"true\">∫</mo>",
        "<mo stretchy=\"true\"><mo>∫</mo></mo>",
        "<mtext>LatexSnipperInternalNaryGrow</mtext>",
        "<mtext>LatexSnipperInternalNaryNoGrow</mtext>",
        "<mtext>LatexSnipperInternalNary&#x47;row</mtext>",
        "<mtext><![CDATA[LatexSnipperInternalNaryGrow]]></mtext>",
        "<mstyle mathcolor=\"LatexSnipperInternalNaryGrow\"><mi>x</mi></mstyle>",
    ] {
        let xml = format!("<math xmlns=\"http://www.w3.org/1998/Math/MathML\">{operator}</math>");
        assert!(
            DocumentConverter::convert_formula_to_target(
                &xml,
                FormulaInputFormat::Mathml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .is_err(),
            "{xml}"
        );
    }
    for stretchy in ["true", "false"] {
        let xml = format!("<math><mo stretchy=\"{stretchy}\">∫</mo></math>");
        let restored = DocumentConverter::convert_formula_to_target(
            &xml,
            FormulaInputFormat::Mathml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(
            restored,
            if stretchy == "true" {
                "∫128"
            } else {
                "∫64"
            }
        );
    }
}

#[test]
fn structured_cross_xml_growth_and_html_preserve_modes_and_never_need_source_scripts() {
    use latexsnipper_conversion::FormulaOutputTarget;
    for (source, growing) in [
        ("∫65_0^1▒x+b", false),
        ("∑130_i^n▒x+b", true),
        ("∫89▒x", false),
        ("∑154▒x", true),
        ("∫64▒∑130_i^n▒Foo⁡x", false),
        ("■(∫64▒x&∑130_i^n▒y)", false),
        ("(∫128▒x)/y", true),
        ("√(∫64▒x)", false),
    ] {
        let omml = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        let mathml = DocumentConverter::convert_formula_string(
            &omml,
            FormulaInputFormat::Omml,
            OutputFormat::MathML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(
            mathml.contains(if growing {
                "stretchy=\"true\""
            } else {
                "stretchy=\"false\""
            }),
            "{source}: {mathml}"
        );
        let restored = DocumentConverter::convert_formula_to_target(
            &mathml,
            FormulaInputFormat::Mathml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        let rebased = DocumentConverter::convert_formula_string(
            &mathml,
            FormulaInputFormat::Mathml,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        let rebased_mathml = DocumentConverter::convert_formula_string(
            &rebased,
            FormulaInputFormat::Omml,
            OutputFormat::MathML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(
            DocumentConverter::convert_formula_to_target(
                &rebased,
                FormulaInputFormat::Omml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .unwrap(),
            DocumentConverter::convert_formula_to_target(
                &omml,
                FormulaInputFormat::Omml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .unwrap()
        );
        let original_growing = mathml.matches("stretchy=\"true\"").count();
        let original_fixed = mathml.matches("stretchy=\"false\"").count();
        let rebased_growing = rebased_mathml.matches("stretchy=\"true\"").count();
        let rebased_fixed = rebased_mathml.matches("stretchy=\"false\"").count();
        assert_eq!(
            rebased_growing, original_growing,
            "{source}: {rebased_mathml}"
        );
        assert_eq!(rebased_fixed, original_fixed, "{source}: {rebased_mathml}");
        assert!(!restored.contains("LatexSnipperInternal"), "{restored}");
        for (input, content) in [
            (FormulaInputFormat::UnicodeMath, source),
            (FormulaInputFormat::Omml, omml.as_str()),
            (FormulaInputFormat::Mathml, mathml.as_str()),
        ] {
            let html = DocumentConverter::convert_formula_string(
                content,
                input,
                OutputFormat::Html,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert!(html.contains("<math xmlns=") && html.contains("stretchy="));
            assert!(
                !html.contains("<script")
                    && !html.contains("MathJax")
                    && !html.contains("LatexSnipperInternal")
            );
            let start = html.find("<math ").unwrap();
            let end = html.find("</math>").unwrap() + "</math>".len();
            assert!(DocumentConverter::convert_formula_to_target(
                &html[start..end],
                FormulaInputFormat::Mathml,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .is_ok());
        }
    }
    for property in ["<m:grow/>", "<m:grow></m:grow>", "<m:grow m:val=\"true\"/>"] {
        let xml = format!("<m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:nary><m:naryPr><m:chr m:val=\"∫\"/>{property}</m:naryPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>");
        let mathml = DocumentConverter::convert_formula_string(
            &xml,
            FormulaInputFormat::Omml,
            OutputFormat::MathML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(mathml.contains("stretchy=\"true\""));
    }
}

#[test]
fn mathml_explicit_function_application_accepts_bounded_ascii_identifier_runs() {
    let source = r"\operatorname{ind}T=\dim\ker T-\dim\operatorname{coker}T";
    let mathml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    assert!(mathml.contains("<mi mathvariant=\"normal\">ind</mi>"));
    assert!(mathml.contains("<mi mathvariant=\"normal\">coker</mi>"));
    for target in [
        OutputFormat::Latex,
        OutputFormat::Typst,
        OutputFormat::OMML,
        OutputFormat::Html,
    ] {
        let output = DocumentConverter::convert_formula_string(
            &mathml,
            FormulaInputFormat::Mathml,
            target,
            FormulaConversionMode::BestEffort,
        )
        .unwrap_or_else(|error| panic!("{target:?}: {error}"));
        assert!(!output.trim().is_empty(), "{target:?}");
    }
}

#[test]
fn structured_xml_scope_refuses_partial_document_or_equation_extraction() {
    use latexsnipper_conversion::FormulaOutputTarget;
    let equation = "<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>";
    for xml in [
        format!("<w:document>{equation}</w:document>"),
        format!("<m:oMathPara>{equation}{equation}</m:oMathPara>"),
        format!("{equation}{equation}"),
        format!("prose{equation}"),
        format!("{equation}prose"),
        "<m:oMathPara/>".into(),
        format!("<m:oMath>{equation}</m:oMath>"),
    ] {
        for target in [OutputFormat::MathML, OutputFormat::OMML, OutputFormat::Html] {
            assert!(
                DocumentConverter::convert_formula_string(
                    &xml,
                    FormulaInputFormat::Omml,
                    target,
                    FormulaConversionMode::BestEffort
                )
                .is_err(),
                "{xml}"
            );
        }
        assert!(DocumentConverter::convert_formula_to_target(
            &xml,
            FormulaInputFormat::Omml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort
        )
        .is_err());
    }
    for xml in [
        "<math><mi>x</mi></math><math><mi>y</mi></math>",
        "<html><math><mi>x</mi></math></html>",
        "prose<math><mi>x</mi></math>",
        "<math><math><mi>x</mi></math></math>",
    ] {
        assert!(DocumentConverter::convert_formula_to_target(
            xml,
            FormulaInputFormat::Mathml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort
        )
        .is_err());
    }
}

#[test]
fn omml_growth_defaults_presence_and_invalid_properties_are_distinct() {
    use latexsnipper_conversion::FormulaOutputTarget;
    let prefix = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:naryPr><m:chr m:val="∫"/>"#;
    let suffix = r#"</m:naryPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#;
    for (property, expected) in [
        ("", "∫64▒x"),
        (r#"<m:grow m:val="0"/>"#, "∫64▒x"),
        (r#"<m:grow m:val="false"></m:grow>"#, "∫64▒x"),
        ("<m:grow/>", "∫▒x"),
        ("<m:grow></m:grow>", "∫▒x"),
        (r#"<m:grow m:val="true"/>"#, "∫▒x"),
    ] {
        let xml = format!("{prefix}{property}{suffix}");
        let restored = DocumentConverter::convert_formula_to_target(
            &xml,
            FormulaInputFormat::Omml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(restored, canonical(expected), "{xml}");
    }
    for property in [
        r#"<m:grow m:val="maybe"/>"#,
        r#"<m:grow m:val=""/>"#,
        r#"<m:grow m:val=""></m:grow>"#,
        r#"<m:grow m:val="0"><m:r/></m:grow>"#,
        "<m:grow>0</m:grow>",
        r#"<m:grow m:val="0" m:val="1"/>"#,
        r#"<m:grow m:val="0"/><m:grow m:val="1"/>"#,
    ] {
        assert!(DocumentConverter::convert_formula_to_target(
            &format!("{prefix}{property}{suffix}"),
            FormulaInputFormat::Omml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort
        )
        .is_err());
    }
    let missing_properties = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#;
    assert_eq!(
        DocumentConverter::convert_formula_to_target(
            missing_properties,
            FormulaInputFormat::Omml,
            FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort
        )
        .unwrap(),
        canonical("∫64▒x")
    );
    let forged = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>\LatexSnipperInternalNaryNoGrow{\int}</m:t></m:r></m:oMath>"#;
    assert!(DocumentConverter::convert_formula_to_target(
        forged,
        FormulaInputFormat::Omml,
        FormulaOutputTarget::UnicodeMath,
        FormulaConversionMode::BestEffort
    )
    .is_err());
}

#[test]
fn explicit_nary_placement_preserves_limits_body_and_xml_layout() {
    for (source, mode, tag, modifier) in [
        ("∫1_0^1▒x+b", "undOvr", "munderover", "\\limits"),
        ("∑2_i^n▒x+b", "subSup", "msubsup", "\\nolimits"),
        ("∬1_0▒x+b", "undOvr", "munder", "\\limits"),
        ("∏2^n▒x+b", "subSup", "msup", "\\nolimits"),
    ] {
        let canonical_source = canonical(source);
        assert_eq!(canonical(&canonical_source), canonical_source);
        let latex = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::LatexDisplay,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        let latex =
            latexsnipper_conversion::formula_fragment::latex_display_to_fragment(&latex).unwrap();
        assert!(latex.contains(modifier), "{source}: {latex}");
        assert!(!latex.contains("\\limits}"), "{latex}");
        assert!(!latex.contains("\\nolimits}"), "{latex}");
        let restored = DocumentConverter::convert_formula_to_target(
            &latex,
            FormulaInputFormat::Latex,
            latexsnipper_conversion::FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(restored, canonical_source, "{latex}");
        let display = DocumentConverter::convert_formula_string(
            &canonical_source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::LatexDisplay,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        let canonical_latex =
            latexsnipper_conversion::formula_fragment::latex_display_to_fragment(&display).unwrap();
        assert!(!canonical_latex.contains("\\limits}"), "{canonical_latex}");
        assert!(
            !canonical_latex.contains("\\nolimits}"),
            "{canonical_latex}"
        );
        assert_eq!(
            DocumentConverter::convert_formula_to_target(
                &canonical_latex,
                FormulaInputFormat::Latex,
                latexsnipper_conversion::FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .unwrap(),
            canonical_source
        );
        for (output, input) in [
            (OutputFormat::OMML, FormulaInputFormat::Omml),
            (OutputFormat::MathML, FormulaInputFormat::Mathml),
        ] {
            let xml = DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::UnicodeMath,
                output,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            if output == OutputFormat::OMML {
                assert!(
                    xml.contains(&format!("<m:limLoc m:val=\"{mode}\"/>")),
                    "{xml}"
                );
                let end = xml.find("</m:nary>").unwrap();
                assert!(xml[end..].contains("<m:t>b</m:t>"), "{xml}");
            } else {
                assert!(xml.contains(&format!("<{tag}>")), "{xml}");
                assert!(xml.contains("movablelimits=\"false\""), "{xml}");
            }
            let restored = DocumentConverter::convert_formula_to_target(
                &xml,
                input,
                latexsnipper_conversion::FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert_eq!(restored, canonical_source, "{source}: {xml}");
        }
    }
    assert_eq!(canonical("∫0_0^1▒x"), canonical("∫_0^1▒x"));
    for glyph in ['∫', '∬', '∭', '∮', '∑', '∏', '∐', '⋃', '⋂'] {
        for (flag, mode, tag) in [('1', "undOvr", "munderover"), ('2', "subSup", "msubsup")] {
            let source = format!("{glyph}{flag}_0^1▒x");
            assert_eq!(canonical(&canonical(&source)), canonical(&source));
            let omml = DocumentConverter::convert_formula_string(
                &source,
                FormulaInputFormat::UnicodeMath,
                OutputFormat::OMML,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert!(
                omml.contains(&format!("<m:limLoc m:val=\"{mode}\"/>")),
                "{source}: {omml}"
            );
            let mathml = DocumentConverter::convert_formula_string(
                &source,
                FormulaInputFormat::UnicodeMath,
                OutputFormat::MathML,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert!(mathml.contains(&format!("<{tag}>")), "{source}: {mathml}");
        }
    }
    for source in [
        "∫3_0▒x",
        "∫4_0▒x",
        "∫32_0▒x",
        "∫48_0▒x",
        "∫192_0▒x",
        "∫256_0▒x",
        "∫12_0▒x",
        "∫1▒x",
        "∫2▒x",
        "∫999999_0▒x",
    ] {
        assert!(parse_unicodemath(source).is_err(), "{source}");
    }
}

#[test]
fn omml_nary_placement_properties_are_bounded_and_not_guessed() {
    let prefix = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:naryPr><m:chr m:val="∫"/>"#;
    let suffix = r#"</m:naryPr><m:sub><m:r><m:t>0</m:t></m:r></m:sub><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#;
    for properties in [
        r#"<m:limLoc m:val="undOvr"/>"#,
        r#"<m:limLoc m:val="undOvr"></m:limLoc>"#,
    ] {
        let output = DocumentConverter::convert_formula_to_target(
            &format!("{prefix}{properties}{suffix}"),
            FormulaInputFormat::Omml,
            latexsnipper_conversion::FormulaOutputTarget::UnicodeMath,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert_eq!(output, canonical("∫65_0▒x"));
    }
    for properties in [
        r#"<m:limLoc/>"#,
        r#"<m:limLoc m:val="other"/>"#,
        r#"<m:limLoc m:val="undOvr"/><m:limLoc m:val="subSup"/>"#,
    ] {
        assert!(
            DocumentConverter::convert_formula_to_target(
                &format!("{prefix}{properties}{suffix}"),
                FormulaInputFormat::Omml,
                latexsnipper_conversion::FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort
            )
            .is_err(),
            "{properties}"
        );
    }
}

#[test]
fn nary_heads_limits_and_owned_bodies_have_fixed_canonical_structure() {
    for (source, expected) in [
        ("∫_0^1 x", "〖〖∫〗_(0)〗^(1)▒〖x〗"),
        ("∑_(i=1)^n x_i", "〖〖∑〗_(i = 1)〗^(n)▒〖〖x〗_(i)〗"),
        ("∫▒(x+y)z", "∫▒〖(x + y)〗 z"),
        ("∫▒〖(x+y)z〗", "∫▒〖(x + y) z〗"),
        ("∫▒xⅆx/(x^2+1)", "∫▒〖(x ⅆ x)/(〖x〗^(2) + 1)〗"),
        ("∑▒∏▒x", "∑▒〖∏▒〖x〗〗"),
        ("sin▒x", "sin⁡〖x〗"),
        ("∫", "∫"),
        ("∫▒2", "∫▒〖2〗"),
    ] {
        assert_eq!(canonical(source), expected, "{source}");
        assert_eq!(canonical(expected), expected, "{source}");
    }
    for glyph in ['∫', '∬', '∭', '∮', '∑', '∏', '∐', '⋃', '⋂'] {
        let source = format!("{glyph}▒x");
        assert_eq!(canonical(&source), format!("{glyph}▒〖x〗"));
        for output in [OutputFormat::MathML, OutputFormat::OMML] {
            let xml = DocumentConverter::convert_formula_string(
                &source,
                FormulaInputFormat::UnicodeMath,
                output,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert!(xml.contains(glyph), "{source}: {xml}");
            assert!(!xml.contains('\\'), "{source}: {xml}");
        }
    }
    let LatexNode::Sequence(nodes) = parse_unicodemath("∫▒(x+y)z").unwrap() else {
        panic!("scope")
    };
    assert_eq!(nodes.len(), 2);
    let LatexNode::Group(call) = &nodes[0] else {
        panic!("call")
    };
    let LatexNode::Group(body) = &call[1] else {
        panic!("owned body")
    };
    assert!(matches!(&body[0], LatexNode::Delimited { .. }));
    assert!(matches!(&nodes[1], LatexNode::Text(text) if text == "z"));
}

#[test]
fn nary_ownership_requires_a_boundary_and_xml_keeps_an_empty_body_owned() {
    assert!(parse_unicodemath("∫64b").is_err());
    for source in ["∫64 b", "∫64▒b"] {
        assert!(parse_unicodemath(source).is_ok(), "{source}");
    }

    let xml = r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:naryPr><m:chr m:val="∫"/><m:grow m:val="1"/></m:naryPr></m:nary><m:r><m:t>+</m:t></m:r><m:r><m:t>b</m:t></m:r></m:oMath>"#;
    let mathml = DocumentConverter::convert_formula_string(
        xml,
        FormulaInputFormat::Omml,
        OutputFormat::MathML,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    assert!(mathml.contains("stretchy=\"true\""), "{mathml}");
    let restored = DocumentConverter::convert_formula_to_target(
        &mathml,
        FormulaInputFormat::Mathml,
        latexsnipper_conversion::FormulaOutputTarget::UnicodeMath,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    assert!(
        restored.contains("∫") && restored.contains("+ b"),
        "{restored}"
    );
    assert!(!restored.contains("▒+"), "{restored}");
}

#[test]
fn nary_projection_keeps_integrand_inside_and_following_terms_outside() {
    let source = "∫_0^1▒〖x+y〗+b";
    let omml = DocumentConverter::convert_formula_string(
        source,
        FormulaInputFormat::UnicodeMath,
        OutputFormat::OMML,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    let end = omml.find("</m:nary>").unwrap();
    assert!(omml[..end].contains("<m:t>y</m:t>"), "{omml}");
    assert!(!omml[..end].contains("<m:t>b</m:t>"));
    assert!(omml[end..].contains("<m:t>b</m:t>"));
    assert!(omml.contains("<m:limLoc m:val=\"subSup\"/>"));
    let mathml = DocumentConverter::convert_formula_string(
        source,
        FormulaInputFormat::UnicodeMath,
        OutputFormat::MathML,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    assert!(mathml.contains("<msubsup>") && mathml.contains("movablelimits=\"false\""));
    for source in [
        "∫1 x",
        "∑192 x",
        "▒x",
        "∫▒",
        "∫▒^2 x",
        "∫⁡x",
        "∫▒()",
        "∯▒x",
        "〖∫^a〗^b▒x",
    ] {
        assert!(parse_unicodemath(source).is_err(), "{source}");
    }
    let deep = format!("{}x", "∑▒".repeat(80));
    assert_eq!(
        parse_unicodemath(&deep).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
    assert_eq!(canonical("∫▒x"), "∫▒〖x〗");
}

#[test]
fn nary_xml_readback_keeps_scopes_instead_of_absorbing_following_terms() {
    for source in [
        "∫_0^1▒〖x+y〗+b",
        "∑_(i=1)^n▒〖x_i+y_i〗+b",
        "∑▒∏▒x",
        "∫▒(x+y)z",
        "∑▒sin x",
        "∫▒x/y",
        "∫▒x",
        "∫▒xⅆx",
        "∑▒xyz",
        "⋃_i▒x",
        "⋂_i▒x",
        "∐_i▒x",
    ] {
        for (target, input) in [
            (OutputFormat::MathML, FormulaInputFormat::Mathml),
            (OutputFormat::OMML, FormulaInputFormat::Omml),
        ] {
            let xml = DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::UnicodeMath,
                target,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            let restored = DocumentConverter::convert_formula_to_target(
                &xml,
                input,
                latexsnipper_conversion::FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort,
            )
            .unwrap_or_else(|error| panic!("{source}: {error}; {xml}"));
            assert_eq!(restored, canonical(source), "{source}: {xml}");
        }
    }
}

#[test]
fn nary_readback_does_not_guess_unknown_or_duplicated_omml_characters() {
    for properties in [
        r#"<m:chr m:val="∯"/>"#,
        r#"<m:chr m:val="∫"/><m:chr m:val="∑"/>"#,
        r#"<m:chr m:val="\input{fixture}"/>"#,
    ] {
        let xml = format!(
            r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:nary><m:naryPr>{properties}</m:naryPr><m:e><m:r><m:t>x</m:t></m:r></m:e></m:nary></m:oMath>"#
        );
        assert!(latexsnipper_conversion::omml_parser::parse_omml_to_latex(&xml).is_err());
    }
    let xml = DocumentConverter::convert_formula_string(
        "∫▒x",
        FormulaInputFormat::UnicodeMath,
        OutputFormat::OMML,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    assert!(
        latexsnipper_conversion::omml_parser::parse_omml_to_latex(&xml)
            .unwrap()
            .contains(r"\int")
    );
}

#[test]
fn named_functions_preserve_argument_boundaries_and_scripts() {
    for (source, expected) in [
        ("sin x", "sin⁡〖x〗"),
        ("sin⁡xy", "sin⁡〖x y〗"),
        ("cos(x)b", "cos⁡〖(x)〗 b"),
        ("sin⁡〖(x)b〗", "sin⁡〖(x) b〗"),
        ("log_2 x", "〖log〗_(2)⁡〖x〗"),
        ("sin^2⁡x", "〖sin〗^(2)⁡〖x〗"),
        ("sin⁡^2 x", "〖sin〗^(2)⁡〖x〗"),
        ("sin cos x", "sin⁡〖cos⁡〖x〗〗"),
        ("sin x/y", "sin⁡〖(x)/(y)〗"),
        ("sin(x)/y", "(sin⁡〖(x)〗)/(y)"),
        ("sin", "sin"),
    ] {
        assert_eq!(canonical(source), expected, "{source}");
        assert_eq!(canonical(expected), expected, "{source}");
    }
    let LatexNode::Sequence(nodes) = parse_unicodemath("sin⁡(x)b").unwrap() else {
        panic!("argument boundary")
    };
    assert_eq!(nodes.len(), 2);
    let LatexNode::Group(call) = &nodes[0] else {
        panic!("call")
    };
    assert!(matches!(&call[0], LatexNode::Operator(name) if name == "sin"));
    let LatexNode::Group(argument) = &call[1] else {
        panic!("argument")
    };
    assert!(matches!(&argument[0], LatexNode::Delimited { content, .. }
        if content.len() == 1 && matches!(&content[0], LatexNode::Text(text) if text == "x")));
    assert!(matches!(&nodes[1], LatexNode::Text(text) if text == "b"));
    for name in [
        "sin", "cos", "tan", "sec", "csc", "cot", "sinh", "cosh", "tanh", "arcsin", "arccos",
        "arctan", "log", "ln", "exp", "lim", "min", "max", "det", "dim", "gcd",
    ] {
        let source = format!("{name}⁡x");
        assert_eq!(canonical(&source), format!("{name}⁡〖x〗"));
        for target in [OutputFormat::MathML, OutputFormat::OMML] {
            let xml = DocumentConverter::convert_formula_string(
                &source,
                FormulaInputFormat::UnicodeMath,
                target,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert!(!xml.contains(&format!(r"\{name}")), "{name}: {xml}");
            assert!(xml.contains(name) && xml.contains('x'));
        }
    }
    let external = LatexNode::Sequence(vec![
        LatexNode::Operator("sin".into()),
        LatexNode::Text("ab".into()),
        LatexNode::Text("+".into()),
        LatexNode::Operator("cos".into()),
        LatexNode::Text("cd".into()),
    ]);
    assert_eq!(
        write_unicodemath(&external).unwrap(),
        "sin⁡〖a b〗 + cos⁡〖c d〗"
    );
}

#[test]
fn function_projection_owns_grouped_operand_and_does_not_guess_unknown_names() {
    let source = "sin⁡〖x+y〗+b";
    let omml = DocumentConverter::convert_formula_string(
        source,
        FormulaInputFormat::UnicodeMath,
        OutputFormat::OMML,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    let end = omml.find("</m:func>").expect("function");
    assert!(omml[..end].contains("<m:t>x</m:t>"), "{omml}");
    assert!(omml[..end].contains("<m:t>y</m:t>"), "{omml}");
    assert!(!omml[..end].contains("<m:t>b</m:t>"));
    assert!(omml[end..].contains("<m:t>b</m:t>"));
    for source in ["sin⁡", "sin⁡+x", "sin⁡()", "sin2x", "sin^2⁡^3 x", "sin⁡⁡x"] {
        assert!(parse_unicodemath(source).is_err(), "{source}");
    }
    assert_eq!(
        parse_unicodemath("α+sin⁡").unwrap_err().offset,
        "α+sin⁡".len()
    );
    let deep = format!("{}x", "sin⁡".repeat(80));
    assert_eq!(
        parse_unicodemath(&deep).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
    assert_eq!(canonical("sin x"), "sin⁡〖x〗");
    for source in [
        "sin⁡\"label\"",
        "sin^2 x",
        "log_2 x",
        "■(sin x&cos y@exp z&dim w)",
    ] {
        let xml = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::MathML,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(xml.contains("mathvariant=\"normal\""), "{xml}");
        assert!(!xml.contains('\\'), "{xml}");
    }
    let log = DocumentConverter::convert_formula_string(
        "log_2 x",
        FormulaInputFormat::UnicodeMath,
        OutputFormat::MathML,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    assert!(log.contains("<msub>") && !log.contains("<munder>"));
}

#[test]
fn explicit_identifier_functions_preserve_names_scripts_and_owned_arguments() {
    use latexsnipper_conversion::FormulaOutputTarget;
    for (source, expected) in [
        ("f⁡x", "f⁡〖x〗"),
        ("sine⁡x", "sine⁡〖x〗"),
        ("asin⁡x", "asin⁡〖x〗"),
        ("sin2_i⁡x", "sin2⁡_(i) 〖x〗"),
        ("arctan2⁡_y x", "arctan2⁡_(y) 〖x〗"),
        ("CDF42⁡(x)b", "CDF42⁡〖(x)〗 b"),
        ("Foo_i^n⁡〖x+y〗+b", "Foo⁡_(i)^(n) 〖x + y〗 + b"),
        ("Foo⁡_i^n 〖x+y〗+b", "Foo⁡_(i)^(n) 〖x + y〗 + b"),
        ("Foo▒x", "Foo⁡〖x〗"),
        ("x▒y", "x⁡〖y〗"),
        ("int⁡x", "int⁡〖x〗"),
        ("sqrt⁡x", "sqrt⁡〖x〗"),
        ("Foo⁡Bar⁡x", "Foo⁡〖Bar⁡〖x〗〗"),
    ] {
        assert_eq!(canonical(source), expected, "{source}");
        assert_eq!(canonical(expected), expected);
        let latex = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::Latex,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(latex.contains("\\operatorname{"), "{latex}");
        for (target, input) in [
            (OutputFormat::MathML, FormulaInputFormat::Mathml),
            (OutputFormat::OMML, FormulaInputFormat::Omml),
        ] {
            let xml = DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::UnicodeMath,
                target,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            let restored = DocumentConverter::convert_formula_to_target(
                &xml,
                input,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort,
            )
            .unwrap_or_else(|e| panic!("{source}: {e}; {xml}"));
            assert_eq!(restored, expected, "{source}: {xml}");
        }
    }
    let xml = DocumentConverter::convert_formula_string(
        "Foo⁡〖x+y〗+b",
        FormulaInputFormat::UnicodeMath,
        OutputFormat::OMML,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    let end = xml.find("</m:func>").unwrap();
    assert!(xml[..end].contains("<m:nor/>") && xml[..end].contains("Foo"));
    assert!(xml[..end].contains("<m:t>y</m:t>") && !xml[..end].contains("<m:t>b</m:t>"));
    assert!(xml[end..].contains("<m:t>b</m:t>"));
    assert!(matches!(
        parse_unicodemath("Foo x").unwrap(),
        LatexNode::Sequence(_)
    ));
    assert!(!parse_unicodemath("Foo x")
        .unwrap()
        .to_string()
        .contains("operatorname"));
}

#[test]
fn unicode_and_no_break_space_function_names_roundtrip_with_owned_arguments() {
    use latexsnipper_conversion::FormulaOutputTarget;
    for (source, expected, name) in [
        ("α⁡x", "α⁡〖x〗", "α"),
        ("CDFα2⁡(x)b", "CDFα2⁡〖(x)〗 b", "CDFα2"),
        ("概率⁡〖x+y〗+b", "概率⁡〖x + y〗 + b", "概率"),
        ("Écart_i^n⁡x", "Écart⁡_(i)^(n) 〖x〗", "Écart"),
        ("قياس⁡_i x", "قياس⁡_(i) 〖x〗", "قياس"),
        ("arg\u{00a0}max⁡x", "arg\u{00a0}max⁡〖x〗", "arg\u{00a0}max"),
        (
            "条件\u{00a0}概率_i⁡〖x+y〗+b",
            "条件\u{00a0}概率⁡_(i) 〖x + y〗 + b",
            "条件\u{00a0}概率",
        ),
        ("α\u{00a0}β⁡x", "α\u{00a0}β⁡〖x〗", "α\u{00a0}β"),
    ] {
        assert_eq!(canonical(source), expected, "{source}");
        assert_eq!(canonical(expected), expected, "{source}");
        let latex = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            OutputFormat::LatexDisplay,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(
            latex.contains(&format!("\\operatorname{{{name}}}")),
            "{latex}"
        );
        assert_eq!(
            DocumentConverter::convert_formula_to_target(
                &latex,
                FormulaInputFormat::Latex,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort,
            )
            .unwrap(),
            expected,
            "{source}: {latex}"
        );
        for (target, input) in [
            (OutputFormat::MathML, FormulaInputFormat::Mathml),
            (OutputFormat::OMML, FormulaInputFormat::Omml),
        ] {
            let xml = DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::UnicodeMath,
                target,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert!(xml.contains(name), "{xml}");
            let restored = DocumentConverter::convert_formula_to_target(
                &xml,
                input,
                FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort,
            )
            .unwrap_or_else(|error| panic!("{source}: {error}; {xml}"));
            assert_eq!(restored, expected, "{source}: {xml}");
        }
    }
    for source in [
        "α⁡概率⁡x",
        "■(概率⁡x&arg\u{00a0}max⁡y@a&b)",
        "x^(条件\u{00a0}概率⁡y)",
        "(概率⁡x)/y",
    ] {
        let expected = canonical(source);
        assert_eq!(canonical(&expected), expected);
        for (target, input) in [
            (OutputFormat::MathML, FormulaInputFormat::Mathml),
            (OutputFormat::OMML, FormulaInputFormat::Omml),
        ] {
            let xml = DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::UnicodeMath,
                target,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            assert_eq!(
                DocumentConverter::convert_formula_to_target(
                    &xml,
                    input,
                    FormulaOutputTarget::UnicodeMath,
                    FormulaConversionMode::BestEffort,
                )
                .unwrap(),
                expected,
                "{source}: {xml}"
            );
        }
    }
}

#[test]
fn extended_function_names_keep_byte_limits_and_explicit_application_gate() {
    use latexsnipper_conversion::FormulaOutputTarget;
    for source in [
        "arg\u{00a0}max x",
        "arg\u{00a0}max(x)",
        "\u{00a0}Foo⁡x",
        "Foo\u{00a0}⁡x",
        "Foo\u{00a0}\u{00a0}Bar⁡x",
        "Foo\u{00a0}2⁡x",
        "条件⁡",
        "条件⁡()",
        "〖条件〗⁡x",
        "x_arg\u{00a0}max",
        "f\u{0301}⁡x",
        "f\u{200b}⁡x",
        "f\u{202e}⁡x",
        "LatexSnipperInternal\u{00a0}函数⁡x",
    ] {
        let error = parse_unicodemath(source).unwrap_err();
        assert!(source.is_char_boundary(error.offset), "{source}: {error}");
    }
    for name in ["函".repeat(21), "α".repeat(32), "A\u{00a0}B".into()] {
        assert!(parse_unicodemath(&format!("{name}⁡x")).is_ok());
    }
    for name in ["函".repeat(22), "α".repeat(33)] {
        assert!(parse_unicodemath(&format!("{name}⁡x")).is_err());
    }
    for source in ["条件 x", "α x", "arg max⁡x"] {
        let latex = parse_unicodemath(source).unwrap().to_string();
        assert!(!latex.contains("operatorname"), "{source}: {latex}");
    }
    for source in ["概率⁡x", "arg\u{00a0}max⁡x"] {
        assert!(DocumentConverter::convert_formula_to_target(
            source,
            FormulaInputFormat::UnicodeMath,
            FormulaOutputTarget::AsciiMath,
            FormulaConversionMode::BestEffort,
        )
        .is_err());
    }
}

#[test]
fn identifier_function_limits_and_ambiguous_names_are_explicitly_refused() {
    for source in [
        "Foo⁡",
        "Foo⁡()",
        "Foo⁡+x",
        "Foo⁡⁡x",
        "〖a+b〗⁡x",
        "42⁡x",
        "〖a b〗⁡x",
        "〖ab〗⁡x",
        "Foo^2⁡^3 x",
        "LatexSnipperInternalNaryGrow⁡x",
    ] {
        assert!(parse_unicodemath(source).is_err(), "{source}");
    }
    assert!(parse_unicodemath(&format!("{}⁡x", "f".repeat(64))).is_ok());
    assert!(parse_unicodemath(&format!("{}⁡x", "f".repeat(65))).is_err());
    assert!(parse_unicodemath(&format!("{}x", "Foo⁡".repeat(80))).is_err());
    let mut name = LatexNode::Text("Foo".into());
    for _ in 0..70 {
        name = LatexNode::Group(vec![name]);
    }
    let node = LatexNode::OperatorName {
        name: "operatorname".into(),
        args: vec![name],
    };
    assert_eq!(
        write_unicodemath(&node).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
}

#[test]
fn finite_function_xml_readback_retains_owned_arguments_and_refuses_stray_apply() {
    for source in [
        "sin⁡〖x+y〗+b",
        "sin cos x",
        "sin⁡(x)b",
        "sin x/y",
        "sin^2 x",
        "log_2 x",
        "sin⁡\"label\"",
    ] {
        for (target, input) in [
            (OutputFormat::MathML, FormulaInputFormat::Mathml),
            (OutputFormat::OMML, FormulaInputFormat::Omml),
        ] {
            if source.contains('"') && input == FormulaInputFormat::Omml {
                // OMML normal runs do not identify the original literal-text AST.
                continue;
            }
            let xml = DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::UnicodeMath,
                target,
                FormulaConversionMode::BestEffort,
            )
            .unwrap();
            let restored = DocumentConverter::convert_formula_to_target(
                &xml,
                input,
                latexsnipper_conversion::FormulaOutputTarget::UnicodeMath,
                FormulaConversionMode::BestEffort,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "{source}: {error}; {xml}; {:?}",
                    if input == FormulaInputFormat::Mathml {
                        latexsnipper_conversion::mathml_parser::parse_mathml_to_latex(&xml)
                    } else {
                        latexsnipper_conversion::omml_parser::parse_omml_to_latex(&xml)
                    }
                )
            });
            assert_eq!(restored, canonical(source), "{source}: {xml}");
        }
    }
    for xml in ["<math><mo>&#x2061;</mo><mi>x</mi></math>",
        "<math><mi mathvariant=\"normal\">sin</mi><mrow>&#x2061;</mrow><mi>x</mi></math>",
        "<math><mi mathvariant=\"normal\">sin</mi><mo>&#x2061;</mo></math>",
        "<math><mi>42</mi><mo>&#x2061;</mo><mi>x</mi></math>",
        "<math><mfrac><mi mathvariant=\"normal\">sin</mi><mo>&#x2061;</mo><mi>x</mi></mfrac></math>"] {
        assert!(latexsnipper_conversion::mathml_parser::parse_mathml_to_latex(xml).is_err());
    }
}

#[test]
fn quoted_text_is_atomic_literal_not_variables_or_math_syntax() {
    for (source, latex, output) in [
        (r#""rate""#, r"\text{rate}", r#""rate""#),
        (r#""if x > 0""#, r"\text{if x > 0}", r#""if x > 0""#),
        (r#""条件 α""#, r"\text{条件 α}", r#""条件 α""#),
        (r#""a\"b""#, r#"\text{a"b}"#, r#""a\"b""#),
        (
            r#""C:\\path\\""#,
            r"\text{C:\backslash{}path\backslash{}}",
            r#""C:\\path\\""#,
        ),
        (
            r#""&%#_${}^~""#,
            r"\text{\&\%\#\_\$\{\}\textasciicircum{}\textasciitilde{}}",
            r#""&%#_${}^~""#,
        ),
        (r#""""#, r"\text{}", r#""""#),
    ] {
        let node = parse_unicodemath(source).unwrap();
        assert!(matches!(node, LatexNode::Command { ref name, .. } if name == "text"));
        assert_eq!(node.to_string(), latex);
        assert_eq!(write_unicodemath(&node).unwrap(), output);
        assert_eq!(canonical(output), output);
    }
    assert_eq!(
        canonical(r#""rate"="distance"/"time""#),
        r#""rate" = ("distance")/("time")"#
    );
    assert_eq!(canonical(r#""label"_1"#), r#"〖"label"〗_(1)"#);
    let LatexNode::Matrix { rows, .. } = parse_unicodemath(r#"■("a&b"&"c@d"@"(x)"&"")"#).unwrap()
    else {
        panic!("matrix")
    };
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.len() == 2));
    assert!(parse_asciimath(r#""rate""#).is_err());
}

#[test]
fn quoted_text_projection_escapes_tex_and_xml_without_executing_commands() {
    let source = r#""\input{fixture}&100%_""#;
    let ast = parse_unicodemath(source).unwrap();
    assert_eq!(
        ast.to_string(),
        r"\text{\backslash{}input\{fixture\}\&100\%\_}"
    );
    for format in [OutputFormat::MathML, OutputFormat::OMML] {
        let result = DocumentConverter::convert_formula_string(
            source,
            FormulaInputFormat::UnicodeMath,
            format,
            FormulaConversionMode::BestEffort,
        )
        .unwrap();
        assert!(result.contains("fixture"));
        assert!(result.contains("&amp;100%_"), "{result}");
        assert!(!result.contains("<mfrac>") && !result.contains("<m:f>"));
    }
    let latex = DocumentConverter::convert_formula_string(
        source,
        FormulaInputFormat::UnicodeMath,
        OutputFormat::LatexDisplay,
        FormulaConversionMode::BestEffort,
    )
    .unwrap();
    assert!(!latex.contains(r"\input{") && latex.contains(r"\backslash{}input"));
}

#[test]
fn quoted_text_diagnostics_and_work_budgets_cannot_be_bypassed() {
    for source in [r#""unfinished"#, "x+\"a\nb\"", "\"a\tb\"", "\"a\u{2028}b\""] {
        let error = parse_unicodemath(source).unwrap_err();
        assert!(source.is_char_boundary(error.offset));
    }
    assert_eq!(parse_unicodemath("x+\"unfinished").unwrap_err().offset, 2);
    assert_eq!(parse_unicodemath("\"a\nb\"").unwrap_err().offset, 2);
    let long = format!("\"{}\"", "a".repeat(4095));
    assert_eq!(
        parse_unicodemath(&long).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
    let escaped = format!("\"{}\"", "^".repeat(4094));
    assert_eq!(
        parse_unicodemath(&escaped).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
    let injected = LatexNode::Command {
        name: "text".into(),
        args: vec![LatexNode::Text(r"\input{x}".into())],
    };
    assert!(write_unicodemath(&injected).is_err());
    let rich = LatexNode::Command {
        name: "text".into(),
        args: vec![LatexNode::Fraction {
            num: Box::new(LatexNode::Text("a".into())),
            den: Box::new(LatexNode::Text("b".into())),
        }],
    };
    assert!(write_unicodemath(&rich).is_err());
    assert_eq!(canonical(r#""valid""#), r#""valid""#);
}

#[test]
fn versioned_fixture_has_fixed_expected_structure_and_output() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/unicodemath-pilot-v1.json")).unwrap();
    assert_eq!(fixtures["schemaVersion"], 1);
    assert_eq!(fixtures["accepted"].as_array().unwrap().len(), 21);
    assert_eq!(fixtures["rejected"].as_array().unwrap().len(), 9);
    for fixture in fixtures["accepted"].as_array().unwrap() {
        let source = fixture["source"].as_str().unwrap();
        let node = parse_unicodemath(source).unwrap();
        let root = match node {
            LatexNode::Fraction { .. } => "fraction",
            LatexNode::SquareRoot { .. } => "root",
            LatexNode::Matrix { .. } => "matrix",
            LatexNode::Subscript { .. } => "subscript",
            LatexNode::Superscript { .. } => "superscript",
            LatexNode::Sequence(_) => "sequence",
            LatexNode::Group(_) => "group",
            _ => panic!("unlisted fixture root"),
        };
        assert_eq!(root, fixture["root"].as_str().unwrap(), "{source}");
        let output = write_unicodemath(&node).unwrap();
        assert_eq!(output, fixture["canonical"].as_str().unwrap(), "{source}");
        assert_eq!(canonical(&output), output, "{source}");
    }
    for fixture in fixtures["rejected"].as_array().unwrap() {
        let source = fixture["source"].as_str().unwrap();
        let err = parse_unicodemath(source).unwrap_err();
        assert_eq!(
            format!("{:?}", err.kind),
            fixture["kind"].as_str().unwrap(),
            "{source}"
        );
        assert!(source.is_char_boundary(err.offset));
    }
}

#[test]
fn ast_bridges_to_existing_mathml_and_omml_without_losing_matrix_operands() {
    let ast = parse_unicodemath("⒨(1/2&√x@√(3&y)&z_1^2)").unwrap();
    let latex = ast.to_string();
    let mathml = DocumentConverter::convert_latex_string(&latex, OutputFormat::MathML).unwrap();
    let omml = DocumentConverter::convert_latex_string(&latex, OutputFormat::OMML).unwrap();
    assert_eq!(mathml.matches("<mtr>").count(), 2, "{mathml}");
    assert_eq!(mathml.matches("<mtd>").count(), 4);
    assert_eq!(mathml.matches("<mfrac>").count(), 1);
    assert_eq!(mathml.matches("<msqrt>").count(), 1);
    assert_eq!(mathml.matches("<mroot>").count(), 1);
    assert_eq!(omml.matches("<m:mr>").count(), 2, "{omml}");
    assert_eq!(omml.matches("<m:f>").count(), 1);
    assert_eq!(omml.matches("<m:rad>").count(), 2);
    assert!(omml.contains("<m:sSubSup>") || omml.contains("<m:sSup>"));
}

#[test]
fn fractions_use_runs_and_left_association_not_asciimath_rules() {
    let LatexNode::Fraction { num, den } = parse_unicodemath("abc/d").unwrap() else {
        panic!("fraction")
    };
    assert_eq!(num.to_string(), "abc");
    assert_eq!(den.to_string(), "d");
    assert!(matches!(
        parse_asciimath("abc/d").unwrap(),
        LatexNode::Sequence(_)
    ));
    let LatexNode::Fraction { num, den } = parse_unicodemath("a/b/c").unwrap() else {
        panic!("fraction")
    };
    assert!(matches!(*num, LatexNode::Fraction { .. }));
    assert_eq!(den.to_string(), "c");
    assert!(parse_asciimath("a/b/c").is_err());
    assert_eq!(canonical("a/(b/c)"), "(a)/((b)/(c))");
    assert_eq!(canonical("[a+b]/c"), "([a + b])/(c)");
    assert_eq!(canonical("((a+b))/c"), "((a + b))/(c)");
}

#[test]
fn scripts_have_right_association_mixed_common_base_and_final_letter_base() {
    assert_eq!(canonical("ab^2"), "a 〖b〗^(2)");
    assert_eq!(canonical("a^bc"), "〖a〗^(b c)");
    assert_eq!(canonical("a^b_c"), canonical("a_c^b"));
    let LatexNode::Superscript { base, exp } = parse_unicodemath("a^b^c").unwrap() else {
        panic!("sup")
    };
    assert_eq!(base.to_string(), "a");
    assert!(matches!(*exp, LatexNode::Superscript { .. }));
    let LatexNode::Subscript { sub, .. } = parse_unicodemath("a_b_c").unwrap() else {
        panic!("sub")
    };
    assert!(matches!(*sub, LatexNode::Subscript { .. }));
    assert_ne!(canonical("a^b_c"), canonical("a^(b_c)"));
    assert_eq!(canonical("a^-2"), "〖a〗^(- 2)");
    assert_eq!(canonical("〖ab〗^2"), "〖a b〗^(2)");
}

#[test]
fn spaces_end_operands_and_are_not_erased_by_the_lexer() {
    let LatexNode::Sequence(nodes) = parse_unicodemath("a b/c").unwrap() else {
        panic!("sequence")
    };
    assert_eq!(nodes.len(), 2);
    assert!(matches!(nodes[0], LatexNode::Text(_)));
    assert!(matches!(nodes[1], LatexNode::Fraction { .. }));
    assert_ne!(canonical("a b/c"), canonical("ab/c"));
    assert_eq!(canonical("a_1 b_2"), "〖a〗_(1) 〖b〗_(2)");
    assert_eq!(canonical(" a / (b+c) "), "(a)/(b + c)");
}

#[test]
fn radicals_support_indices_and_nested_scripts() {
    assert_eq!(canonical("√abc^2"), "√(a b 〖c〗^(2))");
    assert_eq!(canonical("√(a)^2"), "〖√(a)〗^(2)");
    assert_eq!(canonical("∛(a+b)"), "√(3&a + b)");
    assert_eq!(canonical("∜x"), "√(4&x)");
    let LatexNode::SquareRoot {
        index: Some(index),
        content,
    } = parse_unicodemath("√(n+1&√(x_1))").unwrap()
    else {
        panic!("indexed root")
    };
    assert_eq!(index.to_string(), "n+1");
    assert!(matches!(*content, LatexNode::SquareRoot { .. }));
}

#[test]
fn matrices_keep_nested_cells_empty_cells_and_pad_short_rows() {
    for (glyph, env) in [
        ("■", "matrix"),
        ("⒨", "pmatrix"),
        ("ⓢ", "bmatrix"),
        ("Ⓢ", "Bmatrix"),
        ("⒱", "vmatrix"),
        ("⒩", "Vmatrix"),
    ] {
        let LatexNode::Matrix { env: actual, rows } =
            parse_unicodemath(&format!("{glyph}(a&b@c)")).unwrap()
        else {
            panic!("matrix")
        };
        assert_eq!(actual, env);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.len() == 2));
        assert!(rows[1][1].is_empty());
        assert_eq!(
            canonical(&format!("{glyph}(a&b@c)")),
            format!("{glyph}(a&b@c&)")
        );
    }
    assert_eq!(canonical("■(a&&b@c&d&e)"), "■(a&&b@c&d&e)");
    assert_eq!(canonical("■()"), "■()");
    let LatexNode::Matrix { rows, .. } = parse_unicodemath("■(■(a&b@c&d)&1/2@√x&y_1^2)").unwrap()
    else {
        panic!("nested")
    };
    assert!(matches!(rows[0][0], LatexNode::Matrix { .. }));
    assert!(matches!(rows[0][1], LatexNode::Fraction { .. }));
    assert!(matches!(rows[1][0], LatexNode::SquareRoot { .. }));
}

#[test]
fn unicode_mapping_and_utf8_diagnostics_are_explicit() {
    assert_eq!(canonical("α×β≤∞"), "α × β ≤ ∞");
    assert_eq!(canonical("ϵ+ε+φ+ϕ"), "ϵ + ε + φ + ϕ");
    assert_eq!(canonical("12.5/2,5"), "(12.5)/(2,5)");
    let err = parse_unicodemath("α + \\foo").unwrap_err();
    assert_eq!(err.kind, UnicodeMathErrorKind::UnsupportedSyntax);
    assert_eq!(err.offset, "α + ".len());
    assert_eq!(parse_unicodemath("α_ ").unwrap_err().offset, "α_".len());
}

#[test]
fn unsupported_and_malformed_sources_never_return_partial_success() {
    for source in [
        "\\frac{1}{2}",
        "∫1 x",
        "∑192 x",
        "a⁄b",
        "a∕b",
        "a¦b",
        "a/b#1",
        "x\t+y",
        "x\n+y",
        "𝑥+y",
        "x̂",
        "a^b(c)",
    ] {
        assert_eq!(
            parse_unicodemath(source).unwrap_err().kind,
            UnicodeMathErrorKind::UnsupportedSyntax,
            "{source}"
        );
    }
    for source in [
        "", " ", "cos⁡", "a/", "/a", "(a]", "a_", "a^ b", "■(a&b", "a&b", "a@b", "√()", "√(n&)",
        "(a", "a)", "a_b^c_d", "1.2.3",
    ] {
        assert_eq!(
            parse_unicodemath(source).unwrap_err().kind,
            UnicodeMathErrorKind::UnexpectedToken,
            "{source}"
        );
    }
}

#[test]
fn external_ast_writer_preserves_binding_or_rejects_instead_of_dropping_nodes() {
    let node = LatexNode::Subscript {
        base: Box::new(LatexNode::Superscript {
            base: Box::new(LatexNode::text("ab")),
            exp: Box::new(LatexNode::text("2")),
        }),
        sub: Box::new(LatexNode::text("1")),
    };
    let output = write_unicodemath(&node).unwrap();
    assert_eq!(canonical(&output), output);
    let ragged = LatexNode::Matrix {
        env: "matrix".into(),
        rows: vec![
            vec![LatexNode::text("a"), LatexNode::text("b")],
            vec![LatexNode::text("c")],
        ],
    };
    assert_eq!(
        write_unicodemath(&ragged).unwrap_err().kind,
        UnicodeMathErrorKind::UnsupportedSyntax
    );
    for node in [
        LatexNode::command("sin", vec![]),
        LatexNode::text("sin"),
        LatexNode::text("\\unknown"),
        LatexNode::Operator("oiint".into()),
        LatexNode::Cases(vec![]),
    ] {
        assert_eq!(
            write_unicodemath(&node).unwrap_err().kind,
            UnicodeMathErrorKind::UnsupportedSyntax
        );
    }
}

#[test]
fn finite_generated_inputs_are_stable_after_canonicalization() {
    let atoms = [
        "a",
        "b",
        "12",
        "α",
        "∞",
        "√x",
        "(a+b)",
        "[a]",
        "〖ab〗",
        "■(a&b@c&d)",
    ];
    let joins = ["", " ", "+", "/", "_", "^"];
    let mut accepted = 0;
    for left in atoms {
        for middle in joins {
            for right in atoms {
                let source = format!("{left}{middle}{right}");
                if let Ok(node) = parse_unicodemath(&source) {
                    let output =
                        write_unicodemath(&node).unwrap_or_else(|err| panic!("{source}: {err}"));
                    assert_eq!(canonical(&output), output, "{source}");
                    accepted += 1;
                }
            }
        }
    }
    assert!(
        accepted >= 400,
        "accepted {accepted} of 600 generated inputs"
    );
}

#[test]
fn input_ast_padding_depth_and_output_budgets_are_enforced() {
    for source in [
        "x".repeat(65537),
        "x".repeat(4097),
        format!("{}x{}", "(".repeat(65), ")".repeat(65)),
        format!("x{}", "^x".repeat(65)),
        format!("x{}", "/x".repeat(65)),
        format!("■({}@{})", "x&".repeat(100), "x@".repeat(100)),
    ] {
        assert_eq!(
            parse_unicodemath(&source).unwrap_err().kind,
            UnicodeMathErrorKind::LimitExceeded
        );
    }
    let wide = LatexNode::Sequence(vec![
        LatexNode::Sequence(vec![LatexNode::text("x"); 4096]);
        3
    ]);
    assert_eq!(
        write_unicodemath(&wide).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
    let output_wide = LatexNode::Sequence(vec![LatexNode::text("1".repeat(128)); 600]);
    assert_eq!(
        write_unicodemath(&output_wide).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
    let bounded_deep = (0..12).fold(LatexNode::text("x"), |content, _| LatexNode::SquareRoot {
        index: None,
        content: Box::new(content),
    });
    let written = write_unicodemath(&bounded_deep).unwrap();
    assert_eq!(canonical(&written), written);
    assert_eq!(
        write_unicodemath(&LatexNode::text("x".repeat(65537)))
            .unwrap_err()
            .kind,
        UnicodeMathErrorKind::LimitExceeded
    );
    let deep = (0..65).fold(LatexNode::text("x"), |content, _| LatexNode::SquareRoot {
        index: None,
        content: Box::new(content),
    });
    assert_eq!(
        write_unicodemath(&deep).unwrap_err().kind,
        UnicodeMathErrorKind::LimitExceeded
    );
    assert_eq!(canonical("x+1"), "x + 1");
}

#[test]
fn finite_input_routes_preserve_strict_and_platform_boundaries() {
    for target in [
        CapabilityTarget::Native,
        CapabilityTarget::Wasm32UnknownUnknown,
    ] {
        for mode in [
            FormulaConversionMode::Strict,
            FormulaConversionMode::BestEffort,
        ] {
            let capability = CapabilityRegistry::formula_conversion(
                FormulaInputFormat::UnicodeMath,
                OutputFormat::OMML,
                mode,
                target,
            );
            assert_eq!(
                capability.available,
                mode == FormulaConversionMode::BestEffort
            );
            assert_eq!(capability.path, "bounded-linear-ast");
            if mode == FormulaConversionMode::Strict {
                assert!(capability
                    .unavailable_reason
                    .unwrap()
                    .contains("strict validation"));
            }
        }
    }
}
