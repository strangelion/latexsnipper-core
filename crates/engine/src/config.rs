use std::path::PathBuf;
use latexsnipper_runtime::AccelerationMode;

/// Engine configuration.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub models_dir: PathBuf,
    pub acceleration: AccelerationMode,
    pub max_threads: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            models_dir: PathBuf::from("models"),
            acceleration: AccelerationMode::Auto,
            max_threads: 4,
        }
    }
}
