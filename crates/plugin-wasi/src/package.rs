use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use latexsnipper_plugin::{
    PluginArtifactKindV3, PluginExecutionClassV3, PluginManifestV3, COMPONENT_WIT_VERSION_V1,
};
use semver::{Version, VersionReq};
use sha2::{Digest, Sha256};

use crate::{ComponentPermissions, WasiDiagnostic, WasiDiagnosticCode, WasiHostPolicy};

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
        .map_err(|error| protocol_mismatch(bounded_message(error)))?;
    manifest
        .validate_contract()
        .map_err(|error| protocol_mismatch(bounded_message(error)))?;
    WasiHostPolicy::default().grant(&manifest.permissions.limits)?;
    validate_component_contract(core_version, &manifest, component)?;
    validate_metadata(&manifest)?;
    Ok(manifest)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasiPackagePolicy {
    pub max_entries: usize,
    pub max_files: usize,
    pub max_directories: usize,
    pub max_recursion_depth: usize,
    pub max_total_bytes: u64,
    pub max_metadata_bytes: u64,
    pub max_path_bytes: usize,
    pub verification_timeout: Duration,
}

impl Default for WasiPackagePolicy {
    fn default() -> Self {
        Self {
            max_entries: 128,
            max_files: 96,
            max_directories: 32,
            max_recursion_depth: 8,
            max_total_bytes: 320 * 1024 * 1024,
            max_metadata_bytes: MAX_MANIFEST_BYTES,
            max_path_bytes: 1024,
            verification_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedComponentPackage {
    pub root: PathBuf,
    pub component_path: PathBuf,
    pub component_sha256: String,
    pub manifest: PluginManifestV3,
    pub permissions: ComponentPermissions,
    package_directory: Arc<Dir>,
    component_relative: PathBuf,
    component_limit: u64,
}

impl VerifiedComponentPackage {
    pub(crate) fn read_component_for_compilation(&self) -> Result<Vec<u8>, WasiDiagnostic> {
        let bytes = read_bounded(
            &self.package_directory,
            &self.component_relative,
            self.component_limit,
            None,
        )?;
        let digest = hex::encode(Sha256::digest(&bytes));
        if digest != self.component_sha256 {
            return Err(protocol_mismatch(
                "component changed after package verification",
            ));
        }
        Ok(bytes)
    }
}

#[derive(Debug, Clone)]
pub struct WasiComponentPackageVerifier {
    core_version: Version,
    host_policy: WasiHostPolicy,
    package_policy: WasiPackagePolicy,
}

impl WasiComponentPackageVerifier {
    pub fn new(core_version: Version) -> Self {
        Self {
            core_version,
            host_policy: WasiHostPolicy::default(),
            package_policy: WasiPackagePolicy::default(),
        }
    }

    pub fn with_host_policy(mut self, policy: WasiHostPolicy) -> Self {
        self.host_policy = policy;
        self
    }

    pub fn with_package_policy(mut self, policy: WasiPackagePolicy) -> Self {
        self.package_policy = policy;
        self
    }

    pub fn verify_directory(
        &self,
        package_root: impl AsRef<Path>,
    ) -> Result<VerifiedComponentPackage, WasiDiagnostic> {
        let metadata = std::fs::symlink_metadata(package_root.as_ref()).map_err(host_failure)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(protocol_mismatch("package root is not a real directory"));
        }
        let root = package_root.as_ref().canonicalize().map_err(host_failure)?;
        let package_directory =
            Dir::open_ambient_dir(&root, ambient_authority()).map_err(host_failure)?;
        let deadline = Instant::now()
            .checked_add(self.package_policy.verification_timeout)
            .ok_or_else(|| host_failure("package verification deadline overflow"))?;
        let tree = inspect_package_tree(&package_directory, &self.package_policy, deadline)?;
        let manifest_entry = tree
            .files
            .get(MANIFEST_FILE)
            .ok_or_else(|| protocol_mismatch("component package is missing plugin.json"))?;
        let manifest_bytes = read_bounded(
            &package_directory,
            &manifest_entry.relative,
            self.package_policy.max_metadata_bytes,
            Some(manifest_entry.size),
        )?;
        let manifest: PluginManifestV3 = serde_json::from_slice(&manifest_bytes)
            .map_err(|error| protocol_mismatch(bounded_message(error)))?;
        manifest
            .validate_contract()
            .map_err(|error| protocol_mismatch(bounded_message(error)))?;
        self.host_policy.grant(&manifest.permissions.limits)?;
        validate_manifest_contract(&self.core_version, &manifest)?;
        let artifact = manifest
            .artifact
            .as_ref()
            .ok_or_else(|| protocol_mismatch("component artifact is missing"))?;
        let artifact_relative = Path::new(&artifact.path);
        reject_package_path(artifact_relative)?;
        let artifact_key = normalized_package_path(artifact_relative, &self.package_policy)?;
        let component_entry = tree
            .files
            .get(&artifact_key)
            .ok_or_else(|| protocol_mismatch("declared component artifact is missing"))?;
        validate_allowed_tree(&tree, &manifest, &artifact_key, &self.package_policy)?;
        let component = read_bounded(
            &package_directory,
            &component_entry.relative,
            MAX_COMPONENT_BYTES,
            Some(component_entry.size),
        )?;
        validate_component_contract(&self.core_version, &manifest, &component)?;
        validate_metadata(&manifest)?;
        let permissions = ComponentPermissions::from_manifest_with_directory(
            &manifest.permissions,
            &root,
            &package_directory,
            &self.host_policy,
        )?;
        Ok(VerifiedComponentPackage {
            root: root.clone(),
            component_path: root.join(&component_entry.relative),
            component_sha256: hex::encode(Sha256::digest(&component)),
            manifest,
            permissions,
            package_directory: Arc::new(package_directory),
            component_relative: component_entry.relative.clone(),
            component_limit: MAX_COMPONENT_BYTES,
        })
    }

    pub fn verify_path(
        &self,
        package: impl AsRef<Path>,
    ) -> Result<VerifiedComponentPackage, WasiDiagnostic> {
        let metadata = std::fs::symlink_metadata(package.as_ref()).map_err(host_failure)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(protocol_mismatch(
                "only unpacked directory packages are supported; archives and compressed packages are rejected",
            ));
        }
        self.verify_directory(package)
    }
}

#[derive(Debug)]
struct PackageTree {
    files: BTreeMap<String, PackageFile>,
    directories: BTreeSet<String>,
}

#[derive(Debug)]
struct PackageFile {
    relative: PathBuf,
    size: u64,
}

fn inspect_package_tree(
    root: &Dir,
    policy: &WasiPackagePolicy,
    deadline: Instant,
) -> Result<PackageTree, WasiDiagnostic> {
    let mut pending = vec![(
        root.try_clone().map_err(host_failure)?,
        PathBuf::new(),
        0usize,
    )];
    let mut files = BTreeMap::new();
    let mut directories = BTreeSet::new();
    let mut entries = 0usize;
    let mut file_count = 0usize;
    let mut directory_count = 0usize;
    let mut total_bytes = 0u64;
    while let Some((directory, relative_root, depth)) = pending.pop() {
        ensure_before_deadline(deadline)?;
        let mut children = Vec::new();
        for entry in directory.entries().map_err(host_failure)? {
            ensure_before_deadline(deadline)?;
            entries = entries
                .checked_add(1)
                .ok_or_else(|| protocol_mismatch("package entry count overflow"))?;
            if entries > policy.max_entries {
                return Err(protocol_mismatch("package contains too many entries"));
            }
            let entry = entry.map_err(host_failure)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| protocol_mismatch("package paths must be valid UTF-8"))?;
            if name.len() > policy.max_path_bytes {
                return Err(protocol_mismatch("package path exceeds length limit"));
            }
            children.push((name, entry));
        }
        children.sort_by(|left, right| left.0.cmp(&right.0));
        for (name, entry) in children {
            ensure_before_deadline(deadline)?;
            let relative = relative_root.join(&name);
            let key = normalized_package_path(&relative, policy)?;
            let file_type = entry.file_type().map_err(host_failure)?;
            if file_type.is_symlink() {
                return Err(protocol_mismatch(
                    "component packages cannot contain symlinks",
                ));
            }
            if file_type.is_dir() {
                directory_count = directory_count
                    .checked_add(1)
                    .ok_or_else(|| protocol_mismatch("package directory count overflow"))?;
                if directory_count > policy.max_directories || depth >= policy.max_recursion_depth {
                    return Err(protocol_mismatch(
                        "package directory count or recursion depth exceeds limits",
                    ));
                }
                if !directories.insert(key) {
                    return Err(protocol_mismatch(
                        "package contains duplicate normalized directory paths",
                    ));
                }
                let child = directory.open_dir_nofollow(&name).map_err(|_| {
                    protocol_mismatch("package directory changed during verification")
                })?;
                pending.push((child, relative, depth + 1));
                continue;
            }
            if !file_type.is_file() {
                return Err(protocol_mismatch(
                    "package contains an unsupported filesystem entry",
                ));
            }
            file_count = file_count
                .checked_add(1)
                .ok_or_else(|| protocol_mismatch("package file count overflow"))?;
            if file_count > policy.max_files {
                return Err(protocol_mismatch("package contains too many files"));
            }
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let file = entry
                .open_with(&options)
                .map_err(|_| protocol_mismatch("package file changed during verification"))?;
            let metadata = file.metadata().map_err(host_failure)?;
            if !metadata.is_file() || metadata.nlink() > 1 {
                return Err(protocol_mismatch(
                    "package files must be regular files with a single link",
                ));
            }
            total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| protocol_mismatch("package byte count overflow"))?;
            if total_bytes > policy.max_total_bytes {
                return Err(protocol_mismatch("package total bytes exceed limit"));
            }
            if files
                .insert(
                    key,
                    PackageFile {
                        relative,
                        size: metadata.len(),
                    },
                )
                .is_some()
            {
                return Err(protocol_mismatch(
                    "package contains duplicate normalized file paths",
                ));
            }
        }
    }
    Ok(PackageTree { files, directories })
}

fn validate_allowed_tree(
    tree: &PackageTree,
    manifest: &PluginManifestV3,
    artifact_key: &str,
    policy: &WasiPackagePolicy,
) -> Result<(), WasiDiagnostic> {
    let grant_roots = manifest
        .permissions
        .paths
        .iter()
        .map(|grant| normalized_package_path(Path::new(&grant.path), policy))
        .collect::<Result<Vec<_>, _>>()?;
    for (key, file) in &tree.files {
        let allowed = key == MANIFEST_FILE
            || key == artifact_key
            || is_bounded_metadata_file(key, file.size, policy.max_metadata_bytes)
            || grant_roots
                .iter()
                .any(|root| key == root || key.starts_with(&format!("{root}/")));
        if !allowed {
            return Err(protocol_mismatch(format!(
                "package contains undeclared payload: {key}"
            )));
        }
    }
    for directory in &tree.directories {
        let allowed = directory == "docs"
            || directory.starts_with("docs/")
            || grant_roots
                .iter()
                .any(|root| directory == root || directory.starts_with(&format!("{root}/")));
        if !allowed {
            return Err(protocol_mismatch(format!(
                "package contains undeclared directory: {directory}"
            )));
        }
    }
    Ok(())
}

fn is_bounded_metadata_file(path: &str, size: u64, maximum: u64) -> bool {
    if size > maximum {
        return false;
    }
    matches!(
        path,
        "readme"
            | "readme.md"
            | "readme.txt"
            | "license"
            | "license.md"
            | "license.txt"
            | "icon.png"
            | "icon.svg"
            | "configuration.schema.json"
    ) || (path.starts_with("docs/") && (path.ends_with(".md") || path.ends_with(".txt")))
}

fn validate_component_contract(
    core_version: &Version,
    manifest: &PluginManifestV3,
    component: &[u8],
) -> Result<(), WasiDiagnostic> {
    validate_manifest_contract(core_version, manifest)?;
    let artifact = manifest
        .artifact
        .as_ref()
        .ok_or_else(|| protocol_mismatch("component artifact is missing"))?;
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
    Ok(())
}

fn validate_manifest_contract(
    core_version: &Version,
    manifest: &PluginManifestV3,
) -> Result<(), WasiDiagnostic> {
    if manifest.execution_class != PluginExecutionClassV3::WasiComponent
        || manifest.interfaces.component_wit != Some(COMPONENT_WIT_VERSION_V1)
    {
        return Err(protocol_mismatch(
            "package is not a Component WIT v1 plugin",
        ));
    }
    let requirement = VersionReq::parse(&manifest.core_version_requirement)
        .map_err(|error| protocol_mismatch(bounded_message(error)))?;
    if !requirement.matches(core_version) {
        return Err(protocol_mismatch(
            "plugin does not support this core version",
        ));
    }
    let artifact = manifest
        .artifact
        .as_ref()
        .ok_or_else(|| protocol_mismatch("component artifact is missing"))?;
    if !valid_manifest_path_text(&artifact.path) {
        return Err(protocol_mismatch(
            "artifact path is not portable across supported platforms",
        ));
    }
    if artifact.kind != PluginArtifactKindV3::WasiComponent {
        return Err(protocol_mismatch("artifact kind is not wasi_component"));
    }
    reject_package_path(Path::new(&artifact.path))
}

fn validate_metadata(manifest: &PluginManifestV3) -> Result<(), WasiDiagnostic> {
    if manifest.license.as_deref().is_none_or(str::is_empty) {
        return Err(protocol_mismatch("external component license is missing"));
    }
    if let Some(signature) = &manifest.signature {
        if signature.algorithm != "ed25519"
            || signature.key_id.is_empty()
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
        let bytes = serde_json::to_vec(schema).map_err(host_failure)?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(protocol_mismatch("configuration schema exceeds size limit"));
        }
    }
    let normalized_capabilities = manifest
        .capabilities
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    let normalized_paths = manifest
        .permissions
        .paths
        .iter()
        .map(|grant| grant.path.replace('\\', "/").to_ascii_lowercase())
        .collect::<Vec<_>>();
    let normalized_environment = manifest
        .permissions
        .environment_variables
        .iter()
        .map(|value| value.trim().to_ascii_uppercase())
        .collect::<Vec<_>>();
    let normalized_models = manifest
        .permissions
        .model_artifacts
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<Vec<_>>();
    let network_destinations = manifest
        .permissions
        .network
        .iter()
        .map(|value| {
            (
                value.scheme,
                value.host.trim().trim_end_matches('.').to_ascii_lowercase(),
                value.port,
            )
        })
        .collect::<Vec<_>>();
    let format_endpoints = manifest
        .format_capabilities
        .iter()
        .map(|value| {
            (
                value
                    .input
                    .as_deref()
                    .map(str::trim)
                    .map(str::to_ascii_lowercase),
                value
                    .output
                    .as_deref()
                    .map(str::trim)
                    .map(str::to_ascii_lowercase),
            )
        })
        .collect::<Vec<_>>();
    if normalized_capabilities.iter().any(String::is_empty)
        || manifest
            .permissions
            .paths
            .iter()
            .any(|grant| !valid_manifest_path_text(&grant.path))
        || normalized_environment.iter().any(String::is_empty)
        || normalized_models.iter().any(String::is_empty)
        || format_endpoints
            .iter()
            .any(|(input, output)| input.is_none() && output.is_none())
        || has_duplicates(&normalized_capabilities)
        || has_duplicates(&normalized_paths)
        || has_duplicates(&normalized_environment)
        || has_duplicates(&normalized_models)
        || has_duplicates(&network_destinations)
        || has_duplicates(&format_endpoints)
        || has_duplicates(&manifest.hooks)
    {
        return Err(protocol_mismatch(
            "component manifest contains empty or duplicate authority declarations",
        ));
    }
    Ok(())
}

fn has_duplicates<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
}

fn valid_manifest_path_text(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && !value.contains(':')
        && value.split('/').all(|segment| {
            !segment.is_empty() && !segment.ends_with('.') && !segment.ends_with(' ')
        })
}

fn read_bounded(
    directory: &Dir,
    path: &Path,
    max_bytes: u64,
    expected_size: Option<u64>,
) -> Result<Vec<u8>, WasiDiagnostic> {
    reject_package_path(path)?;
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = directory
        .open_with(path, &options)
        .map_err(|_| protocol_mismatch("package file changed or is not safely accessible"))?;
    let before = file.metadata().map_err(host_failure)?;
    if !before.is_file()
        || before.nlink() > 1
        || before.len() > max_bytes
        || expected_size.is_some_and(|expected| expected != before.len())
    {
        return Err(protocol_mismatch(
            "package file has an invalid type, link count, or size",
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(before.len()).unwrap_or(0));
    (&mut file)
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(host_failure)?;
    let after = file.metadata().map_err(host_failure)?;
    if bytes.len() as u64 != before.len()
        || bytes.len() as u64 > max_bytes
        || after.len() != before.len()
        || after.nlink() != before.nlink()
    {
        return Err(protocol_mismatch(
            "package file changed during verification",
        ));
    }
    Ok(bytes)
}

fn normalized_package_path(
    path: &Path,
    policy: &WasiPackagePolicy,
) -> Result<String, WasiDiagnostic> {
    reject_package_path(path)?;
    let text = path
        .to_str()
        .ok_or_else(|| protocol_mismatch("package paths must be valid UTF-8"))?;
    if text.len() > policy.max_path_bytes {
        return Err(protocol_mismatch("package path exceeds length limit"));
    }
    let mut normalized = Vec::new();
    for component in path.components() {
        let Component::Normal(segment) = component else {
            return Err(protocol_mismatch("package path is not normalized"));
        };
        let segment = segment
            .to_str()
            .ok_or_else(|| protocol_mismatch("package paths must be valid UTF-8"))?;
        if segment.is_empty()
            || segment.ends_with('.')
            || segment.ends_with(' ')
            || segment.contains(':')
        {
            return Err(protocol_mismatch(
                "package path is ambiguous on supported platforms",
            ));
        }
        normalized.push(segment.to_ascii_lowercase());
    }
    Ok(normalized.join("/"))
}

fn reject_package_path(path: &Path) -> Result<(), WasiDiagnostic> {
    let text = path
        .to_str()
        .ok_or_else(|| protocol_mismatch("package paths must be valid UTF-8"))?;
    if text.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
        })
    {
        return Err(protocol_mismatch(
            "artifact path is not a safe normalized relative path",
        ));
    }
    Ok(())
}

fn ensure_before_deadline(deadline: Instant) -> Result<(), WasiDiagnostic> {
    if Instant::now() >= deadline {
        return Err(WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiTimeout,
            "package verification exceeded its deadline",
        ));
    }
    Ok(())
}

fn bounded_message(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    let boundary = message
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= 1021)
        .last()
        .unwrap_or(0);
    if message.len() <= 1024 {
        message
    } else {
        format!("{}...", &message[..boundary])
    }
}

fn protocol_mismatch(message: impl Into<String>) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiProtocolMismatch, message)
}

fn host_failure(error: impl std::fmt::Display) -> WasiDiagnostic {
    WasiDiagnostic::new(
        WasiDiagnosticCode::PluginWasiHostFailure,
        bounded_message(error),
    )
}
