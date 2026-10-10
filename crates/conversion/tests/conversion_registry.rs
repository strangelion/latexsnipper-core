use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

use latexsnipper_conversion::{
    conversion_registry::*, CapabilityTarget, DocumentConverter, FormulaConversionMode,
};

fn request(input: &str, output: &str) -> RegisteredConversionRequest {
    RegisteredConversionRequest {
        schema_version: CONVERSION_REGISTRY_VERSION,
        input: input.into(),
        output: output.into(),
        mode: FormulaConversionMode::BestEffort,
        backend: None,
        content: "x_i^2".into(),
        context_sha256: "a".repeat(64),
    }
}
fn descriptor(id: &str, input: &str, output: &str) -> ConversionBackendDescriptor {
    ConversionBackendDescriptor {
        schema_version: CONVERSION_REGISTRY_VERSION,
        id: id.into(),
        version: "1.0.0".into(),
        configuration_sha256: "b".repeat(64),
        license: "MIT".into(),
        limitations: vec!["finite test syntax only".into()],
        routes: vec![ConversionBackendRoute {
            input: input.into(),
            output: output.into(),
            mode: FormulaConversionMode::BestEffort,
            target: CapabilityTarget::Native,
        }],
        max_input_bytes: 1024,
        max_output_bytes: 4096,
    }
}
struct Handler {
    ready: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    output: String,
    fail: bool,
}
impl RegisteredFormulaBackend for Handler {
    fn check_ready(&self) -> Result<(), ConversionRegistryError> {
        if self.ready.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ConversionRegistryError::new(
                ConversionRegistryErrorCode::BackendFailed,
                "revoked",
            ))
        }
    }
    fn convert(
        &self,
        _: &RegisteredConversionRequest,
    ) -> Result<ConversionCandidate, ConversionRegistryError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(ConversionRegistryError::new(
                ConversionRegistryErrorCode::BackendFailed,
                "selected backend refused syntax",
            ));
        }
        Ok(ConversionCandidate {
            content: self.output.clone(),
            losses: vec![ConversionLoss {
                code: "DEMO_LIMITED".into(),
                message: "Finite candidate, not a fidelity guarantee".into(),
            }],
        })
    }
}
fn handler(output: &str) -> (Handler, Arc<AtomicBool>, Arc<AtomicUsize>) {
    let ready = Arc::new(AtomicBool::new(true));
    let calls = Arc::new(AtomicUsize::new(0));
    (
        Handler {
            ready: ready.clone(),
            calls: calls.clone(),
            output: output.into(),
            fail: false,
        },
        ready,
        calls,
    )
}

#[test]
fn builtin_routes_still_execute_without_extension_selection() {
    let registry = SemanticConversionRegistry::default();
    for output in [
        "latex",
        "latex_display",
        "latex_equation",
        "typst",
        "markdown_inline",
        "markdown_block",
        "mathml",
        "omml",
        "html",
    ] {
        let result =
            DocumentConverter::convert_registered_formula(&registry, &request("latex", output))
                .unwrap();
        assert_eq!(result.backend, BUILTIN_CONVERSION_BACKEND);
        assert!(!result.candidate.content.is_empty());
        assert_eq!(result.input_sha256.len(), 64);
        assert_eq!(result.candidate.losses[0].code, "UNVERIFIED_BEST_EFFORT");
    }
    assert!(registry
        .capabilities()
        .iter()
        .any(
            |capability| capability.backend == BUILTIN_CONVERSION_BACKEND && capability.available
        ));
}

#[test]
fn new_formats_have_callable_validation_and_never_shadow_builtins() {
    let mut registry = SemanticConversionRegistry::default();
    let format = ConversionFormatDescriptor {
        schema_version: 1,
        id: "demo:json-formula".into(),
        mime_type: "application/json".into(),
        description: "Finite formula JSON".into(),
    };
    registry
        .register_trusted_format(format.clone(), |source| {
            serde_json::from_str::<serde_json::Value>(source)
                .map(|_| ())
                .map_err(|failure| {
                    ConversionRegistryError::new(
                        ConversionRegistryErrorCode::InputRejected,
                        failure.to_string(),
                    )
                })
        })
        .unwrap();
    assert_eq!(registry.extension_formats()[0].id, format.id);
    assert_eq!(
        registry
            .register_trusted_format(format.clone(), |_| Ok(()))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::DuplicateRegistration
    );
    for id in [
        "omml",
        "mml",
        "core:foreign",
        "Demo:foreign",
        "demo:../foreign",
        "demo:foreign:extra",
    ] {
        let mut rejected = format.clone();
        rejected.id = id.into();
        assert_eq!(
            registry
                .register_trusted_format(rejected, |_| Ok(()))
                .unwrap_err()
                .code,
            ConversionRegistryErrorCode::InvalidDeclaration
        );
    }
    let (backend, _, calls) = handler("x^2");
    registry
        .register_trusted_backend(descriptor("demo:json-read", &format.id, "latex"), backend)
        .unwrap();
    registry
        .set_backend_enabled("demo:json-read", true)
        .unwrap();
    let mut input = request(&format.id, "latex");
    input.content = r#"{"variable":"x"}"#.into();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::RouteUnavailable
    );
    input.backend = Some("demo:json-read".into());
    assert_eq!(registry.convert(&input).unwrap().candidate.content, "x^2");
    input.content = "not JSON".into();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::InputRejected
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn extensions_are_disabled_explicit_and_rechecked_after_enable() {
    let mut registry = SemanticConversionRegistry::default();
    let (backend, ready, calls) = handler("repaired");
    registry
        .register_trusted_backend(descriptor("demo:repair", "latex", "latex"), backend)
        .unwrap();
    let mut input = request("latex", "latex");
    input.backend = Some("demo:repair".into());
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::BackendDisabled
    );
    assert!(
        !registry
            .capabilities()
            .iter()
            .find(|capability| capability.backend == "demo:repair")
            .unwrap()
            .available
    );
    registry.set_backend_enabled("demo:repair", true).unwrap();
    let result = registry.convert(&input).unwrap();
    assert_eq!(result.candidate.content, "repaired");
    assert_eq!(result.limitations, ["finite test syntax only"]);
    input.backend = None;
    assert_ne!(
        registry.convert(&input).unwrap().candidate.content,
        "repaired"
    );
    ready.store(false, Ordering::SeqCst);
    input.backend = Some("demo:repair".into());
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::BackendFailed
    );
    assert!(
        !registry
            .capabilities()
            .iter()
            .find(|capability| capability.backend == "demo:repair")
            .unwrap()
            .available
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    registry.set_backend_enabled("demo:repair", false).unwrap();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::BackendDisabled
    );
}

#[test]
fn failing_selected_backend_does_not_fallback_or_retry() {
    let mut registry = SemanticConversionRegistry::default();
    let (mut backend, _, calls) = handler("ignored");
    backend.fail = true;
    registry
        .register_trusted_backend(descriptor("demo:failure", "latex", "latex"), backend)
        .unwrap();
    registry.set_backend_enabled("demo:failure", true).unwrap();
    let mut input = request("latex", "latex");
    input.backend = Some("demo:failure".into());
    let source = input.content.clone();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::BackendFailed
    );
    assert_eq!(input.content, source);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn declarations_versions_routes_and_budgets_are_atomic() {
    let mut registry = SemanticConversionRegistry::default();
    for mutation in 0..7 {
        let mut invalid = descriptor("demo:invalid", "latex", "latex");
        match mutation {
            0 => invalid.schema_version = 2,
            1 => invalid.id = BUILTIN_CONVERSION_BACKEND.into(),
            2 => invalid.routes[0].output = "demo:unregistered".into(),
            3 => invalid.routes.push(invalid.routes[0].clone()),
            4 => invalid.max_input_bytes = 65537,
            5 => invalid.configuration_sha256 = "wrong".into(),
            _ => invalid.routes[0].mode = FormulaConversionMode::Strict,
        }
        assert!(registry
            .register_trusted_backend(invalid, handler("x").0)
            .is_err());
    }
    registry
        .register_trusted_backend(descriptor("demo:invalid", "latex", "latex"), handler("x").0)
        .unwrap();
    assert_eq!(
        registry
            .register_trusted_backend(descriptor("demo:invalid", "latex", "latex"), handler("x").0)
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::DuplicateRegistration
    );
    assert_eq!(
        registry
            .set_backend_enabled("missing", true)
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::UnknownBackend
    );
    let mut input = request("latex", "latex");
    input.schema_version = 2;
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::VersionMismatch
    );
    input.schema_version = 1;
    input.context_sha256 = "missing".into();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::InvalidDeclaration
    );
    input.context_sha256 = "a".repeat(64);
    input.content = "x".repeat(65537);
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::LimitExceeded
    );
}

#[test]
fn backend_route_platform_and_per_handler_limits_are_checked() {
    let mut registry = SemanticConversionRegistry::default();
    let mut declaration = descriptor("demo:bounded", "latex", "latex");
    declaration.max_output_bytes = 4;
    let (backend, _, calls) = handler("too large");
    registry
        .register_trusted_backend(declaration, backend)
        .unwrap();
    registry.set_backend_enabled("demo:bounded", true).unwrap();
    let mut input = request("latex", "latex");
    input.backend = Some("demo:bounded".into());
    input.output = "omml".into();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::RouteUnavailable
    );
    input.output = "latex".into();
    input.content = "x".repeat(1025);
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::LimitExceeded
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    input.content = "x".into();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::LimitExceeded
    );
    let mut wasm = SemanticConversionRegistry::new(CapabilityTarget::Wasm32UnknownUnknown);
    wasm.register_trusted_backend(descriptor("demo:native", "latex", "latex"), handler("x").0)
        .unwrap();
    wasm.set_backend_enabled("demo:native", true).unwrap();
    input.backend = Some("demo:native".into());
    assert_eq!(
        wasm.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::RouteUnavailable
    );
}

#[test]
fn result_identity_binds_source_route_backend_configuration_and_context() {
    fn registered(configuration: &str, version: &str) -> SemanticConversionRegistry {
        let mut registry = SemanticConversionRegistry::default();
        let mut definition = descriptor("demo:identity", "latex", "latex");
        definition.configuration_sha256 = configuration.into();
        definition.version = version.into();
        registry
            .register_trusted_backend(definition, handler("candidate").0)
            .unwrap();
        registry.set_backend_enabled("demo:identity", true).unwrap();
        registry
    }
    let registry = registered(&"b".repeat(64), "1.0.0");
    let mut input = request("latex", "latex");
    input.backend = Some("demo:identity".into());
    let original = registry.convert(&input).unwrap();
    assert_eq!(
        registry.convert(&input).unwrap().route_sha256,
        original.route_sha256
    );
    input.context_sha256 = "c".repeat(64);
    assert_ne!(
        registry.convert(&input).unwrap().route_sha256,
        original.route_sha256
    );
    input.context_sha256 = "a".repeat(64);
    input.content.push('y');
    assert_ne!(
        registry.convert(&input).unwrap().route_sha256,
        original.route_sha256
    );
    input.content.pop();
    assert_ne!(
        registered(&"d".repeat(64), "1.0.0")
            .convert(&input)
            .unwrap()
            .route_sha256,
        original.route_sha256
    );
    assert_ne!(
        registered(&"b".repeat(64), "2.0.0")
            .convert(&input)
            .unwrap()
            .route_sha256,
        original.route_sha256
    );
}

#[test]
fn math_output_is_validated_after_backend_execution() {
    for content in [
        "<script>active</script>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML'><mi href='https://invalid'>x</mi></math>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML'><mi ONCLICK='active'>x</mi></math>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML' altimg='https://invalid'/>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML'><mtext><style>active</style></mtext></math>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML'><script>x</script></math>",
        "<!DOCTYPE math><math xmlns='http://www.w3.org/1998/Math/MathML'/>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML'/><math xmlns='http://www.w3.org/1998/Math/MathML'/>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML'><mi>x</math>",
        "<math><mi>x</mi></math>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML'><mi xmlns='urn:foreign'>x</mi></math>",
        "<math xmlns='http://www.w3.org/1998/Math/MathML'><mi>\0</mi></math>",
        "&amp;<math xmlns='http://www.w3.org/1998/Math/MathML'/>",
    ] {
        let mut registry = SemanticConversionRegistry::default();
        registry.register_trusted_backend(descriptor("demo:unsafe", "latex", "mathml"), handler(content).0).unwrap();
        registry.set_backend_enabled("demo:unsafe", true).unwrap();
        let mut input = request("latex", "mathml"); input.backend = Some("demo:unsafe".into());
        assert_eq!(registry.convert(&input).unwrap_err().code, ConversionRegistryErrorCode::OutputRejected, "{content}");
    }
}

#[test]
fn strict_extension_routes_keep_existing_source_validation() {
    let mut registry = SemanticConversionRegistry::default();
    let output = DocumentConverter::convert_formula_string(
        "x",
        latexsnipper_conversion::FormulaInputFormat::Latex,
        latexsnipper_conversion::OutputFormat::OMML,
        FormulaConversionMode::Strict,
    )
    .unwrap();
    let (backend, _, calls) = handler(&output);
    let mut definition = descriptor("demo:strict", "latex", "omml");
    definition.routes[0].mode = FormulaConversionMode::Strict;
    registry
        .register_trusted_backend(definition, backend)
        .unwrap();
    registry.set_backend_enabled("demo:strict", true).unwrap();
    let mut input = request("latex", "omml");
    input.backend = Some("demo:strict".into());
    input.mode = FormulaConversionMode::Strict;
    assert!(registry.convert(&input).is_ok());
    input.content = r"\unknown{x}".into();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::InputRejected
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn styled_builtin_xml_has_bound_namespaces_and_valid_word_property_owners() {
    use quick_xml::{events::Event, name::ResolveResult, NsReader};
    let registry = SemanticConversionRegistry::default();
    for source in [
        r"\textcolor{red}{x}",
        r"\color{green}x+y",
        r"\large x",
        r"\textcolor{red}{x+\textcolor{blue}{y}+z}",
    ] {
        let mut input = request("latex", "omml");
        input.content = source.into();
        let result = registry.convert(&input).unwrap();
        let mut reader = NsReader::from_str(&result.candidate.content);
        let mut stack: Vec<Vec<u8>> = Vec::new();
        loop {
            let (namespace, event) = reader.read_resolved_event().unwrap();
            match event {
                Event::Start(element) => {
                    assert!(matches!(namespace, ResolveResult::Bound(_)));
                    if element.name().as_ref() == b"w:rPr" {
                        assert_eq!(stack.last().map(Vec::as_slice), Some(b"m:r".as_slice()));
                    }
                    stack.push(element.name().as_ref().to_vec());
                }
                Event::Empty(_) => assert!(matches!(namespace, ResolveResult::Bound(_))),
                Event::End(_) => {
                    stack.pop();
                }
                Event::Eof => break,
                _ => {}
            }
        }
        if source.contains("blue") {
            assert_eq!(
                result.candidate.content.matches("w:val=\"FF0000\"").count(),
                4
            );
            assert_eq!(
                result.candidate.content.matches("w:val=\"0000FF\"").count(),
                1
            );
        }
        input.output = "mathml".into();
        let mathml = registry.convert(&input).unwrap().candidate.content;
        assert!(mathml.contains("<mstyle displaystyle=\"true\">"));
        assert!(!mathml.contains("displaymath"));
    }
}
