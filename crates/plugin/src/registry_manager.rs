use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::{
    canonical_signed_envelope_bytes, decode_signed_metadata, verify_initial_root,
    verify_registry_chain, verify_root_update, HttpsRegistryDownloader, RegistryDownloadKind,
    RegistryError, RegistryTarget, RegistryVersions, RemoteInstalledPlugin, RemotePluginProvenance,
    RemotePluginStore, RootMetadata, SignedMetadata, TargetsMetadata, TrustState,
};

const STATE_SCHEMA_VERSION: u32 = 1;
const METADATA_LIMIT: u64 = 8 * 1024 * 1024;
const PACKAGE_LIMIT: u64 = 64 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20);
const MAXIMUM_REDIRECTS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfiguredRegistry {
    pub name: String,
    pub origin: String,
    pub trust_state: TrustState,
    pub trusted_root: Option<SignedMetadata<RootMetadata>>,
    pub versions: RegistryVersions,
    pub refreshed_at_unix: Option<u64>,
    cache: Option<RegistryCache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryCache {
    timestamp_hex: String,
    snapshot_hex: String,
    targets_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryState {
    schema_version: u32,
    registries: BTreeMap<String, ConfiguredRegistry>,
}

impl Default for RegistryState {
    fn default() -> Self {
        Self {
            schema_version: STATE_SCHEMA_VERSION,
            registries: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRefreshResult {
    pub name: String,
    pub origin: String,
    pub offline: bool,
    pub versions: RegistryVersions,
    pub target_count: usize,
    pub trust_state: TrustState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchResult {
    pub registry_name: String,
    pub registry_origin: String,
    pub targets_version: u64,
    pub target: RegistryTarget,
    pub trust_state: TrustState,
}

pub struct SignedRegistryManager {
    root: PathBuf,
}

impl SignedRegistryManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn remote_store(&self) -> RemotePluginStore {
        RemotePluginStore::new(self.root.join("remote"))
    }

    pub fn list(&self) -> Result<Vec<ConfiguredRegistry>, RegistryError> {
        Ok(self
            .load_state_with_recovery()?
            .registries
            .into_values()
            .collect())
    }

    pub fn add(&self, name: &str, origin: &str) -> Result<ConfiguredRegistry, RegistryError> {
        validate_registry_name(name)?;
        HttpsRegistryDownloader::new(origin, DOWNLOAD_TIMEOUT, MAXIMUM_REDIRECTS, METADATA_LIMIT)?;
        let lock = self.lock_exclusive()?;
        let mut state = self.load_state_with_recovery()?;
        if state.registries.contains_key(name) {
            return Err(RegistryError::InvalidMetadata(format!(
                "registry '{name}' already exists"
            )));
        }
        let registry = ConfiguredRegistry {
            name: name.to_string(),
            origin: normalize_origin(origin),
            trust_state: TrustState::Unverified,
            trusted_root: None,
            versions: RegistryVersions::default(),
            refreshed_at_unix: None,
            cache: None,
        };
        state.registries.insert(name.to_string(), registry.clone());
        self.write_state(&state)?;
        FileExt::unlock(&lock)?;
        Ok(registry)
    }

    pub fn remove(&self, name: &str) -> Result<ConfiguredRegistry, RegistryError> {
        let lock = self.lock_exclusive()?;
        let mut state = self.load_state_with_recovery()?;
        let registry = state.registries.remove(name).ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("registry '{name}' is not configured"))
        })?;
        self.write_state(&state)?;
        FileExt::unlock(&lock)?;
        Ok(registry)
    }

    pub fn trust(
        &self,
        name: &str,
        root_bytes: &[u8],
        now_unix: u64,
    ) -> Result<ConfiguredRegistry, RegistryError> {
        let candidate = decode_signed_metadata::<RootMetadata>(root_bytes)?;
        let lock = self.lock_exclusive()?;
        let mut state = self.load_state_with_recovery()?;
        let registry = state.registries.get_mut(name).ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("registry '{name}' is not configured"))
        })?;
        match registry.trusted_root.as_ref() {
            Some(current) if current.signed.version == candidate.signed.version => {
                verify_initial_root(&candidate, now_unix)?;
                if canonical_signed_envelope_bytes(current)?
                    != canonical_signed_envelope_bytes(&candidate)?
                {
                    return Err(RegistryError::InvalidMetadata(
                        "same-version trust-root replacement rejected".to_string(),
                    ));
                }
            }
            Some(current) => verify_root_update(current, &candidate, now_unix)?,
            None => verify_initial_root(&candidate, now_unix)?,
        }
        registry.versions.root = candidate.signed.version;
        registry.trusted_root = Some(candidate);
        registry.trust_state = TrustState::VerifiedWasiComponent;
        registry.cache = None;
        registry.refreshed_at_unix = None;
        let result = registry.clone();
        self.write_state(&state)?;
        FileExt::unlock(&lock)?;
        Ok(result)
    }

    pub fn refresh(
        &self,
        name: &str,
        offline: bool,
        now_unix: u64,
    ) -> Result<RegistryRefreshResult, RegistryError> {
        let lock = self.lock_exclusive()?;
        let mut state = self.load_state_with_recovery()?;
        let registry = state.registries.get_mut(name).ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("registry '{name}' is not configured"))
        })?;
        let root = registry.trusted_root.as_ref().ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("registry '{name}' has no trusted root"))
        })?;
        let (timestamp_bytes, snapshot_bytes, targets_bytes) = if offline {
            let cache = registry.cache.as_ref().ok_or_else(|| {
                RegistryError::InvalidMetadata("registry has no offline metadata cache".to_string())
            })?;
            (
                decode_hex(&cache.timestamp_hex)?,
                decode_hex(&cache.snapshot_hex)?,
                decode_hex(&cache.targets_hex)?,
            )
        } else {
            let downloader = HttpsRegistryDownloader::new(
                &registry.origin,
                DOWNLOAD_TIMEOUT,
                MAXIMUM_REDIRECTS,
                METADATA_LIMIT,
            )?;
            (
                downloader.get("metadata/timestamp.json", RegistryDownloadKind::Metadata)?,
                downloader.get("metadata/snapshot.json", RegistryDownloadKind::Metadata)?,
                downloader.get("metadata/targets.json", RegistryDownloadKind::Metadata)?,
            )
        };
        let (timestamp, snapshot, targets) = verify_registry_chain(
            root,
            &timestamp_bytes,
            &snapshot_bytes,
            &targets_bytes,
            now_unix,
            registry.versions,
        )?;
        registry.versions = RegistryVersions {
            root: root.signed.version,
            timestamp: timestamp.signed.version,
            snapshot: snapshot.signed.version,
            targets: targets.signed.version,
        };
        registry.refreshed_at_unix = Some(now_unix);
        registry.trust_state = TrustState::VerifiedWasiComponent;
        if !offline {
            registry.cache = Some(RegistryCache {
                timestamp_hex: hex::encode(timestamp_bytes),
                snapshot_hex: hex::encode(snapshot_bytes),
                targets_hex: hex::encode(targets_bytes),
            });
        }
        let revoked_targets = targets
            .signed
            .targets
            .values()
            .filter(|target| target.revoked)
            .map(|target| (target.plugin_id.clone(), target.version.clone()))
            .collect::<Vec<_>>();
        let result = RegistryRefreshResult {
            name: registry.name.clone(),
            origin: registry.origin.clone(),
            offline,
            versions: registry.versions,
            target_count: targets.signed.targets.len(),
            trust_state: registry.trust_state.clone(),
        };
        self.write_state(&state)?;
        FileExt::unlock(&lock)?;
        let remote_store = self.remote_store();
        for (id, version) in revoked_targets {
            if remote_store
                .get(&id)?
                .is_some_and(|installed| installed.manifest.version == version)
            {
                remote_store.revoke(&id)?;
            }
        }
        Ok(result)
    }

    pub fn refresh_all(
        &self,
        offline: bool,
        now_unix: u64,
    ) -> Result<Vec<RegistryRefreshResult>, RegistryError> {
        let names: Vec<String> = self.list()?.into_iter().map(|item| item.name).collect();
        names
            .iter()
            .map(|name| self.refresh(name, offline, now_unix))
            .collect()
    }

    pub fn search(&self, query: Option<&str>) -> Result<Vec<RegistrySearchResult>, RegistryError> {
        let state = self.load_state_with_recovery()?;
        let now_unix = crate::remote_plugin_store::current_unix_time()?;
        let query = query.map(str::to_ascii_lowercase);
        let mut results = Vec::new();
        for registry in state.registries.values() {
            if registry.trust_state != TrustState::VerifiedWasiComponent {
                continue;
            }
            let Some(cache) = registry.cache.as_ref() else {
                continue;
            };
            let timestamp_bytes = decode_hex(&cache.timestamp_hex)?;
            let snapshot_bytes = decode_hex(&cache.snapshot_hex)?;
            let targets_bytes = decode_hex(&cache.targets_hex)?;
            let root = registry.trusted_root.as_ref().ok_or_else(|| {
                RegistryError::InvalidMetadata("verified registry omits trusted root".to_string())
            })?;
            let cache_trust = match verify_registry_chain(
                root,
                &timestamp_bytes,
                &snapshot_bytes,
                &targets_bytes,
                now_unix,
                registry.versions,
            ) {
                Ok(_) => TrustState::VerifiedWasiComponent,
                Err(RegistryError::Expired(_)) => TrustState::Expired,
                Err(error) => return Err(error),
            };
            let targets = decode_signed_metadata::<TargetsMetadata>(&targets_bytes)?;
            for target in targets.signed.targets.into_values() {
                if query.as_ref().is_some_and(|query| {
                    !target.plugin_id.to_ascii_lowercase().contains(query)
                        && !target.version.to_ascii_lowercase().contains(query)
                }) {
                    continue;
                }
                let trust_state = if target.revoked {
                    TrustState::Revoked
                } else if cache_trust == TrustState::Expired {
                    TrustState::Expired
                } else {
                    match target.validate_for_remote(env!("CARGO_PKG_VERSION"), None) {
                        Ok(()) => TrustState::VerifiedWasiComponent,
                        Err(RegistryError::Incompatible(_)) => TrustState::Incompatible,
                        Err(RegistryError::RemoteExecutionClass) => TrustState::Quarantined,
                        Err(error) => return Err(error),
                    }
                };
                results.push(RegistrySearchResult {
                    registry_name: registry.name.clone(),
                    registry_origin: registry.origin.clone(),
                    targets_version: registry.versions.targets,
                    target,
                    trust_state,
                });
            }
        }
        results.sort_by(|left, right| {
            left.target
                .plugin_id
                .cmp(&right.target.plugin_id)
                .then(left.target.version.cmp(&right.target.version))
                .then(left.registry_name.cmp(&right.registry_name))
        });
        Ok(results)
    }

    pub fn install(
        &self,
        id: &str,
        core_version: &str,
    ) -> Result<RemoteInstalledPlugin, RegistryError> {
        let candidate = self.select_target(id)?;
        let installed = self.remote_store().get(id)?;
        candidate.target.validate_for_remote(
            core_version,
            installed.as_ref().map(|installed| {
                (
                    installed.manifest.version.as_str(),
                    installed.manifest.execution_class,
                )
            }),
        )?;
        let downloader = HttpsRegistryDownloader::new(
            &candidate.registry_origin,
            DOWNLOAD_TIMEOUT,
            MAXIMUM_REDIRECTS,
            PACKAGE_LIMIT,
        )?;
        let bytes = downloader.get(
            &candidate.target.package_path,
            RegistryDownloadKind::Package,
        )?;
        let provenance = RemotePluginProvenance {
            registry_name: candidate.registry_name,
            registry_origin: candidate.registry_origin,
            targets_version: candidate.targets_version,
            package_path: candidate.target.package_path.clone(),
            package_sha256: candidate.target.sha256.clone(),
            verified_at_unix: crate::remote_plugin_store::current_unix_time()?,
        };
        self.remote_store()
            .install(&candidate.target, &bytes, provenance, core_version)
    }

    pub fn update_all(
        &self,
        core_version: &str,
    ) -> Result<Vec<RemoteInstalledPlugin>, RegistryError> {
        self.remote_store()
            .list()?
            .into_iter()
            .map(|plugin| self.install(&plugin.manifest.id, core_version))
            .collect()
    }

    fn select_target(&self, id: &str) -> Result<RegistrySearchResult, RegistryError> {
        let mut candidates: Vec<_> = self
            .search(Some(id))?
            .into_iter()
            .filter(|candidate| {
                candidate.target.plugin_id == id
                    && candidate.trust_state == TrustState::VerifiedWasiComponent
                    && !candidate.target.revoked
            })
            .collect();
        candidates.sort_by_key(|candidate| semver::Version::parse(&candidate.target.version).ok());
        candidates.pop().ok_or_else(|| {
            RegistryError::InvalidMetadata(format!(
                "no non-revoked verified registry target for plugin '{id}'"
            ))
        })
    }

    fn lock_exclusive(&self) -> Result<File, RegistryError> {
        fs::create_dir_all(&self.root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("registries.lock"))?;
        lock.lock_exclusive()?;
        Ok(lock)
    }

    fn load_state_with_recovery(&self) -> Result<RegistryState, RegistryError> {
        let path = self.root.join("registries.json");
        if !path.exists() {
            return Ok(RegistryState::default());
        }
        match read_state(&path) {
            Ok(state) => Ok(state),
            Err(primary) => {
                let backup = self.root.join("registries.json.bak");
                if backup.exists() {
                    read_state(&backup)
                } else {
                    Err(primary)
                }
            }
        }
    }

    fn write_state(&self, state: &RegistryState) -> Result<(), RegistryError> {
        fs::create_dir_all(&self.root)?;
        let path = self.root.join("registries.json");
        let backup = self.root.join("registries.json.bak");
        if path.exists() {
            fs::copy(&path, &backup)?;
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&backup)?
                .sync_all()?;
        }
        let temporary = self.root.join("registries.json.tmp");
        {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(state)?)?;
            file.sync_all()?;
        }
        replace_file(&temporary, &path)
    }
}

fn read_state(path: &Path) -> Result<RegistryState, RegistryError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > 32 * 1024 * 1024 {
        return Err(RegistryError::InvalidMetadata(
            "registry state exceeds configured limit".to_string(),
        ));
    }
    let state: RegistryState = serde_json::from_slice(&fs::read(path)?)?;
    if state.schema_version != STATE_SCHEMA_VERSION {
        return Err(RegistryError::InvalidMetadata(
            "unsupported registry state schema".to_string(),
        ));
    }
    Ok(state)
}

fn decode_hex(value: &str) -> Result<Vec<u8>, RegistryError> {
    hex::decode(value)
        .map_err(|_| RegistryError::InvalidMetadata("registry cache is corrupt".to_string()))
}

fn validate_registry_name(value: &str) -> Result<(), RegistryError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(RegistryError::InvalidMetadata(
            "registry name contains unsafe characters".to_string(),
        ));
    }
    Ok(())
}

fn normalize_origin(value: &str) -> String {
    if value.ends_with('/') {
        value.to_string()
    } else {
        format!("{value}/")
    }
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), RegistryError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(RegistryError::InvalidMetadata(format!(
            "atomic registry-state replacement failed: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signed_registry::REGISTRY_SPEC_VERSION;
    use crate::{
        canonical_signed_bytes, canonical_signed_envelope_bytes, metadata_description, MetadataKey,
        MetadataRole, MetadataSignature, RegistryRole, SnapshotMetadata, TimestampMetadata,
    };
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::{BTreeMap, BTreeSet};

    const NOW: u64 = 2_000_000_000;

    fn sign<T: Serialize>(signed: T, key_id: &str, key: &SigningKey) -> SignedMetadata<T> {
        let bytes = canonical_signed_bytes(&signed).unwrap();
        SignedMetadata {
            signed,
            signatures: vec![MetadataSignature {
                key_id: key_id.to_string(),
                signature: hex::encode(key.sign(&bytes).to_bytes()),
            }],
        }
    }

    fn trust_root(key: &SigningKey) -> SignedMetadata<RootMetadata> {
        let key_id = "registry-key".to_string();
        let roles = [
            RegistryRole::Root,
            RegistryRole::Timestamp,
            RegistryRole::Snapshot,
            RegistryRole::Targets,
        ]
        .into_iter()
        .map(|role| {
            (
                role,
                MetadataRole {
                    key_ids: BTreeSet::from([key_id.clone()]),
                    threshold: 1,
                },
            )
        })
        .collect();
        sign(
            RootMetadata {
                spec_version: REGISTRY_SPEC_VERSION.to_string(),
                version: 1,
                expires_unix: NOW + 10_000,
                keys: BTreeMap::from([(
                    key_id.clone(),
                    MetadataKey {
                        key_type: "ed25519".to_string(),
                        scheme: "ed25519".to_string(),
                        public_key: hex::encode(key.verifying_key().to_bytes()),
                    },
                )]),
                roles,
            },
            &key_id,
            key,
        )
    }

    fn cache(key: &SigningKey, expires_unix: u64) -> RegistryCache {
        let targets = sign(
            TargetsMetadata {
                spec_version: REGISTRY_SPEC_VERSION.to_string(),
                version: 1,
                expires_unix,
                targets: BTreeMap::new(),
            },
            "registry-key",
            key,
        );
        let targets_bytes = canonical_signed_envelope_bytes(&targets).unwrap();
        let snapshot = sign(
            SnapshotMetadata {
                spec_version: REGISTRY_SPEC_VERSION.to_string(),
                version: 1,
                expires_unix,
                targets: metadata_description(&targets).unwrap(),
            },
            "registry-key",
            key,
        );
        let snapshot_bytes = canonical_signed_envelope_bytes(&snapshot).unwrap();
        let timestamp = sign(
            TimestampMetadata {
                spec_version: REGISTRY_SPEC_VERSION.to_string(),
                version: 1,
                expires_unix,
                snapshot: metadata_description(&snapshot).unwrap(),
            },
            "registry-key",
            key,
        );
        RegistryCache {
            timestamp_hex: hex::encode(canonical_signed_envelope_bytes(&timestamp).unwrap()),
            snapshot_hex: hex::encode(snapshot_bytes),
            targets_hex: hex::encode(targets_bytes),
        }
    }

    #[test]
    fn registry_requires_https_explicit_trust_and_valid_offline_cache() {
        let temporary = tempfile::tempdir().unwrap();
        let manager = SignedRegistryManager::new(temporary.path().to_path_buf());
        assert!(manager.add("insecure", "http://example.invalid/").is_err());
        manager
            .add("official", "https://registry.example.invalid/")
            .unwrap();
        let key = SigningKey::from_bytes(&[11; 32]);
        let root = trust_root(&key);
        manager
            .trust(
                "official",
                &canonical_signed_envelope_bytes(&root).unwrap(),
                NOW,
            )
            .unwrap();

        let mut state = manager.load_state_with_recovery().unwrap();
        let registry = state.registries.get_mut("official").unwrap();
        registry.cache = Some(cache(&key, NOW + 1_000));
        manager.write_state(&state).unwrap();
        let refreshed = manager.refresh("official", true, NOW).unwrap();
        assert!(refreshed.offline);
        assert_eq!(refreshed.target_count, 0);
        assert_eq!(refreshed.versions.timestamp, 1);
    }

    #[test]
    fn expired_offline_cache_and_corrupt_primary_state_are_not_silently_trusted() {
        let temporary = tempfile::tempdir().unwrap();
        let manager = SignedRegistryManager::new(temporary.path().to_path_buf());
        manager
            .add("official", "https://registry.example.invalid/")
            .unwrap();
        let key = SigningKey::from_bytes(&[12; 32]);
        let root = trust_root(&key);
        manager
            .trust(
                "official",
                &canonical_signed_envelope_bytes(&root).unwrap(),
                NOW,
            )
            .unwrap();
        let mut state = manager.load_state_with_recovery().unwrap();
        state.registries.get_mut("official").unwrap().cache = Some(cache(&key, NOW));
        manager.write_state(&state).unwrap();
        assert!(matches!(
            manager.refresh("official", true, NOW),
            Err(RegistryError::Expired(_))
        ));

        fs::write(temporary.path().join("registries.json"), b"corrupt").unwrap();
        let recovered = manager.list().unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].name, "official");
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), RegistryError> {
    fs::rename(source, destination)?;
    File::open(destination.parent().ok_or_else(|| {
        RegistryError::InvalidMetadata("registry state has no parent".to_string())
    })?)?
    .sync_all()?;
    Ok(())
}
