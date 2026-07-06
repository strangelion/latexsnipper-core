use crate::model_package::{ModelPackage, ModelTask};
use latexsnipper_foundation::{Result, SnipperError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A manifest file describing a model package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    /// Model identifier (category/variant).
    pub id: String,
    /// What task this model performs.
    pub task: ModelTask,
    /// Model version.
    pub version: String,
    /// Adapter type (e.g., "yolov8-detection-v1", "ctc-recognition-v1").
    pub adapter: String,
    /// Input specification.
    pub input: ManifestTensorSpec,
    /// Output specification.
    pub output: Vec<ManifestTensorSpec>,
    /// Model file paths relative to manifest directory.
    pub files: ModelFiles,
    /// Preprocessing configuration.
    #[serde(default)]
    pub preprocessing: Option<ManifestPreprocessing>,
    /// Decoding configuration.
    #[serde(default)]
    pub decoding: Option<ManifestDecoding>,
    /// SHA-256 checksums for model files.
    #[serde(default)]
    pub checksums: HashMap<String, String>,
}

/// Tensor specification in manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestTensorSpec {
    pub name: String,
    pub shape: Vec<i64>,
    pub dtype: String,
}

/// Model file paths.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelFiles {
    /// Primary model file.
    #[serde(default)]
    pub primary: Option<String>,
    /// Encoder model (for encoder-decoder models).
    #[serde(default)]
    pub encoder: Option<String>,
    /// Decoder model.
    #[serde(default)]
    pub decoder: Option<String>,
    /// Tokenizer or vocabulary file.
    #[serde(default)]
    pub tokenizer: Option<String>,
    /// Configuration file.
    #[serde(default)]
    pub config: Option<String>,
}

/// Preprocessing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestPreprocessing {
    /// Resize dimensions.
    #[serde(default)]
    pub resize: Option<ManifestResize>,
    /// Normalization mean values.
    #[serde(default)]
    pub mean: Option<Vec<f32>>,
    /// Normalization std values.
    #[serde(default)]
    pub std: Option<Vec<f32>>,
    /// Color format (RGB, BGR).
    #[serde(default)]
    pub color_format: Option<String>,
}

/// Resize configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResize {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub keep_ratio: Option<bool>,
}

/// Decoding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestDecoding {
    /// Decoding type (ctc_greedy, beam_search, etc.).
    #[serde(rename = "type")]
    pub decoding_type: String,
    /// Beam width for beam search.
    #[serde(default)]
    pub beam_width: Option<usize>,
    /// Blank token ID for CTC.
    #[serde(default)]
    pub blank_id: Option<usize>,
    /// CTC output tensor layout: "ntc" or "tnc".
    #[serde(default)]
    pub output_layout: Option<String>,
    /// Type of logits: "logits", "probabilities", or "log_probabilities".
    #[serde(default)]
    pub logits_kind: Option<String>,
    /// Temperature for sampling.
    #[serde(default)]
    pub temperature: Option<f32>,
}

use serde::{Deserialize, Serialize};

/// Factory function type for creating ModelPackage from manifest.
pub type AdapterFactory =
    Box<dyn Fn(&ModelManifest, &Path) -> Result<Box<dyn ModelPackage>> + Send + Sync>;

/// Registry of available models with adapter routing.
pub struct ModelRegistry {
    models: HashMap<String, ModelEntry>,
    dirs: Vec<PathBuf>,
    /// Adapter name → factory function mapping.
    adapter_factories: HashMap<String, AdapterFactory>,
}

struct ModelEntry {
    manifest: ModelManifest,
    dir: PathBuf,
}

impl ModelRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
            dirs: Vec::new(),
            adapter_factories: HashMap::new(),
        }
    }

    /// Register an adapter factory for a given adapter name.
    ///
    /// This allows the registry to create `ModelPackage` instances from manifests
    /// that declare `adapter = "adapter-name-v1"`.
    pub fn register_adapter(
        &mut self,
        adapter_name: impl Into<String>,
        factory: impl Fn(&ModelManifest, &Path) -> Result<Box<dyn ModelPackage>> + Send + Sync + 'static,
    ) {
        self.adapter_factories
            .insert(adapter_name.into(), Box::new(factory));
    }

    /// Create a ModelPackage from a manifest by looking up the adapter.
    ///
    /// Returns `None` if no adapter is registered for the manifest's adapter name.
    pub fn create_package(
        &self,
        manifest: &ModelManifest,
        model_dir: &Path,
    ) -> Result<Option<Box<dyn ModelPackage>>> {
        let factory = match self.adapter_factories.get(&manifest.adapter) {
            Some(f) => f,
            None => return Ok(None),
        };

        let package = factory(manifest, model_dir)?;
        Ok(Some(package))
    }

    /// Get list of registered adapter names.
    pub fn registered_adapters(&self) -> Vec<&str> {
        self.adapter_factories.keys().map(|s| s.as_str()).collect()
    }

    /// Create from a directory, scanning for manifest files.
    pub fn from_dir(path: impl AsRef<Path>) -> Result<Self> {
        let mut registry = Self::new();
        registry.register_dir(path)?;
        Ok(registry)
    }

    /// Register a directory containing model packages.
    pub fn register_dir(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        if !path.is_dir() {
            return Err(SnipperError::Model(format!(
                "Not a directory: {}",
                path.display()
            )));
        }

        self.dirs.push(path.clone());

        // Scan for manifest files
        for entry in std::fs::read_dir(&path)
            .map_err(|e| SnipperError::Model(format!("Failed to read {}: {}", path.display(), e)))?
        {
            let entry =
                entry.map_err(|e| SnipperError::Model(format!("Failed to read entry: {}", e)))?;

            if !entry.path().is_dir() {
                continue;
            }

            let model_dir = entry.path();
            let manifest_path = model_dir.join("manifest.toml");

            if manifest_path.exists() {
                if let Ok(manifest) = Self::load_manifest(&manifest_path) {
                    let id = manifest.id.clone();
                    self.models.insert(
                        id,
                        ModelEntry {
                            manifest,
                            dir: model_dir,
                        },
                    );
                }
            }
        }

        Ok(())
    }

    /// Load a manifest file.
    fn load_manifest(path: &Path) -> Result<ModelManifest> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            SnipperError::Model(format!("Failed to read {}: {}", path.display(), e))
        })?;
        toml::from_str(&content)
            .map_err(|e| SnipperError::Model(format!("Failed to parse {}: {}", path.display(), e)))
    }

    /// Get a model by ID.
    pub fn get(&self, id: &str) -> Option<&ModelManifest> {
        self.models.get(id).map(|e| &e.manifest)
    }

    /// Get a model directory by ID.
    pub fn get_dir(&self, id: &str) -> Option<&Path> {
        self.models.get(id).map(|e| e.dir.as_path())
    }

    /// Find models by task.
    pub fn find_by_task(&self, task: ModelTask) -> Vec<(&ModelManifest, &Path)> {
        self.models
            .iter()
            .filter(|(_, e)| e.manifest.task == task)
            .map(|(_, e)| (&e.manifest, e.dir.as_path()))
            .collect()
    }

    /// List all model IDs.
    pub fn list_ids(&self) -> Vec<&str> {
        self.models.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a model is available.
    pub fn has(&self, id: &str) -> bool {
        self.models.contains_key(id)
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Register all built-in adapters with the registry.
///
/// This should be called after creating a `ModelRegistry` to enable
/// automatic package creation from manifests.
///
/// Note: The actual adapter implementations are in the `inference` crate.
/// Use `latexsnipper_inference::register_builtin_adapters()` instead.
pub fn register_builtin_adapters(_registry: &mut ModelRegistry) {
    // This is a placeholder. The actual implementation is in the inference crate.
}
