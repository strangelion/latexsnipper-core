use latexsnipper_conversion::{
    conversion_registry::{
        ConversionRegistryErrorCode, RegisteredConversionRequest, SemanticConversionRegistry,
    },
    CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode as Mode,
    FormulaInputFormat as Input, FormulaOutputTarget as Target, OutputFormat,
};

#[test]
fn registered_growth_routes_preserve_source_identity_and_do_not_retry_lossy_targets() {
    use sha2::{Digest, Sha256};
    let registry = SemanticConversionRegistry::default();
    let request = RegisteredConversionRequest {
        schema_version: 1,
        input: "unicode-math".into(),
        output: "omml".into(),
        mode: Mode::BestEffort,
        backend: None,
        content: "∫65_0^1▒x+b".into(),
        context_sha256: "a".repeat(64),
    };
    let result = registry.convert(&request).unwrap();
    assert_eq!(result.backend, "core:builtin");
    assert_eq!(
        result.input_sha256,
        format!("{:x}", Sha256::digest(request.content.as_bytes()))
    );
    assert!(result.candidate.content.contains("<m:grow m:val=\"0\"/>"));
    let mut readback = request.clone();
    readback.input = "omml".into();
    readback.output = "unicode-math".into();
    readback.content = result.candidate.content;
    let restored = registry.convert(&readback).unwrap();
    assert_eq!(restored.candidate.content, "〖〖∫65〗_(0)〗^(1)▒〖x〗 + b");
    let mut mathml = request.clone();
    mathml.output = "mathml".into();
    let result = registry.convert(&mathml).unwrap();
    assert_eq!(result.backend, "core:builtin");
    assert_eq!(
        result.input_sha256,
        format!("{:x}", Sha256::digest(request.content.as_bytes()))
    );
    assert!(result.candidate.content.contains("stretchy=\"false\""));
    readback.input = "mathml".into();
    readback.content = result.candidate.content;
    assert_eq!(
        registry.convert(&readback).unwrap().candidate.content,
        "〖〖∫65〗_(0)〗^(1)▒〖x〗 + b"
    );
    let mut lossy = request.clone();
    lossy.output = "typst".into();
    let rejected = registry.convert(&lossy).unwrap_err();
    assert_eq!(rejected.code, ConversionRegistryErrorCode::InputRejected);
    assert!(rejected.message.contains("growth"));
    assert_eq!(request.content, "∫65_0^1▒x+b");
    assert!(registry.convert(&request).is_ok());
}

#[test]
fn registered_nary_formula_routes_keep_hashes_bound_to_unmodified_input() {
    let source = "∑_(i=1)^n▒〖x_i+y_i〗+b";
    let registry = SemanticConversionRegistry::default();
    for output in ["unicode-math", "omml", "mathml"] {
        let request = RegisteredConversionRequest {
            schema_version: 1,
            input: "unicode-math".into(),
            output: output.into(),
            mode: Mode::BestEffort,
            backend: None,
            content: source.into(),
            context_sha256: "c".repeat(64),
        };
        let result = registry.convert(&request).unwrap();
        assert_eq!(request.content, source);
        use sha2::{Digest, Sha256};
        assert_eq!(
            result.input_sha256,
            format!("{:x}", Sha256::digest(source.as_bytes()))
        );
        assert!(result.candidate.content.contains('∑'));
    }
    assert!(DocumentConverter::convert_formula_to_target(
        source,
        Input::UnicodeMath,
        Target::UnicodeMath,
        Mode::Strict
    )
    .is_err());
}

#[test]
fn registered_named_functions_use_existing_routes_and_remain_best_effort() {
    let registry = SemanticConversionRegistry::default();
    for (source, input) in [
        ("sin⁡〖x+y〗", Input::UnicodeMath),
        (r"{\sin{x+y}}", Input::Latex),
    ] {
        let output = DocumentConverter::convert_formula_to_target(
            source,
            input,
            Target::UnicodeMath,
            Mode::BestEffort,
        )
        .unwrap();
        assert_eq!(output, "sin⁡〖x + y〗");
    }
    for output in ["unicode-math", "omml", "mathml"] {
        let request = RegisteredConversionRequest {
            schema_version: 1,
            input: "unicode-math".into(),
            output: output.into(),
            mode: Mode::BestEffort,
            backend: None,
            content: "exp⁡〖x+y〗".into(),
            context_sha256: "b".repeat(64),
        };
        let result = registry.convert(&request).unwrap();
        assert_eq!(result.input, "unicode-math");
        assert!(!result.candidate.content.is_empty());
        assert!(!result.candidate.content.contains(r"\exp"));
    }
    assert!(DocumentConverter::convert_formula_to_target(
        "sin x",
        Input::UnicodeMath,
        Target::UnicodeMath,
        Mode::Strict
    )
    .is_err());
}

#[test]
fn registered_identifier_functions_keep_source_identity_and_loss_boundaries() {
    use sha2::{Digest, Sha256};
    let registry = SemanticConversionRegistry::default();
    let source = "CDF42_i⁡〖x+y〗+b";
    for output in ["unicode-math", "omml", "mathml"] {
        let request = RegisteredConversionRequest {
            schema_version: 1,
            input: "unicode-math".into(),
            output: output.into(),
            mode: Mode::BestEffort,
            backend: None,
            content: source.into(),
            context_sha256: "a".repeat(64),
        };
        let result = registry.convert(&request).unwrap();
        assert_eq!(
            result.input_sha256,
            format!("{:x}", Sha256::digest(source.as_bytes()))
        );
        assert_eq!(result.backend, "core:builtin");
        assert!(result.candidate.content.contains("CDF42"));
        assert!(!result.candidate.losses.is_empty());
        let rejected = registry
            .convert(&RegisteredConversionRequest {
                output: "ascii-math".into(),
                ..request.clone()
            })
            .unwrap_err();
        assert_eq!(rejected.code, ConversionRegistryErrorCode::InputRejected);
        assert_eq!(request.content, source);
    }
}

#[test]
fn registered_unicode_text_routes_keep_literals_and_original_input_identity() {
    let source = r#""rate"="distance"/"time""#;
    let output = DocumentConverter::convert_formula_to_target(
        r"\text{if x > 0}",
        Input::Latex,
        Target::UnicodeMath,
        Mode::BestEffort,
    )
    .unwrap();
    assert_eq!(output, r#""if x > 0""#);
    let registry = SemanticConversionRegistry::default();
    for target in ["unicode-math", "omml", "mathml"] {
        let request = RegisteredConversionRequest {
            schema_version: 1,
            input: "unicode-math".into(),
            output: target.into(),
            mode: Mode::BestEffort,
            backend: None,
            content: source.into(),
            context_sha256: "a".repeat(64),
        };
        let result = registry.convert(&request).unwrap();
        assert_eq!(request.content, source);
        assert_eq!(result.input_sha256, {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(source.as_bytes()))
        });
        assert!(result
            .candidate
            .losses
            .iter()
            .all(|loss| !loss.code.is_empty()));
        assert!(!result.candidate.content.is_empty());
    }
    assert!(DocumentConverter::convert_formula_to_target(
        source,
        Input::UnicodeMath,
        Target::AsciiMath,
        Mode::BestEffort
    )
    .is_err());
}

#[test]
fn every_accepted_linear_fixture_has_the_exact_registered_canonical_output() {
    for (fixture, input, target) in [
        (
            include_str!("fixtures/asciimath-pilot-v1.json"),
            Input::AsciiMath,
            Target::AsciiMath,
        ),
        (
            include_str!("fixtures/unicodemath-pilot-v1.json"),
            Input::UnicodeMath,
            Target::UnicodeMath,
        ),
    ] {
        let fixture: serde_json::Value = serde_json::from_str(fixture).unwrap();
        for entry in fixture["accepted"].as_array().unwrap() {
            let source = entry["source"].as_str().unwrap();
            let output = DocumentConverter::convert_formula_to_target(
                source,
                input,
                target,
                Mode::BestEffort,
            )
            .unwrap();
            assert_eq!(output, entry["canonical"].as_str().unwrap(), "{source}");
            for format in [
                OutputFormat::LatexDisplay,
                OutputFormat::MathML,
                OutputFormat::OMML,
            ] {
                let output = DocumentConverter::convert_formula_string(
                    source,
                    input,
                    format,
                    Mode::BestEffort,
                )
                .unwrap();
                assert!(!output.is_empty());
            }
        }
        for entry in fixture["rejected"].as_array().unwrap() {
            let source = entry["source"].as_str().unwrap();
            assert!(DocumentConverter::convert_formula_to_target(
                source,
                input,
                target,
                Mode::BestEffort
            )
            .is_err());
        }
    }
}

#[test]
fn formal_dispatch_preserves_grammar_differences_and_unknown_syntax_refusals() {
    let convert = |source, input, target| {
        DocumentConverter::convert_formula_to_target(source, input, target, Mode::BestEffort)
    };
    assert_ne!(
        convert("abc/d", Input::AsciiMath, Target::UnicodeMath).unwrap(),
        convert("abc/d", Input::UnicodeMath, Target::UnicodeMath).unwrap()
    );
    assert!(convert("a/b/c", Input::AsciiMath, Target::AsciiMath).is_err());
    assert!(convert("a/b/c", Input::UnicodeMath, Target::AsciiMath).is_ok());
    for source in [
        r"\unknown{x}",
        r"\documentclass{article}x",
        r"\color{red}{x}",
    ] {
        assert!(
            convert(source, Input::Latex, Target::AsciiMath).is_err(),
            "{source}"
        );
        assert!(
            convert(source, Input::Latex, Target::UnicodeMath).is_err(),
            "{source}"
        );
    }
    assert!(convert("body $x$", Input::Markdown, Target::AsciiMath).is_err());
    assert!(convert("$x$ and $y$", Input::Markdown, Target::UnicodeMath).is_err());
    assert!(convert("$x$", Input::Markdown, Target::AsciiMath).is_ok());
    assert_eq!(OutputFormat::all().len(), 9); // Document conversion remains unchanged.
}

#[test]
fn registered_builtin_linear_routes_validate_candidates_and_preserve_source_identity() {
    let registry = SemanticConversionRegistry::default();
    let request = RegisteredConversionRequest {
        schema_version: 1,
        input: "ascii-math".into(),
        output: "unicode-math".into(),
        mode: Mode::BestEffort,
        backend: None,
        content: "(a+b)/c".into(),
        context_sha256: "a".repeat(64),
    };
    let result = registry.convert(&request).unwrap();
    assert_eq!(result.backend, "core:builtin");
    assert_eq!(result.candidate.content, "(a + b)/(c)");
    assert_eq!(result.mime_type, "text/plain; charset=utf-8");
    assert!(!result.candidate.losses.is_empty());
    let mut invalid = request.clone();
    invalid.content = "color(red)(x)".into();
    assert_eq!(
        registry.convert(&invalid).unwrap_err().code,
        ConversionRegistryErrorCode::InputRejected
    );
    assert!(registry.convert(&request).is_ok());
    for platform in [
        CapabilityTarget::Native,
        CapabilityTarget::Wasm32UnknownUnknown,
    ] {
        assert!(
            CapabilityRegistry::formula_conversion_target(
                Input::AsciiMath,
                Target::UnicodeMath,
                Mode::BestEffort,
                platform
            )
            .available
        );
        assert!(
            !CapabilityRegistry::formula_conversion_target(
                Input::AsciiMath,
                Target::UnicodeMath,
                Mode::Strict,
                platform
            )
            .available
        );
    }
    assert!(DocumentConverter::convert_formula_to_target(
        &"x".repeat(65537),
        Input::UnicodeMath,
        Target::UnicodeMath,
        Mode::BestEffort
    )
    .is_err());
}
