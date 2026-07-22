use std::path::PathBuf;

use latexsnipper_runtime::{
    RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind, RuntimeOptions, TensorMap,
};
use latexsnipper_runtime_executorch::ExecuTorchFactory;
use latexsnipper_tensor::Tensor;

/// Opt-in smoke test for a packaged Windows x64 XNNPACK runtime and `.pte`.
/// Ordinary workspace/CI builds do not require an ExecuTorch installation.
#[test]
fn xnnpack_program_runs_named_methods_without_python() {
    let Some(runtime_home) = std::env::var_os("LATEXSNIPPER_EXECUTORCH_HOME") else {
        return;
    };
    let Some(program_path) = std::env::var_os("LATEXSNIPPER_EXECUTORCH_PROGRAM") else {
        return;
    };

    let factory = ExecuTorchFactory::with_library_path(runtime_home);
    let probe = factory.probe();
    assert!(probe.available, "ExecuTorch probe failed: {probe:?}");
    assert!(probe.capabilities.execution_providers.contains("xnnpack"));

    let artifacts = RuntimeArtifacts::new(RuntimeKind::ExecuTorch)
        .with_file("program", PathBuf::from(program_path));
    let session = factory
        .create_session(&artifacts, &RuntimeOptions::default())
        .expect("XNNPACK program should load");
    assert!(session
        .metadata()
        .methods
        .iter()
        .any(|name| name == "forward"));
    assert!(session
        .metadata()
        .methods
        .iter()
        .any(|name| name == "encode"));
    let input_name = session.metadata().inputs[0].name.clone();

    for method in ["forward", "encode"] {
        let input = Tensor::float32(&input_name, vec![1, 1, 8, 8], vec![0.25; 64]);
        let response = session
            .run(RunRequest {
                method: Some(method.to_owned()),
                inputs: TensorMap::from([(input_name.clone(), input)]),
                requested_outputs: None,
            })
            .expect("named method should execute");
        assert!(!response.outputs.is_empty());
        assert!(response
            .outputs
            .values()
            .all(|output| output.dtype().as_str() == "f32" && !output.is_empty()));
    }
}
