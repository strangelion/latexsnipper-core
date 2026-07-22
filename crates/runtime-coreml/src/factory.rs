//! Core ML factory, artifact validation, and compiled-model preparation.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use latexsnipper_foundation::Result;
#[cfg(target_vendor = "apple")]
use latexsnipper_runtime::{DeviceKind, RuntimeDevice};
use latexsnipper_runtime::{
    RuntimeArtifacts, RuntimeCapabilities, RuntimeFactory, RuntimeKind, RuntimeOptions,
    RuntimeProbe, RuntimeSession,
};

use crate::error::coreml_error;
use crate::options::CoreMlOptions;

#[derive(Debug, Clone, Default)]
pub struct CoreMlFactory;

impl CoreMlFactory {
    pub fn new() -> Self {
        Self
    }
}

impl RuntimeFactory for CoreMlFactory {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::CoreMl
    }

    fn probe(&self) -> RuntimeProbe {
        probe_platform()
    }

    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        if artifacts.runtime != RuntimeKind::CoreMl {
            return Err(coreml_error(format!(
                "Core ML factory received '{}' artifacts",
                artifacts.runtime
            )));
        }
        let model = required_artifact(artifacts)?;
        validate_artifact(model)?;
        let options = CoreMlOptions::from_runtime(options)?;
        create_platform_session(model, &options)
    }
}

fn required_artifact(artifacts: &RuntimeArtifacts) -> Result<&PathBuf> {
    artifacts
        .files
        .get("compiled")
        .or_else(|| artifacts.files.get("package"))
        .or_else(|| artifacts.primary_model())
        .ok_or_else(|| {
            coreml_error("Core ML artifacts are missing 'model', 'package', or 'compiled'")
        })
}

fn validate_artifact(path: &Path) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            coreml_error(format!(
                "Core ML artifact must use .mlmodel, .mlpackage, or .mlmodelc: {}",
                path.display()
            ))
        })?;
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        coreml_error(format!(
            "inspect Core ML artifact '{}': {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(coreml_error(format!(
            "Core ML artifact may not be a symbolic link: {}",
            path.display()
        )));
    }
    let valid_kind = match extension.as_str() {
        "mlmodel" => metadata.is_file(),
        "mlpackage" | "mlmodelc" => metadata.is_dir(),
        _ => false,
    };
    if valid_kind {
        Ok(())
    } else {
        Err(coreml_error(format!(
            "Core ML artifact has an unsupported format or filesystem kind: {}",
            path.display()
        )))
    }
}

fn capabilities() -> RuntimeCapabilities {
    RuntimeCapabilities {
        tensor_dtypes: BTreeSet::from(["f32".to_owned(), "f16".to_owned(), "i32".to_owned()]),
        execution_providers: BTreeSet::from([
            "coreml".to_owned(),
            "cpu".to_owned(),
            "cpu-gpu".to_owned(),
            "cpu-neural-engine".to_owned(),
        ]),
        methods: BTreeSet::from(["predict".to_owned()]),
        features: BTreeSet::from([
            "native-coreml".to_owned(),
            "compiled-model-cache-v1".to_owned(),
            "mlmodel".to_owned(),
            "mlpackage".to_owned(),
            "mlmodelc".to_owned(),
            "mlmultiarray".to_owned(),
            "serial-session".to_owned(),
            "cpu-copy".to_owned(),
        ]),
    }
}

#[cfg(target_vendor = "apple")]
fn probe_platform() -> RuntimeProbe {
    RuntimeProbe {
        available: true,
        version: Some(crate::ffi::runtime_version()),
        devices: vec![RuntimeDevice {
            name: "Apple Core ML automatic compute selection".to_owned(),
            kind: DeviceKind::Auto,
            memory_bytes: None,
        }],
        reason_unavailable: None,
        capabilities: capabilities(),
    }
}

#[cfg(not(target_vendor = "apple"))]
fn probe_platform() -> RuntimeProbe {
    let mut probe = RuntimeProbe::unavailable(format!(
        "Core ML is unavailable on {}; use an Apple target or select a manifest-declared fallback",
        std::env::consts::OS
    ));
    probe.capabilities = capabilities();
    probe
}

#[cfg(target_vendor = "apple")]
fn create_platform_session(
    model: &Path,
    options: &CoreMlOptions,
) -> Result<Box<dyn RuntimeSession>> {
    use std::sync::{Mutex, OnceLock};

    use crate::cache::CoreMlCache;
    use crate::session::{CoreMlSession, TemporaryCompiledModel};

    static COMPILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    if CoreMlCache::is_compiled_model(model) {
        return Ok(Box::new(CoreMlSession::load(model, options, None)?));
    }

    let cache = CoreMlCache::new(options.cache_dir.clone());
    cache.prepare_root()?;
    let runtime_version = crate::ffi::runtime_version();
    let final_path = cache.compiled_path(model, &runtime_version)?;
    let lock = COMPILE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock
        .lock()
        .map_err(|_| coreml_error("Core ML model compilation lock was poisoned"))?;

    if options.cache && CoreMlCache::is_compiled_model(&final_path) {
        return Ok(Box::new(CoreMlSession::load(&final_path, options, None)?));
    }

    let temporary_path = cache.temporary_path(&final_path)?;
    if let Err(error) = crate::ffi::compile_model(model, &temporary_path) {
        if temporary_path.exists() {
            cleanup_compiled_model(&temporary_path);
        }
        return Err(error);
    }
    if !CoreMlCache::is_compiled_model(&temporary_path) {
        cleanup_compiled_model(&temporary_path);
        return Err(coreml_error(format!(
            "Core ML compiler did not produce a .mlmodelc directory: {}",
            temporary_path.display()
        )));
    }

    if options.cache {
        if let Err(error) = std::fs::rename(&temporary_path, &final_path) {
            if CoreMlCache::is_compiled_model(&final_path) {
                cleanup_compiled_model(&temporary_path);
            } else {
                cleanup_compiled_model(&temporary_path);
                return Err(coreml_error(format!(
                    "publish compiled Core ML cache '{}': {error}",
                    final_path.display()
                )));
            }
        }
        Ok(Box::new(CoreMlSession::load(&final_path, options, None)?))
    } else {
        let temporary =
            TemporaryCompiledModel::new(temporary_path.clone(), options.cache_dir.clone());
        Ok(Box::new(CoreMlSession::load(
            &temporary_path,
            options,
            Some(temporary),
        )?))
    }
}

#[cfg(target_vendor = "apple")]
fn cleanup_compiled_model(path: &Path) {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    if let Err(error) = result {
        if error.kind() == std::io::ErrorKind::NotFound {
            return;
        }
        log::warn!(
            "Failed to clean temporary Core ML model '{}': {error}",
            path.display()
        );
    }
}

#[cfg(not(target_vendor = "apple"))]
fn create_platform_session(
    _model: &Path,
    _options: &CoreMlOptions,
) -> Result<Box<dyn RuntimeSession>> {
    Err(coreml_error(format!(
        "Core ML sessions cannot be created on {}",
        std::env::consts::OS
    )))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn fixture_path(case: &str, extension: &str, directory: bool) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "latexsnipper-coreml-artifact-{}-{}.{}",
            std::process::id(),
            case,
            extension
        ));
        if directory {
            fs::create_dir_all(&path).unwrap();
        } else {
            fs::write(&path, b"fixture").unwrap();
        }
        path
    }

    #[test]
    fn validates_all_three_coreml_artifact_formats() {
        let model = fixture_path("valid-model", "mlmodel", false);
        let package = fixture_path("valid-package", "mlpackage", true);
        let compiled = fixture_path("valid-compiled", "mlmodelc", true);
        assert!(validate_artifact(&model).is_ok());
        assert!(validate_artifact(&package).is_ok());
        assert!(validate_artifact(&compiled).is_ok());
        fs::remove_file(model).unwrap();
        fs::remove_dir_all(package).unwrap();
        fs::remove_dir_all(compiled).unwrap();
    }

    #[test]
    fn rejects_extension_kind_mismatch() {
        let invalid = fixture_path("invalid-kind", "mlmodel", true);
        assert!(validate_artifact(&invalid).is_err());
        fs::remove_dir_all(invalid).unwrap();
    }

    #[cfg(not(target_vendor = "apple"))]
    #[test]
    fn non_apple_probe_is_explicitly_unavailable() {
        let probe = CoreMlFactory::new().probe();
        assert!(!probe.available);
        assert!(probe
            .reason_unavailable
            .as_deref()
            .is_some_and(|reason| reason.contains("Apple")));
        assert!(probe.capabilities.features.contains("mlmodelc"));
    }
}
