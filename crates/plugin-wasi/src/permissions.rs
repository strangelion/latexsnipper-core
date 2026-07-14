use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use latexsnipper_plugin::{NetworkSchemeV3, PluginPathAccessV3, PluginPermissionsV3};

use crate::{WasiDiagnostic, WasiDiagnosticCode, WasiResourceLimits};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemGrant {
    pub id: String,
    pub root: PathBuf,
    pub writable: bool,
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
        let package_root = package_root.canonicalize().map_err(host_failure)?;
        let mut filesystem = BTreeMap::new();
        for (index, grant) in value.paths.iter().enumerate() {
            let raw = Path::new(&grant.path);
            reject_unsafe_relative_path(raw)?;
            let joined = package_root.join(raw);
            let canonical = joined.canonicalize().map_err(host_failure)?;
            if !canonical.starts_with(&package_root) {
                return Err(permission_denied("filesystem grant escapes package root"));
            }
            let id = format!("path-{index}");
            filesystem.insert(
                id.clone(),
                FilesystemGrant {
                    id,
                    root: canonical,
                    writable: grant.access == PluginPathAccessV3::Write,
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
            limits: WasiResourceLimits::from_manifest(&value.limits),
        })
    }

    pub fn resolve_path(
        &self,
        grant_id: &str,
        relative: &str,
        write: bool,
    ) -> Result<PathBuf, WasiDiagnostic> {
        let grant = self
            .filesystem
            .get(grant_id)
            .ok_or_else(|| permission_denied("unknown filesystem grant"))?;
        if write && !grant.writable {
            return Err(permission_denied("filesystem grant is read-only"));
        }
        let relative = Path::new(relative);
        reject_unsafe_relative_path(relative)?;
        let candidate = grant.root.join(relative);
        let resolved = if candidate.exists() {
            candidate.canonicalize().map_err(host_failure)?
        } else {
            let parent = candidate
                .parent()
                .ok_or_else(|| permission_denied("path has no parent"))?
                .canonicalize()
                .map_err(host_failure)?;
            let name = candidate
                .file_name()
                .ok_or_else(|| permission_denied("path has no file name"))?;
            parent.join(name)
        };
        if !resolved.starts_with(&grant.root) {
            return Err(permission_denied("filesystem path escapes grant root"));
        }
        Ok(resolved)
    }

    pub fn permits_network(&self, grant: &NetworkGrant) -> bool {
        self.network.contains(&NetworkGrant {
            scheme: grant.scheme,
            host: normalize_host(&grant.host),
            port: grant.port,
        })
    }
}

fn reject_unsafe_relative_path(path: &Path) -> Result<(), WasiDiagnostic> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(permission_denied("path must be a non-empty relative path"));
    }
    Ok(())
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
