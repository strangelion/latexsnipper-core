use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use latexsnipper_ast::CapabilityMatrix;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    PluginArtifactKindV3, PluginExecutionClassV3, PluginManifestV3, RegistryError, RegistryTarget,
    TrustState,
};

const REMOTE_INDEX_SCHEMA_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "plugin.json";

#[derive(Debug, Clone, Copy)]
pub struct RemotePackageLimits {
    pub compressed_bytes: u64,
    pub decompressed_bytes: u64,
    pub files: usize,
    pub single_file_bytes: u64,
}

impl Default for RemotePackageLimits {
    fn default() -> Self {
        Self {
            compressed_bytes: 64 * 1024 * 1024,
            decompressed_bytes: 128 * 1024 * 1024,
            files: 256,
            single_file_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifiedRemotePackage {
    pub manifest: PluginManifestV3,
    pub package_sha256: String,
    pub artifact_sha256: String,
    pub file_count: usize,
    pub decompressed_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePluginProvenance {
    pub registry_name: String,
    pub registry_origin: String,
    pub targets_version: u64,
    pub package_path: String,
    pub package_sha256: String,
    pub verified_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteInstalledPlugin {
    pub manifest: PluginManifestV3,
    pub trust_state: TrustState,
    pub enabled: bool,
    pub active: bool,
    pub provenance: RemotePluginProvenance,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteStoreDoctor {
    pub healthy: bool,
    pub recovered_staging_directories: usize,
    pub orphan_package_directories: Vec<String>,
    pub missing_package_directories: Vec<String>,
    pub quarantined_plugins: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemotePluginVersion {
    plugin: RemoteInstalledPlugin,
    relative_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemotePluginRecord {
    active_version: String,
    last_known_good_version: Option<String>,
    #[serde(default)]
    revoked: bool,
    versions: BTreeMap<String, RemotePluginVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoteStoreIndex {
    schema_version: u32,
    plugins: BTreeMap<String, RemotePluginRecord>,
}

impl Default for RemoteStoreIndex {
    fn default() -> Self {
        Self {
            schema_version: REMOTE_INDEX_SCHEMA_VERSION,
            plugins: BTreeMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct RemotePluginStore {
    root: PathBuf,
    limits: RemotePackageLimits,
}

impl RemotePluginStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            limits: RemotePackageLimits::default(),
        }
    }

    pub fn with_limits(root: PathBuf, limits: RemotePackageLimits) -> Self {
        Self { root, limits }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn install(
        &self,
        target: &RegistryTarget,
        package_bytes: &[u8],
        provenance: RemotePluginProvenance,
        core_version: &str,
    ) -> Result<RemoteInstalledPlugin, RegistryError> {
        validate_store_segment(&target.plugin_id)?;
        validate_store_segment(&target.version)?;
        fs::create_dir_all(&self.root)?;
        let lock = self.lock_exclusive()?;
        let mut index = self.load_index_with_recovery()?;
        if index
            .plugins
            .get(&target.plugin_id)
            .is_some_and(|record| record.revoked)
        {
            return Err(RegistryError::Revoked(target.plugin_id.clone()));
        }
        if provenance.registry_name.trim().is_empty()
            || !provenance.registry_origin.starts_with("https://")
            || provenance.registry_origin.contains('@')
            || provenance.targets_version == 0
            || provenance.package_path != target.package_path
            || !provenance
                .package_sha256
                .eq_ignore_ascii_case(&target.sha256)
        {
            return Err(RegistryError::InvalidMetadata(
                "remote package provenance does not match the signed target".to_string(),
            ));
        }
        let installed = index.plugins.get(&target.plugin_id).and_then(|record| {
            record.versions.get(&record.active_version).map(|version| {
                (
                    version.plugin.manifest.version.as_str(),
                    version.plugin.manifest.execution_class,
                )
            })
        });
        target.validate_for_remote(core_version, installed)?;

        let staging_root = self.root.join("staging");
        fs::create_dir_all(&staging_root)?;
        let staging = tempfile::Builder::new()
            .prefix("install-")
            .tempdir_in(&staging_root)?;
        let staged_package = staging.path().join("package");
        let first =
            extract_and_verify_remote_package(package_bytes, target, &staged_package, self.limits)?;
        let second = verify_staged_remote_package(
            &staged_package,
            target,
            &first.package_sha256,
            self.limits,
        )?;
        if serde_json::to_vec(&first)? != serde_json::to_vec(&second)? {
            return Err(RegistryError::InvalidMetadata(
                "staged package changed during verification".to_string(),
            ));
        }

        let destination = self
            .root
            .join("packages")
            .join(&target.plugin_id)
            .join(&target.version);
        if destination.exists() {
            let existing = verify_staged_remote_package(
                &destination,
                target,
                &first.package_sha256,
                self.limits,
            )?;
            if serde_json::to_vec(&existing)? != serde_json::to_vec(&first)? {
                return Err(RegistryError::InvalidMetadata(
                    "installed version directory conflicts with verified package".to_string(),
                ));
            }
        } else {
            let parent = destination.parent().ok_or_else(|| {
                RegistryError::InvalidMetadata("invalid package destination".to_string())
            })?;
            fs::create_dir_all(parent)?;
            fs::rename(&staged_package, &destination)?;
            sync_directory(parent)?;
        }

        let previous = index
            .plugins
            .get(&target.plugin_id)
            .map(|record| record.active_version.clone());
        let plugin = RemoteInstalledPlugin {
            manifest: first.manifest,
            trust_state: TrustState::VerifiedWasiComponent,
            enabled: false,
            active: true,
            provenance,
        };
        let record = index
            .plugins
            .entry(target.plugin_id.clone())
            .or_insert_with(|| RemotePluginRecord {
                active_version: target.version.clone(),
                last_known_good_version: None,
                revoked: false,
                versions: BTreeMap::new(),
            });
        if let Some(active) = previous.filter(|version| version != &target.version) {
            record.last_known_good_version = Some(active);
        }
        for version in record.versions.values_mut() {
            version.plugin.active = false;
        }
        record.active_version = target.version.clone();
        record.versions.insert(
            target.version.clone(),
            RemotePluginVersion {
                plugin: plugin.clone(),
                relative_directory: package_relative_directory(target),
            },
        );
        self.write_index(&index)?;
        FileExt::unlock(&lock)?;
        Ok(plugin)
    }

    pub fn list(&self) -> Result<Vec<RemoteInstalledPlugin>, RegistryError> {
        let index = self.load_index_with_recovery()?;
        Ok(index
            .plugins
            .values()
            .filter_map(|record| record.versions.get(&record.active_version))
            .map(|version| version.plugin.clone())
            .collect())
    }

    pub fn get(&self, id: &str) -> Result<Option<RemoteInstalledPlugin>, RegistryError> {
        let index = self.load_index_with_recovery()?;
        Ok(index.plugins.get(id).and_then(|record| {
            record
                .versions
                .get(&record.active_version)
                .map(|version| version.plugin.clone())
        }))
    }

    pub fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<RemoteInstalledPlugin, RegistryError> {
        if enabled {
            self.verify_installed(id)?;
        }
        let lock = self.lock_exclusive()?;
        let mut index = self.load_index_with_recovery()?;
        let record = index.plugins.get_mut(id).ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("plugin '{id}' is not installed"))
        })?;
        if record.revoked {
            return Err(RegistryError::Revoked(id.to_string()));
        }
        let version = record
            .versions
            .get_mut(&record.active_version)
            .ok_or_else(|| {
                RegistryError::InvalidMetadata("active remote package is absent".to_string())
            })?;
        if enabled && version.plugin.trust_state != TrustState::VerifiedWasiComponent {
            return Err(RegistryError::InvalidMetadata(
                "only verified WASI components can be enabled".to_string(),
            ));
        }
        version.plugin.enabled = enabled;
        let result = version.plugin.clone();
        self.write_index(&index)?;
        FileExt::unlock(&lock)?;
        Ok(result)
    }

    /// Register enabled, verified WASI format capabilities into an executable
    /// capability matrix. Built-in entries keep precedence.
    pub fn extend_capability_matrix(
        &self,
        matrix: &mut CapabilityMatrix,
    ) -> Result<usize, RegistryError> {
        let plugins = self.list()?;
        let mut added = 0usize;
        for plugin in plugins {
            if !plugin.enabled
                || !plugin.active
                || plugin.trust_state != TrustState::VerifiedWasiComponent
                || plugin.manifest.execution_class != PluginExecutionClassV3::WasiComponent
                || !plugin.manifest.permissions.registrations.capabilities
            {
                continue;
            }
            let verified = self.verify_installed(&plugin.manifest.id)?;
            for capability in &verified.manifest.format_capabilities {
                if !capability.available {
                    continue;
                }
                let importer = capability
                    .input
                    .as_deref()
                    .is_some_and(|input| !input.eq_ignore_ascii_case("ast"));
                let exporter = capability
                    .output
                    .as_deref()
                    .is_some_and(|output| !output.eq_ignore_ascii_case("ast"));
                if (importer && !verified.manifest.permissions.registrations.importers)
                    || (exporter && !verified.manifest.permissions.registrations.exporters)
                    || matrix.entries.iter().any(|existing| {
                        option_eq_ignore_ascii_case(&existing.input, &capability.input)
                            && option_eq_ignore_ascii_case(&existing.output, &capability.output)
                    })
                {
                    continue;
                }
                matrix.entries.push(capability.clone());
                added += 1;
            }
        }
        Ok(added)
    }

    pub fn verify_installed(&self, id: &str) -> Result<VerifiedRemotePackage, RegistryError> {
        let index = self.load_index_with_recovery()?;
        let record = index.plugins.get(id).ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("plugin '{id}' is not installed"))
        })?;
        let version = record.versions.get(&record.active_version).ok_or_else(|| {
            RegistryError::InvalidMetadata("active remote package is absent".to_string())
        })?;
        let target = installed_target(version);
        verify_staged_remote_package(
            &self.root.join(&version.relative_directory),
            &target,
            &version.plugin.provenance.package_sha256,
            self.limits,
        )
    }

    /// Return the verified package directory and its enabled index snapshot.
    /// The WASI host must verify the directory again while acquiring its
    /// handle-relative package view, then bind that result to this snapshot.
    pub fn enabled_package_directory(
        &self,
        id: &str,
    ) -> Result<(PathBuf, RemoteInstalledPlugin), RegistryError> {
        let index = self.load_index_with_recovery()?;
        let record = index.plugins.get(id).ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("plugin '{id}' is not installed"))
        })?;
        if record.revoked {
            return Err(RegistryError::Revoked(id.to_string()));
        }
        let version = record.versions.get(&record.active_version).ok_or_else(|| {
            RegistryError::InvalidMetadata("active remote package is absent".to_string())
        })?;
        if !version.plugin.active
            || !version.plugin.enabled
            || version.plugin.trust_state != TrustState::VerifiedWasiComponent
        {
            return Err(RegistryError::InvalidMetadata(format!(
                "plugin '{id}' is not an enabled verified WASI component"
            )));
        }
        let directory = self.root.join(&version.relative_directory);
        let target = installed_target(version);
        verify_staged_remote_package(
            &directory,
            &target,
            &version.plugin.provenance.package_sha256,
            self.limits,
        )?;
        Ok((directory, version.plugin.clone()))
    }

    pub fn rollback(&self, id: &str) -> Result<RemoteInstalledPlugin, RegistryError> {
        let lock = self.lock_exclusive()?;
        let mut index = self.load_index_with_recovery()?;
        let record = index.plugins.get_mut(id).ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("plugin '{id}' is not installed"))
        })?;
        if record.revoked {
            return Err(RegistryError::Revoked(id.to_string()));
        }
        let rollback = record.last_known_good_version.clone().ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("plugin '{id}' has no last-known-good version"))
        })?;
        let current = record.active_version.clone();
        let target = record.versions.get_mut(&rollback).ok_or_else(|| {
            RegistryError::InvalidMetadata(
                "last-known-good package is missing from index".to_string(),
            )
        })?;
        let directory = self.root.join(&target.relative_directory);
        if !directory.is_dir() {
            return Err(RegistryError::InvalidMetadata(
                "last-known-good package directory is missing".to_string(),
            ));
        }
        target.plugin.active = true;
        let plugin = target.plugin.clone();
        if let Some(current_version) = record.versions.get_mut(&current) {
            current_version.plugin.active = false;
        }
        record.active_version = rollback;
        record.last_known_good_version = Some(current);
        self.write_index(&index)?;
        FileExt::unlock(&lock)?;
        Ok(plugin)
    }

    pub fn revoke(&self, id: &str) -> Result<RemoteInstalledPlugin, RegistryError> {
        let lock = self.lock_exclusive()?;
        let mut index = self.load_index_with_recovery()?;
        let record = index.plugins.get_mut(id).ok_or_else(|| {
            RegistryError::InvalidMetadata(format!("plugin '{id}' is not installed"))
        })?;
        record.revoked = true;
        let plugin = record
            .versions
            .get_mut(&record.active_version)
            .ok_or_else(|| {
                RegistryError::InvalidMetadata("active package is absent".to_string())
            })?;
        plugin.plugin.trust_state = TrustState::Revoked;
        plugin.plugin.enabled = false;
        let result = plugin.plugin.clone();
        self.write_index(&index)?;
        FileExt::unlock(&lock)?;
        Ok(result)
    }

    pub fn doctor(&self) -> Result<RemoteStoreDoctor, RegistryError> {
        fs::create_dir_all(&self.root)?;
        let lock = self.lock_exclusive()?;
        let recovered = remove_staging_directories(&self.root.join("staging"))?;
        let mut index = self.load_index_with_recovery()?;
        let mut doctor = RemoteStoreDoctor {
            healthy: true,
            recovered_staging_directories: recovered,
            ..RemoteStoreDoctor::default()
        };
        let mut indexed = BTreeSet::new();
        for (id, record) in &mut index.plugins {
            for (version, entry) in &mut record.versions {
                indexed.insert(entry.relative_directory.clone());
                let package_directory = self.root.join(&entry.relative_directory);
                let verification_failed = if package_directory.is_dir() {
                    verify_staged_remote_package(
                        &package_directory,
                        &installed_target(entry),
                        &entry.plugin.provenance.package_sha256,
                        self.limits,
                    )
                    .is_err()
                } else {
                    true
                };
                if verification_failed {
                    doctor
                        .missing_package_directories
                        .push(format!("{id}@{version}"));
                    entry.plugin.trust_state = TrustState::Quarantined;
                    entry.plugin.enabled = false;
                    doctor.quarantined_plugins.push(id.clone());
                    doctor.healthy = false;
                }
            }
        }
        let packages = self.root.join("packages");
        if packages.is_dir() {
            for id_entry in fs::read_dir(&packages)? {
                let id_entry = id_entry?;
                if !id_entry.file_type()?.is_dir() {
                    continue;
                }
                for version_entry in fs::read_dir(id_entry.path())? {
                    let version_entry = version_entry?;
                    if version_entry.file_type()?.is_dir() {
                        let relative = path_to_slash(
                            version_entry.path().strip_prefix(&self.root).map_err(|_| {
                                RegistryError::InvalidMetadata(
                                    "package escaped store root".to_string(),
                                )
                            })?,
                        )?;
                        if !indexed.contains(&relative) {
                            doctor.orphan_package_directories.push(relative);
                            doctor.healthy = false;
                        }
                    }
                }
            }
        }
        if !doctor.quarantined_plugins.is_empty() {
            self.write_index(&index)?;
        }
        FileExt::unlock(&lock)?;
        Ok(doctor)
    }

    fn lock_exclusive(&self) -> Result<File, RegistryError> {
        fs::create_dir_all(&self.root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("remote-store.lock"))?;
        lock.lock_exclusive()?;
        Ok(lock)
    }

    fn load_index_with_recovery(&self) -> Result<RemoteStoreIndex, RegistryError> {
        let path = self.root.join("remote-index.json");
        if !path.exists() {
            return Ok(RemoteStoreIndex::default());
        }
        match read_index(&path) {
            Ok(index) => Ok(index),
            Err(primary) => {
                let backup = self.root.join("remote-index.json.bak");
                if backup.exists() {
                    read_index(&backup)
                } else {
                    Err(primary)
                }
            }
        }
    }

    fn write_index(&self, index: &RemoteStoreIndex) -> Result<(), RegistryError> {
        let bytes = serde_json::to_vec_pretty(index)?;
        let path = self.root.join("remote-index.json");
        let backup = self.root.join("remote-index.json.bak");
        if path.exists() {
            fs::copy(&path, &backup)?;
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&backup)?
                .sync_all()?;
        }
        let temporary = self.root.join("remote-index.json.tmp");
        {
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        replace_file(&temporary, &path)?;
        sync_directory(&self.root)?;
        Ok(())
    }
}

fn option_eq_ignore_ascii_case(left: &Option<String>, right: &Option<String>) -> bool {
    match (left.as_deref(), right.as_deref()) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        (None, None) => true,
        _ => false,
    }
}

pub fn extract_and_verify_remote_package(
    package_bytes: &[u8],
    target: &RegistryTarget,
    destination: &Path,
    limits: RemotePackageLimits,
) -> Result<VerifiedRemotePackage, RegistryError> {
    if package_bytes.len() as u64 != target.length {
        return Err(RegistryError::LengthMismatch);
    }
    if package_bytes.len() as u64 > limits.compressed_bytes {
        return Err(RegistryError::InvalidMetadata(
            "compressed package exceeds configured limit".to_string(),
        ));
    }
    let package_sha256 = hex::encode(Sha256::digest(package_bytes));
    if !package_sha256.eq_ignore_ascii_case(&target.sha256) {
        return Err(RegistryError::DigestMismatch);
    }
    if destination.exists() {
        return Err(RegistryError::InvalidMetadata(
            "staging destination already exists".to_string(),
        ));
    }
    fs::create_dir_all(destination)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(package_bytes))?;
    if archive.len() > limits.files {
        return Err(RegistryError::InvalidMetadata(
            "package contains too many files".to_string(),
        ));
    }
    let mut names = BTreeSet::new();
    let mut decompressed = 0_u64;
    let mut extracted_files = 0_usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        let relative = secure_archive_path(&name)?;
        if !names.insert(path_to_slash(&relative)?) {
            return Err(RegistryError::InvalidMetadata(
                "package contains duplicate paths".to_string(),
            ));
        }
        if is_symlink(entry.unix_mode()) {
            return Err(RegistryError::InvalidMetadata(
                "package symlinks are forbidden".to_string(),
            ));
        }
        if entry.size() > limits.single_file_bytes {
            return Err(RegistryError::InvalidMetadata(
                "package member exceeds configured limit".to_string(),
            ));
        }
        decompressed = decompressed
            .checked_add(entry.size())
            .ok_or_else(|| RegistryError::InvalidMetadata("package size overflow".to_string()))?;
        if decompressed > limits.decompressed_bytes {
            return Err(RegistryError::InvalidMetadata(
                "decompressed package exceeds configured limit".to_string(),
            ));
        }
        let output = destination.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }
        let parent = output.parent().ok_or_else(|| {
            RegistryError::InvalidMetadata("archive member has no parent".to_string())
        })?;
        fs::create_dir_all(parent)?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&output)?;
        let copied = io::copy(
            &mut entry
                .by_ref()
                .take(limits.single_file_bytes.saturating_add(1)),
            &mut file,
        )?;
        if copied != entry.size() || copied > limits.single_file_bytes {
            return Err(RegistryError::InvalidMetadata(
                "archive member size changed while extracting".to_string(),
            ));
        }
        file.sync_all()?;
        extracted_files += 1;
    }
    sync_tree_directories(destination)?;
    verify_staged_remote_package_with_counts(
        destination,
        target,
        &package_sha256,
        limits,
        Some((extracted_files, decompressed)),
    )
}

pub fn verify_staged_remote_package(
    package_directory: &Path,
    target: &RegistryTarget,
    package_sha256: &str,
    limits: RemotePackageLimits,
) -> Result<VerifiedRemotePackage, RegistryError> {
    verify_staged_remote_package_with_counts(
        package_directory,
        target,
        package_sha256,
        limits,
        None,
    )
}

fn verify_staged_remote_package_with_counts(
    package_directory: &Path,
    target: &RegistryTarget,
    package_sha256: &str,
    limits: RemotePackageLimits,
    known_counts: Option<(usize, u64)>,
) -> Result<VerifiedRemotePackage, RegistryError> {
    let manifest_path = package_directory.join(MANIFEST_FILE);
    let manifest_bytes = read_bounded_file(&manifest_path, 1024 * 1024)?;
    let manifest: PluginManifestV3 = serde_json::from_slice(&manifest_bytes)?;
    manifest
        .validate_contract()
        .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))?;
    if manifest.id != target.plugin_id
        || manifest.version != target.version
        || manifest.core_version_requirement != target.core_version_requirement
        || manifest.execution_class != target.execution_class
        || manifest.execution_class != PluginExecutionClassV3::WasiComponent
    {
        return Err(RegistryError::RemoteExecutionClass);
    }
    let artifact = manifest.artifact.as_ref().ok_or_else(|| {
        RegistryError::InvalidMetadata("remote manifest omits artifact".to_string())
    })?;
    if artifact.kind != PluginArtifactKindV3::WasiComponent {
        return Err(RegistryError::RemoteExecutionClass);
    }
    let artifact_path = secure_archive_path(&artifact.path)?;
    let artifact_bytes = read_bounded_file(
        &package_directory.join(&artifact_path),
        limits.single_file_bytes,
    )?;
    if !wasmparser::Parser::is_component(&artifact_bytes)
        || wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
            .validate_all(&artifact_bytes)
            .is_err()
    {
        return Err(RegistryError::RemoteExecutionClass);
    }
    if artifact
        .size_bytes
        .is_some_and(|size| size != artifact_bytes.len() as u64)
    {
        return Err(RegistryError::LengthMismatch);
    }
    let artifact_sha256 = hex::encode(Sha256::digest(&artifact_bytes));
    if !artifact_sha256.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(RegistryError::DigestMismatch);
    }
    let (file_count, decompressed_bytes) = match known_counts {
        Some(counts) => counts,
        None => count_staged_tree(package_directory, limits)?,
    };
    Ok(VerifiedRemotePackage {
        manifest,
        package_sha256: package_sha256.to_ascii_lowercase(),
        artifact_sha256,
        file_count,
        decompressed_bytes,
    })
}

fn count_staged_tree(
    root: &Path,
    limits: RemotePackageLimits,
) -> Result<(usize, u64), RegistryError> {
    let canonical_root = fs::canonicalize(root)?;
    let mut pending = vec![root.to_path_buf()];
    let mut files = 0_usize;
    let mut bytes = 0_u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(RegistryError::InvalidMetadata(
                    "staged package contains a symlink".to_string(),
                ));
            }
            let canonical = fs::canonicalize(entry.path())?;
            if !canonical.starts_with(&canonical_root) {
                return Err(RegistryError::InvalidMetadata(
                    "staged package escaped its root".to_string(),
                ));
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                files = files.checked_add(1).ok_or_else(|| {
                    RegistryError::InvalidMetadata("file-count overflow".to_string())
                })?;
                let length = entry.metadata()?.len();
                if length > limits.single_file_bytes {
                    return Err(RegistryError::InvalidMetadata(
                        "staged file exceeds configured limit".to_string(),
                    ));
                }
                bytes = bytes.checked_add(length).ok_or_else(|| {
                    RegistryError::InvalidMetadata("package-size overflow".to_string())
                })?;
                if files > limits.files || bytes > limits.decompressed_bytes {
                    return Err(RegistryError::InvalidMetadata(
                        "staged package exceeds configured limits".to_string(),
                    ));
                }
            } else {
                return Err(RegistryError::InvalidMetadata(
                    "staged package contains a special file".to_string(),
                ));
            }
        }
    }
    Ok((files, bytes))
}

fn secure_archive_path(value: &str) -> Result<PathBuf, RegistryError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.contains('\0')
    {
        return Err(RegistryError::InvalidMetadata(
            "archive contains an unsafe path".to_string(),
        ));
    }
    let path = PathBuf::from(value);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RegistryError::InvalidMetadata(
            "archive contains path traversal".to_string(),
        ));
    }
    Ok(path)
}

fn is_symlink(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| mode & 0o170000 == 0o120000)
}

fn read_bounded_file(path: &Path, maximum: u64) -> Result<Vec<u8>, RegistryError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > maximum {
        return Err(RegistryError::InvalidMetadata(format!(
            "invalid or oversized package file {}",
            path.display()
        )));
    }
    let mut file = File::open(path)?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::io::Read::by_ref(&mut file)
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(RegistryError::LengthMismatch);
    }
    Ok(bytes)
}

fn validate_store_segment(value: &str) -> Result<(), RegistryError> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(RegistryError::InvalidMetadata(
            "plugin ID or version is unsafe for local storage".to_string(),
        ));
    }
    Ok(())
}

fn package_relative_directory(target: &RegistryTarget) -> String {
    format!("packages/{}/{}", target.plugin_id, target.version)
}

fn installed_target(version: &RemotePluginVersion) -> RegistryTarget {
    RegistryTarget {
        plugin_id: version.plugin.manifest.id.clone(),
        version: version.plugin.manifest.version.clone(),
        package_path: version.plugin.provenance.package_path.clone(),
        length: 1,
        sha256: version.plugin.provenance.package_sha256.clone(),
        execution_class: version.plugin.manifest.execution_class,
        core_version_requirement: version.plugin.manifest.core_version_requirement.clone(),
        revoked: version.plugin.trust_state == TrustState::Revoked,
        revocation_reason: None,
    }
}

fn path_to_slash(path: &Path) -> Result<String, RegistryError> {
    let mut output = Vec::new();
    for component in path.components() {
        if let Component::Normal(value) = component {
            output.push(value.to_string_lossy().into_owned());
        } else {
            return Err(RegistryError::InvalidMetadata(
                "path is not a relative package path".to_string(),
            ));
        }
    }
    Ok(output.join("/"))
}

fn read_index(path: &Path) -> Result<RemoteStoreIndex, RegistryError> {
    let bytes = read_bounded_file(path, 8 * 1024 * 1024)?;
    let index: RemoteStoreIndex = serde_json::from_slice(&bytes)?;
    if index.schema_version != REMOTE_INDEX_SCHEMA_VERSION {
        return Err(RegistryError::InvalidMetadata(
            "unsupported remote store index schema".to_string(),
        ));
    }
    Ok(index)
}

fn remove_staging_directories(path: &Path) -> Result<usize, RegistryError> {
    if !path.is_dir() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
            removed += 1;
        } else {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn sync_tree_directories(root: &Path) -> Result<(), RegistryError> {
    let mut directories = vec![root.to_path_buf()];
    let mut index = 0;
    while index < directories.len() {
        for entry in fs::read_dir(&directories[index])? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                directories.push(entry.path());
            }
        }
        index += 1;
    }
    for directory in directories.iter().rev() {
        sync_directory(directory)?;
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), RegistryError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> Result<(), RegistryError> {
    if !fs::metadata(path)?.is_dir() {
        return Err(RegistryError::InvalidMetadata(
            "directory durability target is not a directory".to_string(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> Result<(), RegistryError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let mut source_wide: Vec<u16> = source.as_os_str().encode_wide().collect();
    source_wide.push(0);
    let mut destination_wide: Vec<u16> = destination.as_os_str().encode_wide().collect();
    destination_wide.push(0);
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(RegistryError::InvalidMetadata(format!(
            "atomic index replacement failed: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> Result<(), RegistryError> {
    fs::rename(source, destination)?;
    Ok(())
}

pub fn current_unix_time() -> Result<u64, RegistryError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| RegistryError::InvalidMetadata(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PluginArtifactV3, PluginInterfaceVersionsV3, PluginPermissionsV3,
        PluginRegistrationGrantsV3, PluginResourceLimitsV3, COMPONENT_WIT_VERSION_V1,
        PLUGIN_API_VERSION_FOR_MANIFEST_V3, PLUGIN_MANIFEST_SCHEMA_VERSION_V3,
    };
    use latexsnipper_ast::{FidelityDimensions, FidelityLevel, FormatCapability};
    use std::sync::Arc;
    use zip::write::FileOptions;

    fn component() -> Vec<u8> {
        wat::parse_str("(component)").unwrap()
    }

    fn manifest(version: &str, artifact: &[u8]) -> PluginManifestV3 {
        PluginManifestV3 {
            schema_version: PLUGIN_MANIFEST_SCHEMA_VERSION_V3,
            id: "example.plugin".to_string(),
            name: "Example".to_string(),
            version: version.to_string(),
            core_version_requirement: ">=3.0.0-alpha.1, <4".to_string(),
            execution_class: PluginExecutionClassV3::WasiComponent,
            interfaces: PluginInterfaceVersionsV3 {
                plugin_api: PLUGIN_API_VERSION_FOR_MANIFEST_V3,
                process_ipc: None,
                component_wit: Some(COMPONENT_WIT_VERSION_V1),
            },
            capabilities: Vec::new(),
            format_capabilities: vec![FormatCapability {
                input: Some("fixture".to_string()),
                output: Some("AST".to_string()),
                available: true,
                supports_formula: true,
                supports_table: false,
                supports_image: false,
                supports_svg: false,
                supports_style: false,
                supports_layout: false,
                supports_office_objects: false,
                fidelity: FidelityLevel::SemanticOnly,
                fidelity_dimensions: FidelityDimensions::default(),
                known_loss: Vec::new(),
                notes: Vec::new(),
                required_features: Vec::new(),
                external_dependencies: Vec::new(),
                platform_restrictions: Vec::new(),
                experimental: true,
            }],
            hooks: Vec::new(),
            priority: 0,
            dependencies: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            permissions: PluginPermissionsV3 {
                paths: Vec::new(),
                network: Vec::new(),
                environment_variables: Vec::new(),
                model_artifacts: Vec::new(),
                temporary_directory: false,
                clocks: false,
                randomness: false,
                registrations: PluginRegistrationGrantsV3 {
                    capabilities: true,
                    importers: true,
                    exporters: false,
                    runtimes: false,
                },
                limits: PluginResourceLimitsV3 {
                    timeout_millis: Some(1_000),
                    memory_bytes: Some(16 * 1024 * 1024),
                    input_bytes: Some(1024),
                    output_bytes: Some(1024),
                    diagnostic_count: Some(8),
                    diagnostic_bytes: Some(4096),
                    model_artifact_bytes: Some(0),
                    temporary_storage_bytes: Some(0),
                    table_elements: Some(128),
                    resources: Some(128),
                    fuel: Some(1_000_000),
                    max_concurrent_executions: 1,
                },
            },
            platforms: Vec::new(),
            architectures: Vec::new(),
            license: Some("Apache-2.0".to_string()),
            artifact: Some(PluginArtifactV3 {
                path: "component.wasm".to_string(),
                kind: PluginArtifactKindV3::WasiComponent,
                sha256: hex::encode(Sha256::digest(artifact)),
                size_bytes: Some(artifact.len() as u64),
            }),
            signature: None,
            provenance: None,
            configuration_schema: None,
        }
    }

    fn package(version: &str) -> Vec<u8> {
        package_with_extra(version, None)
    }

    fn package_with_extra(version: &str, extra: Option<(&str, bool)>) -> Vec<u8> {
        package_with_artifact(version, &component(), extra)
    }

    fn package_with_artifact(
        version: &str,
        artifact: &[u8],
        extra: Option<(&str, bool)>,
    ) -> Vec<u8> {
        let manifest = serde_json::to_vec_pretty(&manifest(version, artifact)).unwrap();
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            let options = FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644);
            archive.start_file(MANIFEST_FILE, options).unwrap();
            archive.write_all(&manifest).unwrap();
            archive.start_file("component.wasm", options).unwrap();
            archive.write_all(artifact).unwrap();
            if let Some((name, symlink)) = extra {
                if symlink {
                    archive
                        .add_symlink(name, "component.wasm", FileOptions::default())
                        .unwrap();
                } else {
                    archive.start_file(name, options).unwrap();
                    archive.write_all(b"malicious").unwrap();
                }
            }
            archive.finish().unwrap();
        }
        output.into_inner()
    }

    fn target(version: &str, package: &[u8]) -> RegistryTarget {
        RegistryTarget {
            plugin_id: "example.plugin".to_string(),
            version: version.to_string(),
            package_path: format!("packages/example.plugin-{version}.zip"),
            length: package.len() as u64,
            sha256: hex::encode(Sha256::digest(package)),
            execution_class: PluginExecutionClassV3::WasiComponent,
            core_version_requirement: ">=3.0.0-alpha.1, <4".to_string(),
            revoked: false,
            revocation_reason: None,
        }
    }

    fn provenance(target: &RegistryTarget) -> RemotePluginProvenance {
        RemotePluginProvenance {
            registry_name: "test".to_string(),
            registry_origin: "https://registry.example.invalid".to_string(),
            targets_version: 1,
            package_path: target.package_path.clone(),
            package_sha256: target.sha256.clone(),
            verified_at_unix: 1_900_000_000,
        }
    }

    #[test]
    fn valid_package_is_extracted_and_reverified() {
        let bytes = package("1.0.0");
        let target = target("1.0.0", &bytes);
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("staged");
        let verified = extract_and_verify_remote_package(
            &bytes,
            &target,
            &destination,
            RemotePackageLimits::default(),
        )
        .unwrap();
        assert_eq!(verified.manifest.id, "example.plugin");
        assert_eq!(verified.file_count, 2);
        let reverified = verify_staged_remote_package(
            &destination,
            &target,
            &verified.package_sha256,
            RemotePackageLimits::default(),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_vec(&verified).unwrap(),
            serde_json::to_vec(&reverified).unwrap()
        );
    }

    #[test]
    fn only_enabled_verified_wasi_plugins_extend_runtime_capabilities() {
        let bytes = package("1.0.0");
        let target = target("1.0.0", &bytes);
        let temporary = tempfile::tempdir().unwrap();
        let store = RemotePluginStore::new(temporary.path().join("remote"));
        store
            .install(&target, &bytes, provenance(&target), "3.0.0-alpha.1")
            .unwrap();
        let mut matrix = CapabilityMatrix {
            schema_version: "3.0.0".to_string(),
            entries: Vec::new(),
        };
        assert_eq!(store.extend_capability_matrix(&mut matrix).unwrap(), 0);

        store.set_enabled("example.plugin", true).unwrap();
        assert_eq!(store.extend_capability_matrix(&mut matrix).unwrap(), 1);
        assert_eq!(matrix.entries[0].input.as_deref(), Some("fixture"));

        store.set_enabled("example.plugin", false).unwrap();
        let mut empty = CapabilityMatrix {
            schema_version: "3.0.0".to_string(),
            entries: Vec::new(),
        };
        assert_eq!(store.extend_capability_matrix(&mut empty).unwrap(), 0);
    }

    #[test]
    fn digest_oversize_traversal_and_symlink_are_rejected() {
        let bytes = package("1.0.0");
        let mut wrong_digest = target("1.0.0", &bytes);
        wrong_digest.sha256 = "0".repeat(64);
        let temporary = tempfile::tempdir().unwrap();
        assert!(matches!(
            extract_and_verify_remote_package(
                &bytes,
                &wrong_digest,
                &temporary.path().join("digest"),
                RemotePackageLimits::default()
            ),
            Err(RegistryError::DigestMismatch)
        ));

        let limits = RemotePackageLimits {
            compressed_bytes: 1,
            ..RemotePackageLimits::default()
        };
        assert!(extract_and_verify_remote_package(
            &bytes,
            &target("1.0.0", &bytes),
            &temporary.path().join("oversize"),
            limits
        )
        .is_err());

        let traversal = package_with_extra("1.0.0", Some(("../escape", false)));
        assert!(extract_and_verify_remote_package(
            &traversal,
            &target("1.0.0", &traversal),
            &temporary.path().join("traversal"),
            RemotePackageLimits::default()
        )
        .is_err());

        let symlink = package_with_extra("1.0.0", Some(("component-link", true)));
        assert!(extract_and_verify_remote_package(
            &symlink,
            &target("1.0.0", &symlink),
            &temporary.path().join("symlink"),
            RemotePackageLimits::default()
        )
        .is_err());

        let native_substitution = package_with_artifact("1.0.0", b"MZ-native", None);
        assert!(matches!(
            extract_and_verify_remote_package(
                &native_substitution,
                &target("1.0.0", &native_substitution),
                &temporary.path().join("native"),
                RemotePackageLimits::default()
            ),
            Err(RegistryError::RemoteExecutionClass)
        ));
    }

    #[test]
    fn install_update_and_last_known_good_rollback_are_atomic_in_index() {
        let temporary = tempfile::tempdir().unwrap();
        let store = RemotePluginStore::new(temporary.path().to_path_buf());
        let first = package("1.0.0");
        let first_target = target("1.0.0", &first);
        store
            .install(
                &first_target,
                &first,
                provenance(&first_target),
                "3.0.0-alpha.1",
            )
            .unwrap();

        let second = package("2.0.0");
        let second_target = target("2.0.0", &second);
        store
            .install(
                &second_target,
                &second,
                provenance(&second_target),
                "3.0.0-alpha.1",
            )
            .unwrap();
        assert_eq!(
            store
                .get("example.plugin")
                .unwrap()
                .unwrap()
                .manifest
                .version,
            "2.0.0"
        );

        assert!(matches!(
            store.install(
                &first_target,
                &first,
                provenance(&first_target),
                "3.0.0-alpha.1"
            ),
            Err(RegistryError::VersionDowngrade { .. })
        ));

        let rolled_back = store.rollback("example.plugin").unwrap();
        assert_eq!(rolled_back.manifest.version, "1.0.0");
        assert_eq!(
            store
                .get("example.plugin")
                .unwrap()
                .unwrap()
                .manifest
                .version,
            "1.0.0"
        );
    }

    #[test]
    fn interrupted_staging_corrupt_primary_index_and_revocation_are_recovered() {
        let temporary = tempfile::tempdir().unwrap();
        let store = RemotePluginStore::new(temporary.path().to_path_buf());
        let first = package("1.0.0");
        let first_target = target("1.0.0", &first);
        store
            .install(
                &first_target,
                &first,
                provenance(&first_target),
                "3.0.0-alpha.1",
            )
            .unwrap();
        let second = package("2.0.0");
        let second_target = target("2.0.0", &second);
        store
            .install(
                &second_target,
                &second,
                provenance(&second_target),
                "3.0.0-alpha.1",
            )
            .unwrap();
        let interrupted = temporary.path().join("staging").join("install-interrupted");
        fs::create_dir_all(&interrupted).unwrap();
        fs::write(interrupted.join("partial"), b"partial").unwrap();
        fs::write(temporary.path().join("remote-index.json"), b"corrupt").unwrap();

        let recovered = store.list().unwrap();
        assert_eq!(recovered[0].manifest.version, "1.0.0");
        let doctor = store.doctor().unwrap();
        assert_eq!(doctor.recovered_staging_directories, 1);
        let revoked = store.revoke("example.plugin").unwrap();
        assert_eq!(revoked.trust_state, TrustState::Revoked);
        assert!(!revoked.enabled);
        assert!(matches!(
            store.install(
                &second_target,
                &second,
                provenance(&second_target),
                "3.0.0-alpha.1"
            ),
            Err(RegistryError::Revoked(id)) if id == "example.plugin"
        ));
        assert!(matches!(
            store.rollback("example.plugin"),
            Err(RegistryError::Revoked(id)) if id == "example.plugin"
        ));
    }

    #[test]
    fn concurrent_install_is_serialized_by_cross_process_lock() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Arc::new(RemotePluginStore::new(temporary.path().to_path_buf()));
        let bytes = Arc::new(package("1.0.0"));
        let target = Arc::new(target("1.0.0", &bytes));
        let mut threads = Vec::new();
        for _ in 0..4 {
            let store = Arc::clone(&store);
            let bytes = Arc::clone(&bytes);
            let target = Arc::clone(&target);
            threads.push(std::thread::spawn(move || {
                store.install(&target, &bytes, provenance(&target), "3.0.0-alpha.1")
            }));
        }
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
        assert_eq!(store.list().unwrap().len(), 1);
        assert!(store.doctor().unwrap().healthy);
    }
}
