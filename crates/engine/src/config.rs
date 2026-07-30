use latexsnipper_pipeline::DocumentParseMode;
use latexsnipper_runtime::{AccelerationMode, ModelTask, SelectionPreference};
use std::path::PathBuf;

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub models_dir: PathBuf,
    /// Trusted, release-owned model quality baselines.
    ///
    /// When unset, the engine retains the legacy
    /// `<models parent>/quality/baselines` lookup for compatibility.
    pub quality_baselines_dir: Option<PathBuf>,
    /// Versioned tensor fixture used for explicit provider smoke validation.
    pub provider_smoke_fixture: Option<PathBuf>,
    pub acceleration: AccelerationMode,
    pub max_threads: usize,

    // Model selection overrides (None = auto-discover)
    pub formula_det_model: Option<String>,
    pub formula_rec_model: Option<String>,
    pub text_det_model: Option<String>,
    pub text_rec_model: Option<String>,
    pub table_det_model: Option<String>,
    pub table_struct_model: Option<String>,
    pub handwriting_det_model: Option<String>,
    pub doc_ori_model: Option<String>,

    /// Document parsing mode.
    pub parse_mode: DocumentParseMode,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            quality_baselines_dir: None,
            provider_smoke_fixture: None,
            acceleration: AccelerationMode::Auto,
            max_threads: 4,
            formula_det_model: None,
            formula_rec_model: None,
            text_det_model: None,
            text_rec_model: None,
            table_det_model: None,
            table_struct_model: None,
            handwriting_det_model: None,
            doc_ori_model: None,
            parse_mode: DocumentParseMode::default(),
        }
    }
}

impl EngineConfig {
    /// Create config with explicit model paths.
    pub fn with_models_dir(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            ..Default::default()
        }
    }

    /// Set the trusted model quality baseline directory explicitly.
    pub fn with_quality_baselines_dir(mut self, quality_baselines_dir: PathBuf) -> Self {
        self.quality_baselines_dir = Some(quality_baselines_dir);
        self
    }

    /// Set the versioned provider smoke fixture.
    pub fn with_provider_smoke_fixture(mut self, provider_smoke_fixture: PathBuf) -> Self {
        self.provider_smoke_fixture = Some(provider_smoke_fixture);
        self
    }

    /// Set text detection model variant (e.g. "v6-medium", "ppocrv5-mobile").
    pub fn set_text_det(mut self, variant: &str) -> Self {
        self.text_det_model = Some(variant.to_string());
        self
    }

    /// Set text recognition model variant (e.g. "v6-medium", "ppocrv5-mobile").
    pub fn set_text_rec(mut self, variant: &str) -> Self {
        self.text_rec_model = Some(variant.to_string());
        self
    }

    /// Set table detection model variant.
    pub fn set_table_det(mut self, variant: &str) -> Self {
        self.table_det_model = Some(variant.to_string());
        self
    }

    /// Set table structure model variant.
    pub fn set_table_struct(mut self, variant: &str) -> Self {
        self.table_struct_model = Some(variant.to_string());
        self
    }

    /// Set handwriting detection model variant.
    pub fn set_handwriting_det(mut self, variant: &str) -> Self {
        self.handwriting_det_model = Some(variant.to_string());
        self
    }

    /// Set formula detection model variant.
    pub fn set_formula_det(mut self, variant: &str) -> Self {
        self.formula_det_model = Some(variant.to_string());
        self
    }

    /// Set formula recognition model variant.
    pub fn set_formula_rec(mut self, variant: &str) -> Self {
        self.formula_rec_model = Some(variant.to_string());
        self
    }

    /// Set document parsing mode.
    pub fn set_parse_mode(mut self, mode: DocumentParseMode) -> Self {
        self.parse_mode = mode;
        self
    }

    /// Resolve the explicit model override for a given task, if configured.
    ///
    /// Priority order:
    /// 1. User explicit override (from config)
    /// 2. ModelSelectionPolicy
    /// 3. Default model per category
    /// 4. Highest priority among available candidates
    pub fn model_override(&self, task: ModelTask) -> Option<&str> {
        match task {
            ModelTask::FormulaDetection => self.formula_det_model.as_deref(),
            ModelTask::FormulaRecognition => self.formula_rec_model.as_deref(),
            ModelTask::TextDetection => self.text_det_model.as_deref(),
            ModelTask::TextRecognition => self.text_rec_model.as_deref(),
            ModelTask::TableDetection => self.table_det_model.as_deref(),
            ModelTask::TableStructure => self.table_struct_model.as_deref(),
            _ => None,
        }
    }

    /// Get the model selection preference based on acceleration mode.
    /// Gpu → Accuracy (optimize for quality), Cpu/Auto → Balanced.
    pub fn model_selection_preference(&self) -> SelectionPreference {
        match self.acceleration {
            AccelerationMode::Gpu => SelectionPreference::Accuracy,
            _ => SelectionPreference::Balanced,
        }
    }
}
