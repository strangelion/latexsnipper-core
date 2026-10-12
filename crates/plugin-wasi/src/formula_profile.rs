//! Closed, bounded configuration for an already installed WASI formula route.
use latexsnipper_conversion::{
    conversion_registry::{
        ConversionFormatDescriptor, ConversionRegistryError, ConversionRegistryErrorCode,
        SemanticConversionRegistry, CONVERSION_REGISTRY_VERSION,
    },
    CapabilityTarget, FormulaInputFormat, FormulaOutputTarget,
};
use latexsnipper_plugin::{CancellationToken, RemotePluginStore};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ActivatedRemoteWasiPlugin, WasiFormulaBackend};

const MAX_PROFILE_BYTES: usize = 16 * 1024;
const MAX_PATTERN_BYTES: usize = 2048;
const REGEX_SIZE_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WasiFormulaProfile {
    pub schema_version: u16,
    pub plugin_id: String,
    pub backend_id: String,
    pub transport: String,
    pub operation: FormulaProfileOperation,
    pub format: FormulaProfileFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum FormulaProfileOperation {
    Importer { output: String },
    Exporter { input: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaProfileFormat {
    pub id: String,
    pub mime_type: String,
    pub description: String,
    /// Full-string regular language, not a script, macro or guest callback.
    pub validation: FormulaProfileValidation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum FormulaProfileValidation {
    Regex { pattern: String },
}

/// A fresh registry: registration is atomic relative to any caller-owned registry.
/// The profile backend remains disabled until the caller explicitly enables it.
pub struct PreparedFormulaRegistry {
    pub registry: SemanticConversionRegistry,
    pub backend_id: String,
    pub profile_sha256: String,
    pub approval_controller: std::sync::Arc<crate::SensitiveActionController>,
    /// Optional stable scope metadata, not evidence of consent.
    pub remembered_binding_sha256: Option<String>,
}

impl WasiFormulaProfile {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ConversionRegistryError> {
        if bytes.len() > MAX_PROFILE_BYTES {
            return Err(failure(
                ConversionRegistryErrorCode::LimitExceeded,
                "formula profile exceeds 16 KiB",
            ));
        }
        let profile: Self = serde_json::from_slice(bytes)
            .map_err(|_| invalid("formula profile is not a closed v1 JSON declaration"))?;
        profile.validated_registry()?;
        Ok(profile)
    }

    pub fn sha256(&self) -> Result<String, ConversionRegistryError> {
        self.validated_registry()?;
        Ok(format!("{:x}", Sha256::digest(self.canonical_bytes()?)))
    }

    /// Never installs, downloads, enables or revokes a package. Only an existing,
    /// verified and explicitly enabled store entry can be activated.
    pub fn prepare(
        &self,
        store: &RemotePluginStore,
    ) -> Result<PreparedFormulaRegistry, ConversionRegistryError> {
        self.prepare_with_cancellation(store, CancellationToken::default())
    }

    pub fn prepare_with_cancellation(
        &self,
        store: &RemotePluginStore,
        cancellation: CancellationToken,
    ) -> Result<PreparedFormulaRegistry, ConversionRegistryError> {
        self.prepare_bound(store, cancellation, None)
    }

    /// Bind a trusted host domain plus verified archive and complete profile identity.
    /// Does not enable a backend, approve an action or restore any execution grant.
    pub fn prepare_with_remembered_approval_binding(
        &self,
        store: &RemotePluginStore,
        cancellation: CancellationToken,
        trusted_domain: &str,
    ) -> Result<PreparedFormulaRegistry, ConversionRegistryError> {
        if trusted_domain.is_empty() || trusted_domain.len() > 4096 {
            return Err(invalid("remembered approval host identity exceeds bounds"));
        }
        self.prepare_bound(store, cancellation, Some(trusted_domain))
    }

    fn prepare_bound(
        &self,
        store: &RemotePluginStore,
        cancellation: CancellationToken,
        trusted_domain: Option<&str>,
    ) -> Result<PreparedFormulaRegistry, ConversionRegistryError> {
        // Validate all profile fields and compile the bounded validator before
        // touching the package store or compiling guest code.
        let mut registry = self.validated_registry()?;
        if cancellation.is_cancelled() {
            return Err(failure(
                ConversionRegistryErrorCode::BackendFailed,
                "PLUGIN_WASI_CANCELLED: cancellation requested before activation",
            ));
        }
        let profile_sha256 = format!("{:x}", Sha256::digest(self.canonical_bytes()?));
        let mut plugin =
            ActivatedRemoteWasiPlugin::activate(store, &self.plugin_id).map_err(|error| {
                failure(
                    ConversionRegistryErrorCode::BackendFailed,
                    format!("{}: {}", error.code, error.message),
                )
            })?;
        if let Some(domain) = trusted_domain {
            plugin = plugin
                .bind_remembered_profile(domain, Some(&profile_sha256))
                .map_err(|error| {
                    failure(
                        ConversionRegistryErrorCode::BackendFailed,
                        format!("{}: {}", error.code, error.message),
                    )
                })?;
        }
        let remembered_binding_sha256 = plugin.remembered_approval_binding().map(str::to_owned);
        let approval_controller = plugin.approval_controller();
        let backend = match &self.operation {
            FormulaProfileOperation::Importer { output } => WasiFormulaBackend::importer_to_target(
                plugin,
                &self.backend_id,
                &self.format.id,
                &self.transport,
                builtin_output(output)?,
            )?,
            FormulaProfileOperation::Exporter { input } => WasiFormulaBackend::exporter(
                plugin,
                &self.backend_id,
                builtin_input(input)?,
                &self.format.id,
                &self.transport,
            )?,
        };
        backend
            .with_cancellation(cancellation)
            .bind_profile_identity(&profile_sha256)
            .register(&mut registry)?;
        Ok(PreparedFormulaRegistry {
            registry,
            backend_id: self.backend_id.clone(),
            profile_sha256,
            approval_controller,
            remembered_binding_sha256,
        })
    }

    fn canonical_bytes(&self) -> Result<Vec<u8>, ConversionRegistryError> {
        let bytes =
            serde_json::to_vec(self).map_err(|_| invalid("profile serialization failed"))?;
        if bytes.len() > MAX_PROFILE_BYTES {
            return Err(failure(
                ConversionRegistryErrorCode::LimitExceeded,
                "formula profile exceeds 16 KiB",
            ));
        }
        Ok(bytes)
    }

    fn validated_registry(&self) -> Result<SemanticConversionRegistry, ConversionRegistryError> {
        self.canonical_bytes()?;
        if self.schema_version != CONVERSION_REGISTRY_VERSION {
            return Err(failure(
                ConversionRegistryErrorCode::VersionMismatch,
                "unsupported WASI formula profile version",
            ));
        }
        if !namespaced_id(&self.backend_id)
            || self.plugin_id.len() > 128
            || !identifier_part(&self.plugin_id)
            || self.transport.is_empty()
            || self.transport.len() > 128
            || self.transport.chars().any(char::is_control)
            || !self.transport.contains('/')
        {
            return Err(invalid("invalid backend, installed plugin ID or transport"));
        }
        match &self.operation {
            FormulaProfileOperation::Importer { output } => {
                builtin_output(output)?;
            }
            FormulaProfileOperation::Exporter { input } => {
                builtin_input(input)?;
            }
        }
        let validator = compile_validation(&self.format.validation)?;
        let mut registry = SemanticConversionRegistry::new(CapabilityTarget::Native);
        registry.register_trusted_format(
            ConversionFormatDescriptor {
                schema_version: self.schema_version,
                id: self.format.id.clone(),
                mime_type: self.format.mime_type.clone(),
                description: self.format.description.clone(),
            },
            move |source| {
                if source.len() <= 64 * 1024 && validator.is_match(source) {
                    Ok(())
                } else {
                    Err(failure(
                        ConversionRegistryErrorCode::InputRejected,
                        "content does not match the declared complete formula subset",
                    ))
                }
            },
        )?;
        Ok(registry)
    }
}

fn compile_validation(
    validation: &FormulaProfileValidation,
) -> Result<Regex, ConversionRegistryError> {
    let FormulaProfileValidation::Regex { pattern } = validation;
    if pattern.is_empty() || pattern.len() > MAX_PATTERN_BYTES {
        return Err(invalid("profile regex must contain 1..2048 bytes"));
    }
    RegexBuilder::new(&format!(r"\A(?:{pattern})\z"))
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_SIZE_LIMIT)
        .nest_limit(32)
        .build()
        .map_err(|_| invalid("invalid or over-budget profile regex"))
}

fn builtin_output(id: &str) -> Result<FormulaOutputTarget, ConversionRegistryError> {
    FormulaOutputTarget::all()
        .into_iter()
        .find(|format| format.name() == id)
        .ok_or_else(|| invalid("importer output must be a canonical built-in formula target"))
}
fn builtin_input(id: &str) -> Result<FormulaInputFormat, ConversionRegistryError> {
    [
        FormulaInputFormat::Latex,
        FormulaInputFormat::Mathml,
        FormulaInputFormat::Omml,
        FormulaInputFormat::Typst,
        FormulaInputFormat::AsciiMath,
        FormulaInputFormat::UnicodeMath,
    ]
    .into_iter()
    .find(|format| format.name() == id)
    .ok_or_else(|| invalid("exporter input must have an implemented AST projection"))
}
fn namespaced_id(id: &str) -> bool {
    id.split_once(':').is_some_and(|(namespace, name)| {
        id.len() <= 96 && namespace != "core" && identifier_part(namespace) && identifier_part(name)
    })
}
fn identifier_part(id: &str) -> bool {
    id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}
fn invalid(message: &str) -> ConversionRegistryError {
    failure(ConversionRegistryErrorCode::InvalidDeclaration, message)
}
fn failure(
    code: ConversionRegistryErrorCode,
    message: impl Into<String>,
) -> ConversionRegistryError {
    ConversionRegistryError::new(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition() -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 1, "pluginId": "fixture.component", "backendId": "fixture:import",
            "transport": "application/vnd.fixture.power", "operation": {"kind": "importer", "output": "omml"},
            "format": {"id": "fixture:power", "mimeType": "application/vnd.fixture.power",
                "description": "Finite ASCII variable and exponent", "validation": {"kind": "regex", "pattern": "[a-z]:[0-9]"}}
        })
    }
    #[test]
    fn closed_profile_rejects_unknown_fields_versions_and_loader_paths() {
        for field in ["url", "path", "enabled", "permissions"] {
            let mut value = definition();
            value[field] = serde_json::json!("unexpected");
            assert!(WasiFormulaProfile::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        for (field, value) in [
            ("schemaVersion", serde_json::json!(2)),
            ("pluginId", serde_json::json!("../plugin")),
            ("backendId", serde_json::json!("core:builtin")),
            ("transport", serde_json::json!("bad\ntransport")),
        ] {
            let mut profile = definition();
            profile[field] = value;
            assert!(WasiFormulaProfile::from_json(&serde_json::to_vec(&profile).unwrap()).is_err());
        }
        for path in [0, 1, 2] {
            let mut value = definition();
            match path {
                0 => value["operation"]["extra"] = serde_json::json!(true),
                1 => value["format"]["extra"] = serde_json::json!(true),
                _ => value["format"]["validation"]["extra"] = serde_json::json!(true),
            }
            assert!(WasiFormulaProfile::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        assert!(WasiFormulaProfile::from_json(&vec![b' '; MAX_PROFILE_BYTES + 1]).is_err());
    }
    #[test]
    fn regular_subset_is_whole_string_and_has_compilation_budgets() {
        let regex = compile_validation(&FormulaProfileValidation::Regex {
            pattern: "[a-z]:[0-9]".into(),
        })
        .unwrap();
        assert!(regex.is_match("x:2"));
        for value in ["prefix x:2", "x:2+y", "x:2\n", "é:2", "x:22", ""] {
            assert!(!regex.is_match(value));
        }
        for pattern in [
            "".into(),
            "[".into(),
            "a".repeat(MAX_PATTERN_BYTES + 1),
            "a{1000000}".into(),
            "(".repeat(64),
        ] {
            assert!(compile_validation(&FormulaProfileValidation::Regex { pattern }).is_err());
        }
    }
    #[test]
    fn identity_is_canonical_but_changes_with_validation_or_direction() {
        let value = definition();
        let first = WasiFormulaProfile::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
        let spaced =
            WasiFormulaProfile::from_json(&serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        assert_eq!(first.sha256().unwrap(), spaced.sha256().unwrap());
        let mut direction = value.clone();
        direction["operation"] = serde_json::json!({"kind":"exporter", "input":"latex"});
        assert_ne!(
            first.sha256().unwrap(),
            WasiFormulaProfile::from_json(&serde_json::to_vec(&direction).unwrap())
                .unwrap()
                .sha256()
                .unwrap()
        );
        let mut altered = value.clone();
        altered["format"]["validation"]["pattern"] = serde_json::json!("[a-z]:[0-8]");
        assert_ne!(
            first.sha256().unwrap(),
            WasiFormulaProfile::from_json(&serde_json::to_vec(&altered).unwrap())
                .unwrap()
                .sha256()
                .unwrap()
        );
        for input in ["markdown", "mtef", "LaTeX", "svg"] {
            let mut altered = value.clone();
            altered["operation"] = serde_json::json!({"kind":"exporter", "input":input});
            assert!(WasiFormulaProfile::from_json(&serde_json::to_vec(&altered).unwrap()).is_err());
        }
    }

    #[test]
    fn linear_profiles_require_canonical_ids_and_bind_each_direction() {
        let mut identities = std::collections::BTreeSet::new();
        for format in ["ascii-math", "unicode-math"] {
            for operation in [
                serde_json::json!({"kind":"importer", "output":format}),
                serde_json::json!({"kind":"exporter", "input":format}),
            ] {
                let mut value = definition();
                value["operation"] = operation;
                let profile =
                    WasiFormulaProfile::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
                assert!(identities.insert(profile.sha256().unwrap()));
            }
        }
        for format in [
            "ascii_math",
            "Unicode-Math",
            "mtef",
            "ole",
            "svg",
            "custom:unknown",
        ] {
            for operation in [
                serde_json::json!({"kind":"importer", "output":format}),
                serde_json::json!({"kind":"exporter", "input":format}),
            ] {
                let mut value = definition();
                value["operation"] = operation;
                assert!(
                    WasiFormulaProfile::from_json(&serde_json::to_vec(&value).unwrap()).is_err()
                );
            }
        }
    }
}
