use std::collections::BTreeMap;
use std::path::PathBuf;

use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;
use latexsnipper_runtime::{
    ExecutionProviderSpec, RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind,
    RuntimeOptions, TensorMap,
};
use latexsnipper_runtime_tensorrt::{TensorRtFactory, TensorRtRtxFactory};
use latexsnipper_tensor::Tensor;

#[test]
fn native_tensorrt_matches_onnx_cpu_when_test_runtime_is_configured() {
    let Some(runtime) = std::env::var_os("LATEXSNIPPER_TENSORRT_TEST_RUNTIME") else {
        return;
    };
    let model = std::env::var_os("LATEXSNIPPER_TENSORRT_TEST_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../wasm/tests/fixtures/tiny-text-rec.onnx")
        });
    let factory = TensorRtFactory::with_library_path(runtime);
    assert_native_parity(&factory, RuntimeKind::TensorRt, model);
}

#[test]
fn tensorrt_rtx_matches_onnx_cpu_when_test_runtime_is_configured() {
    let Some(runtime) = std::env::var_os("LATEXSNIPPER_TENSORRT_RTX_TEST_RUNTIME") else {
        return;
    };
    let model = std::env::var_os("LATEXSNIPPER_TENSORRT_RTX_TEST_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../wasm/tests/fixtures/tiny-text-rec.onnx")
        });
    let factory = TensorRtRtxFactory::with_library_path(runtime);
    assert_native_parity(&factory, RuntimeKind::TensorRtRtx, model);
}

fn assert_native_parity(native_factory: &dyn RuntimeFactory, runtime: RuntimeKind, model: PathBuf) {
    let shape = vec![1, 3, 48, 320];
    let input = Tensor::float32("x", shape, vec![0.25; 3 * 48 * 320]);
    let artifacts = RuntimeArtifacts::new(runtime).with_file("source", model.clone());
    let native_options = RuntimeOptions {
        extra: BTreeMap::from([("cache".to_owned(), serde_json::Value::Bool(false))]),
        ..RuntimeOptions::default()
    };
    let native = native_factory
        .create_session(&artifacts, &native_options)
        .unwrap();

    let cpu_factory = OnnxRuntimeFactory::new(model.parent().unwrap().to_path_buf());
    let cpu_artifacts = RuntimeArtifacts::new(RuntimeKind::OnnxRuntime).with_file("model", model);
    let cpu_options = RuntimeOptions {
        providers: vec![ExecutionProviderSpec::cpu()],
        ..RuntimeOptions::default()
    };
    let cpu = cpu_factory
        .create_session(&cpu_artifacts, &cpu_options)
        .unwrap();

    let request = || RunRequest::new(TensorMap::from([("x".to_owned(), input.clone())]));
    let native_outputs = native.run(request()).unwrap().outputs;
    let cpu_outputs = cpu.run(request()).unwrap().outputs;
    assert_eq!(
        native_outputs.keys().collect::<Vec<_>>(),
        cpu_outputs.keys().collect::<Vec<_>>()
    );
    for (name, expected) in cpu_outputs {
        let actual = &native_outputs[&name];
        assert_eq!(actual.shape(), expected.shape());
        let actual = actual.as_f32_slice().expect("parity fixture outputs f32");
        let expected = expected.as_f32_slice().expect("parity fixture outputs f32");
        let max_error = actual
            .iter()
            .zip(expected)
            .map(|(actual, expected)| (actual - expected).abs())
            .fold(0.0_f32, f32::max);
        assert!(max_error <= 1e-4, "output {name} max_abs_error={max_error}");
    }
}
