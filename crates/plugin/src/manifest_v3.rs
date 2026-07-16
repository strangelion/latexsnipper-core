use latexsnipper_ast::FormatCapability;
use latexsnipper_foundation::{
    MigrationOutcome, MigrationReport, MigrationStatus, MigrationWarning,
};
use serde::{Deserialize, Serialize};

use crate::{PluginDependency, PluginHook, PluginManifest, PluginPermissions, PLUGIN_API_VERSION};

pub const PLUGIN_MANIFEST_SCHEMA_VERSION_V3: u32 = 3;
pub const PLUGIN_API_VERSION_FOR_MANIFEST_V3: u32 = 2;
pub const PROCESS_PLUGIN_PROTOCOL_VERSION_V1: u32 = 1;
pub const COMPONENT_WIT_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginExecutionClassV3 {
    TrustedInProcess,
    IsolatedNativeProcess,
    WasiComponent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginPathAccessV3 {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPathGrantV3 {
    pub path: String,
    pub access: PluginPathAccessV3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkSchemeV3 {
    Https,
    Http,
    Tcp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDestinationV3 {
    pub scheme: NetworkSchemeV3,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRegistrationGrantsV3 {
    pub capabilities: bool,
    pub importers: bool,
    pub exporters: bool,
    pub runtimes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginResourceLimitsV3 {
    pub timeout_millis: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub input_bytes: Option<u64>,
    pub output_bytes: Option<u64>,
    pub diagnostic_count: Option<u32>,
    pub diagnostic_bytes: Option<u64>,
    pub model_artifact_bytes: Option<u64>,
    pub temporary_storage_bytes: Option<u64>,
    pub table_elements: Option<u32>,
    pub resources: Option<u32>,
    pub fuel: Option<u64>,
    pub max_concurrent_executions: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissionsV3 {
    #[serde(default)]
    pub paths: Vec<PluginPathGrantV3>,
    #[serde(default)]
    pub network: Vec<NetworkDestinationV3>,
    #[serde(default)]
    pub environment_variables: Vec<String>,
    #[serde(default)]
    pub model_artifacts: Vec<String>,
    pub temporary_directory: bool,
    pub clocks: bool,
    pub randomness: bool,
    pub registrations: PluginRegistrationGrantsV3,
    pub limits: PluginResourceLimitsV3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginArtifactKindV3 {
    NativeExecutable,
    WasiComponent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginArtifactV3 {
    pub path: String,
    pub kind: PluginArtifactKindV3,
    pub sha256: String,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSignatureV3 {
    pub algorithm: String,
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginProvenanceV3 {
    pub source: String,
    pub revision: String,
    pub statement_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInterfaceVersionsV3 {
    pub plugin_api: u32,
    pub process_ipc: Option<u32>,
    pub component_wit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifestV3 {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub core_version_requirement: String,
    pub execution_class: PluginExecutionClassV3,
    pub interfaces: PluginInterfaceVersionsV3,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub format_capabilities: Vec<FormatCapability>,
    #[serde(default)]
    pub hooks: Vec<PluginHook>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    #[serde(default)]
    pub before: Vec<String>,
    #[serde(default)]
    pub after: Vec<String>,
    pub permissions: PluginPermissionsV3,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub architectures: Vec<String>,
    pub license: Option<String>,
    pub artifact: Option<PluginArtifactV3>,
    pub signature: Option<PluginSignatureV3>,
    pub provenance: Option<PluginProvenanceV3>,
    pub configuration_schema: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PluginManifestV3Error {
    #[error("invalid plugin manifest JSON: {0}")]
    InvalidJson(String),
    #[error("unsupported plugin manifest schema version {0}")]
    UnsupportedSchemaVersion(u64),
    #[error("unsupported v2 plugin API version {0}")]
    UnsupportedSourceApi(u32),
    #[error("native dynamic-library plugins cannot migrate to the v3 trust model")]
    NativeAbiRemoved,
    #[error("a reserved v2 WASI manifest lacks an explicit Component WIT version")]
    WasiRequiresExplicitContract,
    #[error("isolated native plugin is missing an entrypoint")]
    MissingEntrypoint,
    #[error("external plugin is missing its declared artifact")]
    MissingArtifact,
    #[error("isolated native plugin is missing a SHA-256 digest")]
    MissingDigest,
    #[error("invalid plugin semantic version: {0}")]
    InvalidVersion(String),
    #[error("invalid core version requirement: {0}")]
    InvalidCoreRequirement(String),
    #[error("invalid SHA-256 digest")]
    InvalidDigest,
    #[error("external v3 plugins require license metadata")]
    MissingLicense,
    #[error("execution class and interface versions are inconsistent")]
    InterfaceMismatch,
}

/// Version-aware plugin manifest loader. Runtime callers must branch on the
/// explicit contract and may not deserialize a v3 document as the legacy type.
#[derive(Debug, Clone)]
pub enum LoadedPluginManifest {
    V2(Box<PluginManifest>),
    V3(Box<PluginManifestV3>),
}

impl LoadedPluginManifest {
    pub fn parse_json(bytes: &[u8]) -> Result<Self, PluginManifestV3Error> {
        let raw: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|error| PluginManifestV3Error::InvalidJson(error.to_string()))?;
        match raw
            .get("schemaVersion")
            .or_else(|| raw.get("schema_version"))
        {
            Some(version) => {
                let version = version.as_u64().ok_or_else(|| {
                    PluginManifestV3Error::InvalidJson(
                        "schema version must be an unsigned integer".to_string(),
                    )
                })?;
                if version == PLUGIN_MANIFEST_SCHEMA_VERSION_V3 as u64 {
                    let manifest: PluginManifestV3 = serde_json::from_value(raw)
                        .map_err(|error| PluginManifestV3Error::InvalidJson(error.to_string()))?;
                    manifest.validate_contract()?;
                    Ok(Self::V3(Box::new(manifest)))
                } else if version <= 2 {
                    serde_json::from_value(raw)
                        .map(Box::new)
                        .map(Self::V2)
                        .map_err(|error| PluginManifestV3Error::InvalidJson(error.to_string()))
                } else {
                    Err(PluginManifestV3Error::UnsupportedSchemaVersion(version))
                }
            }
            None => serde_json::from_value(raw)
                .map(Box::new)
                .map(Self::V2)
                .map_err(|error| PluginManifestV3Error::InvalidJson(error.to_string())),
        }
    }

    pub const fn schema_version(&self) -> u32 {
        match self {
            Self::V2(_) => 2,
            Self::V3(_) => 3,
        }
    }
}

impl PluginManifestV3 {
    pub fn migrate_from_v2(
        source: PluginManifest,
    ) -> Result<MigrationOutcome<Self>, PluginManifestV3Error> {
        if source.plugin_api_version != PLUGIN_API_VERSION {
            return Err(PluginManifestV3Error::UnsupportedSourceApi(
                source.plugin_api_version,
            ));
        }
        semver::Version::parse(&source.version)
            .map_err(|error| PluginManifestV3Error::InvalidVersion(error.to_string()))?;
        semver::VersionReq::parse(&source.core_version_requirement)
            .map_err(|error| PluginManifestV3Error::InvalidCoreRequirement(error.to_string()))?;

        let (execution_class, process_ipc, artifact) = match source.class {
            crate::PluginClass::BuiltInRust => {
                (PluginExecutionClassV3::TrustedInProcess, None, None)
            }
            crate::PluginClass::IsolatedProcess => {
                let path = source
                    .entrypoint
                    .clone()
                    .ok_or(PluginManifestV3Error::MissingEntrypoint)?;
                let sha256 = source
                    .checksum_sha256
                    .clone()
                    .ok_or(PluginManifestV3Error::MissingDigest)?;
                if !valid_sha256(&sha256) {
                    return Err(PluginManifestV3Error::InvalidDigest);
                }
                (
                    PluginExecutionClassV3::IsolatedNativeProcess,
                    Some(
                        source
                            .abi_version
                            .ok_or(PluginManifestV3Error::InterfaceMismatch)?,
                    ),
                    Some(PluginArtifactV3 {
                        path,
                        kind: PluginArtifactKindV3::NativeExecutable,
                        sha256,
                        size_bytes: None,
                    }),
                )
            }
            crate::PluginClass::NativeAbi => return Err(PluginManifestV3Error::NativeAbiRemoved),
            crate::PluginClass::WasiComponent => {
                return Err(PluginManifestV3Error::WasiRequiresExplicitContract)
            }
        };

        let mut report = MigrationReport::new(
            "plugin-manifest",
            "1",
            "plugin-manifest",
            PLUGIN_MANIFEST_SCHEMA_VERSION_V3.to_string(),
            MigrationStatus::Migrated,
        );
        let permissions = migrate_permissions(&source.permissions, &mut report);
        let signature = if source.signature.is_some() {
            report.require_manual_action(
                MigrationWarning::new(
                    "PLUGIN_V3_SIGNATURE_METADATA_REQUIRED",
                    "The v2 signature lacks an algorithm and key identifier and was not migrated",
                )
                .with_field("signature"),
            );
            None
        } else {
            None
        };
        if execution_class != PluginExecutionClassV3::TrustedInProcess && source.license.is_none() {
            report.require_manual_action(
                MigrationWarning::new(
                    "PLUGIN_V3_LICENSE_REQUIRED",
                    "External plugins require explicit license metadata before verification",
                )
                .with_field("license"),
            );
        }

        Ok(MigrationOutcome {
            value: Self {
                schema_version: PLUGIN_MANIFEST_SCHEMA_VERSION_V3,
                id: source.id,
                name: source.name,
                version: source.version,
                core_version_requirement: source.core_version_requirement,
                execution_class,
                interfaces: PluginInterfaceVersionsV3 {
                    plugin_api: PLUGIN_API_VERSION_FOR_MANIFEST_V3,
                    process_ipc,
                    component_wit: None,
                },
                capabilities: source.capabilities,
                format_capabilities: source.format_capabilities,
                hooks: source.hooks,
                priority: source.priority,
                dependencies: source.dependencies,
                before: source.before,
                after: source.after,
                permissions,
                platforms: source.platforms,
                architectures: source.architectures,
                license: source.license,
                artifact,
                signature,
                provenance: None,
                configuration_schema: source.configuration_schema,
            },
            report,
        })
    }

    pub fn validate_contract(&self) -> Result<(), PluginManifestV3Error> {
        semver::Version::parse(&self.version)
            .map_err(|error| PluginManifestV3Error::InvalidVersion(error.to_string()))?;
        semver::VersionReq::parse(&self.core_version_requirement)
            .map_err(|error| PluginManifestV3Error::InvalidCoreRequirement(error.to_string()))?;
        if self.schema_version != PLUGIN_MANIFEST_SCHEMA_VERSION_V3
            || self.interfaces.plugin_api != PLUGIN_API_VERSION_FOR_MANIFEST_V3
            || self.id.trim().is_empty()
            || self.name.trim().is_empty()
            || self.permissions.limits.max_concurrent_executions == 0
            || self
                .permissions
                .paths
                .iter()
                .any(|grant| grant.path.trim().is_empty())
            || self
                .permissions
                .network
                .iter()
                .any(|destination| destination.host.trim().is_empty() || destination.port == 0)
            || self.signature.as_ref().is_some_and(|signature| {
                signature.algorithm.trim().is_empty()
                    || signature.key_id.trim().is_empty()
                    || signature.signature.trim().is_empty()
            })
            || self.provenance.as_ref().is_some_and(|provenance| {
                provenance.source.trim().is_empty() || provenance.revision.trim().is_empty()
            })
        {
            return Err(PluginManifestV3Error::InterfaceMismatch);
        }
        match self.execution_class {
            PluginExecutionClassV3::TrustedInProcess => {
                if self.interfaces.process_ipc.is_some()
                    || self.interfaces.component_wit.is_some()
                    || self.artifact.is_some()
                {
                    return Err(PluginManifestV3Error::InterfaceMismatch);
                }
            }
            PluginExecutionClassV3::IsolatedNativeProcess => {
                validate_external_artifact(self, PluginArtifactKindV3::NativeExecutable)?;
                if self.interfaces.process_ipc != Some(PROCESS_PLUGIN_PROTOCOL_VERSION_V1)
                    || self.interfaces.component_wit.is_some()
                {
                    return Err(PluginManifestV3Error::InterfaceMismatch);
                }
            }
            PluginExecutionClassV3::WasiComponent => {
                validate_external_artifact(self, PluginArtifactKindV3::WasiComponent)?;
                if self.interfaces.component_wit != Some(COMPONENT_WIT_VERSION_V1)
                    || self.interfaces.process_ipc.is_some()
                {
                    return Err(PluginManifestV3Error::InterfaceMismatch);
                }
            }
        }
        Ok(())
    }
}

fn validate_external_artifact(
    manifest: &PluginManifestV3,
    kind: PluginArtifactKindV3,
) -> Result<(), PluginManifestV3Error> {
    if manifest
        .license
        .as_deref()
        .is_none_or(|license| license.trim().is_empty())
    {
        return Err(PluginManifestV3Error::MissingLicense);
    }
    let artifact = manifest
        .artifact
        .as_ref()
        .ok_or(PluginManifestV3Error::MissingArtifact)?;
    if !valid_package_path(&artifact.path) {
        return Err(PluginManifestV3Error::MissingEntrypoint);
    }
    if artifact.kind != kind {
        return Err(PluginManifestV3Error::InterfaceMismatch);
    }
    if !valid_sha256(&artifact.sha256) {
        return Err(PluginManifestV3Error::InvalidDigest);
    }
    Ok(())
}

fn migrate_permissions(
    source: &PluginPermissions,
    report: &mut MigrationReport,
) -> PluginPermissionsV3 {
    let mut paths = Vec::new();
    for path in source
        .filesystem_paths
        .iter()
        .chain(source.filesystem_read_paths.iter())
    {
        paths.push(PluginPathGrantV3 {
            path: path.clone(),
            access: PluginPathAccessV3::Read,
        });
    }
    for path in &source.filesystem_write_paths {
        paths.push(PluginPathGrantV3 {
            path: path.clone(),
            access: PluginPathAccessV3::Write,
        });
    }
    paths.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.access.cmp(&right.access))
    });
    paths.dedup();

    if !source.network_hosts.is_empty() {
        report.require_manual_action(
            MigrationWarning::new(
                "PLUGIN_V3_NETWORK_DESTINATION_REQUIRED",
                "Host-only v2 network grants cannot infer scheme and port and were not migrated",
            )
            .with_field("permissions.networkHosts"),
        );
    }

    PluginPermissionsV3 {
        paths,
        network: Vec::new(),
        environment_variables: source.environment_variables.clone(),
        model_artifacts: source.model_access.clone(),
        temporary_directory: source.temporary_directory,
        clocks: false,
        randomness: false,
        registrations: PluginRegistrationGrantsV3 {
            capabilities: source.capability_registration,
            importers: source.importer_registration,
            exporters: source.exporter_registration,
            runtimes: source.runtime_registration,
        },
        limits: PluginResourceLimitsV3 {
            timeout_millis: source.timeout_millis,
            memory_bytes: source.memory_limit_bytes,
            input_bytes: None,
            output_bytes: source.output_limit_bytes,
            diagnostic_count: None,
            diagnostic_bytes: None,
            model_artifact_bytes: None,
            temporary_storage_bytes: None,
            table_elements: None,
            resources: None,
            fuel: None,
            max_concurrent_executions: source.max_concurrent_executions.max(1),
        },
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_package_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_classes_have_the_required_v3_wire_names() {
        assert_eq!(
            serde_json::to_string(&PluginExecutionClassV3::IsolatedNativeProcess).unwrap(),
            "\"isolated_native_process\""
        );
    }

    #[test]
    fn reviewed_native_manifest_migrates_without_claiming_an_os_sandbox() {
        let mut source = PluginManifest::built_in("fixture.native", "2.1.0");
        source.class = crate::PluginClass::IsolatedProcess;
        source.abi_version = Some(1);
        source.entrypoint = Some("plugin.exe".to_string());
        source.checksum_sha256 = Some("a".repeat(64));
        source.license = Some("Apache-2.0".to_string());
        source.permissions.network_hosts = vec!["example.invalid".to_string()];

        let migrated = PluginManifestV3::migrate_from_v2(source).unwrap();
        assert_eq!(
            migrated.value.execution_class,
            PluginExecutionClassV3::IsolatedNativeProcess
        );
        assert_eq!(
            migrated.report.status,
            MigrationStatus::RequiresManualAction
        );
        assert!(migrated.value.permissions.network.is_empty());
        migrated.value.validate_contract().unwrap();
    }

    #[test]
    fn unsafe_or_ambiguous_v2_classes_require_explicit_reauthoring() {
        let mut native_abi = PluginManifest::built_in("fixture.abi", "2.0.0");
        native_abi.class = crate::PluginClass::NativeAbi;
        assert_eq!(
            PluginManifestV3::migrate_from_v2(native_abi).unwrap_err(),
            PluginManifestV3Error::NativeAbiRemoved
        );

        let mut wasi = PluginManifest::built_in("fixture.wasi", "2.0.0");
        wasi.class = crate::PluginClass::WasiComponent;
        assert_eq!(
            PluginManifestV3::migrate_from_v2(wasi).unwrap_err(),
            PluginManifestV3Error::WasiRequiresExplicitContract
        );
    }

    #[test]
    fn external_contract_rejects_traversal_and_empty_network_destinations() {
        let mut source = PluginManifest::built_in("fixture.native", "2.1.0");
        source.class = crate::PluginClass::IsolatedProcess;
        source.abi_version = Some(1);
        source.entrypoint = Some("plugin.exe".to_string());
        source.checksum_sha256 = Some("a".repeat(64));
        source.license = Some("Apache-2.0".to_string());
        let mut migrated = PluginManifestV3::migrate_from_v2(source).unwrap().value;

        migrated.artifact.as_mut().unwrap().path = "../plugin.exe".to_string();
        assert_eq!(
            migrated.validate_contract().unwrap_err(),
            PluginManifestV3Error::MissingEntrypoint
        );

        migrated.artifact.as_mut().unwrap().path = "plugin.exe".to_string();
        migrated.permissions.network.push(NetworkDestinationV3 {
            scheme: NetworkSchemeV3::Https,
            host: String::new(),
            port: 443,
        });
        assert_eq!(
            migrated.validate_contract().unwrap_err(),
            PluginManifestV3Error::InterfaceMismatch
        );
    }

    #[test]
    fn versioned_loader_selects_v3_and_rejects_future_schemas() {
        let source = PluginManifest::built_in("fixture.loader", "1.0.0");
        let migrated = PluginManifestV3::migrate_from_v2(source).unwrap().value;
        let encoded = serde_json::to_vec(&migrated).unwrap();
        let loaded = LoadedPluginManifest::parse_json(&encoded).unwrap();
        assert!(matches!(loaded, LoadedPluginManifest::V3(_)));
        assert_eq!(loaded.schema_version(), 3);

        assert_eq!(
            LoadedPluginManifest::parse_json(br#"{"schemaVersion":4}"#).unwrap_err(),
            PluginManifestV3Error::UnsupportedSchemaVersion(4)
        );
    }
}
