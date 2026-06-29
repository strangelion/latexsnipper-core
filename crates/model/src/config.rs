use serde::{Deserialize, Serialize};
use std::path::Path;
use latexsnipper_foundation::{SnipperError, Result};

/// Model configuration parsed from config.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub model_type: String,
    #[serde(default)]
    pub model_family: Option<String>,
    pub input: InputConfig,
    pub output: OutputConfig,
    #[serde(default)]
    pub preprocessing: Option<PreprocessConfig>,
    #[serde(default)]
    pub postprocessing: Option<PostprocessConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    pub name: String,
    pub shape: Vec<i64>,
    pub dtype: String,
    #[serde(default)]
    pub mean: Option<Vec<f32>>,
    #[serde(default)]
    pub std: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub name: String,
    pub shape: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessConfig {
    #[serde(default)]
    pub resize_width: Option<u32>,
    #[serde(default)]
    pub resize_height: Option<u32>,
    #[serde(default)]
    pub keep_ratio: Option<bool>,
    #[serde(default)]
    pub pad_value: Option<f32>,
    #[serde(default)]
    pub divisible_by: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostprocessConfig {
    #[serde(default)]
    pub confidence_threshold: Option<f32>,
    #[serde(default)]
    pub iou_threshold: Option<f32>,
    #[serde(default)]
    pub nms: Option<bool>,
}

impl ModelConfig {
    /// Load config.json from a model directory.
    pub fn load(model_dir: &Path) -> Result<Self> {
        let config_path = model_dir.join("config.json");
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| SnipperError::Model(format!("Failed to read {}: {}", config_path.display(), e)))?;
        Self::parse(&content)
    }

    /// Parse config.json from a string.
    pub fn parse(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| SnipperError::Model(format!("Invalid config.json: {}", e)))
    }

    /// Find the main ONNX model file in a directory.
    pub fn find_model_file(&self, model_dir: &Path) -> Option<std::path::PathBuf> {
        // Try known names first
        let candidates = ["model.onnx", "model_int8.onnx"];
        for name in &candidates {
            let path = model_dir.join(name);
            if path.exists() {
                return Some(path);
            }
        }
        // Fallback: any .onnx file
        std::fs::read_dir(model_dir).ok()
            .and_then(|entries| {
                entries.filter_map(|e| e.ok())
                    .find(|e| e.path().extension().map_or(false, |ext| ext == "onnx"))
                    .map(|e| e.path())
            })
    }

    /// Find encoder ONNX file (for TrOCR models).
    pub fn find_encoder_file(&self, model_dir: &Path) -> Option<std::path::PathBuf> {
        let candidates = ["encoder_model.onnx", "encoder.onnx"];
        for name in &candidates {
            let path = model_dir.join(name);
            if path.exists() { return Some(path); }
        }
        None
    }

    /// Find decoder ONNX file (for TrOCR models).
    pub fn find_decoder_file(&self, model_dir: &Path) -> Option<std::path::PathBuf> {
        let candidates = ["decoder_model.onnx", "decoder.onnx"];
        for name in &candidates {
            let path = model_dir.join(name);
            if path.exists() { return Some(path); }
        }
        None
    }

    /// Find tokenizer file.
    pub fn find_tokenizer_file(&self, model_dir: &Path) -> Option<std::path::PathBuf> {
        let candidates = ["tokenizer.json", "ppocrv5_keys.txt", "dict.txt"];
        for name in &candidates {
            let path = model_dir.join(name);
            if path.exists() { return Some(path); }
        }
        None
    }
}
