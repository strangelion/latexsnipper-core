//! Stable execution-provider descriptors and resolution evidence.
//!
//! This layer intentionally does not expose or bind an ONNX Runtime provider
//! ABI. A runtime-owned adapter may use the resolved descriptor to configure
//! its supported provider API.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDescriptor {
    pub id: String,
    pub runtime_family: String,
    pub platforms: BTreeSet<String>,
    pub architectures: BTreeSet<String>,
    pub capabilities: BTreeSet<String>,
    pub required_libraries: BTreeSet<String>,
    pub priority: i32,
    pub experimental: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEnvironment {
    pub platform: String,
    pub architecture: String,
    pub capabilities: BTreeSet<String>,
    /// Library basenames discovered by the trusted runtime installation.
    /// Model-package directories must never populate this set.
    pub trusted_runtime_libraries: BTreeSet<String>,
}

impl RuntimeEnvironment {
    pub fn current() -> Self {
        Self {
            platform: std::env::consts::OS.to_ascii_lowercase(),
            architecture: std::env::consts::ARCH.to_ascii_lowercase(),
            capabilities: BTreeSet::new(),
            trusted_runtime_libraries: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderOptions {
    #[serde(default)]
    pub values: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProbe {
    pub available: bool,
    pub code: Option<ProviderResolutionCode>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedProvider {
    pub descriptor: ProviderDescriptor,
    pub options: ProviderOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderResolutionCode {
    Accepted,
    ProviderUnknown,
    ProviderPlatformUnsupported,
    ProviderArchitectureUnsupported,
    ProviderLibraryMissing,
    ProviderCapabilityMissing,
    ProviderConfigurationInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderResolutionReason {
    pub candidate: String,
    pub accepted: bool,
    pub code: ProviderResolutionCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderResolutionTrace {
    pub model: String,
    pub requested_provider: Option<String>,
    pub selected_provider: Option<String>,
    pub fallback: bool,
    pub reasons: Vec<ProviderResolutionReason>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProviderPluginError {
    #[error("provider descriptor id or runtime family is empty")]
    InvalidDescriptor,
    #[error("provider '{0}' configuration is invalid: {1}")]
    InvalidConfiguration(String, String),
    #[error("no execution provider is available for model '{model}'")]
    NoProviderAvailable {
        model: String,
        trace: ProviderResolutionTrace,
    },
}

pub trait ExecutionProviderPlugin: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;

    fn probe(&self, environment: &RuntimeEnvironment)
        -> Result<ProviderProbe, ProviderPluginError>;

    fn configure(&self, options: &ProviderOptions)
        -> Result<ResolvedProvider, ProviderPluginError>;
}

#[derive(Default)]
pub struct ExecutionProviderRegistry {
    plugins: BTreeMap<String, Arc<dyn ExecutionProviderPlugin>>,
}

impl ExecutionProviderRegistry {
    pub fn with_builtin_onnx() -> Self {
        let mut registry = Self::default();
        for plugin in builtin_onnx_provider_plugins() {
            registry
                .register_arc(plugin)
                .expect("built-in provider descriptors are unique and valid");
        }
        registry
    }

    pub fn register(
        &mut self,
        plugin: impl ExecutionProviderPlugin + 'static,
    ) -> Result<(), ProviderPluginError> {
        self.register_arc(Arc::new(plugin))
    }

    pub fn register_arc(
        &mut self,
        plugin: Arc<dyn ExecutionProviderPlugin>,
    ) -> Result<(), ProviderPluginError> {
        let descriptor = plugin.descriptor();
        validate_descriptor(&descriptor)?;
        let id = canonical_provider_id(&descriptor.id);
        if self.plugins.insert(id.clone(), plugin).is_some() {
            return Err(ProviderPluginError::InvalidConfiguration(
                id,
                "provider is already registered".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        model: &str,
        requested: &[(String, ProviderOptions)],
        environment: &RuntimeEnvironment,
        allow_cpu_fallback: bool,
    ) -> Result<(ResolvedProvider, ProviderResolutionTrace), ProviderPluginError> {
        let requested_provider = requested.first().map(|item| item.0.clone());
        let mut candidates = requested.to_vec();
        if candidates.is_empty() {
            let mut descriptors: Vec<_> = self
                .plugins
                .values()
                .map(|plugin| plugin.descriptor())
                .collect();
            descriptors.sort_by(|left, right| {
                right
                    .priority
                    .cmp(&left.priority)
                    .then_with(|| left.id.cmp(&right.id))
            });
            candidates.extend(
                descriptors
                    .into_iter()
                    .map(|descriptor| (descriptor.id, ProviderOptions::default())),
            );
        }
        if allow_cpu_fallback
            && !candidates
                .iter()
                .any(|candidate| canonical_provider_id(&candidate.0) == "cpu")
        {
            candidates.push(("cpu".to_owned(), ProviderOptions::default()));
        }

        let mut reasons = Vec::new();
        for (candidate, options) in candidates {
            let id = canonical_provider_id(&candidate);
            let Some(plugin) = self.plugins.get(&id) else {
                reasons.push(ProviderResolutionReason {
                    candidate,
                    accepted: false,
                    code: ProviderResolutionCode::ProviderUnknown,
                    message: "provider plugin is not registered".to_owned(),
                });
                continue;
            };
            let probe = plugin.probe(environment)?;
            if !probe.available {
                reasons.push(ProviderResolutionReason {
                    candidate: id,
                    accepted: false,
                    code: probe
                        .code
                        .unwrap_or(ProviderResolutionCode::ProviderCapabilityMissing),
                    message: probe.message,
                });
                continue;
            }
            match plugin.configure(&options) {
                Ok(resolved) => {
                    reasons.push(ProviderResolutionReason {
                        candidate: id.clone(),
                        accepted: true,
                        code: ProviderResolutionCode::Accepted,
                        message: "provider is available and configured".to_owned(),
                    });
                    let trace = ProviderResolutionTrace {
                        model: model.to_owned(),
                        requested_provider: requested_provider.clone(),
                        selected_provider: Some(id.clone()),
                        fallback: requested_provider
                            .as_ref()
                            .is_some_and(|requested| canonical_provider_id(requested) != id),
                        reasons,
                    };
                    return Ok((resolved, trace));
                }
                Err(error) => reasons.push(ProviderResolutionReason {
                    candidate: id,
                    accepted: false,
                    code: ProviderResolutionCode::ProviderConfigurationInvalid,
                    message: error.to_string(),
                }),
            }
        }

        let trace = ProviderResolutionTrace {
            model: model.to_owned(),
            requested_provider,
            selected_provider: None,
            fallback: false,
            reasons,
        };
        Err(ProviderPluginError::NoProviderAvailable {
            model: model.to_owned(),
            trace,
        })
    }
}

#[derive(Clone)]
struct StaticProviderPlugin {
    descriptor: ProviderDescriptor,
}

impl ExecutionProviderPlugin for StaticProviderPlugin {
    fn descriptor(&self) -> ProviderDescriptor {
        self.descriptor.clone()
    }

    fn probe(
        &self,
        environment: &RuntimeEnvironment,
    ) -> Result<ProviderProbe, ProviderPluginError> {
        let descriptor = &self.descriptor;
        if !descriptor.platforms.is_empty() && !descriptor.platforms.contains(&environment.platform)
        {
            return Ok(ProviderProbe {
                available: false,
                code: Some(ProviderResolutionCode::ProviderPlatformUnsupported),
                message: format!(
                    "provider '{}' does not support platform '{}'",
                    descriptor.id, environment.platform
                ),
            });
        }
        if !descriptor.architectures.is_empty()
            && !descriptor.architectures.contains(&environment.architecture)
        {
            return Ok(ProviderProbe {
                available: false,
                code: Some(ProviderResolutionCode::ProviderArchitectureUnsupported),
                message: format!(
                    "provider '{}' does not support architecture '{}'",
                    descriptor.id, environment.architecture
                ),
            });
        }
        if let Some(missing) = descriptor
            .required_libraries
            .difference(&environment.trusted_runtime_libraries)
            .next()
        {
            return Ok(ProviderProbe {
                available: false,
                code: Some(ProviderResolutionCode::ProviderLibraryMissing),
                message: format!(
                    "trusted runtime library '{}' is missing for provider '{}'",
                    missing, descriptor.id
                ),
            });
        }
        if let Some(missing) = descriptor
            .capabilities
            .difference(&environment.capabilities)
            .next()
        {
            return Ok(ProviderProbe {
                available: false,
                code: Some(ProviderResolutionCode::ProviderCapabilityMissing),
                message: format!(
                    "runtime capability '{}' is missing for provider '{}'",
                    missing, descriptor.id
                ),
            });
        }
        Ok(ProviderProbe {
            available: true,
            code: Some(ProviderResolutionCode::Accepted),
            message: "provider requirements are satisfied".to_owned(),
        })
    }

    fn configure(
        &self,
        options: &ProviderOptions,
    ) -> Result<ResolvedProvider, ProviderPluginError> {
        if options.values.values().any(|value| {
            matches!(
                value,
                serde_json::Value::Array(_) | serde_json::Value::Object(_)
            )
        }) {
            return Err(ProviderPluginError::InvalidConfiguration(
                self.descriptor.id.clone(),
                "nested provider option values are not supported".to_owned(),
            ));
        }
        Ok(ResolvedProvider {
            descriptor: self.descriptor.clone(),
            options: options.clone(),
        })
    }
}

pub fn builtin_onnx_provider_plugins() -> Vec<Arc<dyn ExecutionProviderPlugin>> {
    [
        descriptor("cpu", &["windows", "linux", "macos"], &[], &[], 0),
        descriptor(
            "directml",
            &["windows"],
            &["directml"],
            &["onnxruntime_providers_directml"],
            70,
        ),
        descriptor(
            "cuda",
            &["windows", "linux"],
            &["cuda"],
            &["onnxruntime_providers_cuda"],
            80,
        ),
        descriptor(
            "tensorrt",
            &["windows", "linux"],
            &["tensorrt", "cuda"],
            &[
                "onnxruntime_providers_tensorrt",
                "onnxruntime_providers_cuda",
            ],
            90,
        ),
        descriptor(
            "coreml",
            &["macos"],
            &["coreml"],
            &["onnxruntime_providers_coreml"],
            70,
        ),
    ]
    .into_iter()
    .map(|descriptor| {
        Arc::new(StaticProviderPlugin { descriptor }) as Arc<dyn ExecutionProviderPlugin>
    })
    .collect()
}

fn descriptor(
    id: &str,
    platforms: &[&str],
    capabilities: &[&str],
    required_libraries: &[&str],
    priority: i32,
) -> ProviderDescriptor {
    ProviderDescriptor {
        id: id.to_owned(),
        runtime_family: "onnx-runtime".to_owned(),
        platforms: platforms.iter().map(|value| (*value).to_owned()).collect(),
        architectures: BTreeSet::from(["x86_64".to_owned(), "aarch64".to_owned()]),
        capabilities: capabilities
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        required_libraries: required_libraries
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        priority,
        experimental: false,
    }
}

fn validate_descriptor(descriptor: &ProviderDescriptor) -> Result<(), ProviderPluginError> {
    if descriptor.id.trim().is_empty() || descriptor.runtime_family.trim().is_empty() {
        Err(ProviderPluginError::InvalidDescriptor)
    } else {
        Ok(())
    }
}

fn canonical_provider_id(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "")
        .trim_end_matches("executionprovider")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows_environment() -> RuntimeEnvironment {
        RuntimeEnvironment {
            platform: "windows".to_owned(),
            architecture: "x86_64".to_owned(),
            capabilities: BTreeSet::new(),
            trusted_runtime_libraries: BTreeSet::new(),
        }
    }

    #[test]
    fn requested_directml_falls_back_to_cpu_with_machine_readable_trace() {
        let registry = ExecutionProviderRegistry::with_builtin_onnx();
        let (resolved, trace) = registry
            .resolve(
                "formula-rec",
                &[("directml".to_owned(), ProviderOptions::default())],
                &windows_environment(),
                true,
            )
            .unwrap();
        assert_eq!(resolved.descriptor.id, "cpu");
        assert_eq!(trace.requested_provider.as_deref(), Some("directml"));
        assert_eq!(trace.selected_provider.as_deref(), Some("cpu"));
        assert!(trace.fallback);
        assert_eq!(
            trace.reasons[0].code,
            ProviderResolutionCode::ProviderLibraryMissing
        );
        assert_eq!(trace.reasons[1].code, ProviderResolutionCode::Accepted);
    }

    #[test]
    fn directml_resolves_when_runtime_reports_capability() {
        let registry = ExecutionProviderRegistry::with_builtin_onnx();
        let mut environment = windows_environment();
        environment.capabilities.insert("directml".to_owned());
        environment
            .trusted_runtime_libraries
            .insert("onnxruntime_providers_directml".to_owned());
        let (resolved, trace) = registry
            .resolve(
                "formula-rec",
                &[(
                    "DirectMLExecutionProvider".to_owned(),
                    ProviderOptions::default(),
                )],
                &environment,
                true,
            )
            .unwrap();
        assert_eq!(resolved.descriptor.id, "directml");
        assert!(!trace.fallback);
    }

    #[test]
    fn cpu_fallback_must_be_explicitly_allowed() {
        let registry = ExecutionProviderRegistry::with_builtin_onnx();
        let error = registry
            .resolve(
                "formula-rec",
                &[("cuda".to_owned(), ProviderOptions::default())],
                &windows_environment(),
                false,
            )
            .unwrap_err();
        let ProviderPluginError::NoProviderAvailable { trace, .. } = error else {
            panic!("expected no-provider error");
        };
        assert_eq!(trace.selected_provider, None);
        assert_eq!(trace.reasons.len(), 1);
    }
}
