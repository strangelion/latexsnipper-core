use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_runtime::{ModelHandle, RuntimeBackend};
use std::path::Path;
use std::sync::Arc;

use crate::context::PipelineContext;

/// Get the runtime backend from context.
pub fn get_backend(ctx: &PipelineContext) -> Result<Arc<dyn RuntimeBackend>> {
    ctx.backend
        .clone()
        .ok_or_else(|| SnipperError::Runtime("No backend configured".into()))
}

/// Load model config from the first variant directory under a category.
pub fn load_config(models: &Path, category: &str) -> Result<latexsnipper_model::ModelConfig> {
    let cat_dir = models.join(category);
    let variant_dir = std::fs::read_dir(&cat_dir)
        .map_err(|e| SnipperError::Model(format!("Cannot read {}: {}", cat_dir.display(), e)))?
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .ok_or_else(|| SnipperError::Model(format!("No variant in {}", cat_dir.display())))?;
    latexsnipper_model::ModelConfig::load(&variant_dir.path())
}

/// Resolve a model handle using the model resolver if available, otherwise fall back to file path.
pub fn resolve_model_handle(
    ctx: &PipelineContext,
    id: &str,
    fallback_path: std::path::PathBuf,
) -> Result<ModelHandle> {
    if let Some(resolver) = &ctx.model_resolver {
        let model_id = latexsnipper_runtime::ModelId::from_composite_key(id);
        resolver.resolve(&model_id)
    } else {
        Ok(ModelHandle::with_path(id, fallback_path))
    }
}

/// Get or create a cached session.
pub fn get_or_create_session(
    ctx: &mut PipelineContext,
    key: &str,
    backend: &Arc<dyn RuntimeBackend>,
    handle: &ModelHandle,
) -> Result<Arc<Box<dyn latexsnipper_runtime::InferenceSession>>> {
    if let Some(s) = ctx.get_session(key) {
        return Ok(s);
    }
    let session = backend.create_session(handle, latexsnipper_runtime::AccelerationMode::Cpu)?;
    ctx.cache_session(key, session);
    ctx.get_session(key)
        .ok_or_else(|| SnipperError::Runtime(format!("Failed to cache session: {}", key)))
}

/// Find the best model using config, with fallback to primary variant.
pub fn find_best_with_fallback(
    models: &Path,
    category: &str,
    primary_config: &latexsnipper_model::ModelConfig,
) -> Option<(
    latexsnipper_model::ModelConfig,
    std::path::PathBuf,
    std::path::PathBuf,
)> {
    if let Some(result) = latexsnipper_model::ModelConfig::find_best(models, category) {
        return Some(result);
    }

    let fallback_dirs = primary_config.pipeline_fallback_dirs(models);
    for dir in &fallback_dirs {
        if !dir.is_dir() {
            continue;
        }
        if let Some(path) = primary_config.pipeline_model_path(dir) {
            log::info!("Using fallback model: {}", path.display());
            return Some((primary_config.clone(), path, dir.clone()));
        }
    }

    None
}
