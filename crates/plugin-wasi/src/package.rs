use std::fs;
use std::path::{Path, PathBuf};

use latexsnipper_plugin::{
    PluginArtifactKindV3, PluginExecutionClassV3, PluginManifestV3, COMPONENT_WIT_VERSION_V1,
};
use semver::{Version, VersionReq};
use sha2::{Digest, Sha256};

use crate::{ComponentPermissions, WasiDiagnostic, WasiDiagnosticCode};

const MANIFEST_FILE: &str = "plugin.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_COMPONENT_BYTES: u64 = 256 * 1024 * 1024;

pub fn verify_component_artifact_bytes(
    core_version: &Version,
    manifest_bytes: &[u8],
    component: &[u8],
) -> Result<PluginManifestV3, WasiDiagnostic> {
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES
        || component.len() as u64 > MAX_COMPONENT_BYTES
    {
        return Err(protocol_mismatch(
            "component package input exceeds size limits",
        ));
    }
    let manifest: PluginManifestV3 = serde_json::from_slice(manifest_bytes)
        .map_err(|error| protocol_mismatch(error.to_string()))?;
    manifest
        .validate_contract()
        .map_err(|error| protocol_mismatch(error.to_string()))?;
    if manifest.execution_class != PluginExecutionClassV3::WasiComponent
        || manifest.interfaces.component_wit != Some(COMPONENT_WIT_VERSION_V1)
    {
        return Err(protocol_mismatch(
            "package is not a Component WIT v1 plugin",
        ));
    }
    let requirement = VersionReq::parse(&manifest.core_version_requirement)
        .map_err(|error| protocol_mismatch(error.to_string()))?;
    if !requirement.matches(core_version) {
        return Err(protocol_mismatch(
            "plugin does not support this core version",
        ));
    }
    let artifact = manifest
        .artifact
        .as_ref()
        .ok_or_else(|| protocol_mismatch("component artifact is missing"))?;
    if artifact.kind != PluginArtifactKindV3::WasiComponent {
        return Err(protocol_mismatch("artifact kind is not wasi_component"));
    }
    reject_package_path(Path::new(&artifact.path))?;
    if artifact
        .size_bytes
        .is_some_and(|expected| expected != component.len() as u64)
    {
        return Err(protocol_mismatch("component size does not match manifest"));
    }
    let digest = hex::encode(Sha256::digest(component));
    if !digest.eq_ignore_ascii_case(&artifact.sha256) {
        return Err(protocol_mismatch(
            "component SHA-256 does not match manifest",
        ));
    }
    validate_metadata(&manifest)?;
    Ok(manifest)
}

#[derive(Debug, Clone)]
pub struct VerifiedComponentPackage {
    pub root: PathBuf,
    pub component_path: PathBuf,
    pub component_sha256: String,
    pub manifest: PluginManifestV3,
    pub permissions: ComponentPermissions,
}

#[derive(Debug, Clone)]
pub struct WasiComponentPackageVerifier {
    core_version: Version,
}

impl WasiComponentPackageVerifier {
    pub fn new(core_version: Version) -> Self {
        Self { core_version }
    }

    pub fn verify_directory(
        &self,
        package_root: impl AsRef<Path>,
    ) -> Result<VerifiedComponentPackage, WasiDiagnostic> {
        let root = package_root.as_ref().canonicalize().map_err(host_failure)?;
        reject_symlink_tree(&root)?;
        let manifest_path = root.join(MANIFEST_FILE);
        let manifest_bytes = read_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
        let manifest: PluginManifestV3 = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| protocol_mismatch(error.to_string()))?;
        manifest
            .validate_contract()
            .map_err(|error| protocol_mismatch(error.to_string()))?;
        if manifest.execution_class != PluginExecutionClassV3::WasiComponent
            || manifest.interfaces.component_wit != Some(COMPONENT_WIT_VERSION_V1)
        {
            return Err(protocol_mismatch(
                "package is not a Component WIT v1 plugin",
            ));
        }
        let requirement = VersionReq::parse(&manifest.core_version_requirement)
            .map_err(|error| protocol_mismatch(error.to_string()))?;
        if !requirement.matches(&self.core_version) {
            return Err(protocol_mismatch(
                "plugin does not support this core version",
            ));
        }
        let artifact = manifest
            .artifact
            .as_ref()
            .ok_or_else(|| protocol_mismatch("component artifact is missing"))?;
        if artifact.kind != PluginArtifactKindV3::WasiComponent {
            return Err(protocol_mismatch("artifact kind is not wasi_component"));
        }
        let artifact_relative = Path::new(&artifact.path);
        reject_package_path(artifact_relative)?;
        let component_path = root.join(artifact_relative);
        let canonical_component = component_path.canonicalize().map_err(host_failure)?;
        if !canonical_component.starts_with(&root) {
            return Err(protocol_mismatch("component artifact escapes package root"));
        }
        let component = read_bounded(&canonical_component, MAX_COMPONENT_BYTES)?;
        let digest = hex::encode(Sha256::digest(&component));
        let manifest =
            verify_component_artifact_bytes(&self.core_version, &manifest_bytes, &component)?;
        let permissions = ComponentPermissions::from_manifest(&manifest.permissions, &root)?;
        Ok(VerifiedComponentPackage {
            root,
            component_path: canonical_component,
            component_sha256: digest,
            manifest,
            permissions,
        })
    }

    pub fn verify_path(
        &self,
        package: impl AsRef<Path>,
    ) -> Result<VerifiedComponentPackage, WasiDiagnostic> {
        let metadata = fs::symlink_metadata(package.as_ref()).map_err(host_failure)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(protocol_mismatch(
                "only unpacked directory packages are supported; archives and compressed packages are rejected",
            ));
        }
        self.verify_directory(package)
    }
}

fn validate_metadata(manifest: &PluginManifestV3) -> Result<(), WasiDiagnostic> {
    if manifest.license.as_deref().is_none_or(str::is_empty) {
        return Err(protocol_mismatch("external component license is missing"));
    }
    if let Some(signature) = &manifest.signature {
        if signature.algorithm != "ed25519"
            || signature.key_id.len() > 256
            || signature.signature.len() != 128
            || !signature
                .signature
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(protocol_mismatch(
                "component signature metadata is malformed",
            ));
        }
    }
    if let Some(schema) = &manifest.configuration_schema {
        if !schema.is_object() {
            return Err(protocol_mismatch("configuration schema must be an object"));
        }
    }
    Ok(())
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<Vec<u8>, WasiDiagnostic> {
    let metadata = fs::symlink_metadata(path).map_err(host_failure)?;
    if !metadata.file_type().is_file() || metadata.len() > max_bytes {
        return Err(protocol_mismatch(
            "package file has an invalid type or size",
        ));
    }
    fs::read(path).map_err(host_failure)
}

fn reject_symlink_tree(root: &Path) -> Result<(), WasiDiagnostic> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(host_failure)? {
            let entry = entry.map_err(host_failure)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(host_failure)?;
            if metadata.file_type().is_symlink() {
                return Err(protocol_mismatch(
                    "component packages cannot contain symlinks",
                ));
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(())
}

fn reject_package_path(path: &Path) -> Result<(), WasiDiagnostic> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(protocol_mismatch(
            "artifact path is not a safe relative path",
        ));
    }
    Ok(())
}

fn protocol_mismatch(message: impl Into<String>) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiProtocolMismatch, message)
}

fn host_failure(error: impl std::fmt::Display) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiHostFailure, error.to_string())
}
