//! Tests for engine model bootstrap: auto-register adapters, auto-scan, auto-create packages.

use latexsnipper_engine::{EngineConfig, SnipperEngine};
use latexsnipper_runtime::{ModelTask, StubRuntime};
use std::fs;
use std::path::Path;

fn create_temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

fn write_manifest(dir: &Path, manifest: &str) {
    fs::create_dir_all(dir).expect("Failed to create dir");
    fs::write(dir.join("manifest.toml"), manifest).expect("Failed to write manifest");
}

const TEXT_REC_MANIFEST: &str = r#"
id = "text-recognition/demo"
task = "TextRecognition"
version = "1.0.0"
adapter = "ctc-recognition-v1"

[input]
name = "x"
shape = [-1, 3, 48, -1]
dtype = "float32"

[[output]]
name = "softmax"
shape = [-1, -1, -1]
dtype = "float32"

[files]
primary = "model.onnx"
"#;

const FORMULA_DET_MANIFEST: &str = r#"
id = "formula-detection/yolov8"
task = "FormulaDetection"
version = "1.0.0"
adapter = "yolov8-detection-v1"

[input]
name = "images"
shape = [1, 3, 640, 640]
dtype = "float32"

[[output]]
name = "output"
shape = [1, 8400, 6]
dtype = "float32"

[files]
primary = "model.onnx"
"#;

// ────────────────────────────────────────────────────────────────────

#[test]
fn engine_new_auto_registers_adapters() {
    let temp = create_temp_dir();

    write_manifest(
        &temp.path().join("text-recognition").join("demo"),
        TEXT_REC_MANIFEST,
    );

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let engine = SnipperEngine::new(config, runtime);

    // After construction, built-in adapters should be registered
    let adapters = engine.model_registry().registered_adapters();
    assert!(adapters.contains(&"ctc-recognition-v1"));
    assert!(adapters.contains(&"yolov8-detection-v1"));
    assert!(adapters.contains(&"trocr-recognition-v1"));
    assert!(adapters.contains(&"dbnet-detection-v1"));
    assert!(adapters.contains(&"picodet-layout-v1"));
    assert!(adapters.contains(&"onnx-formula-v1"));
}

#[test]
fn engine_new_auto_scans_models_dir() {
    let temp = create_temp_dir();

    write_manifest(
        &temp.path().join("text-recognition").join("demo"),
        TEXT_REC_MANIFEST,
    );

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let engine = SnipperEngine::new(config, runtime);

    // Model should be auto-discovered
    assert!(engine.model_registry().has("text-recognition/demo"));
    assert_eq!(engine.model_registry().len(), 1);
}

#[test]
fn engine_new_handles_empty_models_dir_gracefully() {
    let temp = create_temp_dir();

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let engine = SnipperEngine::new(config, runtime);

    // Should not crash; just have no models
    assert!(engine.model_registry().is_empty());
    // Adapters should still be registered
    assert!(!engine
        .model_registry()
        .registered_adapters()
        .is_empty());
}

#[test]
fn try_new_scans_and_succeeds() {
    let temp = create_temp_dir();

    write_manifest(
        &temp.path().join("text-recognition").join("demo"),
        TEXT_REC_MANIFEST,
    );

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let engine = SnipperEngine::try_new(config, runtime).expect("try_new should succeed");

    assert!(engine.model_registry().has("text-recognition/demo"));
}

#[test]
fn select_model_id_returns_explicit_override() {
    let temp = create_temp_dir();

    write_manifest(
        &temp.path().join("text-recognition").join("custom"),
        &TEXT_REC_MANIFEST.replace("demo", "custom"),
    );
    write_manifest(
        &temp.path().join("text-recognition").join("other"),
        &TEXT_REC_MANIFEST.replace("demo", "other"),
    );

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        text_rec_model: Some("text-recognition/custom".to_string()),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let engine = SnipperEngine::new(config, runtime);

    let selected = engine
        .select_model_id(ModelTask::TextRecognition)
        .expect("select_model_id should succeed")
        .expect("should have a selection");

    assert_eq!(selected, "text-recognition/custom");
}

#[test]
fn select_model_id_errors_on_missing_override() {
    let temp = create_temp_dir();

    // Only "demo" available
    write_manifest(
        &temp.path().join("text-recognition").join("demo"),
        TEXT_REC_MANIFEST,
    );

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        text_rec_model: Some("text-recognition/nonexistent".to_string()),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let engine = SnipperEngine::new(config, runtime);

    let result = engine.select_model_id(ModelTask::TextRecognition);

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not installed"));
    assert!(err.contains("text-recognition/nonexistent"));
}

#[test]
fn rescan_models_discovers_new_models() {
    let temp = create_temp_dir();

    // Start with just text-recognition
    write_manifest(
        &temp.path().join("text-recognition").join("demo"),
        TEXT_REC_MANIFEST,
    );

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let mut engine = SnipperEngine::new(config, runtime);

    assert_eq!(engine.model_registry().len(), 1);
    assert!(!engine
        .model_registry()
        .has("formula-detection/yolov8"));

    // Add a new model
    write_manifest(
        &temp.path()
            .join("formula-detection")
            .join("yolov8"),
        FORMULA_DET_MANIFEST,
    );

    // Rescan
    let report = engine.rescan_models().expect("rescan should succeed");
    assert!(report.is_clean());
    assert_eq!(report.loaded_count(), 2);

    // Both models should be present
    assert!(engine.model_registry().has("text-recognition/demo"));
    assert!(engine
        .model_registry()
        .has("formula-detection/yolov8"));
    assert_eq!(engine.model_registry().len(), 2);
}

#[test]
fn reload_all_models_clears_sessions_and_rescans() {
    let temp = create_temp_dir();

    write_manifest(
        &temp.path().join("text-recognition").join("demo"),
        TEXT_REC_MANIFEST,
    );

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let mut engine = SnipperEngine::new(config, runtime);

    assert_eq!(engine.model_registry().len(), 1);

    // Add another model
    write_manifest(
        &temp.path()
            .join("formula-detection")
            .join("yolov8"),
        FORMULA_DET_MANIFEST,
    );

    let report = engine
        .reload_all_models()
        .expect("reload should succeed");

    assert_eq!(report.loaded_count(), 2);
    assert_eq!(engine.model_registry().len(), 2);
}

#[test]
fn get_or_create_model_package_works() {
    let temp = create_temp_dir();

    write_manifest(
        &temp.path().join("text-recognition").join("demo"),
        TEXT_REC_MANIFEST,
    );

    // Create a minimal ONNX file to satisfy the adapter
    let model_dir = temp.path().join("text-recognition").join("demo");
    fs::write(model_dir.join("model.onnx"), b"dummy onnx").unwrap();

    // Also need config.json
    fs::write(
        model_dir.join("config.json"),
        r#"{"model_type":"crnn_ctc"}"#,
    )
    .unwrap();

    // And keys.txt for CTC
    fs::write(model_dir.join("keys.txt"), "abcdefghijklmnopqrstuvwxyz").unwrap();

    let config = EngineConfig {
        models_dir: temp.path().to_path_buf(),
        parse_mode: latexsnipper_engine::DocumentParseMode::default(), ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());

    let engine = SnipperEngine::new(config, runtime);

    let package = engine
        .get_or_create_model_package("text-recognition/demo")
        .expect("should create package");

    // Should have the correct descriptor
    let descriptor = package.descriptor();
    assert_eq!(descriptor.task, ModelTask::TextRecognition);
}
