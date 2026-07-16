use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use latexsnipper_foundation::{Result, SnipperError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::manifest::legacy_core_requirement_matches;
use crate::{
    EffectivePermissionSummary, EffectivePluginPermissions, IsolatedProcessHost,
    IsolatedProcessLimits, IsolatedProcessResult, LoadedPluginManifest, PluginClass, PluginHook,
    PluginManifest, PluginRequest, PLUGIN_ABI_VERSION, PLUGIN_API_VERSION,
};

const MAX_PACKAGE_FILES: usize = 256;
const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ENTRYPOINT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub enabled: bool,
    pub source: String,
    pub installed_at_unix_seconds: u64,
    pub verified_entrypoint_sha256: String,
}

#[derive(Debug, Clone)]
pub struct PluginVerification {
    pub manifest: PluginManifest,
    pub package_root: PathBuf,
    pub manifest_path: PathBuf,
    pub entrypoint_path: PathBuf,
    pub entrypoint_sha256: String,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreIndex {
    schema_version: u32,
    plugins: BTreeMap<String, InstalledPlugin>,
}

impl Default for StoreIndex {
    fn default() -> Self {
        Self {
            schema_version: 1,
            plugins: BTreeMap::new(),
        }
    }
}

/// A deterministic on-disk plugin store. Installation never executes code.
#[derive(Debug, Clone)]
pub struct PluginStore {
    root: PathBuf,
}

impl PluginStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn verify_package(&self, source: &Path) -> Result<PluginVerification> {
        let (package_root, manifest_path) = locate_manifest(source)?;
        let manifest_bytes = read_limited(&manifest_path, 1024 * 1024)?;
        let manifest = match LoadedPluginManifest::parse_json(&manifest_bytes)
            .map_err(|error| plugin_error(format!("Invalid plugin manifest: {error}")))?
        {
            LoadedPluginManifest::V2(manifest) => *manifest,
            LoadedPluginManifest::V3(manifest) => {
                return Err(plugin_error(format!(
                    "Plugin manifest v3 '{}' must be installed through the signed registry/WASI path; the legacy local process store cannot enforce v3 permissions",
                    manifest.id
                )))
            }
        };
        validate_external_manifest(&manifest)?;

        let entrypoint = manifest
            .entrypoint
            .as_deref()
            .ok_or_else(|| plugin_error("External plugin manifest requires entrypoint"))?;
        let entrypoint_path = resolve_package_path(&package_root, entrypoint)?;
        let entrypoint_bytes = read_limited(&entrypoint_path, MAX_ENTRYPOINT_BYTES)?;
        let actual = hex::encode(Sha256::digest(&entrypoint_bytes));
        let expected = manifest
            .checksum_sha256
            .as_deref()
            .ok_or_else(|| plugin_error("External plugin manifest requires checksumSha256"))?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(plugin_error(format!(
                "Plugin entrypoint checksum mismatch: expected {expected}, found {actual}"
            )));
        }

        let (file_count, total_bytes) = inspect_package(&package_root)?;
        Ok(PluginVerification {
            manifest,
            package_root,
            manifest_path,
            entrypoint_path,
            entrypoint_sha256: actual,
            file_count,
            total_bytes,
        })
    }

    pub fn install(&self, source: &Path) -> Result<InstalledPlugin> {
        let verified = self.verify_package(source)?;
        let _lock = self.lock_exclusive()?;
        std::fs::create_dir_all(self.packages_dir())
            .map_err(|error| plugin_error(error.to_string()))?;
        let package_root = verified
            .package_root
            .canonicalize()
            .map_err(|error| plugin_error(error.to_string()))?;
        let packages_root = self
            .packages_dir()
            .canonicalize()
            .map_err(|error| plugin_error(error.to_string()))?;
        if packages_root.starts_with(&package_root) {
            return Err(plugin_error(
                "Plugin store may not be nested inside the source package",
            ));
        }
        let destination = self.packages_dir().join(&verified.manifest.id);
        if destination.exists() {
            return Err(plugin_error(format!(
                "Plugin '{}' is already installed",
                verified.manifest.id
            )));
        }
        let staging = self.packages_dir().join(format!(
            ".install-{}-{}-{}",
            verified.manifest.id,
            std::process::id(),
            unique_suffix()
        ));
        copy_package(&verified.package_root, &staging)?;
        let staged_verification = self.verify_package(&staging)?;
        if staged_verification.manifest.id != verified.manifest.id
            || staged_verification.entrypoint_sha256 != verified.entrypoint_sha256
        {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(plugin_error(
                "Staged plugin verification changed unexpectedly",
            ));
        }
        std::fs::rename(&staging, &destination).map_err(|error| {
            let _ = std::fs::remove_dir_all(&staging);
            plugin_error(format!("Could not activate plugin package: {error}"))
        })?;

        let installed = InstalledPlugin {
            manifest: verified.manifest,
            enabled: false,
            source: "<local-package>".to_string(),
            installed_at_unix_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            verified_entrypoint_sha256: verified.entrypoint_sha256,
        };
        let mut index = self.load_index_unlocked()?;
        index
            .plugins
            .insert(installed.manifest.id.clone(), installed.clone());
        if let Err(error) = self.save_index_unlocked(&index) {
            let _ = std::fs::remove_dir_all(&destination);
            return Err(error);
        }
        Ok(installed)
    }

    pub fn list(&self) -> Result<Vec<InstalledPlugin>> {
        let _lock = self.lock_shared()?;
        Ok(self.load_index_unlocked()?.plugins.into_values().collect())
    }

    pub fn get(&self, id: &str) -> Result<Option<InstalledPlugin>> {
        validate_id(id)?;
        let _lock = self.lock_shared()?;
        Ok(self.load_index_unlocked()?.plugins.get(id).cloned())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<InstalledPlugin> {
        validate_id(id)?;
        let _lock = self.lock_exclusive()?;
        let mut index = self.load_index_unlocked()?;
        let plugin = index
            .plugins
            .get_mut(id)
            .ok_or_else(|| plugin_error(format!("Plugin '{id}' is not installed")))?;
        plugin.enabled = enabled;
        let result = plugin.clone();
        self.save_index_unlocked(&index)?;
        Ok(result)
    }

    pub fn uninstall(&self, id: &str) -> Result<()> {
        validate_id(id)?;
        let _lock = self.lock_exclusive()?;
        let mut index = self.load_index_unlocked()?;
        if !index.plugins.contains_key(id) {
            return Err(plugin_error(format!("Plugin '{id}' is not installed")));
        }
        let package = self.packages_dir().join(id);
        if package.exists() {
            validate_existing_child(&self.packages_dir(), &package)?;
            std::fs::remove_dir_all(&package)
                .map_err(|error| plugin_error(format!("Could not remove plugin: {error}")))?;
        }
        index.plugins.remove(id);
        self.save_index_unlocked(&index)
    }

    pub fn verify_installed(&self, id: &str) -> Result<PluginVerification> {
        validate_id(id)?;
        if self.get(id)?.is_none() {
            return Err(plugin_error(format!("Plugin '{id}' is not installed")));
        }
        self.verify_package(&self.packages_dir().join(id))
    }

    /// Execute an enabled process plugin through the versioned hard-isolation host.
    pub fn execute_isolated(
        &self,
        id: &str,
        request: &PluginRequest,
    ) -> Result<IsolatedProcessResult> {
        let installed = self
            .get(id)?
            .ok_or_else(|| plugin_error(format!("Plugin '{id}' is not installed")))?;
        if !installed.enabled {
            return Err(plugin_error(format!("Plugin '{id}' is disabled")));
        }
        if installed.manifest.class != PluginClass::IsolatedProcess {
            return Err(plugin_error(format!(
                "Plugin '{id}' is not an isolated-process plugin"
            )));
        }
        let verified = self.verify_installed(id)?;
        let permissions = EffectivePluginPermissions::from_manifest(
            &installed.manifest.permissions,
            &verified.package_root,
        )?;
        if let Some(hook) = request.hook() {
            permissions.check_hook_registration(hook)?;
        }
        let limits = IsolatedProcessLimits {
            timeout: std::time::Duration::from_millis(
                permissions.timeout_millis.unwrap_or(30_000).max(1),
            ),
            memory_limit_bytes: permissions
                .memory_limit_bytes
                .unwrap_or(256 * 1024 * 1024)
                .max(1),
            output_limit_bytes: permissions
                .output_limit_bytes
                .unwrap_or(16 * 1024 * 1024)
                .max(1),
        };
        IsolatedProcessHost::execute(&verified.entrypoint_path, &[], request, limits)
    }

    pub fn effective_permissions(&self, id: &str) -> Result<EffectivePermissionSummary> {
        let installed = self
            .get(id)?
            .ok_or_else(|| plugin_error(format!("Plugin '{id}' is not installed")))?;
        let verified = self.verify_installed(id)?;
        EffectivePluginPermissions::from_manifest(
            &installed.manifest.permissions,
            &verified.package_root,
        )
        .map(|permissions| permissions.summary())
    }

    fn packages_dir(&self) -> PathBuf {
        self.root.join("packages")
    }

    fn index_path(&self) -> PathBuf {
        self.root.join("registry.json")
    }

    fn load_index_unlocked(&self) -> Result<StoreIndex> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(StoreIndex::default());
        }
        let bytes = read_limited(&path, 4 * 1024 * 1024)?;
        let index: StoreIndex = serde_json::from_slice(&bytes)
            .map_err(|error| plugin_error(format!("Invalid plugin registry: {error}")))?;
        if index.schema_version != 1 {
            return Err(plugin_error(format!(
                "Unsupported plugin registry schema {}",
                index.schema_version
            )));
        }
        Ok(index)
    }

    fn save_index_unlocked(&self, index: &StoreIndex) -> Result<()> {
        std::fs::create_dir_all(&self.root).map_err(|error| plugin_error(error.to_string()))?;
        let bytes =
            serde_json::to_vec_pretty(index).map_err(|error| plugin_error(error.to_string()))?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".registry-")
            .suffix(".tmp")
            .tempfile_in(&self.root)
            .map_err(|error| plugin_error(error.to_string()))?;
        temporary
            .write_all(&bytes)
            .map_err(|error| plugin_error(error.to_string()))?;
        temporary
            .flush()
            .map_err(|error| plugin_error(error.to_string()))?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| plugin_error(error.to_string()))?;
        let (temporary_file, temporary_path) = temporary
            .keep()
            .map_err(|error| plugin_error(error.error.to_string()))?;
        drop(temporary_file);
        let path = self.index_path();
        if let Err(error) = atomic_replace(&temporary_path, &path) {
            let _ = std::fs::remove_file(&temporary_path);
            return Err(error);
        }
        sync_parent_directory(&self.root)
    }

    fn lock_shared(&self) -> Result<StoreLock> {
        self.lock(false)
    }

    fn lock_exclusive(&self) -> Result<StoreLock> {
        self.lock(true)
    }

    fn lock(&self, exclusive: bool) -> Result<StoreLock> {
        std::fs::create_dir_all(&self.root).map_err(|error| plugin_error(error.to_string()))?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join(".registry.lock"))
            .map_err(|error| plugin_error(error.to_string()))?;
        if exclusive {
            fs2::FileExt::lock_exclusive(&file)
        } else {
            fs2::FileExt::lock_shared(&file)
        }
        .map_err(|error| plugin_error(format!("Could not lock plugin registry: {error}")))?;
        Ok(StoreLock(file))
    }
}

struct StoreLock(std::fs::File);

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
    std::fs::rename(source, destination).map_err(|error| plugin_error(error.to_string()))
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<()> {
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
    // SAFETY: Both UTF-16 path buffers are NUL-terminated and remain valid for the call.
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(plugin_error(format!(
            "Could not atomically replace plugin registry: {}",
            std::io::Error::last_os_error()
        )))
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<()> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| plugin_error(format!("Could not sync plugin registry directory: {error}")))
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<()> {
    Ok(())
}

fn validate_external_manifest(manifest: &PluginManifest) -> Result<()> {
    validate_id(&manifest.id)?;
    semver::Version::parse(&manifest.version)
        .map_err(|error| plugin_error(format!("Invalid plugin version: {error}")))?;
    if manifest.plugin_api_version != PLUGIN_API_VERSION {
        return Err(plugin_error(format!(
            "Plugin API {} is incompatible with host API {}",
            manifest.plugin_api_version, PLUGIN_API_VERSION
        )));
    }
    let core = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| plugin_error(error.to_string()))?;
    let requirement = semver::VersionReq::parse(&manifest.core_version_requirement)
        .map_err(|error| plugin_error(format!("Invalid core version requirement: {error}")))?;
    if !legacy_core_requirement_matches(&requirement, &core) {
        return Err(plugin_error(format!("Plugin does not support core {core}")));
    }
    if matches!(manifest.class, PluginClass::BuiltInRust) {
        return Err(plugin_error(
            "Built-in Rust plugins cannot be installed from disk",
        ));
    }
    if manifest.abi_version != Some(PLUGIN_ABI_VERSION) {
        return Err(plugin_error(format!(
            "Plugin ABI {:?} is incompatible with host ABI {}",
            manifest.abi_version, PLUGIN_ABI_VERSION
        )));
    }
    for hook in &manifest.hooks {
        let allowed = match hook {
            PluginHook::RegisterImporter => manifest.permissions.importer_registration,
            PluginHook::RegisterExporter => manifest.permissions.exporter_registration,
            PluginHook::RegisterRuntime => manifest.permissions.runtime_registration,
            PluginHook::RegisterModelAdapter => manifest.permissions.capability_registration,
            _ => true,
        };
        if !allowed {
            return Err(plugin_error(format!(
                "PLUGIN_PERMISSION_DENIED: Plugin '{}' declares {:?} without the matching registration permission",
                manifest.id, hook
            )));
        }
    }
    if (!manifest.capabilities.is_empty() || !manifest.format_capabilities.is_empty())
        && !manifest.permissions.capability_registration
    {
        return Err(plugin_error(format!(
            "PLUGIN_PERMISSION_DENIED: Plugin '{}' declares capabilities without capabilityRegistration",
            manifest.id
        )));
    }
    if !manifest.platforms.is_empty()
        && !manifest
            .platforms
            .iter()
            .any(|value| value == std::env::consts::OS)
    {
        return Err(plugin_error(format!(
            "Plugin does not support platform {}",
            std::env::consts::OS
        )));
    }
    if !manifest.architectures.is_empty()
        && !manifest
            .architectures
            .iter()
            .any(|value| value == std::env::consts::ARCH)
    {
        return Err(plugin_error(format!(
            "Plugin does not support architecture {}",
            std::env::consts::ARCH
        )));
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-'))
    {
        return Err(plugin_error(format!("Invalid plugin ID '{id}'")));
    }
    Ok(())
}

fn locate_manifest(source: &Path) -> Result<(PathBuf, PathBuf)> {
    if source.is_dir() {
        for name in ["plugin.json", "plugin-manifest.json"] {
            let candidate = source.join(name);
            if candidate.is_file() {
                return Ok((source.to_path_buf(), candidate));
            }
        }
        return Err(plugin_error("Plugin directory has no plugin.json manifest"));
    }
    if source.is_file() {
        let root = source
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        return Ok((root, source.to_path_buf()));
    }
    Err(plugin_error(format!(
        "Plugin package does not exist: {}",
        source.display()
    )))
}

fn resolve_package_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|value| !matches!(value, Component::Normal(_)))
    {
        return Err(plugin_error(
            "Plugin entrypoint must be a bounded relative path",
        ));
    }
    let path = root.join(relative);
    validate_existing_child(root, &path)?;
    if !path.is_file() {
        return Err(plugin_error(format!(
            "Plugin entrypoint is not a file: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn validate_existing_child(root: &Path, path: &Path) -> Result<()> {
    let root = root
        .canonicalize()
        .map_err(|error| plugin_error(error.to_string()))?;
    let path = path
        .canonicalize()
        .map_err(|error| plugin_error(error.to_string()))?;
    if !path.starts_with(&root) {
        return Err(plugin_error("Plugin path escapes its package root"));
    }
    Ok(())
}

fn inspect_package(root: &Path) -> Result<(usize, u64)> {
    let mut pending = vec![root.to_path_buf()];
    let mut file_count = 0usize;
    let mut total_bytes = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in
            std::fs::read_dir(&directory).map_err(|error| plugin_error(error.to_string()))?
        {
            let entry = entry.map_err(|error| plugin_error(error.to_string()))?;
            let metadata = std::fs::symlink_metadata(entry.path())
                .map_err(|error| plugin_error(error.to_string()))?;
            if metadata.file_type().is_symlink() {
                return Err(plugin_error(
                    "Plugin packages may not contain symbolic links",
                ));
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                file_count += 1;
                total_bytes = total_bytes.saturating_add(metadata.len());
                if file_count > MAX_PACKAGE_FILES || total_bytes > MAX_PACKAGE_BYTES {
                    return Err(plugin_error(
                        "Plugin package exceeds its file or byte budget",
                    ));
                }
            }
        }
    }
    Ok((file_count, total_bytes))
}

fn copy_package(source: &Path, destination: &Path) -> Result<()> {
    let result = (|| {
        std::fs::create_dir_all(destination).map_err(|error| plugin_error(error.to_string()))?;
        let mut pending = vec![(source.to_path_buf(), destination.to_path_buf())];
        while let Some((from, to)) = pending.pop() {
            for entry in
                std::fs::read_dir(&from).map_err(|error| plugin_error(error.to_string()))?
            {
                let entry = entry.map_err(|error| plugin_error(error.to_string()))?;
                let metadata = std::fs::symlink_metadata(entry.path())
                    .map_err(|error| plugin_error(error.to_string()))?;
                let target = to.join(entry.file_name());
                if metadata.file_type().is_symlink() {
                    return Err(plugin_error(
                        "Plugin packages may not contain symbolic links",
                    ));
                }
                if metadata.is_dir() {
                    std::fs::create_dir(&target)
                        .map_err(|error| plugin_error(error.to_string()))?;
                    pending.push((entry.path(), target));
                } else if metadata.is_file() {
                    std::fs::copy(entry.path(), target)
                        .map_err(|error| plugin_error(error.to_string()))?;
                }
            }
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(destination);
    }
    result
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path).map_err(|error| plugin_error(error.to_string()))?;
    let length = file
        .metadata()
        .map_err(|error| plugin_error(error.to_string()))?
        .len();
    if length > limit {
        return Err(plugin_error(format!(
            "File exceeds {} byte limit: {}",
            limit,
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| plugin_error(error.to_string()))?;
    if bytes.len() as u64 > limit {
        return Err(plugin_error("File grew beyond its read limit"));
    }
    Ok(bytes)
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn plugin_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Plugin(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "latexsnipper-plugin-store-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn package(root: &Path, id: &str, entrypoint: &str, bytes: &[u8]) -> PathBuf {
        let path = root.join(id);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(entrypoint), bytes).unwrap();
        let mut manifest = PluginManifest::built_in(id, "1.0.0");
        manifest.class = PluginClass::WasiComponent;
        manifest.abi_version = Some(PLUGIN_ABI_VERSION);
        manifest.entrypoint = Some(entrypoint.to_string());
        manifest.checksum_sha256 = Some(hex::encode(Sha256::digest(bytes)));
        std::fs::write(
            path.join("plugin.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        path
    }

    #[test]
    fn verified_install_is_disabled_until_enabled_and_can_be_removed() {
        let root = workspace();
        let source = package(&root, "example.plugin", "plugin.wasm", b"fixture component");
        let store = PluginStore::new(root.join("store"));

        let verified = store.verify_package(&source).unwrap();
        assert_eq!(verified.manifest.id, "example.plugin");
        assert_eq!(verified.file_count, 2);
        let installed = store.install(&source).unwrap();
        assert!(!installed.enabled);
        assert_eq!(store.list().unwrap().len(), 1);
        assert!(store.set_enabled("example.plugin", true).unwrap().enabled);
        assert!(store.verify_installed("example.plugin").is_ok());
        store.uninstall("example.plugin").unwrap();
        assert!(store.list().unwrap().is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checksum_mismatch_and_entrypoint_traversal_are_rejected() {
        let root = workspace();
        let source = package(&root, "tampered", "plugin.wasm", b"trusted");
        std::fs::write(source.join("plugin.wasm"), b"changed").unwrap();
        let store = PluginStore::new(root.join("store"));
        assert!(store.verify_package(&source).is_err());

        let traversal = package(&root, "traversal", "plugin.wasm", b"trusted");
        let manifest_path = traversal.join("plugin.json");
        let mut manifest: PluginManifest =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest.entrypoint = Some("../outside.wasm".to_string());
        std::fs::write(manifest_path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
        assert!(store.verify_package(&traversal).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn incompatible_abi_and_ungranted_capability_registration_are_rejected() {
        let root = workspace();
        let store = PluginStore::new(root.join("store"));

        let incompatible = package(&root, "bad-abi", "plugin.wasm", b"fixture");
        let manifest_path = incompatible.join("plugin.json");
        let mut manifest: PluginManifest =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest.abi_version = Some(PLUGIN_ABI_VERSION + 1);
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(store.verify_package(&incompatible).is_err());

        let ungranted = package(&root, "ungranted", "plugin.wasm", b"fixture");
        let manifest_path = ungranted.join("plugin.json");
        let mut manifest: PluginManifest =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        manifest.hooks.push(PluginHook::RegisterExporter);
        manifest.capabilities.push("export:fixture".to_string());
        std::fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(store
            .verify_package(&ungranted)
            .unwrap_err()
            .to_string()
            .contains("PLUGIN_PERMISSION_DENIED"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_registry_updates_remain_parseable_and_leave_no_temporary_gap() {
        let root = workspace();
        let source = package(&root, "concurrent.plugin", "plugin.wasm", b"fixture");
        let store = PluginStore::new(root.join("store"));
        store.install(&source).unwrap();

        let mut workers = Vec::new();
        for worker in 0..8 {
            let store = store.clone();
            workers.push(std::thread::spawn(move || {
                for iteration in 0..20 {
                    store
                        .set_enabled("concurrent.plugin", (worker + iteration) % 2 == 0)
                        .unwrap();
                }
            }));
        }
        for worker in workers {
            worker.join().unwrap();
        }

        assert!(store.get("concurrent.plugin").unwrap().is_some());
        let registry: StoreIndex =
            serde_json::from_slice(&std::fs::read(store.index_path()).unwrap()).unwrap();
        assert_eq!(registry.plugins.len(), 1);
        assert!(std::fs::read_dir(store.root())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".registry-")
                || entry.file_name() == ".registry.lock"));

        std::fs::remove_dir_all(root).unwrap();
    }
}
