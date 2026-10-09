use latexsnipper_ast::Block;
use latexsnipper_conversion::{
    latex_parser::parse_latex,
    latex_utils::split_matrix_rows,
    omml::{latex_to_omml, validate_omml_latex},
    parse_mathml_to_latex, parse_omml_to_latex, DocumentConverter, OutputFormat,
};
use latexsnipper_syntax::latex::parse_latex_with_source_map;

#[test]
fn comments_never_become_operands_or_close_groups_in_any_line_ending() {
    for ending in ["\n", "\r\n", "\r"] {
        for (source, plain) in [
            (
                format!("\\frac{{a% FAKE }}\\sqrt{{{ending}+b}}{{c% FAKE $$ {ending}+d}}"),
                r"\frac{a+b}{c+d}",
            ),
            (format!("x^% FAKE }}\\end{{matrix}}{ending}2+y"), "x^2+y"),
            (format!("\\sqrt[% FAKE ]{ending}3]{{x}}"), r"\sqrt[3]{x}"),
            (
                format!("\\alpha% FAKE \\beta{ending}x+\\text{{foo% FAKE }}{ending}bar}}"),
                r"\alpha x+\text{foobar}",
            ),
        ] {
            validate_omml_latex(&source).unwrap_or_else(|error| panic!("{source:?}: {error}"));
            for target in [
                OutputFormat::OMML,
                OutputFormat::MathML,
                OutputFormat::Typst,
            ] {
                let actual = DocumentConverter::convert_latex_string(&source, target).unwrap();
                let expected = DocumentConverter::convert_latex_string(plain, target).unwrap();
                assert_eq!(actual, expected, "{target:?}: {source:?}");
                assert!(!actual.contains("FAKE"));
            }
            assert!(!parse_latex(&source).to_string().contains("FAKE"));
        }
    }
}

#[test]
fn matrix_comments_cannot_create_rows_columns_or_environment_boundaries() {
    let source =
        "\\begin{array}{l% RAW }\r\nc}a&b% FAKE & \\\\ \\end{array}\r\n\\\\c&d\\end{array}";
    validate_omml_latex(source).unwrap();
    let output = latex_to_omml(source);
    assert_eq!(output.matches("<m:mr>").count(), 2);
    assert_eq!(output.matches("<m:e>").count(), 4);
    assert!(!output.contains("FAKE"));
    assert!(parse_latex(source).to_string().contains("l% RAW }\r\nc"));
    let rows = split_matrix_rows("a% } & \\\\ \\begin{matrix}\n&b\\\\c&d");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].len(), 2);
    let escaped =
        latex_to_omml("\\begin{matrix}50\\%&a\\\\% FAKE & \\end{matrix}\nb&c\\end{matrix}");
    assert_eq!(escaped.matches("<m:mr>").count(), 2);
    assert!(escaped.contains("<m:t>%</m:t>"));
    assert!(!escaped.contains("FAKE"));
    let spaced =
        "\\begin% FAKE {wrong}\r\n{arr% FAKE }\nay}{lc}a&b\\end% FAKE }\r{arr% FAKE }\nay}";
    validate_omml_latex(spaced).unwrap();
    assert_eq!(
        latex_to_omml(spaced),
        latex_to_omml(r"\begin{array}{lc}a&b\end{array}")
    );
}

#[test]
fn escaped_symbols_survive_ast_reprojection_and_text_comments() {
    let source = r"50\%+\&+\#+\_+\$+\{+\}";
    validate_omml_latex(source).unwrap();
    assert_eq!(parse_latex(source).to_string(), source);
    let output = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    for symbol in ["%", "&amp;", "#", "_", "$", "{", "}"] {
        assert!(output.contains(&format!("<mo>{symbol}</mo>")), "{output}");
    }
    for target in [
        OutputFormat::OMML,
        OutputFormat::MathML,
        OutputFormat::Typst,
    ] {
        let output = DocumentConverter::convert_latex_string(r"\text{a\%b\&c}", target).unwrap();
        assert!(output.contains("a%b"), "{target:?}: {output}");
        assert!(!output.contains(r"\%"), "{output}");
    }
}

#[test]
fn xml_visible_percent_and_separators_do_not_turn_into_latex_syntax() {
    let mathml = "<math><mrow><mn>50</mn><mo>%</mo><mo>+</mo><mi>x</mi></mrow></math>";
    let source = parse_mathml_to_latex(mathml).unwrap();
    assert!(source.contains(r"\%"));
    assert!(source.ends_with('x'));
    validate_omml_latex(&source).unwrap();
    let restored = DocumentConverter::convert_latex_string(&source, OutputFormat::MathML).unwrap();
    assert!(restored.contains("<mo>%</mo>"));
    assert!(restored.contains("<mi>x</mi>"));
    let source = parse_omml_to_latex("<m:oMath><m:r><m:t>50%+x</m:t></m:r></m:oMath>").unwrap();
    assert_eq!(source, r"50\%+x");
    let restored = latex_to_omml(&source);
    assert!(restored.contains("<m:t>%</m:t>"));
    assert!(restored.contains("<m:t>x</m:t>"));
    for (xml, expected) in [
        (
            "<math><mtext>a%b&amp;c_{d}$#</mtext></math>",
            r"\text{a\%b\&c\_\{d\}\$\#}",
        ),
        ("<math><mo>{</mo><mi>x</mi><mo>}</mo></math>", r"\{x\}"),
    ] {
        assert_eq!(parse_mathml_to_latex(xml).unwrap(), expected);
    }
    let layout = latexsnipper_conversion::omml_parser::parse_omml_to_layout(
        "<oMath><r><t>50%+x</t></r></oMath>",
    )
    .unwrap();
    let source = layout.canonical_latex();
    assert_eq!(source, r"50\%+x");
    for target in [
        OutputFormat::OMML,
        OutputFormat::MathML,
        OutputFormat::Typst,
    ] {
        let source =
            parse_mathml_to_latex(r"<math><mtext>literal \frac%+x^y~z</mtext></math>").unwrap();
        let output = DocumentConverter::convert_latex_string(&source, target).unwrap();
        assert!(!output.contains("<m:f>"));
        assert!(!output.contains("<mfrac>"));
        assert!(output.contains("%+x^y~z"), "{target:?}: {output}");
        assert!(!output.contains("textasciicircum"), "{output}");
    }
}

#[test]
fn commented_delimiters_do_not_create_formulas_or_truncate_source_spans() {
    let input = "中文 % $FAKE$ $$FAKE$$ \\(FAKE\\)\r\n\\% $a% FAKE $\n+b$ \\(c% FAKE \\)\r+d\\) \\[e% FAKE \\]\r\n+f\\] $$g% FAKE $$\n+h$$";
    let parsed = parse_latex_with_source_map(input).unwrap();
    assert!(
        parsed.document.diagnostics.is_empty(),
        "{:?}",
        parsed.document.diagnostics
    );
    let formulas: Vec<_> = parsed
        .document
        .all_blocks()
        .into_iter()
        .filter_map(|block| {
            if let Block::Formula(formula) = block {
                Some(formula)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(formulas.len(), 4);
    for formula in formulas {
        let span = formula.source.as_ref().unwrap().span.unwrap();
        let mapped = parsed
            .source_map
            .span_for(
                formula
                    .source
                    .as_ref()
                    .unwrap()
                    .stable_id
                    .as_deref()
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(span, mapped);
        assert!(input[span.start..span.end].contains(formula.formula.as_latex()));
        assert!(
            formula.formula.as_latex().contains("FAKE"),
            "Raw commented source was not retained."
        );
    }
    let parsed = parse_latex_with_source_map(r"\[a\\]b\]").unwrap();
    let Block::Formula(formula) = &parsed.document.pages[0].blocks[0] else {
        panic!("Expected formula");
    };
    assert_eq!(formula.formula.as_latex(), r"a\\]b");
}

#[test]
fn comment_only_and_actually_malformed_inputs_remain_non_replaceable() {
    for source in [
        "% only",
        "\\frac{% fake }\na}{",
        "% fake {}\n\\unknown{x}",
        "x^% no operand",
    ] {
        assert!(validate_omml_latex(source).is_err(), "{source:?}");
    }
    let source = format!("x% {}\n+y", "\\frac{{".repeat(600));
    validate_omml_latex(&source).unwrap();
    assert_eq!(latex_to_omml(&source), latex_to_omml("x+y"));
}

#[test]
fn unknown_and_verbatim_environments_are_preserved_not_partially_accepted() {
    for source in [
        "\\begin{verbatim}a% literal\\frac{b}{c}\n\\end{verbatim}",
        r"\begin{unknown}x+y\end{unknown}",
    ] {
        assert_eq!(parse_latex(source).to_string(), source);
        assert!(validate_omml_latex(source).is_err());
        let output = latex_to_omml(source);
        assert!(output.contains("\\begin{"), "{output}");
        assert!(!output.contains("<m:f>"), "{output}");
        let output = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
        assert!(output.contains("\\begin{"), "{output}");
        assert!(output.contains("<mtext>"), "{output}");
    }
}
