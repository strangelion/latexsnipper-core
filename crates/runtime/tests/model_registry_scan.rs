//! Tests for `ModelRegistry::register_models_root()` two-level scanning.
//!
//! Category/variant layout:
//! ```text
//! <root>/
//!   text-recognition/
//!     demo/
//!       manifest.toml
//! ```

use latexsnipper_runtime::{ModelManifest, ModelRegistry, ModelTask};
use std::fs;
use std::path::Path;

const VALID_MANIFEST: &str = r#"
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

const BAD_MANIFEST: &str = r#"
id = "text-recognition/bad"
task = "TextRecognition"
# missing required fields: version, adapter
"#;

const DUPLICATE_MANIFEST_A: &str = r#"
id = "text-recognition/dupe"
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

const DUPLICATE_MANIFEST_B: &str = r#"
id = "text-recognition/dupe"
task = "TextRecognition"
version = "2.0.0"
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
primary = "model_v2.onnx"
"#;

const NO_ARTIFACTS_MANIFEST: &str = r#"
id = "text-recognition/no-artifacts"
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
"#;

const EMPTY_ID_MANIFEST: &str = r#"
id = ""
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

const NO_SLASH_ID_MANIFEST: &str = r#"
id = "just-name-no-slash"
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

fn create_temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

fn write_file(dir: &Path, name: &str, content: &str) {
    fs::create_dir_all(dir).expect("Failed to create dir");
    fs::write(dir.join(name), content).expect("Failed to write file");
}

// ────────────────────────────────────────────────────────────────────
// Basic scanning
// ────────────────────────────────────────────────────────────────────

#[test]
fn scans_category_variant_layout() {
    let temp = create_temp_dir();

    let model_dir = temp.path().join("text-recognition").join("demo");
    write_file(&model_dir, "manifest.toml", VALID_MANIFEST);

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert_eq!(report.loaded, vec!["text-recognition/demo"]);
    assert!(report.is_clean());
    assert_eq!(report.loaded_count(), 1);
    assert!(registry.has("text-recognition/demo"));
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
}

#[test]
fn scans_multiple_categories_and_variants() {
    let temp = create_temp_dir();

    write_file(
        &temp.path().join("text-recognition").join("ppocr"),
        "manifest.toml",
        &VALID_MANIFEST.replace("demo", "ppocr"),
    );

    write_file(
        &temp.path().join("formula-detection").join("yolov8"),
        "manifest.toml",
        r#"
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
"#,
    );

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert_eq!(report.loaded_count(), 2);
    assert!(registry.has("text-recognition/ppocr"));
    assert!(registry.has("formula-detection/yolov8"));
    assert!(report.is_clean());
}

#[test]
fn creates_models_dir_if_missing() {
    let temp = create_temp_dir();
    let models_root = temp.path().join("nonexistent-models");

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(&models_root)
        .expect("Should create dir and succeed");

    assert!(models_root.is_dir());
    assert!(report.is_clean());
    assert_eq!(report.loaded_count(), 0);
}

#[test]
fn errors_on_file_instead_of_dir() {
    let temp = create_temp_dir();
    let file_path = temp.path().join("not-a-dir");
    fs::write(&file_path, "content").unwrap();

    let mut registry = ModelRegistry::new();
    let result = registry.register_models_root(&file_path);

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("not a directory"));
}

// ────────────────────────────────────────────────────────────────────
// Hidden directories and ignored names
// ────────────────────────────────────────────────────────────────────

#[test]
fn ignores_hidden_directories() {
    let temp = create_temp_dir();

    // Hidden category directory
    write_file(
        &temp.path().join(".hidden-category").join("demo"),
        "manifest.toml",
        VALID_MANIFEST,
    );

    // Hidden model directory
    write_file(
        &temp.path().join("text-recognition").join(".hidden-variant"),
        "manifest.toml",
        VALID_MANIFEST,
    );

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert!(report.loaded.is_empty());
    assert!(report.is_clean());
}

#[test]
fn ignores_underscore_prefixed_directories() {
    let temp = create_temp_dir();

    write_file(
        &temp.path().join("_internal").join("demo"),
        "manifest.toml",
        VALID_MANIFEST,
    );

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert!(report.loaded.is_empty());
}

#[test]
fn ignores_special_names() {
    let temp = create_temp_dir();

    for name in &["cache", "tmp", "temp", "runtimes", "plugins"] {
        write_file(
            &temp.path().join(*name).join("demo"),
            "manifest.toml",
            VALID_MANIFEST,
        );
    }

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert!(report.loaded.is_empty());
}

// ────────────────────────────────────────────────────────────────────
// Bad manifests
// ────────────────────────────────────────────────────────────────────

#[test]
fn reports_bad_manifest() {
    let temp = create_temp_dir();

    write_file(
        &temp.path().join("text-recognition").join("bad"),
        "manifest.toml",
        BAD_MANIFEST,
    );

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert_eq!(report.loaded_count(), 0);
    assert!(!report.is_clean());
    assert_eq!(report.issues.len(), 1);
    assert!(report.issues[0]
        .message
        .contains("missing field"));
}

#[test]
fn reports_empty_id() {
    let temp = create_temp_dir();

    write_file(
        &temp.path().join("text-recognition").join("empty-id"),
        "manifest.toml",
        EMPTY_ID_MANIFEST,
    );

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert_eq!(report.loaded_count(), 0);
    assert!(!report.is_clean());
    assert!(report.issues[0].message.contains("empty id"));
}

#[test]
fn reports_no_slash_in_id() {
    let temp = create_temp_dir();

    write_file(
        &temp.path().join("text-recognition").join("no-slash"),
        "manifest.toml",
        NO_SLASH_ID_MANIFEST,
    );

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert_eq!(report.loaded_count(), 0);
    assert!(!report.is_clean());
    let msg = &report.issues[0].message;
    assert!(
        msg.contains("category/variant") || msg.contains("<category>/<variant>"),
        "Unexpected issue message: {msg}"
    );
}

#[test]
fn reports_missing_artifacts() {
    let temp = create_temp_dir();

    write_file(
        &temp.path().join("text-recognition").join("no-artifacts"),
        "manifest.toml",
        NO_ARTIFACTS_MANIFEST,
    );

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert_eq!(report.loaded_count(), 0);
    assert!(!report.is_clean());
    assert!(report.issues[0].message.contains("no executable artifacts"));
}

// ────────────────────────────────────────────────────────────────────
// Duplicate IDs
// ────────────────────────────────────────────────────────────────────

#[test]
fn reports_duplicate_id() {
    let temp = create_temp_dir();

    // Two different category directories with the same model ID
    write_file(
        &temp.path().join("text-recognition").join("dupe"),
        "manifest.toml",
        DUPLICATE_MANIFEST_A,
    );

    write_file(
        &temp.path().join("text-recognition-alt").join("dupe"),
        "manifest.toml",
        DUPLICATE_MANIFEST_B,
    );

    let mut registry = ModelRegistry::new();
    let report = registry
        .register_models_root(temp.path())
        .expect("Scan should succeed");

    assert_eq!(report.loaded_count(), 1);
    assert!(!report.is_clean());
    assert_eq!(report.issues.len(), 1);
    assert!(report.issues[0].message.contains("Duplicate model id"));

    // The second one was rejected; first one should still be registered
    assert!(registry.has("text-recognition/dupe"));
}

// ────────────────────────────────────────────────────────────────────
// entries() iteration
// ────────────────────────────────────────────────────────────────────

#[test]
fn entries_iterates_all_models() {
    let temp = create_temp_dir();

    write_file(
        &temp.path().join("text-recognition").join("a"),
        "manifest.toml",
        &VALID_MANIFEST.replace("demo", "a"),
    );
    write_file(
        &temp.path().join("text-recognition").join("b"),
        "manifest.toml",
        &VALID_MANIFEST.replace("demo", "b"),
    );

    let mut registry = ModelRegistry::new();
    registry
        .register_models_root(temp.path())
        .unwrap();

    let mut ids: Vec<String> = registry
        .entries()
        .map(|(m, _)| m.id.clone())
        .collect();
    ids.sort();

    assert_eq!(ids, vec!["text-recognition/a", "text-recognition/b"]);
}

// ────────────────────────────────────────────────────────────────────
// clear_models preserves adapter factories
// ────────────────────────────────────────────────────────────────────

#[test]
fn clear_models_preserves_adapters() {
    let temp = create_temp_dir();

    write_file(
        &temp.path().join("text-recognition").join("demo"),
        "manifest.toml",
        VALID_MANIFEST,
    );

    let mut registry = ModelRegistry::new();

    // Register a dummy adapter
    registry.register_adapter("ctc-recognition-v1", |_, _| {
        Err(latexsnipper_foundation::SnipperError::Model(
            "dummy".into(),
        ))
    });

    // Scan models
    registry
        .register_models_root(temp.path())
        .unwrap();

    assert_eq!(registry.len(), 1);

    // Clear models
    registry.clear_models();

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);

    // Adapter factory should still be registered
    let adapters = registry.registered_adapters();
    assert!(adapters.contains(&"ctc-recognition-v1"));
}

// ────────────────────────────────────────────────────────────────────
// ModelTask::id()
// ────────────────────────────────────────────────────────────────────

#[test]
fn model_task_id_is_stable() {
    assert_eq!(ModelTask::FormulaDetection.id(), "formula-detection");
    assert_eq!(ModelTask::FormulaRecognition.id(), "formula-recognition");
    assert_eq!(ModelTask::TextDetection.id(), "text-detection");
    assert_eq!(ModelTask::TextRecognition.id(), "text-recognition");
    assert_eq!(ModelTask::TableDetection.id(), "table-detection");
    assert_eq!(ModelTask::TableStructure.id(), "table-structure");
    assert_eq!(ModelTask::LayoutAnalysis.id(), "layout-analysis");
    assert_eq!(ModelTask::HandwritingRecognition.id(), "handwriting-recognition");
    assert_eq!(
        ModelTask::VisionLanguageRecognition.id(),
        "vision-language-recognition"
    );
    assert_eq!(
        ModelTask::DocumentUnderstanding.id(),
        "document-understanding"
    );
    assert_eq!(ModelTask::FormulaCorrection.id(), "formula-correction");
    assert_eq!(ModelTask::TextCorrection.id(), "text-correction");
    assert_eq!(
        ModelTask::TableSemanticParsing.id(),
        "table-semantic-parsing"
    );
    assert_eq!(
        ModelTask::DiagramUnderstanding.id(),
        "diagram-understanding"
    );
    assert_eq!(ModelTask::ChartUnderstanding.id(), "chart-understanding");
    assert_eq!(
        ModelTask::ReadingOrderAnalysis.id(),
        "reading-order-analysis"
    );
    assert_eq!(ModelTask::StyleClassification.id(), "style-classification");
}

// ────────────────────────────────────────────────────────────────────
// ModelManifest helpers
// ────────────────────────────────────────────────────────────────────

#[test]
fn manifest_category_and_variant() {
    let manifest = ModelManifest {
        id: "formula-recognition/pp-formulanet-s".to_string(),
        task: ModelTask::FormulaRecognition,
        version: "1.0.0".to_string(),
        adapter: "trocr-recognition-v1".to_string(),
        input: latexsnipper_runtime::ManifestTensorSpec {
            name: "x".to_string(),
            shape: vec![-1, 3, 384, 384],
            dtype: "float32".to_string(),
        },
        output: vec![latexsnipper_runtime::ManifestTensorSpec {
            name: "out".to_string(),
            shape: vec![-1, -1],
            dtype: "int64".to_string(),
        }],
        files: Default::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
        runtime_variants: vec![latexsnipper_model::RuntimeVariant::new(
            "onnx",
            "onnx-runtime",
        )],
    };

    assert_eq!(manifest.category(), Some("formula-recognition"));
    assert_eq!(manifest.variant(), Some("pp-formulanet-s"));
}

#[test]
fn manifest_validate_rejects_missing_adapter() {
    let result = ModelManifest {
        id: "text-recognition/test".to_string(),
        task: ModelTask::TextRecognition,
        version: "1.0.0".to_string(),
        adapter: "".to_string(), // empty adapter!
        input: latexsnipper_runtime::ManifestTensorSpec {
            name: "x".to_string(),
            shape: vec![-1, 3, 48, -1],
            dtype: "float32".to_string(),
        },
        output: vec![latexsnipper_runtime::ManifestTensorSpec {
            name: "out".to_string(),
            shape: vec![-1, -1, -1],
            dtype: "float32".to_string(),
        }],
        files: Default::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
        runtime_variants: vec![latexsnipper_model::RuntimeVariant::new(
            "onnx",
            "onnx-runtime",
        )],
    }
    .validate();

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty adapter"));
}
