//! Deterministic model runtime-variant resolution.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Component, Path};

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_model::{RuntimeVariant, VariantStatus};

use crate::{RuntimeArtifacts, RuntimeKind, RuntimeOptions, RuntimeProbe, RuntimeRegistry};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ArtifactValidation {
    #[default]
    RequireExisting,
    /// Used only by the positional legacy adapter, whose historic model handle
    /// may be resolved later by the wrapped backend.
    AllowMissing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRuntimeVariant {
    pub model_id: String,
    pub variant_id: String,
    pub runtime: RuntimeKind,
    pub status: VariantStatus,
    pub artifacts: RuntimeArtifacts,
    pub options: RuntimeOptions,
    pub fallback_from: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeResolutionAttempt {
    pub variant_id: String,
    pub runtime: RuntimeKind,
    pub reason: String,
}

pub struct RuntimeResolver<'registry> {
    registry: &'registry RuntimeRegistry,
    artifact_validation: ArtifactValidation,
    allow_experimental: bool,
}

impl<'registry> RuntimeResolver<'registry> {
    pub fn new(registry: &'registry RuntimeRegistry) -> Self {
        Self {
            registry,
            artifact_validation: ArtifactValidation::RequireExisting,
            allow_experimental: true,
        }
    }

    pub fn with_artifact_validation(mut self, validation: ArtifactValidation) -> Self {
        self.artifact_validation = validation;
        self
    }

    pub fn allow_experimental(mut self, allow: bool) -> Self {
        self.allow_experimental = allow;
        self
    }

    /// Resolve one root variant and only its explicitly declared fallback
    /// graph. Unrelated lower-priority variants are never silently selected
    /// after a runtime load/probe failure.
    pub fn resolve(
        &self,
        model_id: &str,
        variants: &[RuntimeVariant],
        model_dir: &Path,
        preferred_variant: Option<&str>,
    ) -> Result<ResolvedRuntimeVariant> {
        if variants.is_empty() {
            return Err(SnipperError::Model(format!(
                "model '{model_id}' declares no runtime variants"
            )));
        }

        let by_id = unique_variants(model_id, variants)?;
        validate_fallback_references(model_id, &by_id)?;

        let root = if let Some(preferred) = preferred_variant {
            by_id.get(preferred).copied().ok_or_else(|| {
                SnipperError::Model(format!(
                    "model '{model_id}' has no runtime variant '{preferred}'"
                ))
            })?
        } else {
            let mut roots: Vec<&RuntimeVariant> = variants
                .iter()
                .filter(|variant| self.status_allowed(variant.status))
                .filter(|variant| platform_matches(&variant.platforms))
                .collect();
            roots.sort_by(|left, right| {
                right
                    .priority
                    .cmp(&left.priority)
                    .then_with(|| left.id.cmp(&right.id))
            });
            roots.first().copied().ok_or_else(|| {
                SnipperError::Model(format!(
                    "model '{model_id}' has no runtime variant allowed on {}",
                    current_platform()
                ))
            })?
        };

        let mut attempts = Vec::new();
        let mut visiting = HashSet::new();
        if let Some(resolved) = self.resolve_recursive(
            model_id,
            root,
            &by_id,
            model_dir,
            None,
            &mut visiting,
            &mut attempts,
        )? {
            return Ok(resolved);
        }

        let details = attempts
            .iter()
            .map(|attempt| {
                format!(
                    "{} ({}) — {}",
                    attempt.variant_id, attempt.runtime, attempt.reason
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        Err(SnipperError::Runtime(format!(
            "no runnable runtime variant for model '{model_id}': {details}"
        )))
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_recursive(
        &self,
        model_id: &str,
        variant: &RuntimeVariant,
        by_id: &HashMap<&str, &RuntimeVariant>,
        model_dir: &Path,
        fallback_from: Option<&str>,
        visiting: &mut HashSet<String>,
        attempts: &mut Vec<RuntimeResolutionAttempt>,
    ) -> Result<Option<ResolvedRuntimeVariant>> {
        if !visiting.insert(variant.id.clone()) {
            return Err(SnipperError::Model(format!(
                "model '{model_id}' runtime fallback cycle includes '{}'",
                variant.id
            )));
        }

        let runtime = RuntimeKind::from_id(&variant.runtime);
        match self.try_variant(model_id, variant, runtime.clone(), model_dir, fallback_from) {
            Ok(resolved) => {
                visiting.remove(&variant.id);
                return Ok(Some(resolved));
            }
            Err(reason) => attempts.push(RuntimeResolutionAttempt {
                variant_id: variant.id.clone(),
                runtime,
                reason,
            }),
        }

        for fallback_id in &variant.fallbacks {
            let fallback = by_id[fallback_id.as_str()];
            if let Some(resolved) = self.resolve_recursive(
                model_id,
                fallback,
                by_id,
                model_dir,
                Some(&variant.id),
                visiting,
                attempts,
            )? {
                visiting.remove(&variant.id);
                return Ok(Some(resolved));
            }
        }

        visiting.remove(&variant.id);
        Ok(None)
    }

    fn try_variant(
        &self,
        model_id: &str,
        variant: &RuntimeVariant,
        runtime: RuntimeKind,
        model_dir: &Path,
        fallback_from: Option<&str>,
    ) -> std::result::Result<ResolvedRuntimeVariant, String> {
        if !self.status_allowed(variant.status) {
            return Err(format!("status {:?} is not selectable", variant.status));
        }
        if !platform_matches(&variant.platforms) {
            return Err(format!("not supported on {}", current_platform()));
        }

        let probe = self
            .registry
            .probe(&runtime)
            .ok_or_else(|| "runtime is not registered".to_owned())?;
        if !probe.available {
            return Err(probe
                .reason_unavailable
                .unwrap_or_else(|| "runtime probe reported unavailable".to_owned()));
        }
        ensure_capabilities(&probe, &variant.capabilities)?;

        if variant.artifacts.is_empty() {
            return Err("variant declares no artifacts".to_owned());
        }
        let mut files = BTreeMap::new();
        for (role, declared) in &variant.artifacts {
            let relative = Path::new(declared);
            if relative.is_absolute()
                || relative.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(format!(
                    "artifact '{role}' must be a package-relative path: {declared}"
                ));
            }
            if forbidden_executable_artifact(relative) {
                return Err(format!(
                    "artifact '{role}' is executable content and cannot be loaded from a model package: {declared}"
                ));
            }
            let resolved = model_dir.join(relative);
            if self.artifact_validation == ArtifactValidation::RequireExisting && !resolved.exists()
            {
                return Err(format!(
                    "artifact '{role}' does not exist: {}",
                    resolved.display()
                ));
            }
            files.insert(role.clone(), resolved);
        }

        let options = parse_options(variant)?;
        Ok(ResolvedRuntimeVariant {
            model_id: model_id.to_owned(),
            variant_id: variant.id.clone(),
            runtime: runtime.clone(),
            status: variant.status,
            artifacts: RuntimeArtifacts {
                runtime,
                files,
                buffers: BTreeMap::new(),
                options: BTreeMap::new(),
            },
            options,
            fallback_from: fallback_from.map(ToOwned::to_owned),
        })
    }

    fn status_allowed(&self, status: VariantStatus) -> bool {
        status == VariantStatus::Stable
            || (self.allow_experimental && status == VariantStatus::Experimental)
    }
}

fn forbidden_executable_artifact(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "dll"
                    | "so"
                    | "dylib"
                    | "exe"
                    | "com"
                    | "bat"
                    | "cmd"
                    | "ps1"
                    | "sh"
                    | "py"
                    | "pyc"
                    | "js"
                    | "mjs"
                    | "cjs"
            )
        })
}

fn unique_variants<'a>(
    model_id: &str,
    variants: &'a [RuntimeVariant],
) -> Result<HashMap<&'a str, &'a RuntimeVariant>> {
    let mut result = HashMap::with_capacity(variants.len());
    for variant in variants {
        if variant.id.trim().is_empty() {
            return Err(SnipperError::Model(format!(
                "model '{model_id}' contains an empty runtime variant id"
            )));
        }
        if result.insert(variant.id.as_str(), variant).is_some() {
            return Err(SnipperError::Model(format!(
                "model '{model_id}' contains duplicate runtime variant '{}'",
                variant.id
            )));
        }
    }
    Ok(result)
}

fn validate_fallback_references(
    model_id: &str,
    variants: &HashMap<&str, &RuntimeVariant>,
) -> Result<()> {
    for variant in variants.values() {
        let mut seen = HashSet::new();
        for fallback in &variant.fallbacks {
            if !seen.insert(fallback) {
                return Err(SnipperError::Model(format!(
                    "model '{model_id}' variant '{}' repeats fallback '{fallback}'",
                    variant.id
                )));
            }
            if !variants.contains_key(fallback.as_str()) {
                return Err(SnipperError::Model(format!(
                    "model '{model_id}' variant '{}' references missing fallback '{fallback}'",
                    variant.id
                )));
            }
        }
    }
    Ok(())
}

fn parse_options(variant: &RuntimeVariant) -> std::result::Result<RuntimeOptions, String> {
    let Some(options) = &variant.options else {
        return Ok(RuntimeOptions::default());
    };
    let object = options
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    serde_json::from_value(serde_json::Value::Object(object)).map_err(|error| {
        format!(
            "variant '{}' has invalid runtime options: {error}",
            variant.id
        )
    })
}

fn ensure_capabilities(
    probe: &RuntimeProbe,
    required: &[String],
) -> std::result::Result<(), String> {
    let available: BTreeSet<String> = probe
        .capabilities
        .features
        .iter()
        .cloned()
        .chain(
            probe
                .capabilities
                .execution_providers
                .iter()
                .map(|provider| format!("provider:{provider}")),
        )
        .chain(
            probe
                .capabilities
                .tensor_dtypes
                .iter()
                .map(|dtype| format!("dtype:{dtype}")),
        )
        .chain(
            probe
                .capabilities
                .methods
                .iter()
                .map(|method| format!("method:{method}")),
        )
        .collect();
    let missing: Vec<&str> = required
        .iter()
        .map(String::as_str)
        .filter(|capability| !available.contains(*capability))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "missing runtime capabilities: {}",
            missing.join(", ")
        ))
    }
}

pub fn platform_matches(constraints: &[String]) -> bool {
    if constraints.is_empty() {
        return true;
    }
    let tags = current_platform_tags();
    constraints.iter().any(|constraint| {
        constraint.eq_ignore_ascii_case("any") || tags.contains(&constraint.to_ascii_lowercase())
    })
}

pub fn current_platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn current_platform_tags() -> BTreeSet<String> {
    let os = std::env::consts::OS.to_ascii_lowercase();
    let arch = std::env::consts::ARCH.to_ascii_lowercase();
    let mut tags = BTreeSet::from([
        os.clone(),
        arch.clone(),
        format!("{os}-{arch}"),
        format!("{os}/{arch}"),
    ]);
    if cfg!(target_family = "unix") {
        tags.insert("unix".to_owned());
    }
    if cfg!(target_family = "windows") {
        tags.insert("windows".to_owned());
    }
    if cfg!(target_vendor = "apple") {
        tags.insert("apple".to_owned());
    }
    tags
}

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn model_artifacts_cannot_be_native_libraries_or_scripts() {
        for artifact in [
            "provider.dll",
            "libprovider.so",
            "provider.dylib",
            "install.ps1",
            "install.sh",
            "setup.py",
            "bootstrap.js",
        ] {
            assert!(
                forbidden_executable_artifact(Path::new(artifact)),
                "{artifact} must be rejected"
            );
        }
        for artifact in [
            "model.onnx",
            "model.ort",
            "tokenizer.json",
            "program.pdmodel",
        ] {
            assert!(
                !forbidden_executable_artifact(Path::new(artifact)),
                "{artifact} must remain a data artifact"
            );
        }
    }
}
