use latexsnipper_runtime::AccelerationMode;
use std::path::PathBuf;

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub models_dir: PathBuf,
    pub acceleration: AccelerationMode,
    pub max_threads: usize,

    // Model selection overrides (None = auto-discover)
    pub formula_det_model: Option<String>,
    pub formula_rec_model: Option<String>,
    pub text_det_model: Option<String>,
    pub text_rec_model: Option<String>,
    pub doc_ori_model: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            acceleration: AccelerationMode::Auto,
            max_threads: 4,
            formula_det_model: None,
            formula_rec_model: None,
            text_det_model: None,
            text_rec_model: None,
            doc_ori_model: None,
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
}
