use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use latexsnipper_plugin::{NetworkSchemeV3, PluginPathAccessV3, PluginPermissionsV3};

use crate::{WasiDiagnostic, WasiDiagnosticCode, WasiHostPolicy, WasiResourceLimits};

#[derive(Debug, Clone)]
pub struct FilesystemGrant {
    pub id: String,
    pub root: PathBuf,
    pub writable: bool,
    directory: Arc<Dir>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemOperationError {
    PermissionDenied,
    NotFound,
    InvalidPath,
    SizeLimit,
    HostFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComponentNetworkScheme {
    Https,
    Http,
    Tcp,
}

impl From<NetworkSchemeV3> for ComponentNetworkScheme {
    fn from(value: NetworkSchemeV3) -> Self {
        match value {
            NetworkSchemeV3::Https => Self::Https,
            NetworkSchemeV3::Http => Self::Http,
            NetworkSchemeV3::Tcp => Self::Tcp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NetworkGrant {
    pub scheme: ComponentNetworkScheme,
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct ComponentPermissions {
    pub filesystem: BTreeMap<String, FilesystemGrant>,
    pub environment: BTreeSet<String>,
    pub network: BTreeSet<NetworkGrant>,
    pub model_artifacts: BTreeSet<String>,
    pub temporary_storage: bool,
    pub clocks: bool,
    pub randomness: bool,
    pub limits: WasiResourceLimits,
}

impl ComponentPermissions {
    pub fn deny_all() -> Self {
        Self {
            filesystem: BTreeMap::new(),
            environment: BTreeSet::new(),
            network: BTreeSet::new(),
            model_artifacts: BTreeSet::new(),
            temporary_storage: false,
            clocks: false,
            randomness: false,
            limits: WasiResourceLimits::default(),
        }
    }

    pub fn from_manifest(
        value: &PluginPermissionsV3,
        package_root: &Path,
    ) -> Result<Self, WasiDiagnostic> {
        Self::from_manifest_with_policy(value, package_root, &WasiHostPolicy::default())
    }

    pub fn from_manifest_with_policy(
        value: &PluginPermissionsV3,
        package_root: &Path,
        policy: &WasiHostPolicy,
    ) -> Result<Self, WasiDiagnostic> {
        let package_root = package_root.canonicalize().map_err(host_failure)?;
        let package_directory =
            Dir::open_ambient_dir(&package_root, ambient_authority()).map_err(host_failure)?;
        Self::from_manifest_with_directory(value, &package_root, &package_directory, policy)
    }

    pub(crate) fn from_manifest_with_directory(
        value: &PluginPermissionsV3,
        package_root: &Path,
        package_directory: &Dir,
        policy: &WasiHostPolicy,
    ) -> Result<Self, WasiDiagnostic> {
        let mut filesystem = BTreeMap::new();
        for (index, grant) in value.paths.iter().enumerate() {
            let raw = Path::new(&grant.path);
            reject_unsafe_relative_path(raw).map_err(|_| {
                WasiDiagnostic::new(
                    WasiDiagnosticCode::PluginWasiPermissionDenied,
                    "filesystem grant path is unsafe",
                )
            })?;
            let directory = package_directory
                .open_dir_nofollow(raw)
                .map_err(|_| permission_denied("filesystem grant is not a safe directory"))?;
            let id = format!("path-{index}");
            filesystem.insert(
                id.clone(),
                FilesystemGrant {
                    id,
                    root: package_root.join(raw),
                    writable: grant.access == PluginPathAccessV3::Write,
                    directory: Arc::new(directory),
                },
            );
        }
        Ok(Self {
            filesystem,
            environment: value
                .environment_variables
                .iter()
                .map(|name| name.to_ascii_uppercase())
                .collect(),
            network: value
                .network
                .iter()
                .map(|destination| NetworkGrant {
                    scheme: destination.scheme.into(),
                    host: normalize_host(&destination.host),
                    port: destination.port,
                })
                .collect(),
            model_artifacts: value.model_artifacts.iter().cloned().collect(),
            temporary_storage: value.temporary_directory,
            clocks: value.clocks,
            randomness: value.randomness,
            limits: policy.grant(&value.limits)?,
        })
    }

    pub fn read_file(
        &self,
        grant_id: &str,
        relative: &str,
        limit: usize,
    ) -> Result<Vec<u8>, FilesystemOperationError> {
        let grant = self
            .filesystem
            .get(grant_id)
            .ok_or(FilesystemOperationError::PermissionDenied)?;
        let relative = Path::new(relative);
        reject_unsafe_relative_path(relative)?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = grant
            .directory
            .open_with(relative, &options)
            .map_err(map_io_error)?;
        let metadata = file.metadata().map_err(map_io_error)?;
        if !metadata.is_file() || metadata.nlink() > 1 {
            return Err(FilesystemOperationError::InvalidPath);
        }
        let length =
            usize::try_from(metadata.len()).map_err(|_| FilesystemOperationError::SizeLimit)?;
        if length > limit {
            return Err(FilesystemOperationError::SizeLimit);
        }
        let read_limit = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
        let mut bytes = Vec::with_capacity(length);
        file.take(read_limit)
            .read_to_end(&mut bytes)
            .map_err(map_io_error)?;
        if bytes.len() > limit {
            return Err(FilesystemOperationError::SizeLimit);
        }
        Ok(bytes)
    }

    pub fn write_file(
        &self,
        grant_id: &str,
        relative: &str,
        payload: &[u8],
        limit: usize,
    ) -> Result<(), FilesystemOperationError> {
        let grant = self
            .filesystem
            .get(grant_id)
            .ok_or(FilesystemOperationError::PermissionDenied)?;
        if !grant.writable {
            return Err(FilesystemOperationError::PermissionDenied);
        }
        if payload.len() > limit {
            return Err(FilesystemOperationError::SizeLimit);
        }
        let relative = Path::new(relative);
        reject_unsafe_relative_path(relative)?;
        let mut options = OpenOptions::new();
        options.write(true).create(true).follow(FollowSymlinks::No);
        let mut file = grant
            .directory
            .open_with(relative, &options)
            .map_err(map_io_error)?;
        let metadata = file.metadata().map_err(map_io_error)?;
        if !metadata.is_file() || metadata.nlink() > 1 {
            return Err(FilesystemOperationError::InvalidPath);
        }
        file.set_len(0).map_err(map_io_error)?;
        file.write_all(payload).map_err(map_io_error)?;
        file.sync_all().map_err(map_io_error)?;
        Ok(())
    }

    pub fn permits_network(&self, grant: &NetworkGrant) -> bool {
        self.network.contains(&NetworkGrant {
            scheme: grant.scheme,
            host: normalize_host(&grant.host),
            port: grant.port,
        })
    }
}

fn reject_unsafe_relative_path(path: &Path) -> Result<(), FilesystemOperationError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(FilesystemOperationError::InvalidPath);
    }
    Ok(())
}

fn map_io_error(error: std::io::Error) -> FilesystemOperationError {
    match error.kind() {
        std::io::ErrorKind::NotFound => FilesystemOperationError::NotFound,
        std::io::ErrorKind::PermissionDenied => FilesystemOperationError::PermissionDenied,
        std::io::ErrorKind::InvalidInput => FilesystemOperationError::InvalidPath,
        _ => FilesystemOperationError::HostFailure,
    }
}

fn normalize_host(host: &str) -> String {
    host.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn permission_denied(message: impl Into<String>) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiPermissionDenied, message)
}

fn host_failure(error: impl std::fmt::Display) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiHostFailure, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_all_has_no_ambient_authority() {
        let permissions = ComponentPermissions::deny_all();
        assert!(permissions.filesystem.is_empty());
        assert!(permissions.environment.is_empty());
        assert!(permissions.network.is_empty());
        assert!(permissions.model_artifacts.is_empty());
        assert!(!permissions.temporary_storage);
        assert!(!permissions.clocks);
        assert!(!permissions.randomness);
    }

    #[test]
    fn unsafe_paths_are_rejected_before_access() {
        for path in ["../secret", "/absolute", ""] {
            assert!(
                reject_unsafe_relative_path(Path::new(path)).is_err(),
                "{path}"
            );
        }
        #[cfg(windows)]
        assert!(reject_unsafe_relative_path(Path::new("C:\\absolute")).is_err());
    }

    #[test]
    fn network_matching_is_normalized_and_exact() {
        let mut permissions = ComponentPermissions::deny_all();
        permissions.network.insert(NetworkGrant {
            scheme: ComponentNetworkScheme::Https,
            host: "models.example.invalid".to_string(),
            port: 443,
        });
        assert!(permissions.permits_network(&NetworkGrant {
            scheme: ComponentNetworkScheme::Https,
            host: "MODELS.EXAMPLE.INVALID.".to_string(),
            port: 443,
        }));
        assert!(!permissions.permits_network(&NetworkGrant {
            scheme: ComponentNetworkScheme::Https,
            host: "evilmodels.example.invalid".to_string(),
            port: 443,
        }));
    }
}
