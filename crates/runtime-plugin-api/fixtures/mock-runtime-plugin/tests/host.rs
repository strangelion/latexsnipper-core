use std::path::{Path, PathBuf};

use latexsnipper_runtime::{
    RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind, RuntimeOptions, RuntimeRegistry,
    TensorMap,
};
use latexsnipper_runtime_plugin_api::{
    RuntimePluginDescriptor, RuntimePluginDiscovery, RuntimePluginTrustStore,
    RUNTIME_PLUGIN_DESCRIPTOR,
};
use latexsnipper_tensor::Tensor;
use sha2::{Digest, Sha256};

#[test]
fn trusted_dynamic_plugin_preserves_session_and_output_ownership() {
    let source_library = fixture_library();
    let root = std::env::temp_dir().join(format!(
        "latexsnipper-runtime-plugin-host-{}",
        std::process::id()
    ));
    let package = root.join("mock-runtime");
    std::fs::create_dir_all(&package).unwrap();
    let library_name = source_library.file_name().unwrap();
    let installed_library = package.join(library_name);
    std::fs::copy(&source_library, &installed_library).unwrap();
    let digest = hex_sha256(&installed_library);
    let descriptor = RuntimePluginDescriptor {
        schema_version: 1,
        runtime_id: "mock-runtime".to_owned(),
        plugin_version: "1.0.0".to_owned(),
        library: library_name.to_string_lossy().into_owned(),
        sha256: digest.clone(),
    };
    std::fs::write(
        package.join(RUNTIME_PLUGIN_DESCRIPTOR),
        serde_json::to_vec_pretty(&descriptor).unwrap(),
    )
    .unwrap();

    let mut trust = RuntimePluginTrustStore::new();
    trust
        .enroll("mock-runtime", &installed_library, &digest)
        .unwrap();
    let disabled = RuntimePluginDiscovery::new([&root], &trust).discover();
    assert!(disabled.factories.is_empty());
    assert!(disabled
        .issues
        .iter()
        .any(|issue| issue.reason.contains("not explicitly enabled")));

    trust.set_enabled("mock-runtime", true).unwrap();
    let discovered = RuntimePluginDiscovery::new([&root], &trust).discover();
    assert!(discovered.issues.is_empty(), "{:?}", discovered.issues);
    assert_eq!(discovered.factories.len(), 1);
    let mut registry = RuntimeRegistry::new();
    discovered.register_all(&mut registry).unwrap();
    assert!(registry
        .get(&RuntimeKind::Custom("mock-runtime".to_owned()))
        .is_some());
    let factory = &discovered.factories[0];
    let probe = factory.probe();
    assert!(probe.available, "{probe:?}");
    assert_eq!(factory.runtime_id(), "mock-runtime");

    let model = package.join("model.mock");
    std::fs::write(&model, b"model").unwrap();
    let artifacts = RuntimeArtifacts::new(RuntimeKind::Custom("mock-runtime".to_owned()))
        .with_file("model", model);
    let session = factory
        .create_session(&artifacts, &RuntimeOptions::default())
        .unwrap();
    assert_eq!(run_values(session.as_ref(), "active-sessions"), vec![1.0]);
    assert_eq!(run_values(session.as_ref(), "predict"), vec![2.0, 4.0]);
    assert_eq!(run_values(session.as_ref(), "free-count"), vec![2.0]);

    let malformed = run(session.as_ref(), "malformed").unwrap_err().to_string();
    assert!(malformed.contains("requires"));
    assert_eq!(run_values(session.as_ref(), "free-count"), vec![4.0]);

    let failed = run(session.as_ref(), "fail-after-allocation")
        .unwrap_err()
        .to_string();
    assert!(failed.contains("intentional failure"));
    assert_eq!(run_values(session.as_ref(), "free-count"), vec![6.0]);

    drop(session);
    let second = factory
        .create_session(&artifacts, &RuntimeOptions::default())
        .unwrap();
    assert_eq!(run_values(second.as_ref(), "active-sessions"), vec![1.0]);
    drop(second);
    drop(registry);
    drop(discovered);
    drop(trust);
    std::fs::remove_dir_all(root).unwrap();
}

fn run(
    session: &dyn latexsnipper_runtime::RuntimeSession,
    method: &str,
) -> latexsnipper_foundation::Result<latexsnipper_runtime::RunResponse> {
    let values = if method == "predict" {
        vec![1.0, 2.0]
    } else {
        vec![0.0]
    };
    let input = Tensor::float32("x", vec![values.len()], values);
    session.run(RunRequest {
        method: Some(method.to_owned()),
        inputs: TensorMap::from([("x".to_owned(), input)]),
        requested_outputs: None,
    })
}

fn run_values(session: &dyn latexsnipper_runtime::RuntimeSession, method: &str) -> Vec<f32> {
    run(session, method).unwrap().outputs["y"]
        .as_f32_slice()
        .unwrap()
        .to_vec()
}

fn fixture_library() -> PathBuf {
    let executable = std::env::current_exe().unwrap();
    let deps = executable.parent().unwrap();
    let name = if cfg!(target_os = "windows") {
        "latexsnipper_runtime_plugin_mock.dll"
    } else if cfg!(target_os = "macos") {
        "liblatexsnipper_runtime_plugin_mock.dylib"
    } else {
        "liblatexsnipper_runtime_plugin_mock.so"
    };
    for directory in [Some(deps), deps.parent()].into_iter().flatten() {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "mock runtime cdylib '{name}' was not built beside {}",
        executable.display()
    );
}

fn hex_sha256(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap();
    format!("{:x}", Sha256::digest(bytes))
}
