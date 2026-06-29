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
}

impl ModelHandle {
    pub fn new(id: impl Into<String>, category: impl Into<String>, variant: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            category: category.into(),
            variant: variant.into(),
            model_path: None,
        }
    }

    /// Create with explicit model path (avoids path guessing).
    pub fn with_path(id: impl Into<String>, path: PathBuf) -> Self {
        let file_stem = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        Self {
            id: id.into(),
            category: file_stem.clone(),
            variant: file_stem,
            model_path: Some(path),
        }
    }

    pub fn id(&self) -> &str { &self.id }
    pub fn category(&self) -> &str { &self.category }
    pub fn variant(&self) -> &str { &self.variant }
    pub fn model_path(&self) -> Option<&Path> { self.model_path.as_deref() }
}

use std::path::Path;
