use latexsnipper_conversion::{
    DocumentConverter, FormulaConversionMode, FormulaInputFormat, OutputFormat,
};

fn convert(source: &str, format: OutputFormat) -> String {
    DocumentConverter::convert_formula_string(
        source,
        FormulaInputFormat::Latex,
        format,
        if format == OutputFormat::OMML {
            FormulaConversionMode::Strict
        } else {
            FormulaConversionMode::BestEffort
        },
    )
    .unwrap()
}

fn well_formed(xml: &str) {
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        if matches!(
            reader.read_event().expect("well-formed XML"),
            quick_xml::events::Event::Eof
        ) {
            break;
        }
    }
}

#[test]
fn explicit_limits_override_inline_style_and_last_modifier_wins() {
    for op in ["min", "max"] {
        for (modifiers, vertical) in [
            (r"\limits", true),
            (r"\displaylimits", false),
            (r"\limits\nolimits", false),
            (r"\nolimits\limits", true),
        ] {
            let source = format!("$\\{op}{modifiers}_{{x\\in A}}^{{n}}$");
            let omml = convert(&source, OutputFormat::OMML);
            let mathml = convert(&source, OutputFormat::MathML);
            assert_eq!(omml.contains("<m:limLow>"), vertical, "{source}: {omml}");
            assert_eq!(
                mathml.contains("<munderover>"),
                vertical,
                "{source}: {mathml}"
            );
            assert!(omml.contains('∈') && mathml.contains('∈'));
        }
    }
}

#[test]
fn typst_limit_modifiers_and_ast_source_projection_keep_scripts() {
    for op in ["min", "max"] {
        for (modifier, wrapper) in [
            ("limits", "limits"),
            ("nolimits", "scripts"),
            ("displaylimits", ""),
        ] {
            let source = format!("\\{op}\\{modifier}_{{i}}^{{n}}");
            let typst = convert(&source, OutputFormat::Typst);
            let expected = if wrapper.is_empty() {
                format!("{op}_(i)^(n)")
            } else {
                format!("{wrapper}({op})_(i)^(n)")
            };
            assert!(typst.contains(&expected), "{typst}");
            let ast = latexsnipper_conversion::latex_parser::parse_latex(&source);
            let projection = ast.to_string();
            assert_eq!(projection, source);
            let omml = convert(&projection, OutputFormat::OMML);
            assert!(omml.contains(">i<") && omml.contains(">n<"), "{omml}");
        }
    }
}

#[test]
fn mixed_function_calls_keep_extrema_relations_and_trailing_operands() {
    for op in ["min", "max"] {
        let source = format!("\\{op}_{{x\\in A}}^{{n}}\\sin(x)+z");
        let result = convert(&source, OutputFormat::MathML);
        assert!(
            result.contains("<munderover>") && result.contains('∈') && result.contains(">z<"),
            "{result}"
        );
        assert!(!result.contains('\\'), "{result}");
    }
}

#[test]
fn modifiers_after_scripts_comments_and_nested_matrix_cells_are_preserved() {
    for op in ["min", "max"] {
        for source in [
            format!("\\{op}_{{x\\in A}}\\limits^{{n}}"),
            format!("\\{op}% ignored\n\\limits_{{x\\in A}}^{{n}}"),
            format!("\\left(\\{op}\\limits_{{x\\in A}}^{{n}}\\right)"),
        ] {
            let omml = convert(&source, OutputFormat::OMML);
            let mathml = convert(&source, OutputFormat::MathML);
            assert!(
                omml.contains("<m:limLow>") && omml.contains("<m:limUpp>"),
                "{omml}"
            );
            assert!(
                mathml.contains("<munderover>") && mathml.contains('∈'),
                "{mathml}"
            );
        }
        let source = format!("\\begin{{matrix}}\\{op}_{{i}}&\\{op}\\limits_{{j}}\\end{{matrix}}");
        let omml = convert(&source, OutputFormat::OMML);
        let mathml = convert(&source, OutputFormat::MathML);
        assert!(
            omml.contains("<m:sSub>") && omml.contains("<m:limLow>"),
            "{omml}"
        );
        assert!(
            mathml.contains("<msub>") && mathml.contains("<munder>"),
            "{mathml}"
        );
    }
}

#[test]
fn extrema_are_upright_operators_not_literal_commands() {
    for op in ["min", "max"] {
        for tail in ["", "(x,y)"] {
            let source = format!("\\{op}{tail}");
            let omml = convert(&source, OutputFormat::OMML);
            let mathml = convert(&source, OutputFormat::MathML);
            assert!(
                omml.contains(&format!("<m:nor/></m:rPr><m:t>{op}</m:t>")),
                "{omml}"
            );
            assert!(
                mathml.contains(&format!("<mi mathvariant=\"normal\">{op}</mi>")),
                "{mathml}"
            );
            assert!(!mathml.contains(&format!("\\{op}")), "{mathml}");
            well_formed(&omml);
            well_formed(&mathml);
        }
    }
}

#[test]
fn explicit_limits_preserve_both_scripts_in_either_order() {
    for op in ["min", "max"] {
        for modifier in ["", "\\limits", "\\displaylimits"] {
            for scripts in [r"_{x\in A}^{n}", r"^{n}_{x\in A}"] {
                let source = format!("a+\\{op}{modifier}{scripts} f(x)");
                let omml = convert(&source, OutputFormat::OMML);
                let mathml = convert(&source, OutputFormat::MathML);
                assert!(
                    omml.contains("<m:limLow>") && omml.contains("<m:limUpp>"),
                    "{source}: {omml}"
                );
                assert!(mathml.contains("<munderover>"), "{source}: {mathml}");
                for result in [&omml, &mathml] {
                    assert!(
                        result.contains('∈') && result.contains(">n<"),
                        "{source}: {result}"
                    );
                    assert!(!result.contains("limits"), "{source}: {result}");
                    well_formed(result);
                }
            }
        }
    }
}

#[test]
fn nolimits_keep_side_scripts_even_in_display_math() {
    for op in ["min", "max"] {
        for scripts in [r"_{i}", r"^{n}", r"_{i}^{n}", r"^{n}_{i}"] {
            let source = format!("\\{op}\\nolimits{scripts}");
            let omml = convert(&source, OutputFormat::OMML);
            let mathml = convert(&source, OutputFormat::MathML);
            assert!(
                !omml.contains("<m:limLow>") && !omml.contains("<m:limUpp>"),
                "{omml}"
            );
            assert!(
                !mathml.contains("<munder>")
                    && !mathml.contains("<munderover>")
                    && !mathml.contains("<mover>"),
                "{mathml}"
            );
            assert!(omml.contains("<m:s") && mathml.contains("<ms"), "{source}");
        }
    }
}

#[test]
fn standalone_wrappers_choose_inline_or_display_without_visible_brackets() {
    for op in ["min", "max"] {
        for (open, close, display) in [
            ("$", "$", false),
            (r"\(", r"\)", false),
            ("$$", "$$", true),
            (r"\[", r"\]", true),
        ] {
            let source = format!("{open}\\{op}_{{i}}{close}");
            let omml = convert(&source, OutputFormat::OMML);
            let mathml = convert(&source, OutputFormat::MathML);
            assert_eq!(omml.contains("<m:oMathPara"), display, "{omml}");
            assert_eq!(omml.contains("<m:limLow>"), display, "{omml}");
            assert_eq!(mathml.contains("<munder>"), display, "{mathml}");
            assert!(
                !omml.contains("<m:t>[</m:t>") && !omml.contains("<m:t>(</m:t>"),
                "{omml}"
            );
        }
    }
}

#[test]
fn nested_extrema_and_explicit_style_keep_relations_and_scripts() {
    for op in ["min", "max"] {
        for source in [
            format!("\\frac{{\\{op}_{{x\\in A}}}}{{2}}"),
            format!("\\textstyle{{\\{op}_{{x\\in A}}}}"),
            format!("\\{op}_{{\\max_{{i}}}}"),
        ] {
            let omml = convert(&source, OutputFormat::OMML);
            let mathml = convert(&source, OutputFormat::MathML);
            assert!(omml.contains("<m:sSub>"), "{source}: {omml}");
            assert!(mathml.contains("<msub>"), "{source}: {mathml}");
            assert!(
                !mathml.contains("\\min") && !mathml.contains("\\max"),
                "{mathml}"
            );
        }
    }
}

#[test]
fn unrelated_or_malformed_limit_commands_are_not_silently_accepted() {
    for source in [
        r"x\limits_{i}",
        r"\limits_{i}",
        r"\minimum_{i}",
        r"\maximum_{i}",
        r"\min_{i}_{j}",
        r"\max\nolimits_{i}_{j}",
    ] {
        assert!(
            DocumentConverter::convert_formula_string(
                source,
                FormulaInputFormat::Latex,
                OutputFormat::OMML,
                FormulaConversionMode::Strict
            )
            .is_err(),
            "{source}"
        );
    }
}
