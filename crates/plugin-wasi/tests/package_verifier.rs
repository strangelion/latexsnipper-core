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
                "tableElements": 100,
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
    let manifest = serde_json::to_vec_pretty(&manifest(component)).unwrap();
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
        .with_network_broker(Arc::new(FixtureNetworkBroker));
    let compiled = host.compile().unwrap();
    assert_eq!(
        transform(&host, &compiled, b"broker:environment").unwrap(),
        b"environment"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:filesystem-read").unwrap(),
        b"filesystem"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:filesystem-write").unwrap(),
        b"written"
    );
    assert_eq!(
        fs::read(granted_package.path().join("write").join("output.txt")).unwrap(),
        b"written"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:model").unwrap(),
        b"model"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:temporary").unwrap(),
        b"temporary"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:network").unwrap(),
        b"network"
    );
    assert_eq!(
        transform(&host, &compiled, b"broker:system").unwrap().len(),
        8
    );
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
