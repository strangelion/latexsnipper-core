use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, TryLockError};

use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use latexsnipper_plugin::{NetworkSchemeV3, PluginPathAccessV3, PluginPermissionsV3};
use sha2::{Digest, Sha256};

use crate::{WasiDiagnostic, WasiDiagnosticCode, WasiHostPolicy, WasiResourceLimits};

#[derive(Debug, Clone)]
pub struct FilesystemGrant {
    pub id: String,
    pub root: PathBuf,
    pub writable: bool,
    directory: Arc<Dir>,
    publication: Arc<Mutex<()>>,
}

impl FilesystemGrant {
    #[cfg(windows)]
    pub(crate) fn directory(&self) -> &Dir {
        &self.directory
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemOperationError {
    PermissionDenied,
    NotFound,
    InvalidPath,
    SizeLimit,
    Conflict,
    MetadataUnsupported,
    HostFailure,
}

#[derive(Debug)]
pub(crate) enum CheckedWriteError<E> {
    Filesystem(FilesystemOperationError),
    Interrupted(E),
}

impl<E> From<FilesystemOperationError> for CheckedWriteError<E> {
    fn from(error: FilesystemOperationError) -> Self {
        Self::Filesystem(error)
    }
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
                    publication: Arc::new(Mutex::new(())),
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
        self.read_file_with_parent_policy(grant_id, relative, limit, false)
    }

    pub(crate) fn read_file_no_follow_parents(
        &self,
        grant_id: &str,
        relative: &str,
        limit: usize,
    ) -> Result<Vec<u8>, FilesystemOperationError> {
        self.read_file_with_parent_policy(grant_id, relative, limit, true)
    }

    fn read_file_with_parent_policy(
        &self,
        grant_id: &str,
        relative: &str,
        limit: usize,
        no_follow_parents: bool,
    ) -> Result<Vec<u8>, FilesystemOperationError> {
        let grant = self
            .filesystem
            .get(grant_id)
            .ok_or(FilesystemOperationError::PermissionDenied)?;
        let relative = Path::new(relative);
        reject_unsafe_relative_path(relative)?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = if no_follow_parents {
            let (parent, target) = write_parent(grant, relative)?;
            parent.open_with(target, &options).map_err(map_io_error)?
        } else {
            grant
                .directory
                .open_with(relative, &options)
                .map_err(map_io_error)?
        };
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
        let grant = self.validate_write_target(grant_id, relative)?;
        if payload.len() > limit {
            return Err(FilesystemOperationError::SizeLimit);
        }
        let (parent, target) = write_parent(grant, Path::new(relative))?;
        let expected = target_digest(&parent, &target, limit)?;
        match self.write_file_checked(
            grant_id,
            relative,
            payload,
            limit,
            expected.as_deref(),
            || Ok::<(), std::convert::Infallible>(()),
        ) {
            Ok(()) => Ok(()),
            Err(CheckedWriteError::Filesystem(error)) => Err(error),
            Err(CheckedWriteError::Interrupted(never)) => match never {},
        }
    }

    pub(crate) fn write_file_checked<E>(
        &self,
        grant_id: &str,
        relative: &str,
        payload: &[u8],
        limit: usize,
        expected: Option<&str>,
        mut checkpoint: impl FnMut() -> Result<(), E>,
    ) -> Result<(), CheckedWriteError<E>> {
        checkpoint().map_err(CheckedWriteError::Interrupted)?;
        let grant = self.validate_write_target(grant_id, relative)?;
        if payload.len() > limit {
            return Err(FilesystemOperationError::SizeLimit.into());
        }
        let (parent, target) = write_parent(grant, Path::new(relative))?;
        #[cfg(windows)]
        let basic = crate::write_basic_metadata::WriteBasicMetadata::read(&parent, &target)?;
        let security = crate::write_security::WriteSecurity::read(&parent, &target)?;
        #[cfg(windows)]
        let compression = crate::write_compression::WriteCompression::read(&parent, &target)?;
        #[cfg(windows)]
        let streams =
            crate::write_streams::WriteStreams::read(&parent, &target, limit, &mut checkpoint)?;
        let (mut staged, mut file) = StagedWrite::create(&parent, &security)?;
        #[cfg(windows)]
        basic.prepare(&file)?;
        #[cfg(windows)]
        compression.apply(&file)?;
        for chunk in payload.chunks(64 * 1024) {
            checkpoint().map_err(CheckedWriteError::Interrupted)?;
            file.write_all(chunk).map_err(map_io_error)?;
        }
        #[cfg(windows)]
        streams.apply(&parent, &staged.name, &mut checkpoint)?;
        #[cfg(windows)]
        compression.verify_staging(&file)?;
        #[cfg(windows)]
        basic.apply(&file)?;
        file.sync_all().map_err(map_io_error)?;
        // Clones of this grant share publication ordering. Other hosts and
        // external editors still require a final observed-content check.
        let _publication = loop {
            checkpoint().map_err(CheckedWriteError::Interrupted)?;
            match grant.publication.try_lock() {
                Ok(lock) => break lock,
                Err(TryLockError::WouldBlock) => std::thread::yield_now(),
                Err(TryLockError::Poisoned(_)) => {
                    return Err(FilesystemOperationError::HostFailure.into())
                }
            }
        };
        if target_digest(&parent, &target, limit)?.as_deref() != expected {
            return Err(FilesystemOperationError::Conflict.into());
        }
        security.recheck(&parent, &target)?;
        #[cfg(windows)]
        {
            compression.recheck(&parent, &target)?;
            streams.recheck(&parent, &target, limit, &mut checkpoint)?;
            streams.recheck(&parent, Path::new(&staged.name), limit, &mut checkpoint)?;
            basic.recheck(&parent, &target)?;
            basic.recheck_staging(&parent, Path::new(&staged.name))?;
        }
        // Preserve ordinary permissions as well as platform staging security.
        if let Ok(metadata) = parent.symlink_metadata(&target) {
            let mut options = OpenOptions::new();
            options.write(true).follow(FollowSymlinks::No);
            drop(parent.open_with(&target, &options).map_err(map_io_error)?);
            let permissions = metadata.permissions();
            #[cfg(unix)]
            let permissions = {
                use cap_std::fs::{Permissions, PermissionsExt};
                // Replacing content must not restore setuid/setgid bits.
                Permissions::from_mode(permissions.mode() & 0o777)
            };
            // Apply through the owned file handle. On Windows the directory
            // adapter reconstructs permissions from its parent and may copy
            // unrelated directory attributes to the staging path.
            file.set_permissions(permissions).map_err(map_io_error)?;
        }
        #[cfg(windows)]
        basic.recheck_staging(&parent, Path::new(&staged.name))?;
        file.sync_all().map_err(map_io_error)?;
        drop(file);
        checkpoint().map_err(CheckedWriteError::Interrupted)?;
        parent
            .rename(&staged.name, &parent, &target)
            .map_err(map_io_error)?;
        staged.published = true;
        Ok(())
    }

    pub(crate) fn validate_write_target(
        &self,
        grant_id: &str,
        relative: &str,
    ) -> Result<&FilesystemGrant, FilesystemOperationError> {
        let grant = self
            .filesystem
            .get(grant_id)
            .filter(|grant| grant.writable)
            .ok_or(FilesystemOperationError::PermissionDenied)?;
        let relative = Path::new(relative);
        let (parent, target) = write_parent(grant, relative)?;
        match parent.symlink_metadata(&target) {
            Ok(metadata)
                if !metadata.is_file() || metadata.is_symlink() || metadata.nlink() > 1 =>
            {
                return Err(FilesystemOperationError::InvalidPath)
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(map_io_error(error))
            }
            _ => {}
        }
        #[cfg(windows)]
        crate::write_basic_metadata::WriteBasicMetadata::read(&parent, &target)?;
        #[cfg(windows)]
        crate::write_compression::WriteCompression::read(&parent, &target)?;
        #[cfg(windows)]
        crate::write_security::WriteSecurity::read(&parent, &target)?;
        Ok(grant)
    }

    pub fn permits_network(&self, grant: &NetworkGrant) -> bool {
        self.network.contains(&NetworkGrant {
            scheme: grant.scheme,
            host: normalize_host(&grant.host),
            port: grant.port,
        })
    }
}

fn write_parent(
    grant: &FilesystemGrant,
    relative: &Path,
) -> Result<(Dir, PathBuf), FilesystemOperationError> {
    reject_unsafe_relative_path(relative)?;
    let mut components = relative
        .components()
        .filter(|part| *part != Component::CurDir)
        .peekable();
    let mut directory = grant.directory.try_clone().map_err(map_io_error)?;
    while let Some(Component::Normal(name)) = components.next() {
        if components.peek().is_none() {
            return Ok((directory, PathBuf::from(name)));
        }
        directory = directory.open_dir_nofollow(name).map_err(map_io_error)?;
    }
    Err(FilesystemOperationError::InvalidPath)
}

fn target_digest(
    parent: &Dir,
    target: &Path,
    limit: usize,
) -> Result<Option<String>, FilesystemOperationError> {
    match parent.symlink_metadata(target) {
        Ok(metadata) if !metadata.is_file() || metadata.is_symlink() || metadata.nlink() > 1 => {
            return Err(FilesystemOperationError::InvalidPath)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(map_io_error(error)),
        _ => {}
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = parent.open_with(target, &options).map_err(map_io_error)?;
    let metadata = file.metadata().map_err(map_io_error)?;
    if !metadata.is_file() || metadata.nlink() > 1 {
        return Err(FilesystemOperationError::InvalidPath);
    }
    if metadata.len() > u64::try_from(limit).unwrap_or(u64::MAX) {
        return Err(FilesystemOperationError::SizeLimit);
    }
    let mut digest = Sha256::new();
    let mut reader = file.take(u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1));
    let mut count = 0_usize;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = reader.read(&mut buffer).map_err(map_io_error)?;
        if length == 0 {
            break;
        }
        count = count.saturating_add(length);
        if count > limit {
            return Err(FilesystemOperationError::SizeLimit);
        }
        digest.update(&buffer[..length]);
    }
    Ok(Some(hex::encode(digest.finalize())))
}

struct StagedWrite<'a> {
    parent: &'a Dir,
    name: String,
    published: bool,
}

impl<'a> StagedWrite<'a> {
    fn create(
        parent: &'a Dir,
        security: &crate::write_security::WriteSecurity,
    ) -> Result<(Self, cap_std::fs::File), FilesystemOperationError> {
        for _ in 0..8 {
            let mut nonce = [0_u8; 32];
            getrandom::fill(&mut nonce).map_err(|_| FilesystemOperationError::HostFailure)?;
            let name = format!(".snipper-write-{}.tmp", hex::encode(nonce));
            match security.create(parent, &name) {
                Ok(file) => {
                    return Ok((
                        Self {
                            parent,
                            name,
                            published: false,
                        },
                        file,
                    ))
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(crate::write_security::map_security_error(error)),
            }
        }
        Err(FilesystemOperationError::HostFailure)
    }
}

impl Drop for StagedWrite<'_> {
    fn drop(&mut self) {
        // Only this invocation's exclusively created staging file is removed.
        if !self.published {
            let _ = self.parent.remove_file(&self.name);
        }
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

pub(crate) fn map_io_error(error: std::io::Error) -> FilesystemOperationError {
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

    fn writable_permissions(root: &Path) -> ComponentPermissions {
        let mut permissions = ComponentPermissions::deny_all();
        permissions.filesystem.insert(
            "test".to_string(),
            FilesystemGrant {
                id: "test".to_string(),
                root: root.to_path_buf(),
                writable: true,
                directory: Arc::new(Dir::open_ambient_dir(root, ambient_authority()).unwrap()),
                publication: Arc::new(Mutex::new(())),
            },
        );
        permissions
    }

    fn assert_no_staging(root: &Path) {
        assert!(std::fs::read_dir(root).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".snipper-write-")));
    }

    #[test]
    fn staged_write_replaces_existing_and_creates_nested_target() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("old"), b"old content").unwrap();
        std::fs::create_dir(temporary.path().join("nested")).unwrap();
        let permissions = writable_permissions(temporary.path());
        permissions.write_file("test", "old", b"new", 1024).unwrap();
        permissions
            .write_file("test", "nested/new", b"created", 1024)
            .unwrap();
        assert_eq!(std::fs::read(temporary.path().join("old")).unwrap(), b"new");
        assert_eq!(
            std::fs::read(temporary.path().join("nested/new")).unwrap(),
            b"created"
        );
        assert_no_staging(temporary.path());
        assert_no_staging(&temporary.path().join("nested"));
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn interrupted_partial_staging_preserves_existing_and_absent_targets() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("old"), b"original").unwrap();
        let permissions = writable_permissions(temporary.path());
        let payload = vec![b'x'; 128 * 1024];
        for (target, expected) in [
            ("old", Some(hex::encode(Sha256::digest(b"original")))),
            ("new", None),
        ] {
            let mut checkpoints = 0;
            let result = permissions.write_file_checked(
                "test",
                target,
                &payload,
                payload.len(),
                expected.as_deref(),
                || {
                    checkpoints += 1;
                    if checkpoints == 3 {
                        Err("cancelled after first chunk")
                    } else {
                        Ok(())
                    }
                },
            );
            assert!(matches!(result, Err(CheckedWriteError::Interrupted(_))));
            assert_no_staging(temporary.path());
        }
        assert_eq!(
            std::fs::read(temporary.path().join("old")).unwrap(),
            b"original"
        );
        assert!(!temporary.path().join("new").exists());
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn changed_or_newly_created_target_refuses_publication_without_retry() {
        let temporary = tempfile::tempdir().unwrap();
        let permissions = writable_permissions(temporary.path());
        for expected in [None, Some(hex::encode(Sha256::digest(b"old")))] {
            std::fs::write(temporary.path().join("target"), b"external edit").unwrap();
            let result = permissions.write_file_checked(
                "test",
                "target",
                b"candidate",
                1024,
                expected.as_deref(),
                || Ok::<_, ()>(()),
            );
            assert!(matches!(
                result,
                Err(CheckedWriteError::Filesystem(
                    FilesystemOperationError::Conflict
                ))
            ));
            assert_eq!(
                std::fs::read(temporary.path().join("target")).unwrap(),
                b"external edit"
            );
            assert_no_staging(temporary.path());
        }
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn final_checkpoint_prevents_publication_and_empty_payload_is_safe() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("target"), b"original").unwrap();
        let permissions = writable_permissions(temporary.path());
        let expected = hex::encode(Sha256::digest(b"original"));
        let mut checkpoints = 0;
        let result =
            permissions.write_file_checked("test", "target", b"", 1024, Some(&expected), || {
                checkpoints += 1;
                if checkpoints == 3 {
                    Err("deadline before rename")
                } else {
                    Ok(())
                }
            });
        assert!(matches!(result, Err(CheckedWriteError::Interrupted(_))));
        assert_eq!(
            std::fs::read(temporary.path().join("target")).unwrap(),
            b"original"
        );
        assert_no_staging(temporary.path());
        permissions.write_file("test", "target", b"", 1024).unwrap();
        assert!(std::fs::read(temporary.path().join("target"))
            .unwrap()
            .is_empty());
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn concurrent_cloned_grants_publish_only_one_observed_preimage() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("target"), b"original").unwrap();
        let permissions = writable_permissions(temporary.path());
        let barrier = Arc::new(std::sync::Barrier::new(8));
        let expected = hex::encode(Sha256::digest(b"original"));
        let workers: Vec<_> = (0_u8..8)
            .map(|index| {
                let permissions = permissions.clone();
                let barrier = barrier.clone();
                let expected = expected.clone();
                std::thread::spawn(move || {
                    let mut first = true;
                    permissions.write_file_checked(
                        "test",
                        "target",
                        &[index],
                        1024,
                        Some(&expected),
                        || {
                            if first {
                                first = false;
                                barrier.wait();
                            }
                            Ok::<_, ()>(())
                        },
                    )
                })
            })
            .collect();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(
                    result,
                    Err(CheckedWriteError::Filesystem(
                        FilesystemOperationError::Conflict
                    ))
                ))
                .count(),
            7
        );
        assert_no_staging(temporary.path());
        drop(permissions);
        temporary.close().unwrap();
    }

    #[test]
    fn staging_retains_size_path_and_hardlink_restrictions() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("original"), b"original").unwrap();
        std::fs::hard_link(
            temporary.path().join("original"),
            temporary.path().join("linked"),
        )
        .unwrap();
        let mut permissions = writable_permissions(temporary.path());
        assert_eq!(
            permissions.write_file("test", "linked", b"changed", 1024),
            Err(FilesystemOperationError::InvalidPath)
        );
        assert_eq!(
            permissions.write_file("test", "../escape", b"changed", 1024),
            Err(FilesystemOperationError::InvalidPath)
        );
        assert_eq!(
            permissions.write_file("test", "new", b"too big", 1),
            Err(FilesystemOperationError::SizeLimit)
        );
        permissions.filesystem.get_mut("test").unwrap().writable = false;
        assert_eq!(
            permissions.write_file("test", "new", b"x", 1024),
            Err(FilesystemOperationError::PermissionDenied)
        );
        assert_eq!(
            std::fs::read(temporary.path().join("original")).unwrap(),
            b"original"
        );
        assert_no_staging(temporary.path());
        drop(permissions);
        temporary.close().unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_locked_target_failure_preserves_original_and_cleans_candidate() {
        use std::os::windows::fs::OpenOptionsExt;
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("target");
        std::fs::write(&path, b"original").unwrap();
        let permissions = writable_permissions(temporary.path());
        let expected = hex::encode(Sha256::digest(b"original"));
        // Allow snapshot/read and write checks, but deny rename/delete sharing.
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&path)
            .unwrap();
        let mut checkpoints = 0;
        let result = permissions.write_file_checked(
            "test",
            "target",
            b"candidate",
            1024,
            Some(&expected),
            || {
                checkpoints += 1;
                Ok::<_, ()>(())
            },
        );
        assert!(result.is_err());
        assert_eq!(checkpoints, 4);
        drop(held);
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        assert_no_staging(temporary.path());
        drop(permissions);
        temporary.close().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn unix_permissions_are_preserved_and_symlink_parents_are_refused() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("target");
        std::fs::write(&path, b"original").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        std::fs::create_dir(temporary.path().join("nested")).unwrap();
        symlink("nested", temporary.path().join("linked-dir")).unwrap();
        symlink("target", temporary.path().join("linked-file")).unwrap();
        let permissions = writable_permissions(temporary.path());
        permissions
            .write_file("test", "target", b"replacement", 1024)
            .unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(permissions
            .write_file("test", "linked-dir/new", b"x", 1024)
            .is_err());
        assert!(permissions
            .write_file("test", "linked-file", b"x", 1024)
            .is_err());
        assert!(!temporary.path().join("nested/new").exists());
        assert_no_staging(temporary.path());
        drop(permissions);
        temporary.close().unwrap();
    }

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
