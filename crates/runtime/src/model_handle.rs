use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// An opaque handle to a loaded model.
/// Contains the resolved model path to avoid path guessing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHandle {
    id: String,
    category: String,
    variant: String,
    #[serde(skip)]
    model_path: Option<PathBuf>,
    #[serde(skip)]
    model_bytes: Option<Vec<u8>>,
    #[serde(skip)]
    input_shape: Option<Vec<usize>>,
}

impl ModelHandle {
    pub fn new(
        id: impl Into<String>,
        category: impl Into<String>,
        variant: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            category: category.into(),
            variant: variant.into(),
            model_path: None,
            model_bytes: None,
            input_shape: None,
        }
    }

    /// Create with explicit model path (avoids path guessing).
    pub fn with_path(id: impl Into<String>, path: PathBuf) -> Self {
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        Self {
            id: id.into(),
            category: file_stem.clone(),
            variant: file_stem,
            model_path: Some(path),
            model_bytes: None,
            input_shape: None,
        }
    }

    /// Create with model bytes loaded in memory (for WASM byte-based loading).
    pub fn with_bytes(id: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            id: id.into(),
            category: String::new(),
            variant: String::new(),
            model_path: None,
            model_bytes: Some(bytes),
            input_shape: None,
        }
    }

    /// Attach a concrete input shape declared by the model package.
    /// Dynamic or partially unknown shapes should be omitted.
    pub fn with_input_shape(mut self, input_shape: Vec<usize>) -> Self {
        self.input_shape = Some(input_shape);
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn category(&self) -> &str {
        &self.category
    }
    pub fn variant(&self) -> &str {
        &self.variant
    }
    pub fn model_path(&self) -> Option<&Path> {
        self.model_path.as_deref()
    }
    pub fn model_bytes(&self) -> Option<&[u8]> {
        self.model_bytes.as_deref()
    }
    pub fn input_shape(&self) -> Option<&[usize]> {
        self.input_shape.as_deref()
    }
}

use std::path::Path;
