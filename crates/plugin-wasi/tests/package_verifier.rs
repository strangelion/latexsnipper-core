use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::sync::Arc;

use latexsnipper_plugin::{
    PluginExecutionClassV3, PluginManifestV3, RegistryTarget, RemotePluginProvenance,
    RemotePluginStore,
};
use latexsnipper_plugin_wasi::{
    wit_types, ActivatedRemoteWasiPlugin, ComponentInvocation, ComponentNetworkScheme,
    NetworkBroker, NetworkRequest, NetworkResponse, WasiComponentHost,
    WasiComponentPackageVerifier, WasiDiagnosticCode, WasiDiagnosticDetail, WasiDiagnosticSeverity,
    WasiHostPolicy, WasiPackagePolicy,
};
use semver::Version;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wit_component::{dummy_module, embed_component_metadata, ComponentEncoder, StringEncoding};
use wit_parser::{ManglingAndAbi, Resolve};

fn manifest(component: &[u8]) -> Value {
    json!({
        "schemaVersion": 3,
        "id": "fixture.component",
        "name": "Fixture Component",
        "version": "1.0.0",
        "coreVersionRequirement": ">=3.0.0, <4.0.0",
        "executionClass": "wasi_component",
        "interfaces": {
            "pluginApi": 2,
            "processIpc": null,
            "componentWit": 1
        },
        "capabilities": [
            "document_transform",
            "importer",
            "exporter",
            "filesystem_read",
            "filesystem_write",
            "environment_read",
            "network_request",
            "model_artifact_read",
            "temporary_storage",
            "clock_read",
            "random_read"
        ],
        "formatCapabilities": [
            {
                "input": "text/plain",
                "output": "AST",
                "available": true,
                "supports_formula": true,
                "supports_table": true,
                "supports_image": true,
                "supports_svg": true,
                "supports_style": true,
                "supports_layout": true,
                "supports_office_objects": false,
                "fidelity": "Lossless",
                "known_loss": [],
                "notes": [],
                "required_features": [],
                "external_dependencies": [],
                "platform_restrictions": [],
                "experimental": false
            },
            {
                "input": "AST",
                "output": "application/octet-stream",
                "available": true,
                "supports_formula": true,
                "supports_table": true,
                "supports_image": true,
                "supports_svg": true,
                "supports_style": true,
                "supports_layout": true,
                "supports_office_objects": false,
                "fidelity": "Lossless",
                "known_loss": [],
                "notes": [],
                "required_features": [],
                "external_dependencies": [],
                "platform_restrictions": [],
                "experimental": false
            }
        ],
        "hooks": ["before_conversion", "register_importer", "register_exporter"],
        "priority": 0,
        "dependencies": [],
        "before": [],
        "after": [],
        "permissions": {
            "paths": [],
            "network": [],
            "environmentVariables": [],
            "modelArtifacts": [],
            "temporaryDirectory": false,
            "clocks": false,
            "randomness": false,
            "registrations": {
                "capabilities": true,
                "importers": true,
                "exporters": true,
                "runtimes": false
            },
            "limits": {
                "timeoutMillis": 1000,
                "memoryBytes": 4194304,
                "inputBytes": 1048576,
                "outputBytes": 1048576,
                "diagnosticCount": 16,
                "diagnosticBytes": 16384,
                "modelArtifactBytes": 1048576,
                "temporaryStorageBytes": 1048576,
                "tableElements": 256,
                "resources": 8,
                "fuel": 1000000,
                "maxConcurrentExecutions": 1
            }
        },
        "platforms": [],
        "architectures": [],
        "license": "AGPL-3.0",
        "artifact": {
            "path": "plugin.wasm",
            "kind": "wasi_component",
            "sha256": hex::encode(Sha256::digest(component)),
            "sizeBytes": component.len()
        },
        "signature": null,
        "provenance": {
            "source": "fixture",
            "revision": "test",
            "statementSha256": null
        },
        "configurationSchema": {"type": "object", "additionalProperties": false}
    })
}

fn package(component: &[u8]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write_package(&directory, component, manifest(component));
    directory
}

fn write_package(directory: &tempfile::TempDir, component: &[u8], manifest: Value) {
    fs::write(directory.path().join("plugin.wasm"), component).unwrap();
    fs::write(
        directory.path().join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

fn verifier() -> WasiComponentPackageVerifier {
    WasiComponentPackageVerifier::new(Version::parse("3.0.0").unwrap())
}

fn remembered_binding_fixture() -> (
    tempfile::TempDir,
    latexsnipper_plugin_wasi::VerifiedComponentPackage,
) {
    let component = typed_success_component();
    let directory = package(&component);
    for name in ["read", "write"] {
        fs::create_dir(directory.path().join(name)).unwrap();
    }
    let mut definition = manifest(&component);
    definition["permissions"]["paths"] = json!([
        {"path": "read", "access": "read"},
        {"path": "write", "access": "write"}
    ]);
    definition["permissions"]["environmentVariables"] = json!(["FIXTURE_ENV"]);
    definition["permissions"]["modelArtifacts"] = json!(["fixture-model"]);
    write_package(&directory, &component, definition);
    let verified = verifier().verify_path(directory.path()).unwrap();
    (directory, verified)
}

#[test]
fn remembered_binding_is_stable_across_hosts_and_bound_to_effective_settings() {
    let (_directory, verified) = remembered_binding_fixture();
    let bind = |package| {
        WasiComponentHost::new(package)
            .unwrap()
            .with_remembered_approval_binding("fixture-document", b"deny-network-v1")
            .unwrap()
    };
    let first = bind(verified.clone());
    let binding = first.remembered_approval_binding().unwrap().to_string();
    assert_eq!(binding.len(), 64);
    assert_eq!(
        bind(verified.clone()).remembered_approval_binding(),
        Some(binding.as_str())
    );
    let changed = WasiComponentHost::new(verified.clone())
        .unwrap()
        .with_remembered_approval_binding("other-document", b"deny-network-v1")
        .unwrap();
    assert_ne!(
        changed.remembered_approval_binding(),
        Some(binding.as_str())
    );
    for field in 0..7 {
        let mut package = verified.clone();
        match field {
            0 => package.manifest.version = "1.0.1".into(),
            1 => package.component_sha256 = "f".repeat(64),
            2 => package.permissions.limits.input_bytes -= 1,
            3 => {
                package
                    .permissions
                    .filesystem
                    .get_mut("path-1")
                    .unwrap()
                    .writable = false
            }
            4 => package.permissions.randomness = !package.permissions.randomness,
            5 => {
                assert!(package.permissions.network.insert(
                    latexsnipper_plugin_wasi::NetworkGrant {
                        scheme: ComponentNetworkScheme::Https,
                        host: "fixture.example.invalid".into(),
                        port: 443,
                    }
                ));
            }
            _ => {
                package
                    .permissions
                    .filesystem
                    .get_mut("path-1")
                    .unwrap()
                    .root = package.root.join("read")
            }
        }
        assert_ne!(
            bind(package).remembered_approval_binding(),
            Some(binding.as_str())
        );
    }
}

#[test]
fn remembered_binding_is_cleared_by_changes_and_same_settings_do_not_revoke() {
    let (_directory, verified) = remembered_binding_fixture();
    let configure = || {
        WasiComponentHost::new(verified.clone())
            .unwrap()
            .with_configuration(b"config-v1".to_vec())
            .unwrap()
            .with_environment([("FIXTURE_ENV".into(), "private-env-v1".into())])
            .unwrap()
            .with_model_artifacts([("fixture-model".into(), b"private-model-v1".to_vec())])
            .unwrap()
            .with_remembered_approval_binding("fixture-document", b"deny-network-v1")
            .unwrap()
    };
    let binding = configure()
        .remembered_approval_binding()
        .unwrap()
        .to_string();
    let unchanged = configure()
        .with_configuration(b"config-v1".to_vec())
        .unwrap()
        .with_environment([("FIXTURE_ENV".into(), "private-env-v1".into())])
        .unwrap()
        .with_model_artifacts([("fixture-model".into(), b"private-model-v1".to_vec())])
        .unwrap();
    assert_eq!(
        unchanged.remembered_approval_binding(),
        Some(binding.as_str())
    );
    for field in 0..4 {
        let changed = match field {
            0 => configure()
                .with_configuration(b"config-v2".to_vec())
                .unwrap(),
            1 => configure()
                .with_environment([("FIXTURE_ENV".into(), "private-env-v2".into())])
                .unwrap(),
            2 => configure()
                .with_model_artifacts([("fixture-model".into(), b"private-model-v2".to_vec())])
                .unwrap(),
            _ => configure().with_network_broker(Arc::new(CountingNetworkBroker::default())),
        };
        assert_eq!(changed.remembered_approval_binding(), None);
        let rebound = changed
            .with_remembered_approval_binding(
                "fixture-document",
                if field == 3 {
                    b"broker-v2"
                } else {
                    b"deny-network-v1"
                },
            )
            .unwrap();
        assert_ne!(
            rebound.remembered_approval_binding(),
            Some(binding.as_str())
        );
    }
    for (domain, broker) in [("", b"v1".as_slice()), ("fixture", b"".as_slice())] {
        assert!(WasiComponentHost::new(verified.clone())
            .unwrap()
            .with_remembered_approval_binding(domain, broker)
            .is_err());
    }
    assert!(WasiComponentHost::new(verified)
        .unwrap()
        .with_remembered_approval_binding(&"x".repeat(4097), b"v1")
        .is_err());
}

#[test]
fn remembered_binding_does_not_approve_or_make_session_grants_portable() {
    let (directory, verified) = remembered_binding_fixture();
    let mut host = WasiComponentHost::new(verified.clone())
        .unwrap()
        .with_environment([("FIXTURE_ENV".into(), "private-env-v1".into())])
        .unwrap()
        .with_model_artifacts([("fixture-model".into(), b"private-model-v1".to_vec())])
        .unwrap()
        .with_remembered_approval_binding("private-domain", b"private-broker")
        .unwrap();
    let binding = host.remembered_approval_binding().unwrap().to_string();
    let compiled = host.compile().unwrap();
    let error = transform(&host, &compiled, b"broker:filesystem-write").unwrap_err();
    assert_eq!(
        error.code,
        WasiDiagnosticCode::PluginWasiApprovalRequired,
        "{error:?}"
    );
    assert!(!directory.path().join("write/output.txt").exists());
    let approvals = host.approval_controller();
    let request = approvals.pending().unwrap().remove(0);
    assert_eq!(request.remembered_binding_sha256, Some(binding));
    let serialized = serde_json::to_string(&request).unwrap();
    for private in [
        "private-env-v1",
        "private-model-v1",
        "private-domain",
        "private-broker",
    ] {
        assert!(!serialized.contains(private));
    }
    let policy_key = latexsnipper_plugin_wasi::ApprovalPolicyKey::generate().unwrap();
    // Simulate protected host key storage only in this fixture's memory.
    let restore_key =
        latexsnipper_plugin_wasi::ApprovalPolicyKey::from_secret_bytes(*policy_key.secret_bytes());
    let replay_key =
        latexsnipper_plugin_wasi::ApprovalPolicyKey::from_secret_bytes(*policy_key.secret_bytes());
    let policies = latexsnipper_plugin_wasi::RememberedApprovalPolicies::new(policy_key).unwrap();
    let policy_id = approvals
        .remember(
            &request.id,
            &policies,
            latexsnipper_plugin_wasi::ApprovalScope::SameTarget {
                max_payload_bytes: 16,
            },
            std::time::Duration::from_secs(3600),
        )
        .unwrap();
    let sealed = policies.seal().unwrap();
    let restored = latexsnipper_plugin_wasi::RememberedApprovalPolicies::restore(
        restore_key,
        &sealed.bytes,
        &sealed.checkpoint,
    )
    .unwrap();
    assert!(approvals.grants().unwrap().is_empty());
    assert!(!directory.path().join("write/output.txt").exists());
    assert_eq!(
        transform(&host, &compiled, b"broker:filesystem-write")
            .unwrap_err()
            .code,
        WasiDiagnosticCode::PluginWasiApprovalRequired
    );
    approvals
        .approve(
            &request.id,
            latexsnipper_plugin_wasi::ApprovalScope::SameTarget {
                max_payload_bytes: 16,
            },
            3,
            std::time::Duration::from_secs(60),
        )
        .unwrap();
    host = host
        .with_remembered_approval_binding("private-domain", b"private-broker")
        .unwrap();
    assert_eq!(
        transform(&host, &compiled, b"broker:filesystem-write").unwrap(),
        b"written"
    );
    let fresh = WasiComponentHost::new(verified)
        .unwrap()
        .with_environment([("FIXTURE_ENV".into(), "private-env-v1".into())])
        .unwrap()
        .with_model_artifacts([("fixture-model".into(), b"private-model-v1".to_vec())])
        .unwrap()
        .with_approval_controller(Arc::clone(&approvals))
        .with_remembered_approval_binding("private-domain", b"private-broker")
        .unwrap();
    assert_eq!(
        fresh.remembered_approval_binding(),
        host.remembered_approval_binding()
    );
    // Compiled components belong to their host's Wasmtime engine.
    let fresh_compiled = fresh.compile().unwrap();
    assert_eq!(
        transform(&fresh, &fresh_compiled, b"broker:filesystem-write")
            .unwrap_err()
            .code,
        WasiDiagnosticCode::PluginWasiApprovalRequired
    );
    let fresh_request = approvals.pending().unwrap().remove(0);
    assert_ne!(
        fresh_request.host_context_sha256,
        request.host_context_sha256
    );
    assert_ne!(fresh_request.id, request.id);
    assert_eq!(
        restored
            .matching_policy(&fresh_request)
            .unwrap()
            .unwrap()
            .id,
        policy_id
    );
    assert!(restored.revoke(&policy_id).unwrap());
    assert!(restored.matching_policy(&fresh_request).unwrap().is_none());
    let revoked = restored.seal().unwrap();
    assert!(
        latexsnipper_plugin_wasi::RememberedApprovalPolicies::restore(
            replay_key,
            &sealed.bytes,
            &revoked.checkpoint,
        )
        .is_err()
    );
}

fn typed_trap_component() -> Vec<u8> {
    let wit = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wit/plugin-v1");
    let mut resolve = Resolve::new();
    let (package, _) = resolve.push_dir(&wit).unwrap();
    let world = resolve.select_world(&[package], Some("plugin")).unwrap();
    let mut module = dummy_module(&resolve, world, ManglingAndAbi::Standard32);
    embed_component_metadata(&mut module, &resolve, world, StringEncoding::UTF8).unwrap();
    ComponentEncoder::default()
        .module(&module)
        .unwrap()
        .validate(true)
        .encode()
        .unwrap()
}

fn typed_success_component() -> Vec<u8> {
    ComponentEncoder::default()
        .module(include_bytes!("fixtures/success-component.core.wasm"))
        .unwrap()
        .validate(true)
        .encode()
        .unwrap()
}

fn remote_archive(component: &[u8]) -> Vec<u8> {
    remote_archive_with_manifest(component, manifest(component))
}

fn remote_archive_with_manifest(component: &[u8], manifest: Value) -> Vec<u8> {
    let manifest = serde_json::to_vec_pretty(&manifest).unwrap();
    let mut output = Cursor::new(Vec::new());
    {
        let mut archive = zip::ZipWriter::new(&mut output);
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        archive.start_file("plugin.json", options).unwrap();
        archive.write_all(&manifest).unwrap();
        archive.start_file("plugin.wasm", options).unwrap();
        archive.write_all(component).unwrap();
        archive.finish().unwrap();
    }
    output.into_inner()
}

fn power_store(configure: impl FnOnce(&mut Value)) -> (tempfile::TempDir, RemotePluginStore) {
    let component = typed_success_component();
    let mut definition = manifest(&component);
    let mut import = definition["formatCapabilities"][0].clone();
    import["input"] = json!("application/vnd.fixture.power");
    import["output"] = json!("AST");
    import["fidelity"] = json!("BestEffort");
    import["notes"] = json!(["Finite power fixture, not full math fidelity"]);
    let mut export = import.clone();
    export["input"] = json!("AST");
    export["output"] = json!("application/vnd.fixture.power");
    definition["formatCapabilities"]
        .as_array_mut()
        .unwrap()
        .extend([import, export]);
    configure(&mut definition);
    let archive = remote_archive_with_manifest(&component, definition);
    let target = RegistryTarget {
        plugin_id: "fixture.component".into(),
        version: "1.0.0".into(),
        package_path: "packages/fixture.zip".into(),
        length: archive.len() as u64,
        sha256: hex::encode(Sha256::digest(&archive)),
        execution_class: PluginExecutionClassV3::WasiComponent,
        core_version_requirement: ">=3.0.0, <4.0.0".into(),
        revoked: false,
        revocation_reason: None,
    };
    let temporary = tempfile::tempdir().unwrap();
    let store = RemotePluginStore::new(temporary.path().join("store"));
    store
        .install(
            &target,
            &archive,
            RemotePluginProvenance {
                registry_name: "fixture".into(),
                registry_origin: "https://registry.example.invalid".into(),
                targets_version: 1,
                package_path: target.package_path.clone(),
                package_sha256: target.sha256.clone(),
                verified_at_unix: 1_900_000_000,
            },
            "3.0.0",
        )
        .unwrap();
    store.set_enabled("fixture.component", true).unwrap();
    (temporary, store)
}

fn formula_registry() -> latexsnipper_conversion::conversion_registry::SemanticConversionRegistry {
    use latexsnipper_conversion::conversion_registry::*;
    let mut registry = SemanticConversionRegistry::default();
    registry
        .register_trusted_format(
            ConversionFormatDescriptor {
                schema_version: 1,
                id: "fixture:power".into(),
                mime_type: "application/vnd.fixture.power".into(),
                description: "Finite executable fixture protocol".into(),
            },
            |source| {
                if source.is_ascii() && source.len() <= 128 {
                    Ok(())
                } else {
                    Err(ConversionRegistryError::new(
                        ConversionRegistryErrorCode::InputRejected,
                        "fixture input must be bounded ASCII",
                    ))
                }
            },
        )
        .unwrap();
    registry
}

fn formula_request(
    input: &str,
    output: &str,
    backend: &str,
    content: &str,
) -> latexsnipper_conversion::conversion_registry::RegisteredConversionRequest {
    latexsnipper_conversion::conversion_registry::RegisteredConversionRequest {
        schema_version: 1,
        input: input.into(),
        output: output.into(),
        mode: latexsnipper_conversion::FormulaConversionMode::BestEffort,
        backend: Some(backend.into()),
        content: content.into(),
        context_sha256: "a".repeat(64),
    }
}

#[test]
fn registered_wasi_formula_importer_and_exporter_execute_real_ast_projection() {
    use latexsnipper_conversion::{
        conversion_registry::*, DocumentConverter, FormulaInputFormat, OutputFormat,
    };
    use latexsnipper_plugin_wasi::WasiFormulaBackend;
    let (_temporary, store) = power_store(|_| {});
    let mut registry = formula_registry();
    WasiFormulaBackend::importer(
        ActivatedRemoteWasiPlugin::activate(&store, "fixture.component").unwrap(),
        "fixture:import",
        "fixture:power",
        "application/vnd.fixture.power",
        OutputFormat::OMML,
    )
    .unwrap()
    .register(&mut registry)
    .unwrap();
    WasiFormulaBackend::exporter(
        ActivatedRemoteWasiPlugin::activate(&store, "fixture.component").unwrap(),
        "fixture:export",
        FormulaInputFormat::Latex,
        "fixture:power",
        "application/vnd.fixture.power",
    )
    .unwrap()
    .register(&mut registry)
    .unwrap();
    let request = formula_request("fixture:power", "omml", "fixture:import", "x:2");
    assert_eq!(
        registry.convert(&request).unwrap_err().code,
        ConversionRegistryErrorCode::BackendDisabled
    );
    registry
        .set_backend_enabled("fixture:import", true)
        .unwrap();
    registry
        .set_backend_enabled("fixture:export", true)
        .unwrap();
    for (power, latex) in [("x:2", "x^2"), ("y:4", "y^4")] {
        let imported = DocumentConverter::convert_registered_formula(
            &registry,
            &formula_request("fixture:power", "omml", "fixture:import", power),
        )
        .unwrap();
        assert_eq!(
            imported.candidate.content,
            DocumentConverter::convert_latex_string(latex, OutputFormat::OMML).unwrap()
        );
        assert_eq!(imported.backend, "fixture:import");
        assert!(!imported.candidate.losses.is_empty());
        let exported = registry
            .convert(&formula_request(
                "latex",
                "fixture:power",
                "fixture:export",
                latex,
            ))
            .unwrap();
        assert_eq!(exported.candidate.content, power);
        assert_eq!(exported.mime_type, "application/vnd.fixture.power");
    }
    // One valid math wrapper is normalized, not treated as a mixed document.
    for (latex, power) in [
        (" $x^2$ ", "x:2"),
        ("$$y^4$$", "y:4"),
        (r"\(z^7\)", "z:7"),
        (r"\[a^3\]", "a:3"),
    ] {
        let request = formula_request("latex", "fixture:power", "fixture:export", latex);
        let exported = registry.convert(&request).unwrap();
        assert_eq!(exported.candidate.content, power);
        assert_eq!(exported.backend, "fixture:export");
        assert_eq!(
            exported.input_sha256,
            format!("{:x}", Sha256::digest(latex))
        );
        assert_eq!(request.content, latex);
        assert!(!exported.candidate.losses.is_empty());
    }
    store.set_enabled("fixture.component", false).unwrap();
    let report = registry.convert_with_fallback(
        &request,
        &ConversionFallbackPolicy {
            schema_version: CONVERSION_REGISTRY_VERSION,
            backends: vec![BUILTIN_CONVERSION_BACKEND.into()],
        },
    );
    assert_eq!(report.attempts.len(), 1);
    assert!(
        matches!(report.outcome, ConversionFallbackOutcome::Rejected { error }
        if error.code == ConversionRegistryErrorCode::BackendFailed)
    );
    assert!(
        !registry
            .capabilities()
            .iter()
            .find(|entry| entry.backend == "fixture:import")
            .unwrap()
            .available
    );
    assert_eq!(
        registry.convert(&request).unwrap_err().code,
        ConversionRegistryErrorCode::BackendFailed
    );
    assert_eq!(request.content, "x:2");
    store.set_enabled("fixture.component", true).unwrap();
    store.revoke("fixture.component").unwrap();
    assert_eq!(
        registry.convert(&request).unwrap_err().code,
        ConversionRegistryErrorCode::BackendFailed
    );
}

#[test]
fn registered_wasi_formula_bridge_refuses_media_shape_utf8_and_undeclared_routes() {
    use latexsnipper_conversion::{FormulaInputFormat, OutputFormat};
    use latexsnipper_plugin_wasi::WasiFormulaBackend;
    let (_temporary, store) = power_store(|_| {});
    let activated = ActivatedRemoteWasiPlugin::activate(&store, "fixture.component").unwrap();
    assert!(WasiFormulaBackend::importer(
        activated,
        "fixture:import",
        "fixture:power",
        "undeclared",
        OutputFormat::OMML
    )
    .is_err());
    let mut registry = formula_registry();
    WasiFormulaBackend::importer(
        ActivatedRemoteWasiPlugin::activate(&store, "fixture.component").unwrap(),
        "fixture:import",
        "fixture:power",
        "application/vnd.fixture.power",
        OutputFormat::OMML,
    )
    .unwrap()
    .register(&mut registry)
    .unwrap();
    WasiFormulaBackend::exporter(
        ActivatedRemoteWasiPlugin::activate(&store, "fixture.component").unwrap(),
        "fixture:export",
        FormulaInputFormat::Latex,
        "fixture:power",
        "application/vnd.fixture.power",
    )
    .unwrap()
    .register(&mut registry)
    .unwrap();
    registry
        .set_backend_enabled("fixture:import", true)
        .unwrap();
    registry
        .set_backend_enabled("fixture:export", true)
        .unwrap();
    for source in [
        "control:wrong-media",
        "control:invalid-json",
        "control:extra-block",
        "bad syntax",
    ] {
        let failure = registry
            .convert(&formula_request(
                "fixture:power",
                "omml",
                "fixture:import",
                source,
            ))
            .unwrap_err();
        if source == "bad syntax" {
            assert!(
                failure.message.contains("invalid power syntax"),
                "{failure}"
            );
        } else {
            assert_eq!(failure.code, latexsnipper_conversion::conversion_registry::ConversionRegistryErrorCode::InputRejected, "{source}: {failure}");
        }
    }
    let failure = registry
        .convert(&formula_request(
            "latex",
            "fixture:power",
            "fixture:export",
            "control:invalid-utf8",
        ))
        .unwrap_err();
    assert!(failure.message.contains("non-UTF-8"), "{failure}");
    for source in [
        "\\documentclass{article}\\begin{document}x^2\\end{document}",
        "x^2\\]\\[y^2",
        "$x^2$ $y^3$",
        "$$x^2$$$$y^3$$",
        r"\(x^2\)\(y^3\)",
        r"\[x^2\]\[y^3\]",
        "$x^2",
        "x^2$",
        "Text before $x^2$",
        r"$\documentclass{article}x^2$",
    ] {
        let failure = registry
            .convert(&formula_request(
                "latex",
                "fixture:power",
                "fixture:export",
                source,
            ))
            .unwrap_err();
        assert_eq!(failure.code, latexsnipper_conversion::conversion_registry::ConversionRegistryErrorCode::InputRejected, "{failure}");
    }
    let mut input = formula_request("fixture:power", "omml", "fixture:import", "x:2");
    input.mode = latexsnipper_conversion::FormulaConversionMode::Strict;
    assert!(registry.convert(&input).is_err());
}

#[test]
fn registered_wasi_formula_bridge_retains_host_output_budget_cancellation_and_interrupt() {
    use latexsnipper_conversion::OutputFormat;
    use latexsnipper_plugin_wasi::WasiFormulaBackend;
    let (_temporary, store) = power_store(|definition| {
        definition["permissions"]["limits"]["outputBytes"] = json!(64);
    });
    let mut registry = formula_registry();
    WasiFormulaBackend::importer(
        ActivatedRemoteWasiPlugin::activate(&store, "fixture.component").unwrap(),
        "fixture:limited",
        "fixture:power",
        "application/vnd.fixture.power",
        OutputFormat::OMML,
    )
    .unwrap()
    .register(&mut registry)
    .unwrap();
    registry
        .set_backend_enabled("fixture:limited", true)
        .unwrap();
    let failure = registry
        .convert(&formula_request(
            "fixture:power",
            "omml",
            "fixture:limited",
            "x:2",
        ))
        .unwrap_err();
    assert!(
        failure.message.contains("PLUGIN_WASI_OUTPUT_LIMIT"),
        "{failure}"
    );
    let (temporary2, store2) = power_store(|_| {});
    let cancellation = latexsnipper_plugin::CancellationToken::default();
    WasiFormulaBackend::importer(
        ActivatedRemoteWasiPlugin::activate(&store2, "fixture.component").unwrap(),
        "fixture:cancelled",
        "fixture:power",
        "application/vnd.fixture.power",
        OutputFormat::OMML,
    )
    .unwrap()
    .with_cancellation(cancellation.clone())
    .register(&mut registry)
    .unwrap();
    registry
        .set_backend_enabled("fixture:cancelled", true)
        .unwrap();
    cancellation.cancel();
    let failure = registry
        .convert(&formula_request(
            "fixture:power",
            "omml",
            "fixture:cancelled",
            "x:2",
        ))
        .unwrap_err();
    assert!(failure.message.contains("PLUGIN_WASI_CANCELLED"));
    WasiFormulaBackend::importer(
        ActivatedRemoteWasiPlugin::activate(&store2, "fixture.component").unwrap(),
        "fixture:interrupt",
        "fixture:power",
        "application/vnd.fixture.power",
        OutputFormat::OMML,
    )
    .unwrap()
    .register(&mut registry)
    .unwrap();
    registry
        .set_backend_enabled("fixture:interrupt", true)
        .unwrap();
    let failure = registry
        .convert(&formula_request(
            "fixture:power",
            "omml",
            "fixture:interrupt",
            "control:infinite",
        ))
        .unwrap_err();
    assert!(failure.message.contains("PLUGIN_WASI_TIMEOUT"), "{failure}");
    // The registry was created before the second temporary store. Close its
    // retained directory handles first instead of relying on reverse drop order.
    drop(registry);
    drop(store2);
    temporary2
        .close()
        .expect("second formula store must be cleaned after host shutdown");
}

fn patch_payload(result: latexsnipper_plugin_wasi::ComponentInvocationResult) -> Vec<u8> {
    let latexsnipper_plugin_wasi::ComponentInvocationResult::Patch(patch) = result else {
        panic!("expected patch result");
    };
    let Some(wit_types::PatchOperation::ReplaceDocument(replacement)) =
        patch.operations.into_iter().next()
    else {
        panic!("expected replacement operation");
    };
    replacement.payload
}

fn transform(
    host: &WasiComponentHost,
    compiled: &latexsnipper_plugin_wasi::CompiledWasiComponent,
    payload: &[u8],
) -> Result<Vec<u8>, latexsnipper_plugin_wasi::WasiDiagnostic> {
    host.execute(
        compiled,
        ComponentInvocation::Transform(wit_types::Document {
            schema_version: "1.0.0".to_string(),
            media_type: "application/octet-stream".to_string(),
            payload: payload.to_vec(),
        }),
        latexsnipper_plugin::CancellationToken::default(),
    )
    .map(patch_payload)
}

struct FixtureNetworkBroker;

#[derive(Default)]
struct CountingNetworkBroker(std::sync::atomic::AtomicUsize);
impl NetworkBroker for CountingNetworkBroker {
    fn send(
        &self,
        request: &NetworkRequest,
    ) -> Result<NetworkResponse, latexsnipper_plugin_wasi::WasiDiagnostic> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        FixtureNetworkBroker.send(request)
    }
}

impl NetworkBroker for FixtureNetworkBroker {
    fn send(
        &self,
        request: &NetworkRequest,
    ) -> Result<NetworkResponse, latexsnipper_plugin_wasi::WasiDiagnostic> {
        assert_eq!(request.scheme, ComponentNetworkScheme::Https);
        assert_eq!(request.host, "models.example.invalid");
        assert_eq!(request.port, 443);
        assert_eq!(request.path_and_query, "/fixture");
        Ok(NetworkResponse {
            status: 200,
            body: b"network".to_vec(),
        })
    }
}

struct TimeoutNetworkBroker;

impl NetworkBroker for TimeoutNetworkBroker {
    fn send(
        &self,
        _request: &NetworkRequest,
    ) -> Result<NetworkResponse, latexsnipper_plugin_wasi::WasiDiagnostic> {
        Err(latexsnipper_plugin_wasi::WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiTimeout,
            "upstream timeout",
        )
        .with_details(vec![WasiDiagnosticDetail {
            code: WasiDiagnosticCode::PluginWasiHostFailure,
            severity: WasiDiagnosticSeverity::Warning,
            message: "upstream detail".to_string(),
            field: Some("network".to_string()),
        }]))
    }
}

#[test]
fn verifies_exact_component_digest_and_contract() {
    let component = wat::parse_str("(component)").unwrap();
    let package = package(&component);
    let verified = verifier().verify_path(package.path()).unwrap();
    assert_eq!(verified.manifest.id, "fixture.component");
    assert_eq!(
        verified.component_sha256,
        hex::encode(Sha256::digest(&component))
    );
}

#[test]
fn rejects_component_replacement_after_manifest_creation() {
    let component = wat::parse_str("(component)").unwrap();
    let package = package(&component);
    fs::write(
        package.path().join("plugin.wasm"),
        wat::parse_str("(component (core module))").unwrap(),
    )
    .unwrap();
    let error = verifier().verify_path(package.path()).unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
    assert!(error.message.contains("SHA-256") || error.message.contains("size"));
}

#[test]
fn rejects_traversal_and_malformed_signature_metadata() {
    let component = wat::parse_str("(component)").unwrap();
    let package = package(&component);
    let manifest_path = package.path().join("plugin.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    value["artifact"]["path"] = json!("../plugin.wasm");
    value["signature"] = json!({
        "algorithm": "unknown",
        "keyId": "fixture",
        "signature": "not-hex"
    });
    fs::write(&manifest_path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let error = verifier().verify_path(package.path()).unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
}

#[test]
fn rejects_malformed_signature_metadata() {
    let component = wat::parse_str("(component)").unwrap();
    let package = package(&component);
    let manifest_path = package.path().join("plugin.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    value["signature"] = json!({
        "algorithm": "unknown",
        "keyId": "fixture",
        "signature": "not-hex"
    });
    fs::write(&manifest_path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let error = verifier().verify_path(package.path()).unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
    assert!(error.message.contains("signature"));
}

#[test]
fn rejects_incompatible_wit_version() {
    let component = wat::parse_str("(component)").unwrap();
    let package = package(&component);
    let manifest_path = package.path().join("plugin.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    value["interfaces"]["componentWit"] = json!(2);
    fs::write(&manifest_path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let error = verifier().verify_path(package.path()).unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
}

#[test]
fn rejects_archives_instead_of_implicitly_extracting_them() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("plugin.zip");
    fs::write(&archive, b"not an archive").unwrap();
    let error = verifier().verify_path(&archive).unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
    assert!(error.message.contains("archives"));
}

#[test]
fn manifest_fixture_deserializes_as_the_public_v3_contract() {
    let component = wat::parse_str("(component)").unwrap();
    let parsed: PluginManifestV3 = serde_json::from_value(manifest(&component)).unwrap();
    parsed.validate_contract().unwrap();
}

#[test]
fn real_typed_component_instantiates_and_trap_is_contained() {
    let component = typed_trap_component();
    let package = package(&component);
    let verified = verifier().verify_path(package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();
    let error = host
        .execute(
            &compiled,
            ComponentInvocation::Transform(wit_types::Document {
                schema_version: "1.0.0".to_string(),
                media_type: "application/json".to_string(),
                payload: b"{}".to_vec(),
            }),
            latexsnipper_plugin::CancellationToken::default(),
        )
        .unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiTrap);
}

#[test]
fn real_typed_component_runs_transform_import_and_export() {
    let component = typed_success_component();
    let package = package(&component);
    let verified = verifier().verify_path(package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();

    let transformed = host
        .execute(
            &compiled,
            ComponentInvocation::Transform(wit_types::Document {
                schema_version: "1.0.0".to_string(),
                media_type: "application/json".to_string(),
                payload: b"{\"fixture\":true}".to_vec(),
            }),
            latexsnipper_plugin::CancellationToken::default(),
        )
        .unwrap();
    let latexsnipper_plugin_wasi::ComponentInvocationResult::Patch(patch) = transformed else {
        panic!("expected patch result");
    };
    assert_eq!(patch.base_schema_version, "1.0.0");
    assert_eq!(patch.operations.len(), 1);

    let imported = host
        .execute(
            &compiled,
            ComponentInvocation::Import(wit_types::ImportRequest {
                format: "text/plain".to_string(),
                payload: b"imported".to_vec(),
            }),
            latexsnipper_plugin::CancellationToken::default(),
        )
        .unwrap();
    let latexsnipper_plugin_wasi::ComponentInvocationResult::Document(document) = imported else {
        panic!("expected document result");
    };
    assert_eq!(document.payload, b"imported");

    let exported = host
        .execute(
            &compiled,
            ComponentInvocation::Export(wit_types::ExportRequest {
                format: "application/octet-stream".to_string(),
                document: wit_types::Document {
                    schema_version: "1.0.0".to_string(),
                    media_type: "application/json".to_string(),
                    payload: b"exported".to_vec(),
                },
            }),
            latexsnipper_plugin::CancellationToken::default(),
        )
        .unwrap();
    let latexsnipper_plugin_wasi::ComponentInvocationResult::Export(output) = exported else {
        panic!("expected export result");
    };
    assert_eq!(output.media_type, "application/octet-stream");
    assert_eq!(output.payload, b"exported");
}

#[test]
fn signed_remote_component_requires_enablement_and_executes_after_activation() {
    let component = typed_success_component();
    let archive = remote_archive(&component);
    let target = RegistryTarget {
        plugin_id: "fixture.component".to_string(),
        version: "1.0.0".to_string(),
        package_path: "packages/fixture.component-1.0.0.zip".to_string(),
        length: archive.len() as u64,
        sha256: hex::encode(Sha256::digest(&archive)),
        execution_class: PluginExecutionClassV3::WasiComponent,
        core_version_requirement: ">=3.0.0, <4.0.0".to_string(),
        revoked: false,
        revocation_reason: None,
    };
    let provenance = RemotePluginProvenance {
        registry_name: "fixture".to_string(),
        registry_origin: "https://registry.example.invalid".to_string(),
        targets_version: 1,
        package_path: target.package_path.clone(),
        package_sha256: target.sha256.clone(),
        verified_at_unix: 1_900_000_000,
    };
    let temporary = tempfile::tempdir().unwrap();
    let store = RemotePluginStore::new(temporary.path().join("remote"));
    store
        .install(&target, &archive, provenance, "3.0.0")
        .unwrap();

    let Err(disabled) = ActivatedRemoteWasiPlugin::activate(&store, "fixture.component") else {
        panic!("disabled remote plugin activated");
    };
    assert_eq!(
        disabled.code,
        WasiDiagnosticCode::PluginWasiProtocolMismatch
    );

    store.set_enabled("fixture.component", true).unwrap();
    let activated = ActivatedRemoteWasiPlugin::activate(&store, "fixture.component").unwrap();
    assert_eq!(activated.manifest().id, "fixture.component");
    assert_eq!(
        activated.component_sha256(),
        hex::encode(Sha256::digest(&component))
    );
    let result = activated
        .execute(
            ComponentInvocation::Import(wit_types::ImportRequest {
                format: "text/plain".to_string(),
                payload: b"remote-import".to_vec(),
            }),
            latexsnipper_plugin::CancellationToken::default(),
        )
        .unwrap();
    let latexsnipper_plugin_wasi::ComponentInvocationResult::Document(document) = result else {
        panic!("expected document result");
    };
    assert_eq!(document.payload, b"remote-import");

    store.set_enabled("fixture.component", false).unwrap();
    let disabled = activated
        .execute(
            ComponentInvocation::Import(wit_types::ImportRequest {
                format: "text/plain".to_string(),
                payload: b"blocked-after-disable".to_vec(),
            }),
            latexsnipper_plugin::CancellationToken::default(),
        )
        .unwrap_err();
    assert_eq!(
        disabled.code,
        WasiDiagnosticCode::PluginWasiProtocolMismatch
    );

    store.set_enabled("fixture.component", true).unwrap();
    let revoked = ActivatedRemoteWasiPlugin::activate(&store, "fixture.component").unwrap();
    store.revoke("fixture.component").unwrap();
    let revoked = revoked
        .execute(
            ComponentInvocation::Import(wit_types::ImportRequest {
                format: "text/plain".to_string(),
                payload: b"blocked-after-revoke".to_vec(),
            }),
            latexsnipper_plugin::CancellationToken::default(),
        )
        .unwrap_err();
    assert_eq!(revoked.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
}

#[test]
fn brokers_enforce_default_deny_and_explicit_grants() {
    let component = typed_success_component();
    let denied_package = package(&component);
    let denied = verifier().verify_path(denied_package.path()).unwrap();
    let denied_host = WasiComponentHost::new(denied).unwrap();
    let denied_component = denied_host.compile().unwrap();
    let error = transform(&denied_host, &denied_component, b"broker:environment").unwrap_err();
    assert_eq!(
        error.code,
        WasiDiagnosticCode::PluginWasiPermissionDenied,
        "{error:?}"
    );

    let granted_package = tempfile::tempdir().unwrap();
    fs::create_dir(granted_package.path().join("read")).unwrap();
    fs::create_dir(granted_package.path().join("write")).unwrap();
    fs::write(
        granted_package.path().join("read").join("input.txt"),
        b"filesystem",
    )
    .unwrap();
    let mut granted_manifest = manifest(&component);
    granted_manifest["permissions"]["paths"] = json!([
        {"path": "read", "access": "read"},
        {"path": "write", "access": "write"}
    ]);
    granted_manifest["permissions"]["network"] = json!([{
        "scheme": "https",
        "host": "models.example.invalid",
        "port": 443
    }]);
    granted_manifest["permissions"]["environmentVariables"] = json!(["FIXTURE_ENV"]);
    granted_manifest["permissions"]["modelArtifacts"] = json!(["fixture-model"]);
    granted_manifest["permissions"]["temporaryDirectory"] = json!(true);
    granted_manifest["permissions"]["clocks"] = json!(true);
    granted_manifest["permissions"]["randomness"] = json!(true);
    write_package(&granted_package, &component, granted_manifest);

    let verified = verifier().verify_path(granted_package.path()).unwrap();
    let network = Arc::new(CountingNetworkBroker::default());
    let host = WasiComponentHost::new(verified)
        .unwrap()
        .with_environment(BTreeMap::from([(
            "FIXTURE_ENV".to_string(),
            "environment".to_string(),
        )]))
        .unwrap()
        .with_model_artifacts(BTreeMap::from([(
            "fixture-model".to_string(),
            b"model".to_vec(),
        )]))
        .unwrap()
        .with_network_broker(network.clone());
    let compiled = host.compile().unwrap();
    assert_eq!(
        transform(&host, &compiled, b"broker:environment").unwrap(),
        b"environment"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:filesystem-read").unwrap(),
        b"filesystem"
    );
    let approvals = host.approval_controller();
    let error = transform(&host, &compiled, b"broker:filesystem-write").unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiApprovalRequired);
    assert!(!granted_package.path().join("write/output.txt").exists());
    let request = approvals.pending().unwrap().pop().unwrap();
    assert!(matches!(
        request.action,
        latexsnipper_plugin_wasi::SensitiveAction::FileWrite { .. }
    ));
    approvals
        .approve(
            &request.id,
            latexsnipper_plugin_wasi::ApprovalScope::SameTarget {
                max_payload_bytes: 7,
            },
            2,
            std::time::Duration::from_secs(60),
        )
        .unwrap();
    assert_eq!(
        transform(&host, &compiled, b"broker:filesystem-write").unwrap(),
        b"written"
    );
    assert_eq!(
        fs::read(granted_package.path().join("write").join("output.txt")).unwrap(),
        b"written"
    );
    // The already-approved second use proceeds without a new prompt.
    transform(&host, &compiled, b"broker:filesystem-write").unwrap();
    assert!(approvals.pending().unwrap().is_empty());
    assert_eq!(
        transform(&host, &compiled, b"broker:filesystem-write")
            .unwrap_err()
            .code,
        WasiDiagnosticCode::PluginWasiApprovalRequired
    );
    approvals.revoke(&request.id).unwrap();
    // The observed old file content changed after the approved writes, so the
    // new pending write has a different request identity. Decline it explicitly.
    for pending in approvals.pending().unwrap() {
        if matches!(
            pending.action,
            latexsnipper_plugin_wasi::SensitiveAction::FileWrite { .. }
        ) {
            approvals.revoke(&pending.id).unwrap();
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
        use windows_sys::Win32::System::IO::DeviceIoControl;
        let path = granted_package.path().join("write/output.txt");
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut returned = 0;
        // SAFETY: Test-only marking of an owned fixture, with fixed synchronous
        // arguments. No global privileges or EFS certificates are created.
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    file.as_raw_handle(),
                    FSCTL_SET_SPARSE,
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        drop(file);
        assert_eq!(
            transform(&host, &compiled, b"broker:filesystem-write")
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiMetadataUnsupported
        );
        assert!(approvals.pending().unwrap().is_empty());
        assert_eq!(fs::read(&path).unwrap(), b"written");
    }
    assert_eq!(
        transform(&host, &compiled, b"broker:model").unwrap(),
        b"model"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:temporary").unwrap(),
        b"temporary"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:network")
            .unwrap_err()
            .code,
        WasiDiagnosticCode::PluginWasiApprovalRequired
    );
    assert_eq!(network.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    let request = approvals.pending().unwrap().pop().unwrap();
    assert!(matches!(
        request.action,
        latexsnipper_plugin_wasi::SensitiveAction::NetworkSend { .. }
    ));
    approvals.approve_once(&request.id).unwrap();
    assert_eq!(
        transform(&host, &compiled, b"broker:network").unwrap(),
        b"network"
    );
    assert_eq!(network.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        transform(&host, &compiled, b"broker:network")
            .unwrap_err()
            .code,
        WasiDiagnosticCode::PluginWasiApprovalRequired
    );
    assert_eq!(network.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    approvals.approve_once(&request.id).unwrap();
    let host = host
        .with_configuration(b"updated-host-context".to_vec())
        .unwrap();
    assert!(approvals.grants().unwrap().is_empty());
    assert!(approvals.pending().unwrap().is_empty());
    assert_eq!(
        transform(&host, &compiled, b"broker:network")
            .unwrap_err()
            .code,
        WasiDiagnosticCode::PluginWasiApprovalRequired
    );
    let fresh = approvals.pending().unwrap().pop().unwrap();
    assert_ne!(request.host_context_sha256, fresh.host_context_sha256);
    assert_ne!(request.id, fresh.id);
    assert_eq!(network.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    approvals.approve_once(&fresh.id).unwrap();
    transform(&host, &compiled, b"broker:network").unwrap();
    assert_eq!(network.0.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(
        transform(&host, &compiled, b"broker:system").unwrap().len(),
        8
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:network")
            .unwrap_err()
            .code,
        WasiDiagnosticCode::PluginWasiApprovalRequired
    );
    assert_eq!(approvals.pending().unwrap().len(), 1);
    drop(host);
    assert!(approvals.pending().unwrap().is_empty());
    assert!(approvals.grants().unwrap().is_empty());
}

#[test]
fn broker_failures_preserve_their_original_diagnostic_category() {
    let component = typed_success_component();
    let package = tempfile::tempdir().unwrap();
    let mut value = manifest(&component);
    value["permissions"]["network"] = json!([{
        "scheme": "https",
        "host": "models.example.invalid",
        "port": 443
    }]);
    write_package(&package, &component, value);
    let verified = verifier().verify_path(package.path()).unwrap();
    let host = WasiComponentHost::new(verified)
        .unwrap()
        .with_network_broker(Arc::new(TimeoutNetworkBroker));
    let compiled = host.compile().unwrap();
    let pending = transform(&host, &compiled, b"broker:network").unwrap_err();
    assert_eq!(pending.code, WasiDiagnosticCode::PluginWasiApprovalRequired);
    let approvals = host.approval_controller();
    let request = approvals.pending().unwrap().pop().unwrap();
    approvals.approve_once(&request.id).unwrap();
    let error = transform(&host, &compiled, b"broker:network").unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiTrap);
    assert_eq!(error.details[0].code, WasiDiagnosticCode::PluginWasiTimeout);
    assert_eq!(
        error.details[1].code,
        WasiDiagnosticCode::PluginWasiHostFailure
    );
}

#[test]
fn invalid_patch_and_oversize_output_have_stable_diagnostics() {
    let component = typed_success_component();
    let package = package(&component);
    let verified = verifier().verify_path(package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();
    let invalid = transform(&host, &compiled, b"control:invalid-patch").unwrap_err();
    assert_eq!(invalid.code, WasiDiagnosticCode::PluginWasiInvalidPatch);

    let oversize = transform(&host, &compiled, b"control:oversize-output").unwrap_err();
    assert_eq!(oversize.code, WasiDiagnosticCode::PluginWasiOutputLimit);
}

#[test]
fn memory_limit_is_enforced_before_component_execution() {
    let component = typed_success_component();
    let memory_package = tempfile::tempdir().unwrap();
    let mut value = manifest(&component);
    value["permissions"]["limits"]["memoryBytes"] = json!(1024 * 1024);
    write_package(&memory_package, &component, value);
    let verified = verifier().verify_path(memory_package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();
    let error = transform(&host, &compiled, b"ordinary").unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiMemoryLimit);
}

#[test]
fn timeout_and_running_cancellation_leave_host_reusable() {
    let component = typed_success_component();
    let timeout_package = tempfile::tempdir().unwrap();
    let mut timeout_manifest = manifest(&component);
    timeout_manifest["permissions"]["limits"]["timeoutMillis"] = json!(40);
    timeout_manifest["permissions"]["limits"]["fuel"] = json!(100_000_000_000u64);
    write_package(&timeout_package, &component, timeout_manifest);
    let verified = verifier().verify_path(timeout_package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();
    let timeout = transform(&host, &compiled, b"control:infinite").unwrap_err();
    assert_eq!(timeout.code, WasiDiagnosticCode::PluginWasiTimeout);
    assert_eq!(transform(&host, &compiled, b"reused").unwrap(), b"reused");

    let cancel_package = tempfile::tempdir().unwrap();
    let mut cancel_manifest = manifest(&component);
    cancel_manifest["permissions"]["limits"]["fuel"] = json!(100_000_000_000u64);
    write_package(&cancel_package, &component, cancel_manifest);
    let verified = verifier().verify_path(cancel_package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();
    let cancellation = latexsnipper_plugin::CancellationToken::default();
    let signal = cancellation.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        signal.cancel();
    });
    let cancelled = host
        .execute(
            &compiled,
            ComponentInvocation::Transform(wit_types::Document {
                schema_version: "1.0.0".to_string(),
                media_type: "application/octet-stream".to_string(),
                payload: b"control:infinite".to_vec(),
            }),
            cancellation,
        )
        .unwrap_err();
    canceller.join().unwrap();
    assert_eq!(cancelled.code, WasiDiagnosticCode::PluginWasiCancelled);
    assert_eq!(transform(&host, &compiled, b"reused").unwrap(), b"reused");
}

#[test]
fn guest_errors_preserve_details_and_shutdown_failures_are_not_discarded() {
    let component = typed_success_component();
    let package = package(&component);
    let verified = verifier().verify_path(package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();

    let invocation = transform(&host, &compiled, b"control:guest-error").unwrap_err();
    assert_eq!(invocation.code, WasiDiagnosticCode::PluginWasiTrap);
    assert_eq!(invocation.message, "fixture invocation failed");
    assert_eq!(invocation.details.len(), 1);
    assert_eq!(
        invocation.details[0].code,
        WasiDiagnosticCode::PluginWasiHostFailure
    );
    assert_eq!(
        invocation.details[0].field.as_deref(),
        Some("fixture.field")
    );

    let shutdown = transform(&host, &compiled, b"control:shutdown-error").unwrap_err();
    assert_eq!(shutdown.code, WasiDiagnosticCode::PluginWasiTrap);
    assert_eq!(shutdown.message, "fixture shutdown failed");
    assert_eq!(transform(&host, &compiled, b"reused").unwrap(), b"reused");
}

#[test]
fn initialization_errors_preserve_structured_diagnostics() {
    let component = typed_success_component();
    let package = package(&component);
    let verified = verifier().verify_path(package.path()).unwrap();
    let host = WasiComponentHost::new(verified)
        .unwrap()
        .with_configuration(b"control:init-error".to_vec())
        .unwrap();
    let compiled = host.compile().unwrap();
    let error = transform(&host, &compiled, b"unused").unwrap_err();
    assert_eq!(error.message, "fixture initialize failed");
    assert_eq!(error.details.len(), 1);
}

#[test]
fn runtime_capabilities_must_exactly_match_the_manifest() {
    let component = typed_success_component();
    let package = tempfile::tempdir().unwrap();
    let mut value = manifest(&component);
    value["capabilities"] = json!(["document_transform", "importer"]);
    write_package(&package, &component, value);
    let verified = verifier().verify_path(package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();
    let error = transform(&host, &compiled, b"unused").unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiCapabilityMismatch);
}

#[test]
fn invocation_requires_declared_hook_and_compatible_format() {
    let component = typed_success_component();
    let package_without_hook = tempfile::tempdir().unwrap();
    let mut value = manifest(&component);
    value["hooks"] = json!(["register_importer", "register_exporter"]);
    write_package(&package_without_hook, &component, value);
    let verified = verifier().verify_path(package_without_hook.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();
    let hook_error = transform(&host, &compiled, b"unused").unwrap_err();
    assert_eq!(
        hook_error.code,
        WasiDiagnosticCode::PluginWasiInvocationNotDeclared
    );

    let format_package = package(&component);
    let verified = verifier().verify_path(format_package.path()).unwrap();
    let host = WasiComponentHost::new(verified).unwrap();
    let compiled = host.compile().unwrap();
    let format_error = host
        .execute(
            &compiled,
            ComponentInvocation::Import(wit_types::ImportRequest {
                format: "application/pdf".to_string(),
                payload: b"unused".to_vec(),
            }),
            latexsnipper_plugin::CancellationToken::default(),
        )
        .unwrap_err();
    assert_eq!(
        format_error.code,
        WasiDiagnosticCode::PluginWasiInvocationNotDeclared
    );
}

#[test]
fn model_artifact_injection_fails_explicitly() {
    let component = typed_success_component();
    let environment_package = package(&component);
    let verified = verifier().verify_path(environment_package.path()).unwrap();
    let environment_error = WasiComponentHost::new(verified)
        .unwrap()
        .with_environment(vec![("UNDECLARED".to_string(), "value".to_string())])
        .err()
        .expect("undeclared environment injection must fail");
    assert_eq!(
        environment_error.code,
        WasiDiagnosticCode::PluginWasiPermissionDenied
    );

    let denied_package = package(&component);
    let verified = verifier().verify_path(denied_package.path()).unwrap();
    let denied = WasiComponentHost::new(verified)
        .unwrap()
        .with_model_artifacts(vec![("undeclared".to_string(), vec![1])])
        .err()
        .expect("undeclared artifact must fail");
    assert_eq!(denied.code, WasiDiagnosticCode::PluginWasiPermissionDenied);

    let package = tempfile::tempdir().unwrap();
    let mut value = manifest(&component);
    value["permissions"]["modelArtifacts"] = json!(["fixture-model"]);
    value["permissions"]["limits"]["modelArtifactBytes"] = json!(4);
    write_package(&package, &component, value);
    let verified = verifier().verify_path(package.path()).unwrap();
    let oversized = WasiComponentHost::new(verified)
        .unwrap()
        .with_model_artifacts(vec![("fixture-model".to_string(), vec![0; 5])])
        .err()
        .expect("oversized artifact must fail");
    assert_eq!(oversized.code, WasiDiagnosticCode::PluginWasiOutputLimit);
}

#[test]
fn custom_host_policy_is_authoritative_during_directory_verification() {
    let component = typed_success_component();
    let package = tempfile::tempdir().unwrap();
    let mut value = manifest(&component);
    value["permissions"]["limits"]["memoryBytes"] = json!(1024);
    write_package(&package, &component, value);

    let default_error = verifier().verify_path(package.path()).unwrap_err();
    assert_eq!(
        default_error.code,
        WasiDiagnosticCode::PluginWasiResourcePolicy
    );

    let mut policy = WasiHostPolicy::default();
    policy.minimums.memory_bytes = 1;
    let verified = verifier()
        .with_host_policy(policy)
        .verify_path(package.path())
        .unwrap();
    assert_eq!(verified.permissions.limits.memory_bytes, 1024);
}

#[test]
fn duplicate_authority_and_cross_platform_ambiguous_paths_are_rejected() {
    let component = wat::parse_str("(component)").unwrap();
    let duplicate_package = tempfile::tempdir().unwrap();
    let mut duplicate = manifest(&component);
    duplicate["capabilities"] = json!(["document_transform", "document_transform"]);
    write_package(&duplicate_package, &component, duplicate);
    let duplicate_error = verifier()
        .verify_path(duplicate_package.path())
        .unwrap_err();
    assert_eq!(
        duplicate_error.code,
        WasiDiagnosticCode::PluginWasiProtocolMismatch
    );

    let ambiguous_package = tempfile::tempdir().unwrap();
    let mut ambiguous = manifest(&component);
    ambiguous["artifact"]["path"] = json!("nested\\plugin.wasm");
    write_package(&ambiguous_package, &component, ambiguous);
    let path_error = verifier()
        .verify_path(ambiguous_package.path())
        .unwrap_err();
    assert_eq!(
        path_error.code,
        WasiDiagnosticCode::PluginWasiProtocolMismatch
    );
}

#[test]
fn package_policy_rejects_unbounded_or_undeclared_payloads() {
    let component = wat::parse_str("(component)").unwrap();
    let undeclared_package = package(&component);
    fs::write(undeclared_package.path().join("unexpected.bin"), b"payload").unwrap();
    let error = verifier()
        .verify_path(undeclared_package.path())
        .unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
    assert!(error.message.contains("undeclared payload"));

    let bounded_package = package(&component);
    let policy = WasiPackagePolicy {
        max_entries: 1,
        ..WasiPackagePolicy::default()
    };
    let error = verifier()
        .with_package_policy(policy)
        .verify_path(bounded_package.path())
        .unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
    assert!(error.message.contains("too many entries"));
}

#[test]
fn package_and_broker_reject_hard_link_aliases() {
    let component = typed_success_component();
    let package = tempfile::tempdir().unwrap();
    fs::create_dir(package.path().join("read")).unwrap();
    fs::write(package.path().join("source.txt"), b"linked").unwrap();
    fs::hard_link(
        package.path().join("source.txt"),
        package.path().join("read").join("input.txt"),
    )
    .unwrap();
    let mut value = manifest(&component);
    value["permissions"]["paths"] = json!([{"path": "read", "access": "read"}]);
    write_package(&package, &component, value);
    let error = verifier().verify_path(package.path()).unwrap_err();
    assert_eq!(error.code, WasiDiagnosticCode::PluginWasiProtocolMismatch);
    assert!(error.message.contains("single link"));
}
