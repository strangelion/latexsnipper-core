use crate::model_package::{ModelPackage, ModelTask};
use crate::{ResolvedRuntimeVariant, RuntimeRegistry, RuntimeResolver};
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_model::{RuntimeVariant, VariantStatus};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

// ============================================================================
// Scan reporting types
// ============================================================================

/// An issue encountered while scanning a model directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelScanIssue {
    /// Path where the issue was found (model dir or manifest file).
    pub path: PathBuf,
    /// Human-readable description of the issue.
    pub message: String,
}

/// A report summarizing the results of a model root scan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelScanReport {
    /// Model IDs that were successfully loaded.
    pub loaded: Vec<String>,
    /// Issues encountered during scanning.
    pub issues: Vec<ModelScanIssue>,
}

impl ModelScanReport {
    /// Whether the scan completed without any issues.
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }

    /// Number of successfully loaded models.
    pub fn loaded_count(&self) -> usize {
        self.loaded.len()
    }
}

// ============================================================================
// Manifest types
// ============================================================================

/// A manifest file describing a model package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    /// Model identifier (category/variant), e.g. "text-recognition/ppocr-v5-mobile".
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
    /// Legacy model file paths relative to manifest directory.
    /// For new manifests using `runtimeVariants`, this can be empty.
    #[serde(default)]
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
    /// Multiple runtime variants. If absent and `files` is present,
    /// an implicit ONNX variant is derived from `files`.
    #[serde(default, rename = "runtimeVariants")]
    pub runtime_variants: Vec<RuntimeVariant>,
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

// ============================================================================
// ModelRegistry
// ============================================================================

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

    // ── Iteration & access ──────────────────────────────────────────

    /// Iterate over all registered model entries (manifest + directory).
    pub fn entries(&self) -> impl Iterator<Item = (&ModelManifest, &Path)> {
        self.models
            .values()
            .map(|entry| (&entry.manifest, entry.dir.as_path()))
    }

    /// Number of registered models.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Clear all registered models and scanned directories.
    ///
    /// Note: Adapter factories are preserved so hot-reload doesn't lose them.
    pub fn clear_models(&mut self) {
        self.models.clear();
        self.dirs.clear();
    }

    // ── Scanning ────────────────────────────────────────────────────

    /// Create from a directory, scanning for manifest files.
    pub fn from_dir(path: impl AsRef<Path>) -> Result<Self> {
        let mut registry = Self::new();
        registry.register_dir(path)?;
        Ok(registry)
    }

    /// Register a directory containing model packages (flat scan).
    ///
    /// Scans for `models/<name>/manifest.toml` (single-level).
    /// For the two-level category/variant layout, use [`register_models_root`].
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

    /// Register all models under a models root with category/variant layout.
    ///
    /// Expected layout:
    /// ```text
    /// <root>/
    ///   text-recognition/
    ///     ppocr-v5-mobile/
    ///       manifest.toml
    ///       model.onnx
    ///       keys.txt
    ///   formula-recognition/
    ///     trocr-deit/
    ///       manifest.toml
    ///       ...
    /// ```
    ///
    /// Returns a [`ModelScanReport`] with successfully loaded model IDs and any
    /// issues encountered.
    pub fn register_models_root(&mut self, root: impl AsRef<Path>) -> Result<ModelScanReport> {
        let root = root.as_ref().to_path_buf();

        if !root.exists() {
            std::fs::create_dir_all(&root).map_err(|error| {
                SnipperError::Model(format!(
                    "Failed to create models directory '{}': {error}",
                    root.display()
                ))
            })?;
        }

        if !root.is_dir() {
            return Err(SnipperError::Model(format!(
                "Models root is not a directory: {}",
                root.display()
            )));
        }

        if !self.dirs.contains(&root) {
            self.dirs.push(root.clone());
        }

        let mut report = ModelScanReport::default();

        let category_entries = std::fs::read_dir(&root).map_err(|error| {
            SnipperError::Model(format!(
                "Failed to read models root '{}': {error}",
                root.display()
            ))
        })?;

        for category_entry in category_entries {
            let category_entry = match category_entry {
                Ok(entry) => entry,
                Err(error) => {
                    report.issues.push(ModelScanIssue {
                        path: root.clone(),
                        message: error.to_string(),
                    });
                    continue;
                }
            };

            let category_dir = category_entry.path();

            if !category_dir.is_dir() {
                continue;
            }

            if should_ignore_model_dir(&category_dir) {
                continue;
            }

            let model_entries = match std::fs::read_dir(&category_dir) {
                Ok(entries) => entries,
                Err(error) => {
                    report.issues.push(ModelScanIssue {
                        path: category_dir,
                        message: error.to_string(),
                    });
                    continue;
                }
            };

            for model_entry in model_entries {
                let model_entry = match model_entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        report.issues.push(ModelScanIssue {
                            path: category_dir.clone(),
                            message: error.to_string(),
                        });
                        continue;
                    }
                };

                let model_dir = model_entry.path();

                if !model_dir.is_dir() {
                    continue;
                }

                if should_ignore_model_dir(&model_dir) {
                    continue;
                }

                let manifest_path = model_dir.join("manifest.toml");

                if !manifest_path.is_file() {
                    continue;
                }

                match Self::load_manifest(&manifest_path) {
                    Ok(manifest) => {
                        // Verify that the manifest id matches the directory structure.
                        // Example: models/text-recognition/demo/manifest.toml
                        //          must have id = "text-recognition/demo"
                        let category_name = model_dir
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str());
                        let variant_name = model_dir.file_name().and_then(|n| n.to_str());

                        if let (Some(cat), Some(var)) = (category_name, variant_name) {
                            let expected_id = format!("{}/{}", cat, var);
                            if manifest.id != expected_id {
                                report.issues.push(ModelScanIssue {
                                    path: manifest_path,
                                    message: format!(
                                        "Manifest id '{}' does not match directory path '{}'",
                                        manifest.id, expected_id
                                    ),
                                });
                                continue;
                            }
                        }

                        let id = manifest.id.clone();

                        if let Some(previous) = self.models.get(&id) {
                            report.issues.push(ModelScanIssue {
                                path: model_dir.clone(),
                                message: format!(
                                    "Duplicate model id '{}' already registered from '{}'",
                                    id,
                                    previous.dir.display()
                                ),
                            });
                            continue;
                        }

                        self.models.insert(
                            id.clone(),
                            ModelEntry {
                                manifest,
                                dir: model_dir,
                            },
                        );

                        report.loaded.push(id);
                    }

                    Err(error) => {
                        report.issues.push(ModelScanIssue {
                            path: manifest_path,
                            message: error.to_string(),
                        });
                    }
                }
            }
        }

        report.loaded.sort();

        Ok(report)
    }

    /// Load and validate a manifest file.
    fn load_manifest(path: &Path) -> Result<ModelManifest> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            SnipperError::Model(format!("Failed to read {}: {}", path.display(), e))
        })?;

        let manifest: ModelManifest = toml::from_str(&content).map_err(|e| {
            SnipperError::Model(format!("Failed to parse {}: {}", path.display(), e))
        })?;

        manifest.validate()?;

        Ok(manifest)
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

// ─── Directory filtering ───────────────────────────────────────────────

/// Determines whether a model directory should be ignored during scanning.
fn should_ignore_model_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };

    name.starts_with('.')
        || name.starts_with('_')
        || matches!(name, "cache" | "tmp" | "temp" | "runtimes" | "plugins")
}

// ─── Manifest validation and helpers ─────────────────────────────────

impl ModelManifest {
    /// Validate the manifest for required fields and semantic correctness.
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(SnipperError::Model("Model manifest has empty id".into()));
        }

        // Validate category/variant format: <category>/<variant>
        let Some((category, variant)) = self.id.split_once('/') else {
            return Err(SnipperError::Model(format!(
                "Model id '{}' must use '<category>/<variant>' format",
                self.id
            )));
        };

        if category.is_empty() || variant.is_empty() {
            return Err(SnipperError::Model(format!(
                "Model id '{}' must use '<category>/<variant>' format with non-empty parts",
                self.id
            )));
        }

        if self.version.trim().is_empty() {
            return Err(SnipperError::Model(format!(
                "Model '{}' has empty version",
                self.id
            )));
        }

        if self.adapter.trim().is_empty() {
            return Err(SnipperError::Model(format!(
                "Model '{}' has empty adapter",
                self.id
            )));
        }

        if self.runtime_variants.is_empty()
            && self.files.primary.is_none()
            && self.files.encoder.is_none()
            && self.files.decoder.is_none()
        {
            return Err(SnipperError::Model(format!(
                "Model '{}' declares no executable artifacts",
                self.id
            )));
        }

        Ok(())
    }

    /// Extract the category portion of the model ID.
    ///
    /// For `text-recognition/ppocr-v5-mobile`, returns `"text-recognition"`.
    pub fn category(&self) -> Option<&str> {
        self.id.split_once('/').map(|(category, _)| category)
    }

    /// Extract the variant portion of the model ID.
    ///
    /// For `text-recognition/ppocr-v5-mobile`, returns `"ppocr-v5-mobile"`.
    pub fn variant(&self) -> Option<&str> {
        self.id.split_once('/').map(|(_, variant)| variant)
    }

    // ── Runtime variant resolution ───────────────────────────────────

    /// Resolve the best available runtime variant.
    ///
    /// New manifests use their declared variants. Legacy manifests derive one
    /// implicit ONNX variant from `files`. Availability failures only traverse
    /// explicitly declared fallback ids.
    pub fn resolve_runtime(
        &self,
        registry: &RuntimeRegistry,
        model_dir: &Path,
    ) -> Result<ResolvedRuntimeVariant> {
        self.resolve_runtime_variant(registry, model_dir, None)
    }

    pub fn resolve_runtime_variant(
        &self,
        registry: &RuntimeRegistry,
        model_dir: &Path,
        preferred_variant: Option<&str>,
    ) -> Result<ResolvedRuntimeVariant> {
        let variants = self.all_variants(model_dir);
        RuntimeResolver::new(registry).resolve(&self.id, &variants, model_dir, preferred_variant)
    }

    /// Build an implicit ONNX Runtime variant from legacy `files` field.
    fn implicit_onnx_variant(&self, _model_dir: &Path) -> Option<RuntimeVariant> {
        let mut artifacts = BTreeMap::new();
        if let Some(ref p) = self.files.primary {
            artifacts.insert("model".to_string(), p.clone());
        }
        if let Some(ref e) = self.files.encoder {
            artifacts.insert("encoder".to_string(), e.clone());
        }
        if let Some(ref d) = self.files.decoder {
            artifacts.insert("decoder".to_string(), d.clone());
        }
        if let Some(ref t) = self.files.tokenizer {
            artifacts.insert("tokenizer".to_string(), t.clone());
        }
        if let Some(ref c) = self.files.config {
            artifacts.insert("config".to_string(), c.clone());
        }
        if artifacts.is_empty() {
            return None;
        }
        Some(RuntimeVariant {
            id: "onnx-default".to_string(),
            runtime: "onnx-runtime".to_string(),
            status: VariantStatus::Stable,
            priority: 0,
            artifacts,
            options: None,
            platforms: Vec::new(),
            capabilities: Vec::new(),
            fallbacks: Vec::new(),
        })
    }

    /// List all variants, including the implicit one for legacy manifests.
    pub fn all_variants(&self, model_dir: &Path) -> Vec<RuntimeVariant> {
        if !self.runtime_variants.is_empty() {
            self.runtime_variants.clone()
        } else if let Some(v) = self.implicit_onnx_variant(model_dir) {
            vec![v]
        } else {
            vec![]
        }
    }
}

// ─── Task string IDs ─────────────────────────────────────────────────

impl ModelTask {
    /// Stable string identifier for this task.
    ///
    /// These match the category directory names in `models/`.
    pub const fn id(self) -> &'static str {
        match self {
            Self::FormulaDetection => "formula-detection",
            Self::FormulaRecognition => "formula-recognition",
            Self::TextDetection => "text-detection",
            Self::TextRecognition => "text-recognition",
            Self::TableDetection => "table-detection",
            Self::TableStructure => "table-structure",
            Self::LayoutAnalysis => "layout-analysis",
            Self::HandwritingRecognition => "handwriting-recognition",
            Self::VisionLanguageRecognition => "vision-language-recognition",
            Self::DocumentUnderstanding => "document-understanding",
            Self::FormulaCorrection => "formula-correction",
            Self::TextCorrection => "text-correction",
            Self::TableSemanticParsing => "table-semantic-parsing",
            Self::DiagramUnderstanding => "diagram-understanding",
            Self::ChartUnderstanding => "chart-understanding",
            Self::ReadingOrderAnalysis => "reading-order-analysis",
            Self::StyleClassification => "style-classification",
        }
    }
}

// ─── Default impls ────────────────────────────────────────────────────

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
