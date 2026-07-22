//! Descriptor discovery gated by an application-owned trust store.

use std::collections::{BTreeSet, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use latexsnipper_foundation::Result;
use latexsnipper_runtime::RuntimeRegistry;

use crate::descriptor::RuntimePluginDescriptor;
use crate::error::{plugin_runtime_error, PluginRuntimeResult};
use crate::host::RuntimePluginFactory;
use crate::trust::{canonical_regular_file, sha256_file, RuntimePluginTrustStore};

pub const RUNTIME_PLUGIN_DESCRIPTOR: &str = "runtime-plugin.json";
const MAX_DESCRIPTOR_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoveryIssueKind {
    InvalidDescriptor,
    Untrusted,
    Disabled,
    IntegrityMismatch,
    LoadFailed,
    DuplicateRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryIssue {
    pub descriptor_path: PathBuf,
    pub runtime_id: Option<String>,
    pub kind: DiscoveryIssueKind,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct RuntimePluginDiscoveryReport {
    pub factories: Vec<RuntimePluginFactory>,
    pub issues: Vec<DiscoveryIssue>,
}

impl RuntimePluginDiscoveryReport {
    pub fn register_all(&self, registry: &mut RuntimeRegistry) -> Result<()> {
        for factory in &self.factories {
            let kind = latexsnipper_runtime::RuntimeKind::Custom(factory.runtime_id().to_owned());
            if registry.get(&kind).is_some() {
                return Err(plugin_runtime_error(format!(
                    "runtime factory '{kind}' is already registered; no discovered plugins were added"
                )));
            }
        }
        for factory in &self.factories {
            registry.register(factory.clone())?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct RuntimePluginDiscovery<'trust> {
    roots: Vec<PathBuf>,
    trust: &'trust RuntimePluginTrustStore,
}

impl<'trust> RuntimePluginDiscovery<'trust> {
    pub fn new(
        roots: impl IntoIterator<Item = impl Into<PathBuf>>,
        trust: &'trust RuntimePluginTrustStore,
    ) -> Self {
        Self {
            roots: roots.into_iter().map(Into::into).collect(),
            trust,
        }
    }

    pub fn discover(&self) -> RuntimePluginDiscoveryReport {
        let mut report = RuntimePluginDiscoveryReport::default();
        let mut runtime_ids = HashSet::new();
        for descriptor_path in descriptor_paths(&self.roots) {
            match self.load_descriptor(&descriptor_path) {
                Ok(factory) => {
                    if runtime_ids.insert(factory.runtime_id().to_owned()) {
                        report.factories.push(factory);
                    } else {
                        report.issues.push(DiscoveryIssue {
                            descriptor_path,
                            runtime_id: Some(factory.runtime_id().to_owned()),
                            kind: DiscoveryIssueKind::DuplicateRuntime,
                            reason: "more than one trusted plugin declares this runtime id"
                                .to_owned(),
                        });
                    }
                }
                Err(issue) => report.issues.push(issue),
            }
        }
        report
            .factories
            .sort_by(|left, right| left.runtime_id().cmp(right.runtime_id()));
        report
    }

    fn load_descriptor(
        &self,
        descriptor_path: &Path,
    ) -> std::result::Result<RuntimePluginFactory, DiscoveryIssue> {
        let descriptor = read_descriptor(descriptor_path).map_err(|error| DiscoveryIssue {
            descriptor_path: descriptor_path.to_path_buf(),
            runtime_id: None,
            kind: DiscoveryIssueKind::InvalidDescriptor,
            reason: error.to_string(),
        })?;
        let issue = |kind, reason: String| DiscoveryIssue {
            descriptor_path: descriptor_path.to_path_buf(),
            runtime_id: Some(descriptor.runtime_id.clone()),
            kind,
            reason,
        };
        let Some(trusted) = self.trust.get(&descriptor.runtime_id) else {
            return Err(issue(
                DiscoveryIssueKind::Untrusted,
                "plugin is installed but has not been enrolled in the trusted runtime registry"
                    .to_owned(),
            ));
        };
        if !trusted.enabled {
            return Err(issue(
                DiscoveryIssueKind::Disabled,
                "plugin is trusted but not explicitly enabled".to_owned(),
            ));
        }
        let package_root = descriptor_path
            .parent()
            .ok_or_else(|| {
                issue(
                    DiscoveryIssueKind::InvalidDescriptor,
                    "descriptor has no package directory".to_owned(),
                )
            })?
            .canonicalize()
            .map_err(|error| {
                issue(
                    DiscoveryIssueKind::InvalidDescriptor,
                    format!("canonicalize plugin package: {error}"),
                )
            })?;
        let declared_library = package_root.join(&descriptor.library);
        if !is_dynamic_library(&declared_library) {
            return Err(issue(
                DiscoveryIssueKind::InvalidDescriptor,
                "descriptor library does not use the platform dynamic-library extension".to_owned(),
            ));
        }
        let library = canonical_regular_file(&declared_library)
            .map_err(|error| issue(DiscoveryIssueKind::InvalidDescriptor, error.to_string()))?;
        if !library.starts_with(&package_root) {
            return Err(issue(
                DiscoveryIssueKind::InvalidDescriptor,
                "runtime plugin library escapes its installation package".to_owned(),
            ));
        }
        if library != trusted.library_path {
            return Err(issue(
                DiscoveryIssueKind::IntegrityMismatch,
                format!(
                    "descriptor resolves to '{}', but trust enrollment names '{}'",
                    library.display(),
                    trusted.library_path.display()
                ),
            ));
        }
        let actual = sha256_file(&library)
            .map_err(|error| issue(DiscoveryIssueKind::IntegrityMismatch, error.to_string()))?;
        if !actual.eq_ignore_ascii_case(&descriptor.sha256)
            || !actual.eq_ignore_ascii_case(&trusted.sha256)
        {
            return Err(issue(
                DiscoveryIssueKind::IntegrityMismatch,
                format!(
                    "runtime plugin digest mismatch: descriptor={}, trusted={}, actual={actual}",
                    descriptor.sha256, trusted.sha256
                ),
            ));
        }
        // SAFETY: Canonical path, installation boundary, explicit enablement,
        // descriptor digest, and trust digest all match immediately before load.
        unsafe { RuntimePluginFactory::load_trusted(&library, &descriptor) }
            .map_err(|error| issue(DiscoveryIssueKind::LoadFailed, error.to_string()))
    }
}

fn descriptor_paths(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for root in roots {
        let direct = root.join(RUNTIME_PLUGIN_DESCRIPTOR);
        if direct.is_file() {
            paths.insert(direct);
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let descriptor = path.join(RUNTIME_PLUGIN_DESCRIPTOR);
                if descriptor.is_file() {
                    paths.insert(descriptor);
                }
            }
        }
    }
    paths.into_iter().collect()
}

fn read_descriptor(path: &Path) -> PluginRuntimeResult<RuntimePluginDescriptor> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        plugin_runtime_error(format!(
            "inspect runtime plugin descriptor '{}': {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_DESCRIPTOR_BYTES
    {
        return Err(plugin_runtime_error(format!(
            "runtime plugin descriptor must be a real file no larger than {MAX_DESCRIPTOR_BYTES} bytes"
        )));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_DESCRIPTOR_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|error| {
            plugin_runtime_error(format!(
                "read runtime plugin descriptor '{}': {error}",
                path.display()
            ))
        })?;
    if bytes.len() as u64 > MAX_DESCRIPTOR_BYTES {
        return Err(plugin_runtime_error(
            "runtime plugin descriptor grew beyond its size limit",
        ));
    }
    let descriptor: RuntimePluginDescriptor = serde_json::from_slice(&bytes).map_err(|error| {
        plugin_runtime_error(format!(
            "parse runtime plugin descriptor '{}': {error}",
            path.display()
        ))
    })?;
    descriptor.validate()?;
    Ok(descriptor)
}

fn is_dynamic_library(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    #[cfg(target_os = "windows")]
    return extension.eq_ignore_ascii_case("dll");
    #[cfg(target_os = "linux")]
    return extension.eq_ignore_ascii_case("so");
    #[cfg(target_os = "macos")]
    return extension.eq_ignore_ascii_case("dylib");
    #[allow(unreachable_code)]
    false
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    #[test]
    fn raw_libraries_are_never_discovered_without_descriptors() {
        let root = std::env::temp_dir().join(format!(
            "latexsnipper-runtime-discovery-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("untrusted.dll"), b"native code").unwrap();
        let trust = RuntimePluginTrustStore::new();
        let report = RuntimePluginDiscovery::new([&root], &trust).discover();
        assert!(report.factories.is_empty());
        assert!(report.issues.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn installed_descriptor_stays_unloaded_without_trust() {
        let root = std::env::temp_dir().join(format!(
            "latexsnipper-runtime-untrusted-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let descriptor = RuntimePluginDescriptor {
            schema_version: 1,
            runtime_id: "vendor.npu".to_owned(),
            plugin_version: "1.0.0".to_owned(),
            library: if cfg!(target_os = "windows") {
                "plugin.dll"
            } else if cfg!(target_os = "macos") {
                "libplugin.dylib"
            } else {
                "libplugin.so"
            }
            .to_owned(),
            sha256: "a".repeat(64),
        };
        std::fs::write(
            root.join(RUNTIME_PLUGIN_DESCRIPTOR),
            serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        let trust = RuntimePluginTrustStore::new();
        let report = RuntimePluginDiscovery::new([&root], &trust).discover();
        assert!(report.factories.is_empty());
        assert_eq!(report.issues[0].kind, DiscoveryIssueKind::Untrusted);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mutation_after_enrollment_is_rejected_before_loading() {
        let root = std::env::temp_dir().join(format!(
            "latexsnipper-runtime-tampered-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let library_name = if cfg!(target_os = "windows") {
            "plugin.dll"
        } else if cfg!(target_os = "macos") {
            "libplugin.dylib"
        } else {
            "libplugin.so"
        };
        let library = root.join(library_name);
        std::fs::write(&library, b"trusted bytes").unwrap();
        let digest = format!("{:x}", Sha256::digest(b"trusted bytes"));
        let descriptor = RuntimePluginDescriptor {
            schema_version: 1,
            runtime_id: "vendor.tamper-test".to_owned(),
            plugin_version: "1.0.0".to_owned(),
            library: library_name.to_owned(),
            sha256: digest.clone(),
        };
        std::fs::write(
            root.join(RUNTIME_PLUGIN_DESCRIPTOR),
            serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        let mut trust = RuntimePluginTrustStore::new();
        trust
            .enroll("vendor.tamper-test", &library, &digest)
            .unwrap();
        trust.set_enabled("vendor.tamper-test", true).unwrap();
        std::fs::write(&library, b"changed bytes").unwrap();

        let report = RuntimePluginDiscovery::new([&root], &trust).discover();
        assert!(report.factories.is_empty());
        assert_eq!(report.issues[0].kind, DiscoveryIssueKind::IntegrityMismatch);
        std::fs::remove_dir_all(root).unwrap();
    }
}
