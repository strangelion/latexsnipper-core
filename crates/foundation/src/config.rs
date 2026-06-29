use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Core configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    pub models_dir: PathBuf,
    pub log_level: String,
    pub acceleration: AccelerationMode,
    pub max_threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AccelerationMode {
    Cpu,
    Gpu,
    Auto,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            log_level: "info".to_string(),
            acceleration: AccelerationMode::Auto,
            max_threads: 4,
        }
    }
}

impl CoreConfig {
    pub fn from_file(path: &std::path::Path) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::error::SnipperError::Config(e.to_string()))?;
        let config: Self = serde_json::from_str(&content)
            .map_err(|e| crate::error::SnipperError::Config(e.to_string()))?;
        Ok(config)
    }

    pub fn save(&self, path: &std::path::Path) -> crate::error::Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| crate::error::SnipperError::Config(e.to_string()))?;
        std::fs::write(path, content)
            .map_err(|e| crate::error::SnipperError::Config(e.to_string()))?;
        Ok(())
    }
}
