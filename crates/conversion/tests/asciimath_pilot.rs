use latexsnipper_conversion::{
    asciimath_pilot::{parse_asciimath, write_asciimath, AsciiMathErrorKind},
    latex_ast::LatexNode,
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};

fn canonical(source: &str) -> String {
    write_asciimath(&parse_asciimath(source).unwrap()).unwrap()
}

#[test]
fn grouping_and_fraction_binding_are_not_plain_text_replacements() {
    let LatexNode::Sequence(nodes) = parse_asciimath("a+b/c+d").unwrap() else {
        panic!("sequence")
    };
    assert_eq!(nodes.len(), 5);
    let LatexNode::Fraction { num, den } = &nodes[2] else {
        panic!("fraction")
    };
    assert_eq!(num.to_string(), "b");
    assert_eq!(den.to_string(), "c");
    let LatexNode::Fraction { num, den } = parse_asciimath("(a+b)/(c+d)").unwrap() else {
        panic!("fraction")
    };
    assert!(matches!(*num, LatexNode::Sequence(_)));
    assert!(matches!(*den, LatexNode::Sequence(_)));
    assert_eq!(canonical("(a+b)/(c+d)"), "frac(a + b)(c + d)");
    assert_eq!(canonical("a/(b/c)"), "frac(a)(frac(b)(c))");
}

#[test]
fn recursive_roots_scripts_and_operator_limits_preserve_ownership() {
    let LatexNode::Superscript { base, exp } = parse_asciimath("sum_(i=1)^n").unwrap() else {
        panic!("superscript")
    };
    assert_eq!(exp.to_string(), "n");
    let LatexNode::Subscript { base, sub } = *base else {
        panic!("subscript")
    };
    assert!(matches!(*base, LatexNode::Operator(ref name) if name == "sum"));
    assert_eq!(sub.to_string(), "i=1");
    let LatexNode::SquareRoot {
        index: Some(index),
        content,
    } = parse_asciimath("root(3)(sqrt(x_1^2))").unwrap()
    else {
        panic!("root")
    };
    assert_eq!(index.to_string(), "3");
    assert!(matches!(*content, LatexNode::SquareRoot { .. }));
    assert_eq!(canonical("sqrt x^2"), "sqrt(x)^(2)");
}

#[test]
fn matrix_and_column_vector_rows_are_explicit_ast_nodes() {
    for (source, env, width) in [("[[a,b],[c,d]]", "bmatrix", 2), ("((a),(b))", "pmatrix", 1)] {
        let LatexNode::Matrix { env: actual, rows } = parse_asciimath(source).unwrap() else {
            panic!("matrix {source}")
        };
        assert_eq!(actual, env);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.len() == width));
    }
    let LatexNode::Matrix { rows, .. } =
        parse_asciimath("[[[[a,b],[c,d]],frac(1)(2)],[root(3)(x),y_1^2]]").unwrap()
    else {
        panic!("nested matrix")
    };
    assert!(matches!(rows[0][0], LatexNode::Matrix { .. }));
    assert!(matches!(rows[0][1], LatexNode::Fraction { .. }));
    assert!(matches!(rows[1][0], LatexNode::SquareRoot { .. }));
    assert_eq!(
        parse_asciimath("[[a,b],[c]]").unwrap_err().kind,
        AsciiMathErrorKind::RaggedMatrix
    );
}

#[test]
fn longest_symbols_decimals_and_whitespace_are_tokenized() {
    assert_eq!(canonical("x in RR"), "x in RR");
    assert_eq!(
        canonical("alpha times beta leq infty"),
        "alpha xx beta <= oo"
    );
    assert_eq!(canonical("\t alpha + beta <= .5 \n"), "alpha + beta <= .5");
    assert_eq!(canonical("a**b***c*2"), "a ** b *** c * 2");
    assert_eq!(
        canonical("lim_(x->oo) frac(sin(x))(x)"),
        "lim_(x -> oo) frac(sin (x))(x)"
    );
    assert_eq!(canonical("{:x+y:}_1"), "{:x + y:}_(1)");
    assert_eq!(canonical("abs(x-y)+floor(3.2)"), "abs(x - y) + floor(3.2)");
}

#[test]
fn malformed_and_unsupported_input_is_rejected_without_partial_ast() {
    for source in ["1..2", "1.2.3"] {
        assert_eq!(
            parse_asciimath(source).unwrap_err().kind,
            AsciiMathErrorKind::UnexpectedToken
        );
    }
    for source in ["a nnn b", "a ox b", "a -< b", "a vv b", "rArr"] {
        assert_eq!(
            parse_asciimath(source).unwrap_err().kind,
            AsciiMathErrorKind::UnsupportedSyntax
        );
    }
    for source in [
        "",
        " ",
        "a/",
        "a^",
        "a_",
        "(a]",
        "[a",
        "a)",
        "()",
        "[a,]",
        "[[a,,b],[c,d]]",
        "a^2_1",
        "a_1_2",
        "a/b/c",
        "a,b",
    ] {
        let result = parse_asciimath(source);
        assert!(result.is_err(), "accepted {source:?}");
    }
    for source in [
        r"\alpha",
        "color(red)(x)",
        "hat(x)",
        "\"text\"",
        "x|y",
        "[[a,|,b],[c,|,d]]",
        "x+中",
    ] {
        let failure = parse_asciimath(source).unwrap_err();
        assert_eq!(
            failure.kind,
            AsciiMathErrorKind::UnsupportedSyntax,
            "{source}"
        );
        assert!(source.is_char_boundary(failure.offset));
    }
    assert_eq!(parse_asciimath("x+中").unwrap_err().offset, 2);
    assert_eq!(parse_asciimath("x+color(red)(x)").unwrap_err().offset, 2);
    assert_eq!(canonical("x/2"), "frac(x)(2)");
}

#[test]
fn canonical_outputs_are_stable_for_nested_fixture_corpus() {
    for source in [
        "sum_(i=1)^n i^3=((n(n+1))/2)^2",
        "int_0^1 f(x)dx",
        "(dq)/(dp)",
        "root(3)(x+sqrt(y))",
        "(x+1)/(sqrt(1-x^2))",
        "[[1,2],[3,4]]+[[5,6],[7,8]]",
        "[[[[a,b],[c,d]],frac(1)(2)],[root(3)(x),y_1^2]]",
        "((a),(b))",
        "abs(x)+norm(y)+ceil(3.2)",
        "alpha_1^2+beta->oo",
        "{:x+y:}^2",
        "frac((a+b))(c)",
        "sqrt(sqrt(sqrt(x)))",
    ] {
        let first = canonical(source);
        assert_eq!(canonical(&first), first, "unstable {source}");
    }
}

#[test]
fn pilot_ast_feeds_existing_mathml_and_omml_structure_paths() {
    let ast = parse_asciimath("[[frac(1)(2),sqrt(x)],[root(3)(y),z_1^2]]").unwrap();
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
fn unsupported_output_ast_cannot_be_mislabeled_as_asciimath() {
    for ast in [
        LatexNode::command("mysymbol", vec![]),
        LatexNode::Cases(vec![vec![LatexNode::text("x")]]),
        LatexNode::Matrix {
            env: "matrix".into(),
            rows: vec![vec![LatexNode::text("x")]],
        },
    ] {
        assert_eq!(
            write_asciimath(&ast).unwrap_err().kind,
            AsciiMathErrorKind::UnsupportedSyntax
        );
    }
    assert_eq!(
        write_asciimath(&LatexNode::text("sqrt")).unwrap(),
        "s q r t"
    );
    for target in [
        CapabilityTarget::Native,
        CapabilityTarget::Wasm32UnknownUnknown,
    ] {
        let capability = CapabilityRegistry::formula_conversion(
            FormulaInputFormat::AsciiMath,
            OutputFormat::OMML,
            FormulaConversionMode::BestEffort,
            target,
        );
        assert!(!capability.available);
        assert!(capability
            .unavailable_reason
            .unwrap()
            .contains("pilot parser"));
    }
}

#[test]
fn parser_and_serializer_enforce_bounded_depth_size_and_work() {
    let huge_matrix = LatexNode::Matrix {
        env: "bmatrix".into(),
        rows: vec![vec![LatexNode::text("x")]; 5000],
    };
    assert_eq!(
        write_asciimath(&huge_matrix).unwrap_err().kind,
        AsciiMathErrorKind::LimitExceeded
    );
    let accepted_deep = (0..30).fold(LatexNode::text("x"), |content, _| LatexNode::SquareRoot {
        index: None,
        content: Box::new(content),
    });
    let written = write_asciimath(&accepted_deep).unwrap();
    assert_eq!(canonical(&written), written);
    assert_eq!(
        parse_asciimath(&"x".repeat(65537)).unwrap_err().kind,
        AsciiMathErrorKind::LimitExceeded
    );
    assert_eq!(
        parse_asciimath(&"x ".repeat(4097)).unwrap_err().kind,
        AsciiMathErrorKind::LimitExceeded
    );
    assert_eq!(
        parse_asciimath(&format!("{}x{}", "(".repeat(65), ")".repeat(65)))
            .unwrap_err()
            .kind,
        AsciiMathErrorKind::LimitExceeded
    );
    assert_eq!(
        parse_asciimath(&format!("{}x", "sqrt ".repeat(65)))
            .unwrap_err()
            .kind,
        AsciiMathErrorKind::LimitExceeded
    );
    assert_eq!(
        write_asciimath(&LatexNode::text("x".repeat(65537)))
            .unwrap_err()
            .kind,
        AsciiMathErrorKind::LimitExceeded
    );
    let wide = LatexNode::Sequence(vec![LatexNode::text("1".repeat(32768)); 3]);
    assert_eq!(
        write_asciimath(&wide).unwrap_err().kind,
        AsciiMathErrorKind::LimitExceeded
    );
    let deep = (0..65).fold(LatexNode::text("x"), |content, _| LatexNode::SquareRoot {
        index: None,
        content: Box::new(content),
    });
    assert_eq!(
        write_asciimath(&deep).unwrap_err().kind,
        AsciiMathErrorKind::LimitExceeded
    );
    assert_eq!(canonical("x+1"), "x + 1");
}

#[test]
fn serializer_preserves_compound_script_bases_and_rejects_matrix_ambiguity() {
    let ast = LatexNode::Superscript {
        base: Box::new(LatexNode::text("ab")),
        exp: Box::new(LatexNode::text("2")),
    };
    assert_eq!(write_asciimath(&ast).unwrap(), "{:a b:}^(2)");
    let ast = LatexNode::Subscript {
        base: Box::new(ast),
        sub: Box::new(LatexNode::text("1")),
    };
    let written = write_asciimath(&ast).unwrap();
    assert_eq!(canonical(&written), written);
    // A pair of nested visible square delimiters is not a 1x1 matrix AST.
    let nested = LatexNode::Delimited {
        left: "[".into(),
        right: "]".into(),
        content: vec![LatexNode::Delimited {
            left: "[".into(),
            right: "]".into(),
            content: vec![LatexNode::text("x")],
        }],
    };
    assert_eq!(
        write_asciimath(&nested).unwrap_err().kind,
        AsciiMathErrorKind::UnsupportedSyntax
    );
}

#[test]
fn versioned_synthetic_fixtures_match_canonical_text_and_root_structure() {
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/asciimath-pilot-v1.json")).unwrap();
    assert_eq!(fixtures["schemaVersion"], 1);
    for fixture in fixtures["accepted"].as_array().unwrap() {
        let source = fixture["source"].as_str().unwrap();
        let ast = parse_asciimath(source).unwrap();
        let root = match ast {
            LatexNode::Sequence(_) => "sequence",
            LatexNode::Fraction { .. } => "fraction",
            LatexNode::SquareRoot { .. } => "root",
            LatexNode::Superscript { .. } => "superscript",
            LatexNode::Matrix { .. } => "matrix",
            LatexNode::Delimited { .. } => "delimited",
            _ => panic!("unlisted fixture root"),
        };
        assert_eq!(root, fixture["root"].as_str().unwrap(), "{source}");
        let written = write_asciimath(&ast).unwrap();
        assert_eq!(written, fixture["canonical"].as_str().unwrap(), "{source}");
        assert_eq!(canonical(&written), written);
    }
    for fixture in fixtures["rejected"].as_array().unwrap() {
        let failure = parse_asciimath(fixture["source"].as_str().unwrap()).unwrap_err();
        assert_eq!(
            format!("{:?}", failure.kind),
            fixture["errorKind"].as_str().unwrap()
        );
    }
}

#[test]
fn deterministic_mixed_tokens_never_panic_or_silently_drop_accepted_ast_nodes() {
    let tokens = [
        "x", "2", "alpha", "+", "-", "sqrt", "frac", "root", "/", "_", "^", "(", ")", "[", "]",
        ",", "RR",
    ];
    let mut accepted = 0;
    for first in tokens {
        for second in tokens {
            for third in tokens {
                let source = format!("{first} {second} {third}");
                if let Ok(ast) = parse_asciimath(&source) {
                    accepted += 1;
                    let written =
                        write_asciimath(&ast).unwrap_or_else(|error| panic!("{source}: {error}"));
                    assert_eq!(canonical(&written), written, "{source}");
                }
            }
        }
    }
    assert!(accepted > 100);
}
