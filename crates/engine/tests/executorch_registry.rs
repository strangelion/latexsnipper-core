#![cfg(feature = "executorch")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use latexsnipper_engine::{default_runtime_registry, EngineConfig, SnipperEngine};
use latexsnipper_model::{RuntimeVariant, VariantStatus};
use latexsnipper_runtime::{
    ManifestTensorSpec, ModelFiles, ModelManifest, ModelTask, RunRequest, RuntimeKind, TensorMap,
};
use latexsnipper_tensor::Tensor;

/// Opt-in end-to-end check that an ExecuTorch manifest variant is selected by
/// Engine rather than manually constructing its factory/session.
#[test]
fn engine_selects_declared_executorch_variant() {
    let Some(runtime_home) = std::env::var_os("LATEXSNIPPER_EXECUTORCH_HOME") else {
        return;
    };
    let Some(program_path) = std::env::var_os("LATEXSNIPPER_EXECUTORCH_PROGRAM") else {
        return;
    };
    let program_path = PathBuf::from(program_path);
    let model_dir = program_path
        .parent()
        .expect("program has a parent directory");
    let program_name = program_path
        .file_name()
        .expect("program has a file name")
        .to_string_lossy()
        .into_owned();

    let registry = default_runtime_registry(model_dir).unwrap();
    let engine = SnipperEngine::with_runtime_registry(
        EngineConfig::with_models_dir(model_dir.to_path_buf()),
        registry,
    )
    .unwrap();
    let manifest = ModelManifest {
        id: "tiny-executorch-recognizer".to_owned(),
        task: ModelTask::TextRecognition,
        version: "1".to_owned(),
        adapter: "test".to_owned(),
        input: ManifestTensorSpec {
            name: "image".to_owned(),
            shape: vec![1, 1, 8, 8],
            dtype: "float32".to_owned(),
        },
        output: Vec::new(),
        files: ModelFiles::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
        runtime_variants: vec![RuntimeVariant {
            id: "xnnpack-win-x64".to_owned(),
            runtime: "executorch".to_owned(),
            status: VariantStatus::Stable,
            priority: 100,
            artifacts: BTreeMap::from([("program".to_owned(), program_name)]),
            options: Some(BTreeMap::from([
                (
                    "method".to_owned(),
                    serde_json::Value::String("forward".to_owned()),
                ),
                (
                    "libraryPath".to_owned(),
                    serde_json::Value::String(runtime_home.to_string_lossy().into_owned()),
                ),
            ])),
            platforms: Vec::new(),
            capabilities: Vec::new(),
            fallbacks: Vec::new(),
        }],
    };

    let (resolved, session) = engine
        .create_model_runtime_session(&manifest, model_dir, None)
        .unwrap();
    assert_eq!(resolved.runtime, RuntimeKind::ExecuTorch);
    let input_name = session.metadata().inputs[0].name.clone();
    let response = session
        .run(RunRequest::new(TensorMap::from([(
            input_name.clone(),
            Tensor::float32(input_name, vec![1, 1, 8, 8], vec![0.25; 64]),
        )])))
        .unwrap();
    assert!(!response.outputs.is_empty());
}
