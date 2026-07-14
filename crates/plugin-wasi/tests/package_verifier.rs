use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

use latexsnipper_plugin::PluginManifestV3;
use latexsnipper_plugin_wasi::{
    wit_types, ComponentInvocation, ComponentNetworkScheme, NetworkBroker, NetworkRequest,
    NetworkResponse, WasiComponentHost, WasiComponentPackageVerifier, WasiDiagnosticCode,
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
        "coreVersionRequirement": ">=3.0.0-alpha.1, <4.0.0",
        "executionClass": "wasi_component",
        "interfaces": {
            "pluginApi": 2,
            "processIpc": null,
            "componentWit": 1
        },
        "capabilities": [],
        "formatCapabilities": [],
        "hooks": [],
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
    WasiComponentPackageVerifier::new(Version::parse("3.0.0-alpha.1").unwrap())
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
        .with_model_artifacts(BTreeMap::from([(
            "fixture-model".to_string(),
            b"model".to_vec(),
        )]))
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
    let package = tempfile::tempdir().unwrap();
    let mut value = manifest(&component);
    value["permissions"]["limits"]["memoryBytes"] = json!(1024 * 1024);
    write_package(&package, &component, value);
    let verified = verifier().verify_path(package.path()).unwrap();
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
