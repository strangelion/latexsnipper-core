//! Assembly of built-in runtime factories enabled for this Engine build.

use std::path::Path;

use latexsnipper_foundation::Result;
use latexsnipper_runtime::{providers::onnx_factory::OnnxRuntimeFactory, RuntimeRegistry};

/// Build the canonical runtime registry used by native Engine entry points.
/// Optional native runtimes are registered but remain harmless when their
/// dynamic SDK is absent; runtime-variant resolution will then follow only
/// manifest-declared fallbacks.
pub fn default_runtime_registry(models_dir: &Path) -> Result<RuntimeRegistry> {
    let registry = RuntimeRegistry::with_factory(OnnxRuntimeFactory::new(models_dir.to_path_buf()));
    #[cfg(feature = "paddle")]
    let registry = {
        let mut registry = registry;
        registry.register(latexsnipper_runtime_paddle::PaddleInferenceFactory::new())?;
        registry
    };
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use latexsnipper_runtime::RuntimeKind;

    use super::*;

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
}
