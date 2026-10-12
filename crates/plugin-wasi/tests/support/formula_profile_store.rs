//! Shared local-only test package. Does not impersonate a signed remote service.
use latexsnipper_plugin::{
    PluginExecutionClassV3, RegistryTarget, RemotePluginProvenance, RemotePluginStore,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Write};
use wit_component::ComponentEncoder;

pub fn profile(exporter: bool) -> Value {
    json!({
        "schemaVersion": 1, "pluginId": "fixture.component", "backendId": "fixture:profile",
        "transport": "application/vnd.fixture.power",
        "operation": if exporter { json!({"kind":"exporter","input":"latex"}) } else { json!({"kind":"importer","output":"omml"}) },
        "format": {"id":"fixture:power","mimeType":"application/vnd.fixture.power","description":"Finite power fixture",
            "validation":{"kind":"regex","pattern":"[a-z]:[0-9]"}}
    })
}

pub fn store() -> (tempfile::TempDir, RemotePluginStore) {
    store_with_archive_comment("")
}

pub fn store_with_fuel(fuel: u64) -> (tempfile::TempDir, RemotePluginStore) {
    store_with_options(fuel, "")
}

pub fn store_with_archive_comment(comment: &str) -> (tempfile::TempDir, RemotePluginStore) {
    if comment.is_empty() {
        store_with_fuel(1_000_000)
    } else {
        store_with_options(1_000_000, comment)
    }
}

fn store_with_options(fuel: u64, comment: &str) -> (tempfile::TempDir, RemotePluginStore) {
    let component = ComponentEncoder::default()
        .module(include_bytes!("../fixtures/success-component.core.wasm"))
        .unwrap()
        .validate(true)
        .encode()
        .unwrap();
    let formats: Vec<_> = [("application/vnd.fixture.power", "AST"), ("AST", "application/vnd.fixture.power")]
        .into_iter().map(|(input, output)| json!({
            "input":input,"output":output,"available":true,"supports_formula":true,
            "supports_table":false,"supports_image":false,"supports_svg":false,"supports_style":false,
            "supports_layout":false,"supports_office_objects":false,"fidelity":"BestEffort",
            "known_loss":[],"notes":["Finite fixture only"],"required_features":[],
            "external_dependencies":[],"platform_restrictions":[],"experimental":true
        })).collect();
    let manifest = json!({
        "schemaVersion":3,"id":"fixture.component","name":"Fixture Component","version":"1.0.0",
        "coreVersionRequirement":">=3.0.0, <4.0.0","executionClass":"wasi_component",
        "interfaces":{"pluginApi":2,"processIpc":null,"componentWit":1},
        "capabilities":["document_transform","importer","exporter","filesystem_read","filesystem_write","environment_read","network_request","model_artifact_read","temporary_storage","clock_read","random_read"],"formatCapabilities":formats,
        "hooks":["before_conversion","register_importer","register_exporter"],"priority":0,
        "dependencies":[],"before":[],"after":[],"permissions":{
            "paths":[],"network":[],"environmentVariables":[],"modelArtifacts":[],
            "temporaryDirectory":false,"clocks":false,"randomness":false,
            "registrations":{"capabilities":true,"importers":true,"exporters":true,"runtimes":false},
            "limits":{"timeoutMillis":1000,"memoryBytes":4194304,"inputBytes":1048576,"outputBytes":1048576,
                "diagnosticCount":16,"diagnosticBytes":16384,"modelArtifactBytes":1048576,
                "temporaryStorageBytes":1048576,"tableElements":256,"resources":8,"fuel":fuel,"maxConcurrentExecutions":1}
        },"platforms":[],"architectures":[],"license":"AGPL-3.0",
        "artifact":{"path":"plugin.wasm","kind":"wasi_component","sha256":format!("{:x}",Sha256::digest(&component)),"sizeBytes":component.len()},
        "signature":null,"provenance":{"source":"fixture","revision":"test","statementSha256":null},
        "configurationSchema":{"type":"object","additionalProperties":false}
    });
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive.set_comment(comment);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    archive.start_file("plugin.json", options).unwrap();
    archive
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    archive.start_file("plugin.wasm", options).unwrap();
    archive.write_all(&component).unwrap();
    let archive = archive.finish().unwrap().into_inner();
    let target = RegistryTarget {
        plugin_id: "fixture.component".into(),
        version: "1.0.0".into(),
        package_path: "packages/fixture.zip".into(),
        length: archive.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&archive)),
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
