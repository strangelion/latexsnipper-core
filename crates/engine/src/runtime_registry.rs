//! Assembly of built-in runtime factories enabled for this Engine build.

use std::path::Path;

use latexsnipper_foundation::Result;
#[cfg(not(target_arch = "wasm32"))]
use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;
use latexsnipper_runtime::RuntimeRegistry;
#[cfg(feature = "runtime-plugins")]
use latexsnipper_runtime_plugin_api::{
    RuntimePluginDiscovery, RuntimePluginDiscoveryReport, RuntimePluginTrustStore,
};

/// Build the canonical runtime registry used by native Engine entry points.
/// Optional native runtimes are registered but remain harmless when their
/// dynamic SDK is absent; runtime-variant resolution will then follow only
/// manifest-declared fallbacks.
pub fn default_runtime_registry(models_dir: &Path) -> Result<RuntimeRegistry> {
    #[cfg(not(target_arch = "wasm32"))]
    let registry = RuntimeRegistry::with_factory(OnnxRuntimeFactory::new(models_dir.to_path_buf()));
    #[cfg(target_arch = "wasm32")]
    let registry = {
        let _ = models_dir;
        RuntimeRegistry::default()
    };
    #[cfg(feature = "paddle")]
    let registry = {
        let mut registry = registry;
        registry.register(latexsnipper_runtime_paddle::PaddleInferenceFactory::new())?;
        registry
    };
    #[cfg(feature = "executorch")]
    let registry = {
        let mut registry = registry;
        registry.register(latexsnipper_runtime_executorch::ExecuTorchFactory::new())?;
        registry
    };
    #[cfg(feature = "tensorrt")]
    let registry = {
        let mut registry = registry;
        registry.register(latexsnipper_runtime_tensorrt::TensorRtFactory::new())?;
        registry
    };
    #[cfg(feature = "tensorrt-rtx")]
    let registry = {
        let mut registry = registry;
        registry.register(latexsnipper_runtime_tensorrt::TensorRtRtxFactory::new())?;
        registry
    };
    #[cfg(feature = "coreml")]
    let registry = {
        let mut registry = registry;
        registry.register(latexsnipper_runtime_coreml::CoreMlFactory::new())?;
        registry
    };
    Ok(registry)
}

/// Build the normal registry, then discover only explicitly trusted and
/// enabled native runtime plugins from application-owned installation roots.
/// Model directories are deliberately not accepted as discovery roots here.
#[cfg(feature = "runtime-plugins")]
pub fn runtime_registry_with_plugins(
    models_dir: &Path,
    plugin_install_roots: &[std::path::PathBuf],
    trust: &RuntimePluginTrustStore,
) -> Result<(RuntimeRegistry, RuntimePluginDiscoveryReport)> {
    let mut registry = default_runtime_registry(models_dir)?;
    let report =
        RuntimePluginDiscovery::new(plugin_install_roots.iter().cloned(), trust).discover();
    report.register_all(&mut registry)?;
    Ok((registry, report))
}

#[cfg(test)]
mod tests {
    use latexsnipper_runtime::RuntimeKind;

    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn default_registry_always_contains_onnx() {
        let registry = default_runtime_registry(Path::new("models")).unwrap();
        assert!(registry.get(&RuntimeKind::OnnxRuntime).is_some());
    }

    #[cfg(feature = "paddle")]
    #[test]
    fn paddle_feature_registers_factory_even_when_sdk_is_absent() {
        let registry = default_runtime_registry(Path::new("models")).unwrap();
        assert!(registry.get(&RuntimeKind::PaddleInference).is_some());
    }

    #[cfg(feature = "executorch")]
    #[test]
    fn executorch_feature_registers_factory_even_when_sdk_is_absent() {
        let registry = default_runtime_registry(Path::new("models")).unwrap();
        assert!(registry.get(&RuntimeKind::ExecuTorch).is_some());
    }

    #[cfg(feature = "tensorrt")]
    #[test]
    fn tensorrt_feature_registers_factory_even_when_sdk_is_absent() {
        let registry = default_runtime_registry(Path::new("models")).unwrap();
        assert!(registry.get(&RuntimeKind::TensorRt).is_some());
    }

    #[cfg(feature = "tensorrt-rtx")]
    #[test]
    fn tensorrt_rtx_feature_registers_factory_even_when_sdk_is_absent() {
        let registry = default_runtime_registry(Path::new("models")).unwrap();
        assert!(registry.get(&RuntimeKind::TensorRtRtx).is_some());
    }

    #[cfg(feature = "coreml")]
    #[test]
    fn coreml_feature_registers_factory_on_every_platform() {
        let registry = default_runtime_registry(Path::new("models")).unwrap();
        assert!(registry.get(&RuntimeKind::CoreMl).is_some());
    }

    #[cfg(feature = "runtime-plugins")]
    #[test]
    fn runtime_plugin_discovery_does_not_scan_model_directories_implicitly() {
        let trust = RuntimePluginTrustStore::new();
        let (registry, report) = runtime_registry_with_plugins(
            Path::new("models"),
            &[std::env::temp_dir().join("latexsnipper-no-runtime-plugins")],
            &trust,
        )
        .unwrap();
        assert!(report.factories.is_empty());
        assert!(report.issues.is_empty());
        assert!(registry.get(&RuntimeKind::OnnxRuntime).is_some());
    }
}
