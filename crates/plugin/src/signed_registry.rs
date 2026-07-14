//! Signed registry metadata and remote-package policy.
//!
//! Canonical bytes are compact JSON emitted from the strongly typed schema below.
//! Struct field order is fixed, maps and sets are ordered, floats and arbitrary
//! JSON values are excluded, and signatures are never part of the signed value.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "registry-network")]
use std::io::Read;

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::PluginExecutionClassV3;

pub const REGISTRY_SPEC_VERSION: &str = "1.0";

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("registry metadata is invalid: {0}")]
    InvalidMetadata(String),
    #[error("registry metadata signature threshold was not met for role {0}")]
    SignatureThreshold(RegistryRole),
    #[error("registry metadata references unknown key {0}")]
    UnknownKey(String),
    #[error("registry metadata signature is invalid for key {0}")]
    InvalidSignature(String),
    #[error("registry metadata for role {0} is expired")]
    Expired(RegistryRole),
    #[error("registry metadata rollback detected for role {role}: trusted {trusted}, received {received}")]
    Rollback {
        role: RegistryRole,
        trusted: u64,
        received: u64,
    },
    #[error("registry metadata length mismatch")]
    LengthMismatch,
    #[error("registry metadata SHA-256 mismatch")]
    DigestMismatch,
    #[error("registry target is revoked: {0}")]
    Revoked(String),
    #[error("remote plugins must use the WASI component execution class")]
    RemoteExecutionClass,
    #[error("plugin version downgrade rejected: installed {installed}, requested {requested}")]
    VersionDowngrade {
        installed: String,
        requested: String,
    },
    #[error("plugin execution-class downgrade rejected")]
    ExecutionClassDowngrade,
    #[error("plugin is incompatible with core version {0}")]
    Incompatible(String),
    #[error("registry I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("registry JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("registry archive failed: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[cfg(feature = "registry-network")]
    #[error("registry network request failed: {0}")]
    Network(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistryRole {
    Root,
    Timestamp,
    Snapshot,
    Targets,
}

impl std::fmt::Display for RegistryRole {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Root => "root",
            Self::Timestamp => "timestamp",
            Self::Snapshot => "snapshot",
            Self::Targets => "targets",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetadataSignature {
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedMetadata<T> {
    pub signed: T,
    pub signatures: Vec<MetadataSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetadataKey {
    pub key_type: String,
    pub scheme: String,
    pub public_key: String,
}

impl MetadataKey {
    fn verifying_key(&self) -> Result<VerifyingKey, RegistryError> {
        if self.key_type != "ed25519" || self.scheme != "ed25519" {
            return Err(RegistryError::InvalidMetadata(
                "only Ed25519 registry keys are supported".to_string(),
            ));
        }
        let bytes = decode_array::<32>(&self.public_key).map_err(|_| {
            RegistryError::InvalidMetadata("invalid Ed25519 public key".to_string())
        })?;
        VerifyingKey::from_bytes(&bytes)
            .map_err(|_| RegistryError::InvalidMetadata("invalid Ed25519 public key".to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetadataRole {
    pub key_ids: BTreeSet<String>,
    pub threshold: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootMetadata {
    pub spec_version: String,
    pub version: u64,
    pub expires_unix: u64,
    pub keys: BTreeMap<String, MetadataKey>,
    pub roles: BTreeMap<RegistryRole, MetadataRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetadataDescription {
    pub version: u64,
    pub length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimestampMetadata {
    pub spec_version: String,
    pub version: u64,
    pub expires_unix: u64,
    pub snapshot: MetadataDescription,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotMetadata {
    pub spec_version: String,
    pub version: u64,
    pub expires_unix: u64,
    pub targets: MetadataDescription,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryTarget {
    pub plugin_id: String,
    pub version: String,
    pub package_path: String,
    pub length: u64,
    pub sha256: String,
    pub execution_class: PluginExecutionClassV3,
    pub core_version_requirement: String,
    #[serde(default)]
    pub revoked: bool,
    pub revocation_reason: Option<String>,
}

impl RegistryTarget {
    pub fn validate_for_remote(
        &self,
        core_version: &str,
        installed: Option<(&str, PluginExecutionClassV3)>,
    ) -> Result<(), RegistryError> {
        if self.plugin_id.trim().is_empty()
            || !valid_relative_path(&self.package_path)
            || !valid_sha256(&self.sha256)
            || self.length == 0
        {
            return Err(RegistryError::InvalidMetadata(
                "target identity, path, digest, or length is invalid".to_string(),
            ));
        }
        let requested = semver::Version::parse(&self.version)
            .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))?;
        let requirement = semver::VersionReq::parse(&self.core_version_requirement)
            .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))?;
        let core = semver::Version::parse(core_version)
            .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))?;
        if self.revoked {
            return Err(RegistryError::Revoked(self.plugin_id.clone()));
        }
        if self.execution_class != PluginExecutionClassV3::WasiComponent {
            return Err(RegistryError::RemoteExecutionClass);
        }
        if !requirement.matches(&core) {
            return Err(RegistryError::Incompatible(core_version.to_string()));
        }
        if let Some((installed_version, installed_class)) = installed {
            if installed_class != PluginExecutionClassV3::WasiComponent {
                return Err(RegistryError::ExecutionClassDowngrade);
            }
            let installed = semver::Version::parse(installed_version)
                .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))?;
            if requested < installed {
                return Err(RegistryError::VersionDowngrade {
                    installed: installed.to_string(),
                    requested: requested.to_string(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetsMetadata {
    pub spec_version: String,
    pub version: u64,
    pub expires_unix: u64,
    pub targets: BTreeMap<String, RegistryTarget>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryVersions {
    pub root: u64,
    pub timestamp: u64,
    pub snapshot: u64,
    pub targets: u64,
}

pub type VerifiedRegistryChain = (
    SignedMetadata<TimestampMetadata>,
    SignedMetadata<SnapshotMetadata>,
    SignedMetadata<TargetsMetadata>,
);

pub trait TargetMetadata {
    fn version(&self) -> u64;
    fn expires_unix(&self) -> u64;
    fn spec_version(&self) -> &str;
}

macro_rules! impl_target_metadata {
    ($type:ty) => {
        impl TargetMetadata for $type {
            fn version(&self) -> u64 {
                self.version
            }

            fn expires_unix(&self) -> u64 {
                self.expires_unix
            }

            fn spec_version(&self) -> &str {
                &self.spec_version
            }
        }
    };
}

impl_target_metadata!(RootMetadata);
impl_target_metadata!(TimestampMetadata);
impl_target_metadata!(SnapshotMetadata);
impl_target_metadata!(TargetsMetadata);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustState {
    TrustedBuiltIn,
    ReviewedLocalNativeProcess,
    VerifiedWasiComponent,
    Unverified,
    Expired,
    Revoked,
    Quarantined,
    Incompatible,
}

#[cfg(feature = "registry-network")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryDownloadKind {
    Metadata,
    Package,
}

#[cfg(feature = "registry-network")]
pub struct HttpsRegistryDownloader {
    client: reqwest::blocking::Client,
    trusted_origin: reqwest::Url,
    maximum_redirects: usize,
    maximum_bytes: u64,
}

#[cfg(feature = "registry-network")]
impl HttpsRegistryDownloader {
    pub fn new(
        trusted_origin: &str,
        timeout: std::time::Duration,
        maximum_redirects: usize,
        maximum_bytes: u64,
    ) -> Result<Self, RegistryError> {
        if maximum_bytes == 0 {
            return Err(RegistryError::InvalidMetadata(
                "download byte limit must be nonzero".to_string(),
            ));
        }
        let trusted_origin = parse_trusted_origin(trusted_origin)?;
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .connect_timeout(timeout)
            .user_agent(concat!("latexsnipper-core/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            client,
            trusted_origin,
            maximum_redirects,
            maximum_bytes,
        })
    }

    pub fn get(
        &self,
        relative_path: &str,
        kind: RegistryDownloadKind,
    ) -> Result<Vec<u8>, RegistryError> {
        if !valid_relative_path(relative_path) {
            return Err(RegistryError::InvalidMetadata(
                "registry download path is not a safe relative path".to_string(),
            ));
        }
        let mut current = self
            .trusted_origin
            .join(relative_path)
            .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))?;
        for redirects in 0..=self.maximum_redirects {
            let mut response = self
                .client
                .get(current.clone())
                .header(reqwest::header::ACCEPT_ENCODING, "identity")
                .send()?;
            if response.status().is_redirection() {
                if redirects == self.maximum_redirects {
                    return Err(RegistryError::InvalidMetadata(
                        "registry redirect limit exceeded".to_string(),
                    ));
                }
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .ok_or_else(|| {
                        RegistryError::InvalidMetadata(
                            "registry redirect omitted Location".to_string(),
                        )
                    })?
                    .to_str()
                    .map_err(|_| {
                        RegistryError::InvalidMetadata(
                            "registry redirect Location is not UTF-8".to_string(),
                        )
                    })?;
                let candidate = current
                    .join(location)
                    .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))?;
                ensure_same_origin(&self.trusted_origin, &candidate)?;
                current = candidate;
                continue;
            }
            if !response.status().is_success() {
                return Err(RegistryError::InvalidMetadata(format!(
                    "registry returned HTTP {}",
                    response.status()
                )));
            }
            validate_content_type(response.headers(), kind)?;
            if response
                .headers()
                .get(reqwest::header::CONTENT_ENCODING)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
            {
                return Err(RegistryError::InvalidMetadata(
                    "encoded registry responses are rejected".to_string(),
                ));
            }
            if response
                .content_length()
                .is_some_and(|length| length > self.maximum_bytes)
            {
                return Err(RegistryError::InvalidMetadata(
                    "registry response exceeds configured limit".to_string(),
                ));
            }
            let mut bytes = Vec::new();
            response
                .by_ref()
                .take(self.maximum_bytes.saturating_add(1))
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 > self.maximum_bytes {
                return Err(RegistryError::InvalidMetadata(
                    "registry response exceeds configured limit".to_string(),
                ));
            }
            return Ok(bytes);
        }
        Err(RegistryError::InvalidMetadata(
            "registry redirect state is invalid".to_string(),
        ))
    }
}

#[cfg(feature = "registry-network")]
fn parse_trusted_origin(value: &str) -> Result<reqwest::Url, RegistryError> {
    let url = reqwest::Url::parse(value)
        .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(RegistryError::InvalidMetadata(
            "trusted registry origin must be an HTTPS URL without credentials, query, or fragment"
                .to_string(),
        ));
    }
    Ok(url)
}

#[cfg(feature = "registry-network")]
fn ensure_same_origin(
    trusted: &reqwest::Url,
    candidate: &reqwest::Url,
) -> Result<(), RegistryError> {
    if candidate.scheme() != "https"
        || candidate.scheme() != trusted.scheme()
        || candidate.host_str() != trusted.host_str()
        || candidate.port_or_known_default() != trusted.port_or_known_default()
        || !candidate.username().is_empty()
        || candidate.password().is_some()
    {
        return Err(RegistryError::InvalidMetadata(
            "cross-origin or credential-bearing registry redirect rejected".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "registry-network")]
fn validate_content_type(
    headers: &reqwest::header::HeaderMap,
    kind: RegistryDownloadKind,
) -> Result<(), RegistryError> {
    let content_type = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| RegistryError::InvalidMetadata("missing Content-Type".to_string()))?;
    let allowed = match kind {
        RegistryDownloadKind::Metadata => matches!(
            content_type.as_str(),
            "application/json" | "application/vnd.latexsnipper.registry+json"
        ),
        RegistryDownloadKind::Package => matches!(
            content_type.as_str(),
            "application/zip" | "application/vnd.latexsnipper.plugin+zip"
        ),
    };
    if !allowed {
        return Err(RegistryError::InvalidMetadata(format!(
            "unexpected registry Content-Type '{content_type}'"
        )));
    }
    Ok(())
}

pub fn canonical_signed_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, RegistryError> {
    Ok(serde_json::to_vec(value)?)
}

pub fn verify_metadata<T: Serialize + TargetMetadata>(
    envelope: &SignedMetadata<T>,
    role: RegistryRole,
    trusted_root: &RootMetadata,
    now_unix: u64,
    minimum_version: u64,
) -> Result<(), RegistryError> {
    validate_common(envelope.signed.spec_version(), envelope.signed.version())?;
    if envelope.signed.expires_unix() <= now_unix {
        return Err(RegistryError::Expired(role));
    }
    if envelope.signed.version() < minimum_version {
        return Err(RegistryError::Rollback {
            role,
            trusted: minimum_version,
            received: envelope.signed.version(),
        });
    }
    let role_keys = trusted_root
        .roles
        .get(&role)
        .ok_or_else(|| RegistryError::InvalidMetadata(format!("root omits the {role} role")))?;
    if role_keys.threshold == 0 || role_keys.threshold as usize > role_keys.key_ids.len() {
        return Err(RegistryError::InvalidMetadata(format!(
            "invalid signature threshold for role {role}"
        )));
    }
    let canonical = canonical_signed_bytes(&envelope.signed)?;
    let mut valid_keys = BTreeSet::new();
    for signature in &envelope.signatures {
        if !role_keys.key_ids.contains(&signature.key_id) || valid_keys.contains(&signature.key_id)
        {
            continue;
        }
        let key_id = signature.key_id.clone();
        let key = trusted_root
            .keys
            .get(&key_id)
            .ok_or_else(|| RegistryError::UnknownKey(key_id.clone()))?
            .verifying_key()?;
        let signature_bytes = decode_array::<64>(&signature.signature)
            .map_err(|_| RegistryError::InvalidSignature(key_id.clone()))?;
        let signature = Signature::from_bytes(&signature_bytes);
        key.verify(&canonical, &signature)
            .map_err(|_| RegistryError::InvalidSignature(key_id.clone()))?;
        valid_keys.insert(key_id);
    }
    if valid_keys.len() < role_keys.threshold as usize {
        return Err(RegistryError::SignatureThreshold(role));
    }
    Ok(())
}

pub fn verify_initial_root(
    root: &SignedMetadata<RootMetadata>,
    now_unix: u64,
) -> Result<(), RegistryError> {
    validate_root(&root.signed)?;
    verify_metadata(root, RegistryRole::Root, &root.signed, now_unix, 1)
}

pub fn verify_root_update(
    current: &SignedMetadata<RootMetadata>,
    candidate: &SignedMetadata<RootMetadata>,
    now_unix: u64,
) -> Result<(), RegistryError> {
    validate_root(&current.signed)?;
    validate_root(&candidate.signed)?;
    if candidate.signed.version != current.signed.version.saturating_add(1) {
        return Err(RegistryError::Rollback {
            role: RegistryRole::Root,
            trusted: current.signed.version.saturating_add(1),
            received: candidate.signed.version,
        });
    }
    verify_metadata(
        candidate,
        RegistryRole::Root,
        &current.signed,
        now_unix,
        candidate.signed.version,
    )?;
    verify_metadata(
        candidate,
        RegistryRole::Root,
        &candidate.signed,
        now_unix,
        candidate.signed.version,
    )
}

pub fn verify_registry_chain(
    root: &SignedMetadata<RootMetadata>,
    timestamp_bytes: &[u8],
    snapshot_bytes: &[u8],
    targets_bytes: &[u8],
    now_unix: u64,
    minimum: RegistryVersions,
) -> Result<VerifiedRegistryChain, RegistryError> {
    verify_initial_root(root, now_unix)?;
    if root.signed.version < minimum.root {
        return Err(RegistryError::Rollback {
            role: RegistryRole::Root,
            trusted: minimum.root,
            received: root.signed.version,
        });
    }
    let timestamp = decode_signed_metadata::<TimestampMetadata>(timestamp_bytes)?;
    verify_metadata(
        &timestamp,
        RegistryRole::Timestamp,
        &root.signed,
        now_unix,
        minimum.timestamp,
    )?;
    verify_description(snapshot_bytes, &timestamp.signed.snapshot)?;

    let snapshot = decode_signed_metadata::<SnapshotMetadata>(snapshot_bytes)?;
    if snapshot.signed.version != timestamp.signed.snapshot.version {
        return Err(RegistryError::InvalidMetadata(
            "snapshot version does not match timestamp metadata".to_string(),
        ));
    }
    verify_metadata(
        &snapshot,
        RegistryRole::Snapshot,
        &root.signed,
        now_unix,
        minimum.snapshot,
    )?;
    verify_description(targets_bytes, &snapshot.signed.targets)?;

    let targets = decode_signed_metadata::<TargetsMetadata>(targets_bytes)?;
    if targets.signed.version != snapshot.signed.targets.version {
        return Err(RegistryError::InvalidMetadata(
            "targets version does not match snapshot metadata".to_string(),
        ));
    }
    verify_metadata(
        &targets,
        RegistryRole::Targets,
        &root.signed,
        now_unix,
        minimum.targets,
    )?;
    for (id, target) in &targets.signed.targets {
        if id != &target.plugin_id {
            return Err(RegistryError::InvalidMetadata(
                "target map key does not match plugin ID".to_string(),
            ));
        }
    }
    Ok((timestamp, snapshot, targets))
}

pub fn metadata_description<T: Serialize + TargetMetadata>(
    envelope: &SignedMetadata<T>,
) -> Result<MetadataDescription, RegistryError> {
    let bytes = canonical_signed_envelope_bytes(envelope)?;
    Ok(MetadataDescription {
        version: envelope.signed.version(),
        length: bytes.len() as u64,
        sha256: hex::encode(Sha256::digest(&bytes)),
    })
}

pub fn canonical_signed_envelope_bytes<T: Serialize>(
    envelope: &SignedMetadata<T>,
) -> Result<Vec<u8>, RegistryError> {
    Ok(serde_json::to_vec(envelope)?)
}

pub fn verify_description(
    bytes: &[u8],
    description: &MetadataDescription,
) -> Result<(), RegistryError> {
    if bytes.len() as u64 != description.length {
        return Err(RegistryError::LengthMismatch);
    }
    if !valid_sha256(&description.sha256)
        || !constant_time_equal(
            &Sha256::digest(bytes),
            &hex::decode(&description.sha256).map_err(|_| RegistryError::DigestMismatch)?,
        )
    {
        return Err(RegistryError::DigestMismatch);
    }
    Ok(())
}

fn validate_root(root: &RootMetadata) -> Result<(), RegistryError> {
    validate_common(&root.spec_version, root.version)?;
    for role in [
        RegistryRole::Root,
        RegistryRole::Timestamp,
        RegistryRole::Snapshot,
        RegistryRole::Targets,
    ] {
        let definition = root
            .roles
            .get(&role)
            .ok_or_else(|| RegistryError::InvalidMetadata(format!("root omits the {role} role")))?;
        if definition.threshold == 0 || definition.threshold as usize > definition.key_ids.len() {
            return Err(RegistryError::InvalidMetadata(format!(
                "invalid threshold for {role}"
            )));
        }
        for key_id in &definition.key_ids {
            root.keys
                .get(key_id)
                .ok_or_else(|| RegistryError::UnknownKey(key_id.clone()))?
                .verifying_key()?;
        }
    }
    Ok(())
}

fn validate_common(spec_version: &str, version: u64) -> Result<(), RegistryError> {
    if spec_version != REGISTRY_SPEC_VERSION || version == 0 {
        return Err(RegistryError::InvalidMetadata(
            "unsupported spec version or zero metadata version".to_string(),
        ));
    }
    Ok(())
}

fn decode_array<const N: usize>(value: &str) -> Result<[u8; N], hex::FromHexError> {
    let decoded = hex::decode(value)?;
    decoded
        .try_into()
        .map_err(|_| hex::FromHexError::InvalidStringLength)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_relative_path(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && value
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

pub fn decode_signed_metadata<T: DeserializeOwned>(
    bytes: &[u8],
) -> Result<SignedMetadata<T>, RegistryError> {
    Ok(serde_json::from_slice(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const NOW: u64 = 1_900_000_000;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn metadata_key(key: &SigningKey) -> MetadataKey {
        MetadataKey {
            key_type: "ed25519".to_string(),
            scheme: "ed25519".to_string(),
            public_key: hex::encode(key.verifying_key().to_bytes()),
        }
    }

    fn signed<T: Serialize>(value: T, key_id: &str, key: &SigningKey) -> SignedMetadata<T> {
        let bytes = canonical_signed_bytes(&value).unwrap();
        SignedMetadata {
            signed: value,
            signatures: vec![MetadataSignature {
                key_id: key_id.to_string(),
                signature: hex::encode(key.sign(&bytes).to_bytes()),
            }],
        }
    }

    fn root(
        version: u64,
        root_key: &SigningKey,
        delegated: &SigningKey,
    ) -> SignedMetadata<RootMetadata> {
        root_named(version, "root", root_key, delegated)
    }

    fn root_named(
        version: u64,
        root_id: &str,
        root_key: &SigningKey,
        delegated: &SigningKey,
    ) -> SignedMetadata<RootMetadata> {
        let keys = BTreeMap::from([
            (root_id.to_string(), metadata_key(root_key)),
            ("delegated".to_string(), metadata_key(delegated)),
        ]);
        let roles = BTreeMap::from([
            (
                RegistryRole::Root,
                MetadataRole {
                    key_ids: BTreeSet::from([root_id.to_string()]),
                    threshold: 1,
                },
            ),
            (
                RegistryRole::Timestamp,
                MetadataRole {
                    key_ids: BTreeSet::from(["delegated".to_string()]),
                    threshold: 1,
                },
            ),
            (
                RegistryRole::Snapshot,
                MetadataRole {
                    key_ids: BTreeSet::from(["delegated".to_string()]),
                    threshold: 1,
                },
            ),
            (
                RegistryRole::Targets,
                MetadataRole {
                    key_ids: BTreeSet::from(["delegated".to_string()]),
                    threshold: 1,
                },
            ),
        ]);
        signed(
            RootMetadata {
                spec_version: REGISTRY_SPEC_VERSION.to_string(),
                version,
                expires_unix: NOW + 10_000,
                keys,
                roles,
            },
            root_id,
            root_key,
        )
    }

    #[test]
    fn canonical_serialization_is_stable_and_excludes_signatures() {
        let value = TimestampMetadata {
            spec_version: REGISTRY_SPEC_VERSION.to_string(),
            version: 7,
            expires_unix: 123,
            snapshot: MetadataDescription {
                version: 6,
                length: 42,
                sha256: "a".repeat(64),
            },
        };
        let bytes = canonical_signed_bytes(&value).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            format!(
                "{{\"specVersion\":\"1.0\",\"version\":7,\"expiresUnix\":123,\"snapshot\":{{\"version\":6,\"length\":42,\"sha256\":\"{}\"}}}}",
                "a".repeat(64)
            )
        );
    }

    #[test]
    fn real_ed25519_signature_and_threshold_are_enforced() {
        let root_key = key(1);
        let delegated = key(2);
        let trust = root(1, &root_key, &delegated);
        verify_initial_root(&trust, NOW).unwrap();

        let timestamp = signed(
            TimestampMetadata {
                spec_version: REGISTRY_SPEC_VERSION.to_string(),
                version: 1,
                expires_unix: NOW + 100,
                snapshot: MetadataDescription {
                    version: 1,
                    length: 2,
                    sha256: "a".repeat(64),
                },
            },
            "delegated",
            &delegated,
        );
        verify_metadata(&timestamp, RegistryRole::Timestamp, &trust.signed, NOW, 1).unwrap();

        let mut tampered = timestamp;
        tampered.signed.version = 2;
        assert!(matches!(
            verify_metadata(&tampered, RegistryRole::Timestamp, &trust.signed, NOW, 1),
            Err(RegistryError::InvalidSignature(_))
        ));
    }

    #[test]
    fn expiry_and_rollback_are_rejected() {
        let root_key = key(3);
        let delegated = key(4);
        let mut trust = root(1, &root_key, &delegated);
        trust.signed.expires_unix = NOW;
        trust = signed(trust.signed, "root", &root_key);
        assert!(matches!(
            verify_initial_root(&trust, NOW),
            Err(RegistryError::Expired(RegistryRole::Root))
        ));

        let trust = root(1, &root_key, &delegated);
        assert!(matches!(
            verify_metadata(&trust, RegistryRole::Root, &trust.signed, NOW, 2),
            Err(RegistryError::Rollback { .. })
        ));
    }

    #[test]
    fn unknown_keys_and_unsatisfied_thresholds_are_rejected() {
        let root_key = key(13);
        let delegated = key(14);
        let mut threshold = root(1, &root_key, &delegated);
        let root_role = threshold.signed.roles.get_mut(&RegistryRole::Root).unwrap();
        root_role.key_ids.insert("delegated".to_string());
        root_role.threshold = 2;
        threshold = signed(threshold.signed, "root", &root_key);
        assert!(matches!(
            verify_initial_root(&threshold, NOW),
            Err(RegistryError::SignatureThreshold(RegistryRole::Root))
        ));

        let mut unknown = root(1, &root_key, &delegated);
        unknown
            .signed
            .roles
            .get_mut(&RegistryRole::Root)
            .unwrap()
            .key_ids = BTreeSet::from(["unknown".to_string()]);
        unknown = signed(unknown.signed, "root", &root_key);
        assert!(matches!(
            verify_initial_root(&unknown, NOW),
            Err(RegistryError::UnknownKey(key_id)) if key_id == "unknown"
        ));
    }

    #[test]
    fn root_rotation_requires_old_and_new_thresholds() {
        let old = key(5);
        let delegated = key(6);
        let current = root_named(1, "old-root", &old, &delegated);
        let new = key(7);
        let mut candidate = root_named(2, "new-root", &new, &delegated);

        let new_bytes = canonical_signed_bytes(&candidate.signed).unwrap();
        candidate.signatures.push(MetadataSignature {
            key_id: "old-root".to_string(),
            signature: hex::encode(old.sign(&new_bytes).to_bytes()),
        });
        verify_root_update(&current, &candidate, NOW).unwrap();

        candidate.signatures.retain(|signature| {
            let bytes = decode_array::<64>(&signature.signature).unwrap();
            new.verifying_key()
                .verify(&new_bytes, &Signature::from_bytes(&bytes))
                .is_ok()
        });
        assert!(verify_root_update(&current, &candidate, NOW).is_err());
    }

    #[test]
    fn remote_policy_rejects_native_revoked_incompatible_and_downgrade() {
        let base = RegistryTarget {
            plugin_id: "example.plugin".to_string(),
            version: "2.0.0".to_string(),
            package_path: "packages/example.plugin-2.0.0.zip".to_string(),
            length: 100,
            sha256: "a".repeat(64),
            execution_class: PluginExecutionClassV3::WasiComponent,
            core_version_requirement: ">=3.0.0-alpha.1, <4".to_string(),
            revoked: false,
            revocation_reason: None,
        };
        base.validate_for_remote("3.0.0-alpha.1", None).unwrap();

        let mut native = base.clone();
        native.execution_class = PluginExecutionClassV3::IsolatedNativeProcess;
        assert!(matches!(
            native.validate_for_remote("3.0.0-alpha.1", None),
            Err(RegistryError::RemoteExecutionClass)
        ));

        let mut revoked = base.clone();
        revoked.revoked = true;
        assert!(matches!(
            revoked.validate_for_remote("3.0.0-alpha.1", None),
            Err(RegistryError::Revoked(_))
        ));

        assert!(matches!(
            base.validate_for_remote(
                "3.0.0-alpha.1",
                Some(("3.0.0", PluginExecutionClassV3::WasiComponent))
            ),
            Err(RegistryError::VersionDowngrade { .. })
        ));
    }

    #[cfg(feature = "registry-network")]
    #[test]
    fn downloader_rejects_non_https_cross_origin_and_content_type_confusion() {
        assert!(parse_trusted_origin("http://registry.example.invalid").is_err());
        assert!(parse_trusted_origin("https://user@registry.example.invalid").is_err());
        let trusted = parse_trusted_origin("https://registry.example.invalid/base/").unwrap();
        let same = reqwest::Url::parse("https://registry.example.invalid/next").unwrap();
        ensure_same_origin(&trusted, &same).unwrap();
        let malicious = reqwest::Url::parse("https://mirror.example.invalid/next").unwrap();
        assert!(ensure_same_origin(&trusted, &malicious).is_err());

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("text/html"),
        );
        assert!(validate_content_type(&headers, RegistryDownloadKind::Package).is_err());
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static(
                "application/vnd.latexsnipper.plugin+zip; charset=binary",
            ),
        );
        validate_content_type(&headers, RegistryDownloadKind::Package).unwrap();
    }
}
