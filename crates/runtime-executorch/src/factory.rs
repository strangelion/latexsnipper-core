//! ExecuTorch factory and runtime probe.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use latexsnipper_foundation::Result;
use latexsnipper_runtime::{
    DeviceKind, RuntimeArtifacts, RuntimeCapabilities, RuntimeDevice, RuntimeFactory, RuntimeKind,
    RuntimeOptions, RuntimeProbe, RuntimeSession,
};

use crate::error::executorch_error;
use crate::ffi::ExecuTorchProgram;
use crate::loader::ExecuTorchLibraryLocator;
use crate::options::ExecuTorchOptions;
use crate::session::ExecuTorchSession;

/// Creates sessions backed by a dynamically discovered ExecuTorch C bridge.
#[derive(Debug, Clone, Default)]
pub struct ExecuTorchFactory {
    library_path: Option<PathBuf>,
}

impl ExecuTorchFactory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure either the bridge library or a packaged runtime root.
    pub fn with_library_path(path: impl Into<PathBuf>) -> Self {
        Self {
            library_path: Some(path.into()),
        }
    }

    fn locator(&self, options: Option<&ExecuTorchOptions>) -> ExecuTorchLibraryLocator {
        let explicit = options
            .and_then(|options| options.library_path.clone())
            .or_else(|| self.library_path.clone());
        ExecuTorchLibraryLocator::new(explicit)
    }
}

impl RuntimeFactory for ExecuTorchFactory {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::ExecuTorch
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
                    execution_providers: BTreeSet::from(["xnnpack".to_owned(), "cpu".to_owned()]),
                    methods: BTreeSet::new(),
                    features: BTreeSet::from([
                        "versioned-c-bridge".to_owned(),
                        "cpu-copy".to_owned(),
                        "named-methods".to_owned(),
                        "pte-program".to_owned(),
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
        if artifacts.runtime != RuntimeKind::ExecuTorch {
            return Err(executorch_error(format!(
                "ExecuTorch factory received '{}' artifacts",
                artifacts.runtime
            )));
        }
        let program_path = required_artifact(artifacts)?;
        ensure_regular_file(program_path)?;

        let executorch_options = ExecuTorchOptions::from_runtime(options);
        let api = self.locator(Some(&executorch_options)).load()?;
        let program = ExecuTorchProgram::load(api, program_path)?;
        let session = ExecuTorchSession::new(
            Some(program_path.to_string_lossy().into_owned()),
            program,
            &executorch_options,
        )?;
        Ok(Box::new(session))
    }
}

fn required_artifact(artifacts: &RuntimeArtifacts) -> Result<&PathBuf> {
    artifacts
        .files
        .get("program")
        .or_else(|| artifacts.primary_model())
        .ok_or_else(|| executorch_error("ExecuTorch artifacts are missing 'program'"))
}

fn ensure_regular_file(path: &Path) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(executorch_error(format!(
            "ExecuTorch program artifact is not a readable file: {}",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_runtime_has_an_actionable_probe_reason() {
        let impossible = std::env::temp_dir().join("latexsnipper-missing-executorch-runtime.dll");
        let probe = ExecuTorchFactory::with_library_path(impossible).probe();
        assert!(!probe.available);
        let reason = probe.reason_unavailable.unwrap();
        assert!(reason.contains("LATEXSNIPPER_EXECUTORCH_HOME"));
        assert!(reason.contains("not installed"));
    }

    #[test]
    fn rejects_missing_program_artifact_before_loading_runtime() {
        let artifacts = RuntimeArtifacts::new(RuntimeKind::ExecuTorch);
        let error = ExecuTorchFactory::new()
            .create_session(&artifacts, &RuntimeOptions::default())
            .err()
            .expect("missing artifacts must fail");
        assert!(error.to_string().contains("missing 'program'"));
    }
}
