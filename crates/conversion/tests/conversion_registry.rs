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

fn fallback_policy(ids: &[&str]) -> ConversionFallbackPolicy {
    ConversionFallbackPolicy {
        schema_version: CONVERSION_REGISTRY_VERSION,
        backends: ids.iter().map(|id| (*id).to_string()).collect(),
    }
}

fn fallback_error(report: &ConversionFallbackReport) -> ConversionRegistryErrorCode {
    match &report.outcome {
        ConversionFallbackOutcome::Rejected { error } => error.code,
        other => panic!("expected a rejected decision: {other:?}"),
    }
}

#[test]
fn explicit_fallback_skips_only_registry_owned_availability_failures() {
    let mut registry = SemanticConversionRegistry::default();
    let (disabled, _, disabled_calls) = handler("disabled");
    let (wrong_route, _, wrong_calls) = handler("wrong route");
    let (good, _, good_calls) = handler("safe result");
    registry
        .register_trusted_backend(descriptor("demo:disabled", "latex", "latex"), disabled)
        .unwrap();
    registry
        .register_trusted_backend(
            descriptor("demo:other-route", "mathml", "latex"),
            wrong_route,
        )
        .unwrap();
    registry
        .register_trusted_backend(descriptor("demo:good", "latex", "latex"), good)
        .unwrap();
    registry
        .set_backend_enabled("demo:other-route", true)
        .unwrap();
    registry.set_backend_enabled("demo:good", true).unwrap();
    let mut input = request("latex", "latex");
    input.backend = Some("demo:missing".into());
    let before = serde_json::to_string(&input).unwrap();
    assert_eq!(
        registry.convert(&input).unwrap_err().code,
        ConversionRegistryErrorCode::UnknownBackend
    );
    let report = registry.convert_with_fallback(
        &input,
        &fallback_policy(&["demo:disabled", "demo:other-route", "demo:good"]),
    );
    let ConversionFallbackOutcome::Converted { result } = &report.outcome else {
        panic!("{report:?}");
    };
    assert_eq!(result.backend, "demo:good");
    assert_eq!(result.candidate.content, "safe result");
    assert_eq!(report.requested_backend, "demo:missing");
    assert_eq!(report.attempts.len(), 4);
    for (attempt, code) in report.attempts.iter().zip([
        ConversionRegistryErrorCode::UnknownBackend,
        ConversionRegistryErrorCode::BackendDisabled,
        ConversionRegistryErrorCode::RouteUnavailable,
    ]) {
        assert!(
            matches!(&attempt.status, ConversionFallbackAttemptStatus::Unavailable { error } if error.code == code)
        );
    }
    assert_eq!(disabled_calls.load(Ordering::SeqCst), 0);
    assert_eq!(wrong_calls.load(Ordering::SeqCst), 0);
    assert_eq!(good_calls.load(Ordering::SeqCst), 1);
    assert_eq!(serde_json::to_string(&input).unwrap(), before);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["coreVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(json["route"]["input"], input.input);
    assert_eq!(json["route"]["output"], input.output);
    assert_eq!(json["outcome"]["kind"], "converted");
    assert_eq!(json["attempts"][3]["status"]["kind"], "converted");
}

#[test]
fn fallback_policy_is_default_off_bounded_and_validated_before_any_callback() {
    let registry = SemanticConversionRegistry::default();
    let mut input = request("latex", "latex");
    input.backend = Some("demo:missing".into());
    let report = registry.convert_with_fallback(&input, &ConversionFallbackPolicy::default());
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(
        fallback_error(&report),
        ConversionRegistryErrorCode::UnknownBackend
    );
    for (policy, expected) in [
        (
            fallback_policy(&["demo:missing"]),
            ConversionRegistryErrorCode::DuplicateRegistration,
        ),
        (
            fallback_policy(&["demo:good", "demo:good"]),
            ConversionRegistryErrorCode::DuplicateRegistration,
        ),
        (
            fallback_policy(&["not canonical"]),
            ConversionRegistryErrorCode::InvalidDeclaration,
        ),
        (
            fallback_policy(&["core:unknown"]),
            ConversionRegistryErrorCode::InvalidDeclaration,
        ),
        (
            fallback_policy(&["demo:a", "demo:b", "demo:c", "demo:d"]),
            ConversionRegistryErrorCode::LimitExceeded,
        ),
        (
            ConversionFallbackPolicy {
                schema_version: 2,
                backends: Vec::new(),
            },
            ConversionRegistryErrorCode::VersionMismatch,
        ),
    ] {
        let report = registry.convert_with_fallback(&input, &policy);
        assert_eq!(fallback_error(&report), expected);
        assert!(report.attempts.is_empty());
        assert!(report.route.is_none());
        assert!(report.policy_sha256.is_none());
        assert!(report.request_sha256.is_none());
        assert!(report.decision_sha256.is_none());
    }
    assert!(serde_json::from_str::<ConversionFallbackPolicy>(
        r#"{"schemaVersion":1,"backends":[],"retryOnTimeout":true}"#
    )
    .is_err());
}

struct AvailabilityCodeImpersonator {
    code: ConversionRegistryErrorCode,
    fail_readiness: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}
impl RegisteredFormulaBackend for AvailabilityCodeImpersonator {
    fn check_ready(&self) -> Result<(), ConversionRegistryError> {
        if self.fail_readiness.load(Ordering::SeqCst) {
            Err(ConversionRegistryError::new(
                self.code,
                "revoked or cancelled",
            ))
        } else {
            Ok(())
        }
    }
    fn convert(
        &self,
        _: &RegisteredConversionRequest,
    ) -> Result<ConversionCandidate, ConversionRegistryError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(ConversionRegistryError::new(
            self.code,
            "handler cannot authorize retry",
        ))
    }
}

#[test]
fn handler_errors_cannot_impersonate_preflight_availability_or_trigger_retry() {
    for code in [
        ConversionRegistryErrorCode::UnknownBackend,
        ConversionRegistryErrorCode::BackendDisabled,
        ConversionRegistryErrorCode::RouteUnavailable,
        ConversionRegistryErrorCode::BackendFailed,
        ConversionRegistryErrorCode::LimitExceeded,
        ConversionRegistryErrorCode::InputRejected,
    ] {
        for readiness in [false, true] {
            let mut registry = SemanticConversionRegistry::default();
            let fail_readiness = Arc::new(AtomicBool::new(false));
            let calls = Arc::new(AtomicUsize::new(0));
            registry
                .register_trusted_backend(
                    descriptor("demo:primary", "latex", "latex"),
                    AvailabilityCodeImpersonator {
                        code,
                        fail_readiness: fail_readiness.clone(),
                        calls: calls.clone(),
                    },
                )
                .unwrap();
            let (good, _, good_calls) = handler("safe");
            registry
                .register_trusted_backend(descriptor("demo:good", "latex", "latex"), good)
                .unwrap();
            registry.set_backend_enabled("demo:primary", true).unwrap();
            registry.set_backend_enabled("demo:good", true).unwrap();
            fail_readiness.store(readiness, Ordering::SeqCst);
            let mut input = request("latex", "latex");
            input.backend = Some("demo:primary".into());
            let report = registry.convert_with_fallback(&input, &fallback_policy(&["demo:good"]));
            assert_eq!(fallback_error(&report), code);
            assert_eq!(report.attempts.len(), 1);
            assert!(matches!(
                report.attempts[0].status,
                ConversionFallbackAttemptStatus::Rejected { .. }
            ));
            assert_eq!(calls.load(Ordering::SeqCst), usize::from(!readiness));
            assert_eq!(good_calls.load(Ordering::SeqCst), 0);
        }
    }
}

#[test]
fn fallback_input_output_and_budget_errors_are_terminal() {
    let mut registry = SemanticConversionRegistry::default();
    let (bad, _, _) =
        handler("<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><script/></math>");
    let (good, _, good_calls) =
        handler("<math xmlns=\"http://www.w3.org/1998/Math/MathML\"><mi>x</mi></math>");
    registry
        .register_trusted_backend(descriptor("demo:bad", "latex", "mathml"), bad)
        .unwrap();
    registry
        .register_trusted_backend(descriptor("demo:good", "latex", "mathml"), good)
        .unwrap();
    registry.set_backend_enabled("demo:bad", true).unwrap();
    registry.set_backend_enabled("demo:good", true).unwrap();
    let mut input = request("latex", "mathml");
    input.backend = Some("demo:bad".into());
    let policy = fallback_policy(&["demo:good"]);
    let report = registry.convert_with_fallback(&input, &policy);
    assert_eq!(
        fallback_error(&report),
        ConversionRegistryErrorCode::OutputRejected
    );
    assert_eq!(report.attempts.len(), 1);
    input.content = "x".repeat(1025);
    let report = registry.convert_with_fallback(&input, &policy);
    assert_eq!(
        fallback_error(&report),
        ConversionRegistryErrorCode::LimitExceeded
    );
    assert_eq!(report.attempts.len(), 1);
    input.content = "x".repeat(65537);
    let report = registry.convert_with_fallback(&input, &policy);
    assert_eq!(
        fallback_error(&report),
        ConversionRegistryErrorCode::LimitExceeded
    );
    assert!(report.attempts.is_empty());
    assert_eq!(good_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn fallback_decision_identity_binds_policy_input_context_and_unavailable_versions() {
    let mut registry = SemanticConversionRegistry::default();
    registry
        .register_trusted_backend(
            descriptor("demo:disabled", "latex", "latex"),
            handler("unused").0,
        )
        .unwrap();
    let mut input = request("latex", "latex");
    input.backend = Some("demo:disabled".into());
    let policy = fallback_policy(&[BUILTIN_CONVERSION_BACKEND]);
    let first = registry.convert_with_fallback(&input, &policy);
    assert_eq!(
        first.decision_sha256,
        registry
            .convert_with_fallback(&input, &policy)
            .decision_sha256
    );
    let mut changed = input.clone();
    changed.content = "y_i^2".into();
    assert_ne!(
        first.decision_sha256,
        registry
            .convert_with_fallback(&changed, &policy)
            .decision_sha256
    );
    changed = input.clone();
    changed.context_sha256 = "c".repeat(64);
    assert_ne!(
        first.decision_sha256,
        registry
            .convert_with_fallback(&changed, &policy)
            .decision_sha256
    );
    let reordered = fallback_policy(&["demo:missing", BUILTIN_CONVERSION_BACKEND]);
    assert_ne!(
        first.policy_sha256,
        registry
            .convert_with_fallback(&input, &reordered)
            .policy_sha256
    );
    let mut another = SemanticConversionRegistry::default();
    let mut version = descriptor("demo:disabled", "latex", "latex");
    version.version = "2.0.0".into();
    another
        .register_trusted_backend(version, handler("unused").0)
        .unwrap();
    assert_ne!(
        first.decision_sha256,
        another
            .convert_with_fallback(&input, &policy)
            .decision_sha256
    );
    // Identity is also meaningful when no backend can execute.
    let unavailable = ConversionFallbackPolicy::default();
    assert_ne!(
        registry
            .convert_with_fallback(&input, &unavailable)
            .decision_sha256,
        registry
            .convert_with_fallback(&changed, &unavailable)
            .decision_sha256
    );
}

#[test]
fn fallback_keeps_platform_and_strict_mode_gates() {
    let mut registry = SemanticConversionRegistry::new(CapabilityTarget::Wasm32UnknownUnknown);
    let (native, _, native_calls) = handler("native");
    let (wasm, _, wasm_calls) = handler("wasm");
    registry
        .register_trusted_backend(descriptor("demo:native", "latex", "latex"), native)
        .unwrap();
    let mut wasm_descriptor = descriptor("demo:wasm", "latex", "latex");
    wasm_descriptor.routes[0].target = CapabilityTarget::Wasm32UnknownUnknown;
    registry
        .register_trusted_backend(wasm_descriptor, wasm)
        .unwrap();
    registry.set_backend_enabled("demo:native", true).unwrap();
    registry.set_backend_enabled("demo:wasm", true).unwrap();
    let mut input = request("latex", "latex");
    input.backend = Some("demo:native".into());
    let report = registry.convert_with_fallback(&input, &fallback_policy(&["demo:wasm"]));
    assert!(matches!(
        report.outcome,
        ConversionFallbackOutcome::Converted { .. }
    ));
    assert_eq!(native_calls.load(Ordering::SeqCst), 0);
    assert_eq!(wasm_calls.load(Ordering::SeqCst), 1);

    let mut registry = SemanticConversionRegistry::default();
    let (strict, _, strict_calls) = handler("unused");
    let (best, _, best_calls) = handler("unused");
    let mut strict_descriptor = descriptor("demo:strict", "latex", "omml");
    strict_descriptor.routes[0].mode = FormulaConversionMode::Strict;
    registry
        .register_trusted_backend(strict_descriptor, strict)
        .unwrap();
    registry
        .register_trusted_backend(descriptor("demo:best", "latex", "omml"), best)
        .unwrap();
    registry.set_backend_enabled("demo:strict", true).unwrap();
    registry.set_backend_enabled("demo:best", true).unwrap();
    let mut input = request("latex", "omml");
    input.mode = FormulaConversionMode::Strict;
    input.content = r"\unknown{x}".into();
    input.backend = Some("demo:strict".into());
    let report = registry.convert_with_fallback(&input, &fallback_policy(&["demo:best"]));
    assert_eq!(
        fallback_error(&report),
        ConversionRegistryErrorCode::InputRejected
    );
    assert_eq!(report.attempts.len(), 1);
    assert_eq!(strict_calls.load(Ordering::SeqCst), 0);
    assert_eq!(best_calls.load(Ordering::SeqCst), 0);
    registry.set_backend_enabled("demo:strict", false).unwrap();
    let report = registry.convert_with_fallback(&input, &fallback_policy(&["demo:best"]));
    assert_eq!(
        fallback_error(&report),
        ConversionRegistryErrorCode::RouteUnavailable
    );
    assert_eq!(best_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn validator_refusal_is_not_an_availability_fallback() {
    let mut registry = SemanticConversionRegistry::default();
    registry
        .register_trusted_format(
            ConversionFormatDescriptor {
                schema_version: 1,
                id: "demo:input".into(),
                mime_type: "text/plain".into(),
                description: "Rejected test input".into(),
            },
            |_| {
                Err(ConversionRegistryError::new(
                    ConversionRegistryErrorCode::RouteUnavailable,
                    "validator refusal",
                ))
            },
        )
        .unwrap();
    let (good, _, calls) = handler("unused");
    registry
        .register_trusted_backend(descriptor("demo:good", "demo:input", "latex"), good)
        .unwrap();
    registry.set_backend_enabled("demo:good", true).unwrap();
    let input = request("demo:input", "latex");
    let report = registry.convert_with_fallback(&input, &fallback_policy(&["demo:good"]));
    assert_eq!(
        fallback_error(&report),
        ConversionRegistryErrorCode::InputRejected
    );
    assert!(report.attempts.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
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
