#[path = "support/formula_profile_store.rs"]
mod fixture;
use latexsnipper_conversion::{conversion_registry::*, FormulaConversionMode};
use latexsnipper_plugin_wasi::WasiFormulaProfile;

#[test]
fn remembered_remote_binding_includes_archive_and_profile_without_enabling() {
    use latexsnipper_plugin::CancellationToken;
    use latexsnipper_plugin_wasi::ActivatedRemoteWasiPlugin;

    let (_temporary, store) = fixture::store();
    let (_other_temporary, other_store) = fixture::store_with_archive_comment("different archive");
    let activate = |store: &latexsnipper_plugin::RemotePluginStore| {
        ActivatedRemoteWasiPlugin::activate(store, "fixture.component")
            .unwrap()
            .with_remembered_approval_binding("fixture:trusted-document")
            .unwrap()
    };
    let remote = activate(&store);
    let remote_binding = remote.remembered_approval_binding().unwrap().to_owned();
    let repeated = remote
        .with_remembered_approval_binding("fixture:trusted-document")
        .unwrap();
    assert_eq!(
        repeated.remembered_approval_binding().unwrap(),
        remote_binding
    );
    assert_eq!(
        activate(&store).remembered_approval_binding().unwrap(),
        remote_binding
    );
    let other = activate(&other_store);
    assert_eq!(repeated.component_sha256(), other.component_sha256());
    assert_eq!(
        serde_json::to_value(repeated.manifest()).unwrap(),
        serde_json::to_value(other.manifest()).unwrap()
    );
    assert_ne!(other.remembered_approval_binding().unwrap(), remote_binding);
    assert!(repeated.approval_controller().grants().unwrap().is_empty());

    let definition = fixture::profile(false);
    let profile = WasiFormulaProfile::from_json(&serde_json::to_vec(&definition).unwrap()).unwrap();
    let policies = std::sync::Arc::new(
        latexsnipper_plugin_wasi::RememberedApprovalPolicies::new(
            latexsnipper_plugin_wasi::ApprovalPolicyKey::generate().unwrap(),
        )
        .unwrap(),
    );
    let prepare = |profile: &WasiFormulaProfile, store: &latexsnipper_plugin::RemotePluginStore| {
        profile
            .prepare_with_remembered_policies(
                store,
                CancellationToken::default(),
                "fixture:trusted-document",
                std::sync::Arc::clone(&policies),
            )
            .unwrap()
    };
    let mut prepared = prepare(&profile, &store);
    let binding = prepared.remembered_binding_sha256.clone().unwrap();
    assert_ne!(binding, remote_binding);
    assert_eq!(
        prepare(&profile, &store).remembered_binding_sha256.unwrap(),
        binding
    );
    assert_ne!(
        prepare(&profile, &other_store)
            .remembered_binding_sha256
            .unwrap(),
        binding
    );
    let mut changed = definition;
    changed["format"]["validation"]["pattern"] = serde_json::json!("[a-z]:[0-8]");
    let changed = WasiFormulaProfile::from_json(&serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_ne!(
        prepare(&changed, &store).remembered_binding_sha256.unwrap(),
        binding
    );
    assert_eq!(
        prepared
            .registry
            .convert(&request(false, "x:2"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::BackendDisabled
    );
    assert!(prepared.approval_controller.grants().unwrap().is_empty());
    prepared
        .registry
        .set_backend_enabled(&prepared.backend_id, true)
        .unwrap();
    assert!(prepared
        .registry
        .convert(&request(false, "x:2"))
        .unwrap()
        .candidate
        .content
        .contains("m:sSup"));
    store.set_enabled("fixture.component", false).unwrap();
    assert!(prepared.registry.convert(&request(false, "x:2")).is_err());
    assert!(repeated
        .with_remembered_approval_binding("fixture:trusted-document")
        .is_err());
}

#[test]
fn remembered_profile_binding_checks_domain_before_touching_a_store() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("absent-store");
    let store = latexsnipper_plugin::RemotePluginStore::new(root.clone());
    let profile =
        WasiFormulaProfile::from_json(&serde_json::to_vec(&fixture::profile(false)).unwrap())
            .unwrap();
    for domain in ["".to_owned(), "x".repeat(4097)] {
        assert_eq!(
            profile
                .prepare_with_remembered_approval_binding(
                    &store,
                    latexsnipper_plugin::CancellationToken::default(),
                    &domain,
                )
                .err()
                .unwrap()
                .code,
            ConversionRegistryErrorCode::InvalidDeclaration
        );
        assert!(!root.exists());
    }
}

fn request(exporter: bool, source: &str) -> RegisteredConversionRequest {
    RegisteredConversionRequest {
        schema_version: 1,
        input: if exporter { "latex" } else { "fixture:power" }.into(),
        output: if exporter { "fixture:power" } else { "omml" }.into(),
        mode: FormulaConversionMode::BestEffort,
        backend: Some("fixture:profile".into()),
        content: source.into(),
        context_sha256: "a".repeat(64),
    }
}

#[test]
fn profiles_execute_actual_import_export_and_bind_validator_identity() {
    let (_temporary, store) = fixture::store();
    let definition = fixture::profile(false);
    let profile = WasiFormulaProfile::from_json(&serde_json::to_vec(&definition).unwrap()).unwrap();
    let cancellation = latexsnipper_plugin::CancellationToken::default();
    let mut prepared = profile
        .prepare_with_cancellation(&store, cancellation.clone())
        .unwrap();
    assert!(prepared.remembered_binding_sha256.is_none());
    assert_eq!(
        prepared
            .registry
            .convert(&request(false, "x:2"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::BackendDisabled
    );
    prepared
        .registry
        .set_backend_enabled(&prepared.backend_id, true)
        .unwrap();
    let result = prepared.registry.convert(&request(false, "x:2")).unwrap();
    assert!(result.candidate.content.contains("m:sSup"));
    assert_eq!(
        prepared
            .registry
            .convert(&request(false, "x:2+y"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::InputRejected
    );
    let mut changed = definition.clone();
    changed["format"]["validation"]["pattern"] = serde_json::json!("[a-z]:[0-8]");
    let mut other = WasiFormulaProfile::from_json(&serde_json::to_vec(&changed).unwrap())
        .unwrap()
        .prepare(&store)
        .unwrap();
    other
        .registry
        .set_backend_enabled(&other.backend_id, true)
        .unwrap();
    let altered = other.registry.convert(&request(false, "x:2")).unwrap();
    assert_eq!(result.candidate.content, altered.candidate.content);
    assert_ne!(result.configuration_sha256, altered.configuration_sha256);
    assert_ne!(result.route_sha256, altered.route_sha256);
    cancellation.cancel();
    assert_eq!(
        prepared
            .registry
            .convert(&request(false, "x:2"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::BackendFailed
    );
    let exporter =
        WasiFormulaProfile::from_json(&serde_json::to_vec(&fixture::profile(true)).unwrap())
            .unwrap();
    let mut exported = exporter.prepare(&store).unwrap();
    exported
        .registry
        .set_backend_enabled(&exported.backend_id, true)
        .unwrap();
    assert_eq!(
        exported
            .registry
            .convert(&request(true, "$$y^4$$"))
            .unwrap()
            .candidate
            .content,
        "y:4"
    );
    store.set_enabled("fixture.component", false).unwrap();
    assert_eq!(
        exported
            .registry
            .convert(&request(true, "x^2"))
            .unwrap_err()
            .code,
        ConversionRegistryErrorCode::BackendFailed
    );
    assert!(exporter.prepare(&store).is_err());
    assert!(!store.get("fixture.component").unwrap().unwrap().enabled);
}

#[test]
fn malformed_profile_does_not_create_a_store_or_mutate_caller_registry() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("absent-store");
    let store = latexsnipper_plugin::RemotePluginStore::new(root.clone());
    let mut profile =
        WasiFormulaProfile::from_json(&serde_json::to_vec(&fixture::profile(false)).unwrap())
            .unwrap();
    profile.backend_id = "core:builtin".into();
    assert!(profile.prepare(&store).is_err());
    assert!(!root.exists());
    let profile =
        WasiFormulaProfile::from_json(&serde_json::to_vec(&fixture::profile(false)).unwrap())
            .unwrap();
    let cancellation = latexsnipper_plugin::CancellationToken::default();
    cancellation.cancel();
    assert!(profile
        .prepare_with_cancellation(&store, cancellation)
        .is_err());
    let policies = std::sync::Arc::new(
        latexsnipper_plugin_wasi::RememberedApprovalPolicies::new(
            latexsnipper_plugin_wasi::ApprovalPolicyKey::generate().unwrap(),
        )
        .unwrap(),
    );
    let cancellation = latexsnipper_plugin::CancellationToken::default();
    cancellation.cancel();
    assert!(profile
        .prepare_with_remembered_policies(&store, cancellation, "fixture:document", policies)
        .is_err());
    assert!(!root.exists());
}

#[test]
fn linear_profiles_execute_real_components_without_widening_trust_or_retry() {
    let (temporary, store) = fixture::store();
    let mut identities = std::collections::BTreeSet::new();
    for format in ["ascii-math", "unicode-math"] {
        for exporter in [false, true] {
            let mut definition = fixture::profile(exporter);
            definition["operation"] = if exporter {
                serde_json::json!({"kind":"exporter", "input":format})
            } else {
                serde_json::json!({"kind":"importer", "output":format})
            };
            let profile =
                WasiFormulaProfile::from_json(&serde_json::to_vec(&definition).unwrap()).unwrap();
            let cancellation = latexsnipper_plugin::CancellationToken::default();
            let mut prepared = profile
                .prepare_with_cancellation(&store, cancellation.clone())
                .unwrap();
            let make_request = |source: &str| {
                let mut value = request(exporter, source);
                if exporter {
                    value.input = format.into();
                } else {
                    value.output = format.into();
                }
                value
            };
            let first_source = if exporter { "x^2" } else { "x:2" };
            assert_eq!(
                prepared
                    .registry
                    .convert(&make_request(first_source))
                    .unwrap_err()
                    .code,
                ConversionRegistryErrorCode::BackendDisabled
            );
            prepared
                .registry
                .set_backend_enabled(&prepared.backend_id, true)
                .unwrap();
            let route = prepared
                .registry
                .capabilities()
                .into_iter()
                .find(|route| route.backend == "fixture:profile")
                .unwrap();
            assert!(route.available);
            assert!(route
                .limitations
                .iter()
                .any(|note| note.contains("finite linear")));
            let mut outputs = Vec::new();
            for (variable, exponent) in [("x", "2"), ("y", "4")] {
                let source = if exporter {
                    format!("{variable}^{exponent}")
                } else {
                    format!("{variable}:{exponent}")
                };
                let result = prepared.registry.convert(&make_request(&source)).unwrap();
                if outputs.is_empty() {
                    assert!(identities.insert(result.configuration_sha256.clone()));
                }
                let expected = if exporter {
                    format!("{variable}:{exponent}")
                } else if format == "ascii-math" {
                    format!("{variable}^({exponent})")
                } else {
                    format!("〖{variable}〗^({exponent})")
                };
                assert_eq!(result.candidate.content, expected);
                assert_eq!(
                    result.input,
                    if exporter { format } else { "fixture:power" }
                );
                assert_eq!(
                    result.output,
                    if exporter { "fixture:power" } else { format }
                );
                assert!(!result.candidate.losses.is_empty());
                assert_eq!(result.input_sha256, {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(source.as_bytes()))
                });
                outputs.push(result.candidate.content);
            }
            assert_ne!(outputs[0], outputs[1]);
            if exporter {
                let canonical = if format == "ascii-math" {
                    "{:x:}^(2)"
                } else {
                    "〖x〗^(2)"
                };
                let projected = prepared.registry.convert(&make_request(canonical)).unwrap();
                assert_eq!(projected.candidate.content, "x:2");
                let invalid = if format == "ascii-math" {
                    "color(red)(x)"
                } else {
                    "∫_0^1 x"
                };
                assert_eq!(
                    prepared
                        .registry
                        .convert(&make_request(invalid))
                        .unwrap_err()
                        .code,
                    ConversionRegistryErrorCode::InputRejected
                );
                let report = prepared.registry.convert_with_fallback(
                    &make_request("a/b"),
                    &ConversionFallbackPolicy {
                        schema_version: 1,
                        backends: vec!["core:builtin".into()],
                    },
                );
                assert!(matches!(
                    report.outcome,
                    ConversionFallbackOutcome::Rejected { .. }
                ));
                assert_eq!(report.attempts.len(), 1);
                assert!(prepared
                    .registry
                    .convert(&make_request(first_source))
                    .is_ok());
            }
            cancellation.cancel();
            let cancelled = prepared.registry.convert_with_fallback(
                &make_request(first_source),
                &ConversionFallbackPolicy {
                    schema_version: 1,
                    backends: vec!["core:builtin".into()],
                },
            );
            assert_eq!(cancelled.attempts.len(), 1);
            assert!(matches!(
                cancelled.outcome,
                ConversionFallbackOutcome::Rejected { .. }
            ));
            assert!(prepared.approval_controller.grants().unwrap().is_empty());
            let mut fresh = profile.prepare(&store).unwrap();
            fresh
                .registry
                .set_backend_enabled(&fresh.backend_id, true)
                .unwrap();
            store.set_enabled("fixture.component", false).unwrap();
            assert_eq!(
                fresh
                    .registry
                    .convert(&make_request(first_source))
                    .unwrap_err()
                    .code,
                ConversionRegistryErrorCode::BackendFailed
            );
            assert!(profile.prepare(&store).is_err());
            store.set_enabled("fixture.component", true).unwrap();
        }
    }
    drop(store);
    temporary.close().unwrap();
}
