use std::path::PathBuf;

use latexsnipper_runtime::{
    RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind, RuntimeOptions, TensorMap,
};
use latexsnipper_runtime_paddle::PaddleInferenceFactory;
use latexsnipper_tensor::Tensor;

/// Opt-in smoke test for a packaged native runtime and the official complete
/// PP-FormulaNet graph. It is skipped in ordinary workspace/CI builds.
#[test]
fn official_full_graph_runs_without_python() {
    let Some(runtime_home) = std::env::var_os("LATEXSNIPPER_PADDLE_HOME") else {
        return;
    };
    let Some(model_home) = std::env::var_os("LATEXSNIPPER_PPFN_MODEL_HOME") else {
        return;
    };

    let factory = PaddleInferenceFactory::with_library_path(runtime_home);
    let probe = factory.probe();
    assert!(probe.available, "Paddle probe failed: {probe:?}");

    let model_home = PathBuf::from(model_home);
    let artifacts = RuntimeArtifacts::new(RuntimeKind::PaddleInference)
        .with_file("model", model_home.join("inference.json"))
        .with_file("params", model_home.join("inference.pdiparams"));
    let session = factory
        .create_session(&artifacts, &RuntimeOptions::default())
        .expect("official Paddle graph should load");
    assert_eq!(session.metadata().inputs.len(), 1);
    let input_name = session.metadata().inputs[0].name.clone();

    let mut inputs = TensorMap::new();
    inputs.insert(
        input_name.clone(),
        Tensor::float32(input_name, vec![1, 1, 384, 384], vec![1.0; 384 * 384]),
    );
    let response = session
        .run(RunRequest {
            method: None,
            inputs,
            requested_outputs: None,
        })
        .expect("official Paddle graph should execute");
    let output = response.outputs.values().next().expect("one model output");
    assert_eq!(output.dtype().as_str(), "i64");
    assert!(!output.is_empty());
}
