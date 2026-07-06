//! ModelPackage Integration Tests
//!
//! Tests for the ModelPackage system including:
//! - Adapter routing
//! - ModelPlugin registry
//! - End-to-end model recognition
//!
//! Run: cargo test --test model_package_integration

use latexsnipper_runtime::{
    ManifestAdapter, ManifestPreprocessing, ManifestResize, ManifestTensorSpec, ModelId,
    ModelManifest, ModelManifestView, ModelPlugin, ModelPluginRegistry, ModelRegistry, ModelTask,
};
use std::path::Path;

// ═══════════════════════════════════════════════════════════
// ModelRegistry Adapter Routing Tests
// ═══════════════════════════════════════════════════════════

#[test]
fn test_registry_create_empty() {
    let registry = ModelRegistry::new();
    assert!(registry.list_ids().is_empty());
}

#[test]
fn test_registry_register_adapter() {
    let mut registry = ModelRegistry::new();

    registry.register_adapter("test-adapter-v1", |_manifest, _model_dir| {
        // This would create a real ModelPackage in production
        Err(latexsnipper_foundation::SnipperError::Other(
            "Not implemented".into(),
        ))
    });

    let adapters = registry.registered_adapters();
    assert!(adapters.contains(&"test-adapter-v1"));
}

#[test]
fn test_registry_create_package() {
    let mut registry = ModelRegistry::new();

    registry.register_adapter("test-adapter-v1", |manifest, _model_dir| {
        // Create a simple test package
        let package = TestModelPackage {
            id: manifest.id.clone(),
        };
        Ok(Box::new(package))
    });

    let manifest = ModelManifest {
        id: "test/model".to_string(),
        task: ModelTask::FormulaDetection,
        version: "1.0".to_string(),
        adapter: "test-adapter-v1".to_string(),
        input: ManifestTensorSpec {
            name: "images".to_string(),
            shape: vec![1, 3, 640, 640],
            dtype: "float32".to_string(),
        },
        output: vec![ManifestTensorSpec {
            name: "output".to_string(),
            shape: vec![1, 6, 8400],
            dtype: "float32".to_string(),
        }],
        files: Default::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
    };

    let result = registry.create_package(&manifest, Path::new("."));
    assert!(result.is_ok());

    let package = result.unwrap();
    assert!(package.is_some());

    let package = package.unwrap();
    assert_eq!(package.descriptor().id.composite_key(), "test/model");
}

#[test]
fn test_registry_unknown_adapter() {
    let registry = ModelRegistry::new();

    let manifest = ModelManifest {
        id: "test/model".to_string(),
        task: ModelTask::FormulaDetection,
        version: "1.0".to_string(),
        adapter: "unknown-adapter".to_string(),
        input: ManifestTensorSpec {
            name: "images".to_string(),
            shape: vec![1, 3, 640, 640],
            dtype: "float32".to_string(),
        },
        output: vec![],
        files: Default::default(),
        preprocessing: None,
        decoding: None,
        checksums: Default::default(),
    };

    let result = registry.create_package(&manifest, Path::new("."));
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

// ═══════════════════════════════════════════════════════════
// ModelPlugin Registry Tests
// ═══════════════════════════════════════════════════════════

#[test]
fn test_plugin_registry_empty() {
    let registry = ModelPluginRegistry::new();
    assert!(registry.registered_adapters().is_empty());
    assert!(registry.plugins().is_empty());
}

#[test]
fn test_plugin_registry_register() {
    let mut registry = ModelPluginRegistry::new();
    let plugin = Box::new(TestPlugin {
        name: "test-plugin".to_string(),
    });

    registry.register(plugin).unwrap();

    assert_eq!(registry.registered_adapters(), vec!["test-adapter-v1"]);
    assert_eq!(registry.plugins().len(), 1);
    assert_eq!(registry.plugins()[0].name(), "test-plugin");
}

#[test]
fn test_plugin_registry_find_plugin() {
    let mut registry = ModelPluginRegistry::new();
    let plugin = Box::new(TestPlugin {
        name: "test-plugin".to_string(),
    });

    registry.register(plugin).unwrap();

    assert!(registry.find_plugin("test-adapter-v1").is_some());
    assert!(registry.find_plugin("unknown").is_none());
}

#[test]
fn test_plugin_registry_create_package() {
    let mut registry = ModelPluginRegistry::new();
    let plugin = Box::new(TestPlugin {
        name: "test-plugin".to_string(),
    });

    registry.register(plugin).unwrap();

    let manifest = TestManifest {
        id: "test/model".to_string(),
        adapter: "test-adapter-v1".to_string(),
    };

    let result = registry.create_package("test-adapter-v1", &manifest, Path::new("."));
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}

// ═══════════════════════════════════════════════════════════
// ManifestAdapter Tests
// ═══════════════════════════════════════════════════════════

#[test]
fn test_manifest_adapter() {
    let manifest = ModelManifest {
        id: "formula-det/yolov8".to_string(),
        task: ModelTask::FormulaDetection,
        version: "1.0".to_string(),
        adapter: "yolov8-detection-v1".to_string(),
        input: ManifestTensorSpec {
            name: "images".to_string(),
            shape: vec![1, 3, 640, 640],
            dtype: "float32".to_string(),
        },
        output: vec![ManifestTensorSpec {
            name: "output".to_string(),
            shape: vec![1, 6, 8400],
            dtype: "float32".to_string(),
        }],
        files: Default::default(),
        preprocessing: Some(ManifestPreprocessing {
            resize: Some(ManifestResize {
                width: Some(640),
                height: Some(640),
                keep_ratio: Some(true),
            }),
            mean: Some(vec![0.0, 0.0, 0.0]),
            std: Some(vec![1.0, 1.0, 1.0]),
            color_format: Some("RGB".to_string()),
        }),
        decoding: None,
        checksums: Default::default(),
    };

    let adapter = ManifestAdapter::new(&manifest);

    assert_eq!(adapter.id(), "formula-det/yolov8");
    assert_eq!(adapter.adapter(), "yolov8-detection-v1");
    assert_eq!(adapter.version(), "1.0");
    assert_eq!(adapter.input_name(), "images");
    assert_eq!(adapter.input_shape(), &[1, 3, 640, 640]);
}

// ═══════════════════════════════════════════════════════════
// Builtin Adapter Registration Tests
// ═══════════════════════════════════════════════════════════

#[test]
fn test_register_builtin_adapters() {
    let mut registry = ModelRegistry::new();
    latexsnipper_inference::register_builtin_adapters(&mut registry);

    let adapters = registry.registered_adapters();
    assert!(adapters.contains(&"yolov8-detection-v1"));
    assert!(adapters.contains(&"dbnet-detection-v1"));
    assert!(adapters.contains(&"picodet-layout-v1"));
    assert!(adapters.contains(&"trocr-recognition-v1"));
    assert!(adapters.contains(&"ctc-recognition-v1"));
}

// ═══════════════════════════════════════════════════════════
// Helper Types
// ═══════════════════════════════════════════════════════════

struct TestModelPackage {
    id: String,
}

impl latexsnipper_runtime::ModelPackage for TestModelPackage {
    fn descriptor(&self) -> &latexsnipper_runtime::ModelDescriptor {
        // Leak the descriptor to get a static reference (fine for tests)
        Box::leak(Box::new(latexsnipper_runtime::ModelDescriptor {
            id: ModelId::from_composite_key(&self.id),
            task: ModelTask::FormulaDetection,
            version: "1.0".to_string(),
            input_spec: latexsnipper_runtime::TensorSpec {
                name: "images".to_string(),
                shape: vec![1, 3, 640, 640],
                dtype: latexsnipper_runtime::TensorDtype::Float32,
            },
            output_spec: vec![latexsnipper_runtime::TensorSpec {
                name: "output".to_string(),
                shape: vec![1, 6, 8400],
                dtype: latexsnipper_runtime::TensorDtype::Float32,
            }],
            artifact_paths: Vec::new(),
        }))
    }

    fn create_executor(
        &self,
        _runtime: std::sync::Arc<dyn latexsnipper_runtime::RuntimeBackend>,
    ) -> latexsnipper_foundation::Result<Box<dyn latexsnipper_runtime::ModelExecutor>> {
        Err(latexsnipper_foundation::SnipperError::Other(
            "Test executor not implemented".into(),
        ))
    }
}

struct TestPlugin {
    name: String,
}

impl ModelPlugin for TestPlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn supported_adapters(&self) -> Vec<&str> {
        vec!["test-adapter-v1"]
    }

    fn create_package(
        &self,
        _adapter: &str,
        manifest: &dyn latexsnipper_runtime::ModelManifestView,
        _model_dir: &Path,
    ) -> latexsnipper_foundation::Result<Box<dyn latexsnipper_runtime::ModelPackage>> {
        let package = TestModelPackage {
            id: manifest.id().to_string(),
        };
        Ok(Box::new(package))
    }
}

struct TestManifest {
    id: String,
    adapter: String,
}

impl latexsnipper_runtime::ModelManifestView for TestManifest {
    fn id(&self) -> &str {
        &self.id
    }

    fn task(&self) -> ModelTask {
        ModelTask::FormulaDetection
    }

    fn adapter(&self) -> &str {
        &self.adapter
    }

    fn version(&self) -> &str {
        "1.0"
    }

    fn input_name(&self) -> &str {
        "images"
    }

    fn input_shape(&self) -> &[i64] {
        &[1, 3, 640, 640]
    }

    fn get_file(&self, _name: &str) -> Option<&str> {
        None
    }
}
