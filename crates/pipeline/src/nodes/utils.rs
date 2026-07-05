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

/// Load model config from a category.
/// If ctx specifies a variant for this category, use it directly.
/// Otherwise, pick the first variant alphabetically (deterministic).
pub fn load_config(
    ctx: &PipelineContext,
    models: &Path,
    category: &str,
) -> Result<latexsnipper_model::ModelConfig> {
    let cat_dir = models.join(category);
    let variant_dir = if let Some(variant) = ctx.model_variants.get(category) {
        let dir = cat_dir.join(variant);
        if dir.is_dir() {
            dir
        } else {
            return Err(SnipperError::Model(format!(
                "Requested variant '{}' not found in {}",
                variant,
                cat_dir.display()
            )));
        }
    } else {
        let mut entries: Vec<_> = std::fs::read_dir(&cat_dir)
            .map_err(|e| SnipperError::Model(format!("Cannot read {}: {}", cat_dir.display(), e)))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        // Deterministic: sort by name so behavior is stable across filesystems
        entries.sort_by_key(|a| a.file_name());
        entries
            .first()
            .ok_or_else(|| SnipperError::Model(format!("No variant in {}", cat_dir.display())))?
            .path()
    };
    latexsnipper_model::ModelConfig::load(&variant_dir)
}

/// Unified model variant resolution.
///
/// Returns (ModelConfig, model_file_path, variant_dir) for a category,
/// respecting EngineConfig variant selection when present.
///
/// Resolution order:
/// 1. If ctx.model_variants has a specific variant → use it, error if not found
/// 2. Otherwise → auto-discover with stable alphabetical ordering
///
/// The caller must always use the returned variant_dir for paths (encoder,
/// decoder, tokenizer etc.), ensuring config and ONNX come from the same variant.
pub fn resolve_variant(
    ctx: &PipelineContext,
    models: &Path,
    category: &str,
) -> Result<(
    latexsnipper_model::ModelConfig,
    std::path::PathBuf,
    std::path::PathBuf,
)> {
    if let Some(variant) = ctx.model_variants.get(category) {
        let variant_dir = models.join(category).join(variant);
        if !variant_dir.is_dir() {
            return Err(SnipperError::Model(format!(
                "Requested variant '{}' not found in {}/{}",
                variant,
                models.display(),
                category,
            )));
        }
        let config = latexsnipper_model::ModelConfig::load(&variant_dir)?;
        let path = config.pipeline_model_path(&variant_dir).ok_or_else(|| {
            SnipperError::Model(format!(
                "No ONNX model in {}/{}/{}",
                models.display(),
                category,
                variant
            ))
        })?;
        return Ok((config, path, variant_dir));
    }
    latexsnipper_model::ModelConfig::find_best(models, category)
        .ok_or_else(|| SnipperError::Model(format!("No model found for category '{}'", category)))
}

/// Resolve a model handle using the model resolver if available, otherwise fall back to file path.
/// If the resolver fails, falls back to the provided path instead of returning an error.
pub fn resolve_model_handle(
    ctx: &PipelineContext,
    id: &str,
    fallback_path: std::path::PathBuf,
) -> Result<ModelHandle> {
    if let Some(resolver) = &ctx.model_resolver {
        let model_id = latexsnipper_runtime::ModelId::from_composite_key(id);
        // Try resolver first
        match resolver.resolve(&model_id) {
            Ok(handle) => return Ok(handle),
            Err(e) => {
                log::info!(
                    "Model resolver failed for '{}': {}. Falling back to path {}",
                    id,
                    e,
                    fallback_path.display()
                );
            }
        }
    }
    Ok(ModelHandle::with_path(id, fallback_path))
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
    let session = backend.create_session(handle, ctx.acceleration.clone())?;
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
