use latexsnipper_conversion::{
    asciimath_pilot::parse_asciimath,
    latex_ast::LatexNode,
    unicodemath_pilot::{parse_unicodemath, write_unicodemath, UnicodeMathErrorKind},
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};

fn canonical(source: &str) -> String {
    write_unicodemath(&parse_unicodemath(source).unwrap()).unwrap()
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
    assert_eq!(fixtures["accepted"].as_array().unwrap().len(), 16);
    assert_eq!(fixtures["rejected"].as_array().unwrap().len(), 8);
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
        "∫_0^1 x",
        "∑_1^n x",
        "a⁄b",
        "a∕b",
        "a¦b",
        "a/b#1",
        "sin x",
        "cos(x)",
        "x\t+y",
        "x\n+y",
        "𝑥+y",
        "a⁡b",
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
        "", " ", "a/", "/a", "(a]", "a_", "a^ b", "■(a&b", "a&b", "a@b", "√()", "√(n&)", "(a",
        "a)", "a_b^c_d", "1.2.3",
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
        LatexNode::Operator("int".into()),
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
