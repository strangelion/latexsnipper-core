use std::collections::BTreeMap;
use std::path::Path;

use latexsnipper_engine::{EngineConfig, SnipperEngine};
use latexsnipper_model::{RuntimeVariant, VariantStatus};
use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;
use latexsnipper_runtime::{
    ManifestTensorSpec, ModelFiles, ModelManifest, ModelTask, RunRequest, RuntimeKind,
    RuntimeRegistry, TensorMap,
};
#[cfg(feature = "tensorrt")]
use latexsnipper_runtime_tensorrt::TensorRtFactory;
#[cfg(feature = "tensorrt-rtx")]
use latexsnipper_runtime_tensorrt::TensorRtRtxFactory;
use latexsnipper_tensor::Tensor;

#[test]
fn engine_resolves_and_executes_real_onnx_through_registry() {
    let model_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wasm/tests/fixtures");
    let registry = RuntimeRegistry::with_factory(OnnxRuntimeFactory::new(model_dir.clone()));
    let engine = SnipperEngine::with_runtime_registry(
        EngineConfig::with_models_dir(model_dir.clone()),
        registry,
    )
    .unwrap();
    let manifest = ModelManifest {
        id: "tiny-text-rec".to_owned(),
        task: ModelTask::TextRecognition,
        version: "1".to_owned(),
        adapter: "test".to_owned(),
        input: ManifestTensorSpec {
            name: "x".to_owned(),
            shape: vec![1, 3, 48, 320],
            dtype: "float32".to_owned(),
        },
        output: Vec::new(),
        files: ModelFiles::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
        runtime_variants: vec![RuntimeVariant {
            id: "onnx".to_owned(),
            runtime: "onnx-runtime".to_owned(),
            status: VariantStatus::Stable,
            priority: 1,
            artifacts: BTreeMap::from([("model".to_owned(), "tiny-text-rec.onnx".to_owned())]),
            options: Some(BTreeMap::from([(
                "providers".to_owned(),
                serde_json::json!([
                    { "name": "tensorrt" },
                    { "name": "cuda" },
                    { "name": "cpu" }
                ]),
            )])),
            platforms: Vec::new(),
            capabilities: Vec::new(),
            fallbacks: Vec::new(),
        }],
    };

    let (resolved, session) = engine
        .create_model_runtime_session(&manifest, &model_dir, None)
        .unwrap();
    assert_eq!(resolved.runtime, RuntimeKind::OnnxRuntime);
    assert_eq!(session.metadata().runtime, RuntimeKind::OnnxRuntime);

    let input = Tensor::float32("x", vec![1, 3, 48, 320], vec![0.25; 3 * 48 * 320]);
    let output = session
        .run(RunRequest::new(TensorMap::from([("x".to_owned(), input)])))
        .unwrap();
    assert!(!output.outputs.is_empty());
}

#[cfg(feature = "tensorrt")]
#[test]
fn engine_resolves_and_executes_native_tensorrt_when_test_runtime_is_configured() {
    let Some(runtime) = std::env::var_os("LATEXSNIPPER_TENSORRT_TEST_RUNTIME") else {
        return;
    };
    let model_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wasm/tests/fixtures");
    let model = std::env::var_os("LATEXSNIPPER_TENSORRT_TEST_MODEL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| model_dir.join("tiny-text-rec.onnx"));
    let model_dir = model.parent().unwrap().to_path_buf();
    let model_name = model.file_name().unwrap().to_string_lossy().into_owned();
    let mut registry = RuntimeRegistry::with_factory(OnnxRuntimeFactory::new(model_dir.clone()));
    registry
        .register(TensorRtFactory::with_library_path(runtime))
        .unwrap();
    let engine = SnipperEngine::with_runtime_registry(
        EngineConfig::with_models_dir(model_dir.clone()),
        registry,
    )
    .unwrap();
    let manifest = ModelManifest {
        id: "tiny-text-rec-tensorrt".to_owned(),
        task: ModelTask::TextRecognition,
        version: "1".to_owned(),
        adapter: "test".to_owned(),
        input: ManifestTensorSpec {
            name: "x".to_owned(),
            shape: vec![1, 3, 48, 320],
            dtype: "float32".to_owned(),
        },
        output: Vec::new(),
        files: ModelFiles::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
        runtime_variants: vec![RuntimeVariant {
            id: "tensorrt-native".to_owned(),
            runtime: "tensorrt".to_owned(),
            status: VariantStatus::Stable,
            priority: 100,
            artifacts: BTreeMap::from([("source".to_owned(), model_name)]),
            options: Some(BTreeMap::from([(
                "cache".to_owned(),
                serde_json::Value::Bool(false),
            )])),
            platforms: Vec::new(),
            capabilities: Vec::new(),
            fallbacks: Vec::new(),
        }],
    };

    let (resolved, session) = engine
        .create_model_runtime_session(&manifest, &model_dir, None)
        .unwrap();
    assert_eq!(resolved.runtime, RuntimeKind::TensorRt);
    assert_eq!(session.metadata().runtime, RuntimeKind::TensorRt);

    let input = Tensor::float32("x", vec![1, 3, 48, 320], vec![0.25; 3 * 48 * 320]);
    let output = session
        .run(RunRequest::new(TensorMap::from([("x".to_owned(), input)])))
        .unwrap();
    assert!(!output.outputs.is_empty());
}

#[cfg(feature = "tensorrt-rtx")]
#[test]
fn engine_resolves_and_executes_tensorrt_rtx_when_test_runtime_is_configured() {
    let Some(runtime) = std::env::var_os("LATEXSNIPPER_TENSORRT_RTX_TEST_RUNTIME") else {
        return;
    };
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wasm/tests/fixtures");
    let model = std::env::var_os("LATEXSNIPPER_TENSORRT_RTX_TEST_MODEL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| fixture_dir.join("tiny-text-rec.onnx"));
    let model_dir = model.parent().unwrap().to_path_buf();
    let model_name = model.file_name().unwrap().to_string_lossy().into_owned();
    let mut registry = RuntimeRegistry::with_factory(OnnxRuntimeFactory::new(model_dir.clone()));
    registry
        .register(TensorRtRtxFactory::with_library_path(runtime))
        .unwrap();
    let engine = SnipperEngine::with_runtime_registry(
        EngineConfig::with_models_dir(model_dir.clone()),
        registry,
    )
    .unwrap();
    let manifest = ModelManifest {
        id: "tiny-text-rec-tensorrt-rtx".to_owned(),
        task: ModelTask::TextRecognition,
        version: "1".to_owned(),
        adapter: "test".to_owned(),
        input: ManifestTensorSpec {
            name: "x".to_owned(),
            shape: vec![1, 3, 48, 320],
            dtype: "float32".to_owned(),
        },
        output: Vec::new(),
        files: ModelFiles::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
        runtime_variants: vec![RuntimeVariant {
            id: "tensorrt-rtx".to_owned(),
            runtime: "tensorrt-rtx".to_owned(),
            status: VariantStatus::Stable,
            priority: 100,
            artifacts: BTreeMap::from([("source".to_owned(), model_name)]),
            options: Some(BTreeMap::from([(
                "cache".to_owned(),
                serde_json::Value::Bool(false),
            )])),
            platforms: Vec::new(),
            capabilities: Vec::new(),
            fallbacks: Vec::new(),
        }],
    };

    let (resolved, session) = engine
        .create_model_runtime_session(&manifest, &model_dir, None)
        .unwrap();
    assert_eq!(resolved.runtime, RuntimeKind::TensorRtRtx);
    assert_eq!(session.metadata().runtime, RuntimeKind::TensorRtRtx);

    let input = Tensor::float32("x", vec![1, 3, 48, 320], vec![0.25; 3 * 48 * 320]);
    let output = session
        .run(RunRequest::new(TensorMap::from([("x".to_owned(), input)])))
        .unwrap();
    assert!(!output.outputs.is_empty());
}
