use std::path::Path;
use log::info;

use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_ast::Document;
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::{RuntimeBackend, AccelerationMode};
use latexsnipper_model::ModelManager;

use crate::config::EngineConfig;

/// The main engine that orchestrates all LaTeXSnipper capabilities.
pub struct SnipperEngine {
    config: EngineConfig,
    runtime: Box<dyn RuntimeBackend>,
    model_manager: ModelManager,
}

/// Recognition mode.
#[derive(Debug, Clone, Copy)]
pub enum RecognizeMode {
    Formula,
    Text,
    Mixed,
}

impl SnipperEngine {
    /// Create a new engine with the given config and runtime backend.
    pub fn new(config: EngineConfig, runtime: Box<dyn RuntimeBackend>) -> Self {
        let model_manager = ModelManager::new(config.models_dir.clone());
        Self {
            config,
            runtime,
            model_manager,
        }
    }

    /// Get a reference to the runtime backend.
    pub fn runtime(&self) -> &dyn RuntimeBackend {
        &*self.runtime
    }

    /// Get a reference to the model manager.
    pub fn model_manager(&self) -> &ModelManager {
        &self.model_manager
    }

    /// Recognize content in an image.
    pub async fn recognize(&self, image: SnipperImage, mode: RecognizeMode) -> Result<Document> {
        info!("Recognizing image ({}, {}) in {:?} mode", image.width(), image.height(), mode);

        match mode {
            RecognizeMode::Formula => self.recognize_formula(&image).await,
            RecognizeMode::Text => self.recognize_text(&image).await,
            RecognizeMode::Mixed => self.recognize_mixed(&image).await,
        }
    }

    async fn recognize_formula(&self, image: &SnipperImage) -> Result<Document> {
        // Placeholder — will use inference::formula_detector + formula_recognizer
        info!("Formula recognition not yet fully implemented");
        Ok(Document::new())
    }

    async fn recognize_text(&self, image: &SnipperImage) -> Result<Document> {
        // Placeholder — will use inference::text_detector + text_recognizer
        info!("Text recognition not yet fully implemented");
        Ok(Document::new())
    }

    async fn recognize_mixed(&self, image: &SnipperImage) -> Result<Document> {
        // Placeholder — will combine formula + text detection
        info!("Mixed recognition not yet fully implemented");
        Ok(Document::new())
    }

    /// Get engine configuration.
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }
}
