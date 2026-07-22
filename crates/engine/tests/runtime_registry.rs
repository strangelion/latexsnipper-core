use std::collections::BTreeMap;
use std::path::Path;

use latexsnipper_engine::{EngineConfig, SnipperEngine};
use latexsnipper_model::{RuntimeVariant, VariantStatus};
use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;
use latexsnipper_runtime::{
    ManifestTensorSpec, ModelFiles, ModelManifest, ModelTask, RunRequest, RuntimeKind,
    RuntimeRegistry, TensorMap,
};
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
            options: None,
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
