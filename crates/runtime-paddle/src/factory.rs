//! Paddle Inference factory and runtime probe.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::error::paddle_error;
use crate::ffi::PaddleConfig;
use crate::loader::PaddleLibraryLocator;
use crate::options::PaddleOptions;
use crate::session::PaddleSession;
use latexsnipper_foundation::Result;
use latexsnipper_runtime::{
    DeviceKind, RuntimeArtifacts, RuntimeCapabilities, RuntimeDevice, RuntimeFactory, RuntimeKind,
    RuntimeOptions, RuntimeProbe, RuntimeSession,
};

#[derive(Debug, Clone, Default)]
pub struct PaddleInferenceFactory {
    library_path: Option<PathBuf>,
}

impl PaddleInferenceFactory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure either the Paddle bridge library or its packaged runtime root.
    pub fn with_library_path(path: impl Into<PathBuf>) -> Self {
        Self {
            library_path: Some(path.into()),
        }
    }

    fn locator(&self, options: Option<&PaddleOptions>) -> PaddleLibraryLocator {
        let explicit = options
            .and_then(|options| options.library_path.clone())
            .or_else(|| self.library_path.clone());
        PaddleLibraryLocator::new(explicit)
    }
}

impl RuntimeFactory for PaddleInferenceFactory {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::PaddleInference
    }

    fn probe(&self) -> RuntimeProbe {
        match self.locator(None).load() {
            Ok(api) => RuntimeProbe {
                available: true,
                version: api.version(),
                devices: vec![RuntimeDevice {
                    name: "cpu".to_owned(),
                    kind: DeviceKind::Cpu,
                    memory_bytes: None,
                }],
                reason_unavailable: None,
                capabilities: RuntimeCapabilities {
                    tensor_dtypes: BTreeSet::from([
                        "f32".to_owned(),
                        "f16".to_owned(),
                        "i64".to_owned(),
                        "i32".to_owned(),
                        "u8".to_owned(),
                        "bool".to_owned(),
                    ]),
                    execution_providers: BTreeSet::from(["cpu".to_owned()]),
                    methods: BTreeSet::new(),
                    features: BTreeSet::from([
                        "versioned-c-bridge".to_owned(),
                        "cpu-copy".to_owned(),
                        "full-inference-program".to_owned(),
                    ]),
                },
            },
            Err(error) => RuntimeProbe::unavailable(error.to_string()),
        }
    }

    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        if artifacts.runtime != RuntimeKind::PaddleInference {
            return Err(paddle_error(format!(
                "Paddle factory received '{}' artifacts",
                artifacts.runtime
            )));
        }
        let model_path = required_artifact(artifacts, "model")?;
        let params_path = required_artifact(artifacts, "params")?;
        ensure_regular_file(model_path, "model")?;
        ensure_regular_file(params_path, "params")?;

        let paddle_options = PaddleOptions::from_runtime(options);
        let api = self.locator(Some(&paddle_options)).load()?;
        let config = PaddleConfig::new(api, model_path, params_path, &paddle_options)?;
        let predictor = config.into_predictor()?;
        let session =
            PaddleSession::new(Some(model_path.to_string_lossy().into_owned()), predictor)?;
        Ok(Box::new(session))
    }
}

fn required_artifact<'artifacts>(
    artifacts: &'artifacts RuntimeArtifacts,
    role: &str,
) -> Result<&'artifacts PathBuf> {
    artifacts
        .files
        .get(role)
        .ok_or_else(|| paddle_error(format!("Paddle artifacts are missing '{role}'")))
}

fn ensure_regular_file(path: &Path, role: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(paddle_error(format!(
            "Paddle {role} artifact is not a readable file: {}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_runtime_has_an_actionable_probe_reason() {
        let impossible = std::env::temp_dir().join("latexsnipper-missing-paddle-runtime.dll");
        let probe = PaddleInferenceFactory::with_library_path(impossible).probe();
        assert!(!probe.available);
        let reason = probe.reason_unavailable.unwrap();
        assert!(reason.contains("LATEXSNIPPER_PADDLE_HOME"));
        assert!(reason.contains("not installed"));
    }

    #[test]
    fn rejects_missing_model_artifacts_before_loading_runtime() {
        let artifacts = RuntimeArtifacts::new(RuntimeKind::PaddleInference);
        let error = PaddleInferenceFactory::new()
            .create_session(&artifacts, &RuntimeOptions::default())
            .err()
            .expect("missing artifacts must fail");
        assert!(error.to_string().contains("missing 'model'"));
    }
}
