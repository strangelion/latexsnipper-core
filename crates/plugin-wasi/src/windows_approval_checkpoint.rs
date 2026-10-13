//! Explicit current-user protected key/checkpoint storage. No enumeration,
//! domain credentials, certificates, installer changes or plaintext key files.
use std::fmt;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cap_fs_ext::{FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
use windows_sys::Win32::Security::Credentials::{
    CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_MAX_CREDENTIAL_BLOB_SIZE,
    CRED_PERSIST_LOCAL_MACHINE, CRED_PERSIST_SESSION, CRED_TYPE_GENERIC,
};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    ApprovalPolicyCheckpoint, ApprovalPolicyKey, RememberedApprovalAuthorityGuard,
    RememberedApprovalPolicies, RememberedPolicyIdentity, SealedApprovalPolicies, WasiDiagnostic,
    WasiDiagnosticCode,
};

#[path = "windows_approval_cleanup.rs"]
mod cleanup;
pub use cleanup::ApprovalSnapshotCleanup;

#[path = "windows_approval_isolation.rs"]
mod isolation;

#[path = "windows_approval_private_root.rs"]
mod private_root;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PublicationCut {
    StagingCreated,
    StagingChunkWritten,
    StagingSynced,
    ObjectLinked,
    ObjectReady,
    CheckpointWritten,
}

#[derive(Debug, Clone, Copy)]
pub enum WindowsApprovalPersistence {
    /// Temporary testing or deliberately session-only host choices.
    LogonSession,
    /// This user's subsequent logon sessions on this computer, not machine-wide authority.
    ThisUserThisComputer,
}

pub struct WindowsProtectedApprovalState {
    pub key: ApprovalPolicyKey,
    pub checkpoint: ApprovalPolicyCheckpoint,
}

impl fmt::Debug for WindowsProtectedApprovalState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowsProtectedApprovalState")
            .field("key", &"[REDACTED]")
            .field("checkpoint", &self.checkpoint)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RootRecord {
    schema_version: u16,
    owner_sha256: String,
    key: [u8; 32],
    checkpoint: ApprovalPolicyCheckpoint,
}
impl Drop for RootRecord {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// Low-level checkpoint backend. The host must supply an existing private root
/// outside all guest grants. Snapshot objects are authenticated, not encrypted.
pub struct WindowsProtectedApprovalCheckpoint {
    directory: Dir,
    owner_sha256: String,
    target: Vec<u16>,
    persistence: u32,
    clock: Arc<dyn Fn() -> Result<u64, WasiDiagnostic> + Send + Sync>,
    require_private_dacl: bool,
    #[cfg(test)]
    publication_probe: Option<Arc<dyn Fn(PublicationCut) + Send + Sync>>,
}

impl WindowsProtectedApprovalCheckpoint {
    /// Opens a trusted existing directory without creating credentials. The
    /// namespace binds both canonical root and opaque logical owner identity.
    pub fn open(
        root: &Path,
        owner: &str,
        persistence: WindowsApprovalPersistence,
    ) -> Result<Self, WasiDiagnostic> {
        if owner.is_empty() || owner.len() > 4096 || owner.chars().any(char::is_control) {
            return Err(failed("invalid protected approval owner identity"));
        }
        let metadata = std::fs::symlink_metadata(root)
            .map_err(|_| failed("protected approval root is unavailable"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(failed(
                "protected approval root must be a real trusted directory",
            ));
        }
        let canonical = root
            .canonicalize()
            .map_err(|_| failed("cannot resolve protected approval root"))?;
        let path = canonical
            .to_str()
            .ok_or_else(|| failed("protected approval root must be UTF-8"))?;
        let mode = match persistence {
            WindowsApprovalPersistence::LogonSession => CRED_PERSIST_SESSION,
            WindowsApprovalPersistence::ThisUserThisComputer => CRED_PERSIST_LOCAL_MACHINE,
        };
        let identity =
            serde_json::to_vec(&("core-windows-approval-checkpoint-v1", path, owner, mode))
                .map_err(|_| failed("cannot compute protected approval namespace"))?;
        let owner_sha256 = hex::encode(Sha256::digest(identity));
        let target = format!("LaTeXSnipperCore.Approval.v1/{owner_sha256}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let directory = Dir::open_ambient_dir(&canonical, cap_std::ambient_authority())
            .map_err(|_| failed("cannot open protected approval root"))?;
        Ok(Self {
            directory,
            owner_sha256,
            target,
            persistence: mode,
            clock: Arc::new(unix_time),
            require_private_dacl: false,
            #[cfg(test)]
            publication_probe: None,
        })
    }

    /// Reads only this backend's exact generic credential. Never list credentials.
    pub fn load(&self) -> Result<Option<WindowsProtectedApprovalState>, WasiDiagnostic> {
        self.check_private_dacl()?;
        let mut raw = std::ptr::null_mut();
        // SAFETY: target is NUL-terminated and raw is a writable result pointer.
        if unsafe { CredReadW(self.target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0 {
            // SAFETY: GetLastError immediately follows the failed Win32 call.
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(failed("cannot read protected approval credential"))
            };
        }
        let allocation = CredentialAllocation(
            NonNull::new(raw)
                .ok_or_else(|| failed("invalid protected approval credential allocation"))?,
        );
        // SAFETY: successful CredReadW returns one allocated CREDENTIALW block.
        let value = unsafe { allocation.0.as_ref() };
        if value.Type != CRED_TYPE_GENERIC
            || value.Persist != self.persistence
            || value.CredentialBlobSize == 0
            || value.CredentialBlobSize > CRED_MAX_CREDENTIAL_BLOB_SIZE
            || value.CredentialBlob.is_null()
        {
            return Err(failed("invalid protected approval credential shape"));
        }
        // SAFETY: the trusted OS allocation owns the checked nonnull blob/length.
        let bytes = unsafe {
            std::slice::from_raw_parts(value.CredentialBlob, value.CredentialBlobSize as usize)
        };
        let record: RootRecord = serde_json::from_slice(bytes)
            .map_err(|_| failed("invalid protected approval checkpoint declaration"))?;
        if record.schema_version != 1 || record.owner_sha256 != self.owner_sha256 {
            return Err(failed("protected approval checkpoint owner mismatch"));
        }
        validate_checkpoint(&record.checkpoint)?;
        Ok(Some(WindowsProtectedApprovalState {
            key: ApprovalPolicyKey::from_secret_bytes(record.key),
            checkpoint: record.checkpoint.clone(),
        }))
    }

    /// Saves and syncs an immutable snapshot before selecting it in protected storage.
    /// The shared directory lock prevents two cooperating processes from initializing.
    pub fn initialize(
        &self,
        key: &ApprovalPolicyKey,
        candidate: &SealedApprovalPolicies,
        deadline: Instant,
    ) -> Result<(), WasiDiagnostic> {
        validate_candidate(key, candidate)?;
        let _lock = self.lock(deadline)?;
        if self.load()?.is_some() {
            return Err(conflict("protected approval owner already exists"));
        }
        check_deadline(deadline)?;
        self.save_snapshot(key, candidate, deadline)?;
        self.write(key, &candidate.checkpoint)?;
        #[cfg(test)]
        self.test_publication_cut(PublicationCut::CheckpointWritten);
        // A late write may have committed. Report ambiguity, never silently retry.
        check_deadline(deadline)
    }

    /// Snapshot-identity CAS under a shared OS file lock. Clock-only advances
    /// do not conflict with policy updates or allow lowering the protected floor.
    /// All clients must use
    /// this same trusted root; no in-process mutex is substituted for coordination.
    pub fn publish(
        &self,
        expected: &ApprovalPolicyCheckpoint,
        candidate: &SealedApprovalPolicies,
        deadline: Instant,
    ) -> Result<(), WasiDiagnostic> {
        let _lock = self.lock(deadline)?;
        let current = self
            .load()?
            .ok_or_else(|| failed("protected approval owner is missing"))?;
        if !same_snapshot(&current.checkpoint, expected)
            || expected.observed_unix_secs > current.checkpoint.observed_unix_secs
        {
            return Err(conflict("protected approval checkpoint changed"));
        }
        let candidate_policies = validate_candidate(&current.key, candidate)?;
        if candidate.checkpoint.store_id != expected.store_id
            || candidate.checkpoint.revision < expected.revision
        {
            return Err(conflict(
                "protected approval snapshot cannot change owner or move its revision backwards",
            ));
        }
        if candidate.checkpoint == *expected {
            self.read_snapshot(expected, deadline)?;
            return Ok(());
        }
        if candidate.checkpoint.revision == expected.revision {
            let previous = SealedApprovalPolicies {
                bytes: self.read_snapshot(&current.checkpoint, deadline)?,
                checkpoint: current.checkpoint.clone(),
            };
            if candidate_policies != validate_candidate(&current.key, &previous)? {
                return Err(conflict("policy changes require a new revision"));
            }
        }
        check_deadline(deadline)?;
        self.save_snapshot(&current.key, candidate, deadline)?;
        let mut published = candidate.checkpoint.clone();
        published.observed_unix_secs = published
            .observed_unix_secs
            .max(current.checkpoint.observed_unix_secs);
        self.write(&current.key, &published)?;
        #[cfg(test)]
        self.test_publication_cut(PublicationCut::CheckpointWritten);
        check_deadline(deadline)
    }

    /// Recovers only the object selected by the protected checkpoint. Never scan
    /// old snapshots or silently initialize when selected data is missing/corrupt.
    /// Restored metadata carries a protected clock and live owner guard, but does
    /// not attach execution authority to a host or enable any backend.
    pub fn restore_policies(
        &self,
        deadline: Instant,
    ) -> Result<Option<RememberedApprovalPolicies>, WasiDiagnostic> {
        let lock = self.lock(deadline)?;
        let current = self.load()?;
        check_deadline(deadline)?;
        let Some(current) = current else {
            return Ok(None);
        };
        let bytes = self.read_snapshot(&current.checkpoint, deadline)?;
        // Never enter a policy clock while retaining the owner lock. Policy
        // clocks may hold their own store mutex and acquire this owner lock.
        drop(lock);
        let owner = Arc::new(self.duplicate()?);
        let clock_owner = owner.clone();
        let policies = RememberedApprovalPolicies::restore_with_clock(
            current.key,
            &bytes,
            &current.checkpoint,
            Arc::new(move || clock_owner.observe_time(Instant::now() + Duration::from_millis(50))),
        )?
        .with_authority_guard(owner.clone())
        .with_storage_isolation(owner);
        check_deadline(deadline)?;
        Ok(Some(policies))
    }

    /// Persist a wall-clock high-water mark without creating snapshot objects or
    /// changing consent. A late native write may have committed; no use is granted.
    pub fn observe_time(&self, deadline: Instant) -> Result<u64, WasiDiagnostic> {
        let _lock = self.lock(deadline)?;
        let current = self
            .load()?
            .ok_or_else(|| failed("protected approval owner is missing"))?;
        self.observe_time_locked(&current, deadline)
    }

    fn observe_time_locked(
        &self,
        current: &WindowsProtectedApprovalState,
        deadline: Instant,
    ) -> Result<u64, WasiDiagnostic> {
        check_deadline(deadline)?;
        let now = (self.clock)()?;
        if now < current.checkpoint.observed_unix_secs {
            return Err(failed("protected approval clock moved backwards"));
        }
        if now > current.checkpoint.observed_unix_secs {
            let mut checkpoint = current.checkpoint.clone();
            checkpoint.observed_unix_secs = now;
            check_deadline(deadline)?;
            self.write(&current.key, &checkpoint)?;
        }
        check_deadline(deadline)?;
        Ok(now)
    }

    fn duplicate(&self) -> Result<Self, WasiDiagnostic> {
        Ok(Self {
            directory: self
                .directory
                .try_clone()
                .map_err(|_| failed("cannot retain protected approval root"))?,
            owner_sha256: self.owner_sha256.clone(),
            target: self.target.clone(),
            persistence: self.persistence,
            clock: self.clock.clone(),
            require_private_dacl: self.require_private_dacl,
            #[cfg(test)]
            publication_probe: self.publication_probe.clone(),
        })
    }

    fn snapshot_name(&self, checkpoint: &ApprovalPolicyCheckpoint) -> String {
        format!(
            "approval-{}-{}.lsap",
            self.owner_sha256, checkpoint.snapshot_sha256
        )
    }

    fn read_snapshot(
        &self,
        checkpoint: &ApprovalPolicyCheckpoint,
        deadline: Instant,
    ) -> Result<Vec<u8>, WasiDiagnostic> {
        check_deadline(deadline)?;
        self.check_private_dacl()?;
        validate_checkpoint(checkpoint)?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let mut file = self
            .directory
            .open_with(self.snapshot_name(checkpoint), &options)
            .map_err(|_| failed("selected approval snapshot is unavailable"))?;
        self.check_private_file_dacl(file.as_raw_handle())?;
        let metadata = file
            .metadata()
            .map_err(|_| failed("cannot inspect selected approval snapshot"))?;
        if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > 2 * 1024 * 1024 {
            return Err(failed("invalid selected approval snapshot shape"));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| failed("cannot read selected approval snapshot"))?;
        if bytes.len() > 2 * 1024 * 1024
            || hex::encode(Sha256::digest(&bytes)) != checkpoint.snapshot_sha256
        {
            return Err(failed("selected approval snapshot digest mismatch"));
        }
        check_deadline(deadline)?;
        Ok(bytes)
    }

    /// Called while holding the owner lock. A crash before checkpoint publication
    /// leaves only an unselected object; readers never adopt it as consent.
    fn save_snapshot(
        &self,
        key: &ApprovalPolicyKey,
        candidate: &SealedApprovalPolicies,
        deadline: Instant,
    ) -> Result<(), WasiDiagnostic> {
        check_deadline(deadline)?;
        self.check_private_dacl()?;
        let target = self.snapshot_name(&candidate.checkpoint);
        if self.directory.symlink_metadata(&target).is_ok() {
            if self.read_snapshot(&candidate.checkpoint, deadline)? != candidate.bytes {
                return Err(failed("immutable approval snapshot already differs"));
            }
            return Ok(());
        }
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| failed("cannot name approval staging object"))?;
        let staging = self.staging_name(key, &random)?;
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let mut file = self
            .directory
            .open_with(&staging, &options)
            .map_err(|_| failed("cannot create approval staging object"))?;
        #[cfg(test)]
        self.test_publication_cut(PublicationCut::StagingCreated);
        let result = (|| {
            self.check_private_file_dacl(file.as_raw_handle())?;
            for chunk in candidate.bytes.chunks(64 * 1024) {
                check_deadline(deadline)?;
                file.write_all(chunk)
                    .map_err(|_| failed("cannot write approval staging object"))?;
                #[cfg(test)]
                self.test_publication_cut(PublicationCut::StagingChunkWritten);
            }
            file.sync_all()
                .map_err(|_| failed("cannot sync approval staging object"))?;
            #[cfg(test)]
            self.test_publication_cut(PublicationCut::StagingSynced);
            drop(file);
            check_deadline(deadline)?;
            // Publish a new name only. Do not overwrite an immutable object.
            self.directory
                .hard_link(&staging, &self.directory, &target)
                .map_err(|_| failed("cannot publish immutable approval snapshot"))?;
            #[cfg(test)]
            self.test_publication_cut(PublicationCut::ObjectLinked);
            Ok(())
        })();
        // Only remove the exact staging name successfully created by this call.
        let cleanup = self
            .directory
            .remove_file(&staging)
            .map_err(|_| failed("cannot remove owned approval staging object"));
        result?;
        cleanup?;
        self.read_snapshot(&candidate.checkpoint, deadline)?;
        #[cfg(test)]
        self.test_publication_cut(PublicationCut::ObjectReady);
        Ok(())
    }

    #[cfg(test)]
    fn test_publication_cut(&self, phase: PublicationCut) {
        if let Some(probe) = &self.publication_probe {
            probe(phase);
        }
    }

    fn write(
        &self,
        key: &ApprovalPolicyKey,
        checkpoint: &ApprovalPolicyCheckpoint,
    ) -> Result<(), WasiDiagnostic> {
        self.check_private_dacl()?;
        validate_checkpoint(checkpoint)?;
        let record = RootRecord {
            schema_version: 1,
            owner_sha256: self.owner_sha256.clone(),
            key: *key.secret_bytes(),
            checkpoint: checkpoint.clone(),
        };
        let mut bytes = Zeroizing::new(
            serde_json::to_vec(&record)
                .map_err(|_| failed("cannot serialize protected approval checkpoint"))?,
        );
        if bytes.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
            return Err(failed(
                "protected approval credential exceeds its byte limit",
            ));
        }
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: self.target.as_ptr() as *mut _,
            CredentialBlobSize: bytes.len() as u32,
            CredentialBlob: bytes.as_mut_ptr(),
            Persist: self.persistence,
            ..CREDENTIALW::default()
        };
        // SAFETY: every borrowed buffer remains live; CredWriteW copies the blob.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(failed("cannot publish protected approval credential"));
        }
        Ok(())
    }

    fn lock(&self, deadline: Instant) -> Result<PublicationLock, WasiDiagnostic> {
        check_deadline(deadline)?;
        self.check_private_dacl()?;
        check_deadline(deadline)?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .follow(FollowSymlinks::No);
        let file = self
            .directory
            .open_with("approval-owner.lock", &options)
            .map_err(|_| failed("cannot open protected approval publication lock"))?;
        self.check_private_file_dacl(file.as_raw_handle())?;
        let metadata = file
            .metadata()
            .map_err(|_| failed("cannot inspect protected approval publication lock"))?;
        if !metadata.is_file() || metadata.nlink() > 1 || metadata.len() != 0 {
            return Err(failed("invalid protected approval publication lock"));
        }
        let file = file.into_std();
        loop {
            check_deadline(deadline)?;
            match fs2::FileExt::try_lock_exclusive(&file) {
                Ok(()) => return Ok(PublicationLock(file)),
                Err(error)
                    if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(_) => return Err(failed("cannot acquire protected approval publication lock")),
            }
        }
    }

    #[cfg(test)]
    fn remove_test_credential(&self) -> Result<(), WasiDiagnostic> {
        // SAFETY: tests call this only for their checked random owned namespace.
        if unsafe {
            windows_sys::Win32::Security::Credentials::CredDeleteW(
                self.target.as_ptr(),
                CRED_TYPE_GENERIC,
                0,
            )
        } == 0
            && unsafe { GetLastError() } != ERROR_NOT_FOUND
        {
            return Err(failed("cannot clean the owned test credential"));
        }
        Ok(())
    }
}

impl RememberedApprovalAuthorityGuard for WindowsProtectedApprovalCheckpoint {
    fn check_current(
        &self,
        identity: &RememberedPolicyIdentity,
        deadline: Instant,
    ) -> Result<(), WasiDiagnostic> {
        let _lock = self.lock(deadline)?;
        let current = self
            .load()?
            .ok_or_else(|| failed("protected approval owner is missing"))?;
        let now = self.observe_time_locked(&current, deadline)?;
        let bytes = self.read_snapshot(&current.checkpoint, deadline)?;
        // Authenticate metadata with the captured protected time, not a clock
        // callback that would recursively acquire the owner lock.
        let mut checkpoint = current.checkpoint.clone();
        checkpoint.observed_unix_secs = now;
        let sealed = SealedApprovalPolicies { bytes, checkpoint };
        for policy in validate_candidate(&current.key, &sealed)? {
            if &RememberedPolicyIdentity::for_policy(&current.checkpoint.store_id, &policy)?
                == identity
            {
                check_deadline(deadline)?;
                if now >= policy.expires_at_unix_secs {
                    return Err(WasiDiagnostic::new(
                        WasiDiagnosticCode::PluginWasiApprovalExpired,
                        "protected owner's remembered policy expired",
                    ));
                }
                return Ok(());
            }
        }
        check_deadline(deadline)?;
        Err(WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiApprovalRevoked,
            "protected owner no longer contains this policy version",
        ))
    }
}

struct PublicationLock(std::fs::File);
impl Drop for PublicationLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

struct CredentialAllocation(NonNull<CREDENTIALW>);
impl Drop for CredentialAllocation {
    fn drop(&mut self) {
        // SAFETY: the OS allocation is exclusively owned until CredFree. Only
        // the bounded blob is wiped; no caller-owned or unrelated memory is touched.
        unsafe {
            let credential = self.0.as_ref();
            if !credential.CredentialBlob.is_null()
                && credential.CredentialBlobSize <= CRED_MAX_CREDENTIAL_BLOB_SIZE
            {
                std::slice::from_raw_parts_mut(
                    credential.CredentialBlob,
                    credential.CredentialBlobSize as usize,
                )
                .zeroize();
            }
            CredFree(self.0.as_ptr().cast());
        }
    }
}

fn validate_candidate(
    key: &ApprovalPolicyKey,
    candidate: &SealedApprovalPolicies,
) -> Result<Vec<crate::RememberedApprovalPolicy>, WasiDiagnostic> {
    crate::remembered_approval::validate_sealed_for_publication(key, candidate)
}
fn same_snapshot(a: &ApprovalPolicyCheckpoint, b: &ApprovalPolicyCheckpoint) -> bool {
    a.store_id == b.store_id && a.revision == b.revision && a.snapshot_sha256 == b.snapshot_sha256
}
fn unix_time() -> Result<u64, WasiDiagnostic> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| failed("protected approval wall clock is unavailable"))
}
fn validate_checkpoint(checkpoint: &ApprovalPolicyCheckpoint) -> Result<(), WasiDiagnostic> {
    let valid = |text: &str, length| {
        text.len() == length
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if !valid(&checkpoint.store_id, 32)
        || !valid(&checkpoint.snapshot_sha256, 64)
        || checkpoint.revision == u64::MAX
    {
        return Err(failed("invalid protected approval checkpoint identity"));
    }
    Ok(())
}
fn check_deadline(deadline: Instant) -> Result<(), WasiDiagnostic> {
    if Instant::now() >= deadline {
        return Err(WasiDiagnostic::new(
            WasiDiagnosticCode::PluginWasiTimeout,
            "protected checkpoint operation exceeded its deadline",
        ));
    }
    Ok(())
}
fn failed(message: &str) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiHostFailure, message)
}
fn conflict(message: &str) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiWriteConflict, message)
}

#[cfg(test)]
#[path = "windows_approval_process_tests.rs"]
mod process_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct OwnedCredential<'a> {
        backend: &'a WindowsProtectedApprovalCheckpoint,
        armed: bool,
    }
    impl Drop for OwnedCredential<'_> {
        fn drop(&mut self) {
            if self.armed {
                let _ = self.backend.remove_test_credential();
            }
        }
    }

    fn request(path: &str) -> crate::ApprovalRequest {
        crate::ApprovalRequest {
            schema_version: 1,
            id: "a".repeat(64),
            plugin_id: "fixture.component".into(),
            plugin_version: "1.0.0".into(),
            component_sha256: "b".repeat(64),
            host_context_sha256: "c".repeat(64),
            remembered_binding_sha256: Some("d".repeat(64)),
            action: crate::SensitiveAction::FileWrite {
                grant_id: "path-0".into(),
                root: "fixture-root".into(),
                relative_path: path.into(),
                target_sha256: None,
                payload_sha256: "e".repeat(64),
                payload_bytes: 1,
            },
        }
    }

    #[test]
    fn invalid_namespace_and_checkpoint_are_rejected_without_creating_credentials() {
        let temporary = tempfile::tempdir().unwrap();
        for owner in ["", "\n", &"x".repeat(4097)] {
            assert!(WindowsProtectedApprovalCheckpoint::open(
                temporary.path(),
                owner,
                WindowsApprovalPersistence::LogonSession
            )
            .is_err());
        }
        let checkpoint = ApprovalPolicyCheckpoint {
            store_id: "a".repeat(32),
            revision: u64::MAX,
            snapshot_sha256: "b".repeat(64),
            observed_unix_secs: 1,
        };
        assert!(validate_checkpoint(&checkpoint).is_err());
        assert!(check_deadline(Instant::now()).is_err());
        let mut advanced = checkpoint.clone();
        advanced.observed_unix_secs += 1;
        assert!(same_snapshot(&checkpoint, &advanced));
        advanced.snapshot_sha256 = "c".repeat(64);
        assert!(!same_snapshot(&checkpoint, &advanced));
    }

    #[test]
    #[ignore = "explicit current-user owned clock fixture credential; cleanup removes only that entry"]
    fn native_clock_floor_persists_without_snapshot_growth_and_forgetting_stays_available() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let temporary = tempfile::tempdir().unwrap();
        let mut backend = WindowsProtectedApprovalCheckpoint::open(
            temporary.path(),
            "fixture-protected-clock",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        assert!(backend.load().unwrap().is_none());
        let key = ApprovalPolicyKey::generate().unwrap();
        let policies = RememberedApprovalPolicies::new(ApprovalPolicyKey::from_secret_bytes(
            *key.secret_bytes(),
        ))
        .unwrap();
        policies
            .remember_request(
                &request("one.txt"),
                crate::ApprovalScope::Exact,
                Duration::from_secs(3600),
            )
            .unwrap();
        let initial = policies.seal().unwrap();
        let time = Arc::new(AtomicU64::new(initial.checkpoint.observed_unix_secs + 120));
        let observed = time.clone();
        backend.clock = Arc::new(move || {
            let now = observed.load(Ordering::SeqCst);
            if now == u64::MAX {
                Err(failed("fixture clock is unavailable"))
            } else {
                Ok(now)
            }
        });
        let mut cleanup = OwnedCredential {
            backend: &backend,
            armed: true,
        };
        backend
            .initialize(&key, &initial, Instant::now() + Duration::from_secs(5))
            .unwrap();
        let restored = backend
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .unwrap()
            .unwrap();
        let before = std::fs::read_dir(temporary.path()).unwrap().count();
        for _ in 0..30 {
            assert!(restored
                .authorize(&request("one.txt"))
                .unwrap()
                .unwrap()
                .check()
                .is_ok());
        }
        let held = backend
            .lock(Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            restored.authorize(&request("one.txt")).err().unwrap().code,
            WasiDiagnosticCode::PluginWasiTimeout
        );
        drop(held);
        assert!(restored
            .authorize(&request("one.txt"))
            .unwrap()
            .unwrap()
            .check()
            .is_ok());
        let protected = backend.load().unwrap().unwrap();
        assert_eq!(
            protected.checkpoint.observed_unix_secs,
            time.load(Ordering::SeqCst)
        );
        assert!(same_snapshot(&protected.checkpoint, &initial.checkpoint));
        assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), before);

        let identity = RememberedPolicyIdentity::for_policy(
            &initial.checkpoint.store_id,
            &policies.policies().unwrap()[0],
        )
        .unwrap();
        let expired = initial.checkpoint.observed_unix_secs + 3601;
        time.store(expired, Ordering::SeqCst);
        assert_eq!(
            backend
                .check_current(&identity, Instant::now() + Duration::from_secs(5))
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiApprovalExpired
        );
        time.store(expired - 1, Ordering::SeqCst);
        assert!(restored.authorize(&request("one.txt")).is_err());
        let mut independent = WindowsProtectedApprovalCheckpoint::open(
            temporary.path(),
            "fixture-protected-clock",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        independent.clock = backend.clock.clone();
        assert!(independent
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .is_err());
        assert_eq!(
            backend
                .load()
                .unwrap()
                .unwrap()
                .checkpoint
                .observed_unix_secs,
            expired
        );

        // An old but authenticated local view can still forget consent while the
        // clock is broken. A stale snapshot identity, unlike clock-only advance,
        // must still fail CAS. The protected time floor never moves backwards.
        time.store(u64::MAX, Ordering::SeqCst);
        assert!(independent
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .is_err());
        restored.clear().unwrap();
        let forgotten = restored.seal().unwrap();
        assert!(forgotten.checkpoint.observed_unix_secs < expired);
        backend
            .publish(
                &initial.checkpoint,
                &forgotten,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        let latest = backend.load().unwrap().unwrap();
        assert!(same_snapshot(&latest.checkpoint, &forgotten.checkpoint));
        assert_eq!(latest.checkpoint.observed_unix_secs, expired);
        assert!(backend
            .publish(
                &initial.checkpoint,
                &initial,
                Instant::now() + Duration::from_secs(5)
            )
            .is_err());
        time.store(expired + 1, Ordering::SeqCst);
        assert!(independent
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .unwrap()
            .unwrap()
            .policies()
            .unwrap()
            .is_empty());
        assert_eq!(
            backend
                .check_current(&identity, Instant::now() + Duration::from_secs(5))
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiApprovalRevoked
        );
        assert_eq!(
            std::fs::read_dir(temporary.path()).unwrap().count(),
            before + 1
        );

        backend.remove_test_credential().unwrap();
        cleanup.armed = false;
        assert!(backend.load().unwrap().is_none());
        drop(cleanup);
        drop(restored);
        drop(independent);
        drop(backend);
        temporary.close().unwrap();
    }

    #[test]
    #[ignore = "explicit current-user owned test credential; no existing credentials are enumerated"]
    fn native_current_user_roundtrip_cas_and_owned_cleanup() {
        let temporary = tempfile::tempdir().unwrap();
        let backend = WindowsProtectedApprovalCheckpoint::open(
            temporary.path(),
            "fixture-native-checkpoint",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        assert!(backend.load().unwrap().is_none());
        // Namespace contains a random fixture root. Arm before initialization
        // so an ambiguous late native write still cleans only this owned entry.
        let mut cleanup = OwnedCredential {
            backend: &backend,
            armed: true,
        };
        let key = ApprovalPolicyKey::generate().unwrap();
        let policies = RememberedApprovalPolicies::new(ApprovalPolicyKey::from_secret_bytes(
            *key.secret_bytes(),
        ))
        .unwrap();
        let initial = policies.seal().unwrap();
        backend
            .initialize(&key, &initial, Instant::now() + Duration::from_secs(5))
            .unwrap();
        let loaded = backend.load().unwrap().unwrap();
        assert!(loaded.key.secret_bytes() == key.secret_bytes());
        assert_eq!(loaded.checkpoint, initial.checkpoint);
        assert!(backend
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .unwrap()
            .unwrap()
            .policies()
            .unwrap()
            .is_empty());
        assert!(format!("{loaded:?}").contains("[REDACTED]"));
        assert!(backend
            .initialize(&key, &initial, Instant::now() + Duration::from_secs(5))
            .is_err());

        policies
            .remember_request(
                &request("one.txt"),
                crate::ApprovalScope::Exact,
                Duration::from_secs(3600),
            )
            .unwrap();
        let first = policies.seal().unwrap();
        backend
            .publish(
                &initial.checkpoint,
                &first,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            backend.load().unwrap().unwrap().checkpoint,
            first.checkpoint
        );
        let second_backend = WindowsProtectedApprovalCheckpoint::open(
            temporary.path(),
            "fixture-native-checkpoint",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        assert!(second_backend
            .publish(
                &initial.checkpoint,
                &first,
                Instant::now() + Duration::from_secs(5)
            )
            .is_err());

        let mut corrupt = SealedApprovalPolicies {
            bytes: first.bytes.clone(),
            checkpoint: first.checkpoint.clone(),
        };
        corrupt.bytes[12] ^= 1;
        assert!(backend
            .publish(
                &first.checkpoint,
                &corrupt,
                Instant::now() + Duration::from_secs(5)
            )
            .is_err());
        assert_eq!(
            backend.load().unwrap().unwrap().checkpoint,
            first.checkpoint
        );
        let held = backend
            .lock(Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            second_backend
                .publish(
                    &first.checkpoint,
                    &first,
                    Instant::now() + Duration::from_millis(10)
                )
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiTimeout
        );
        drop(held);

        let make_candidate = |path| {
            let copy = RememberedApprovalPolicies::restore(
                ApprovalPolicyKey::from_secret_bytes(*key.secret_bytes()),
                &first.bytes,
                &first.checkpoint,
            )
            .unwrap();
            copy.remember_request(
                &request(path),
                crate::ApprovalScope::Exact,
                Duration::from_secs(3600),
            )
            .unwrap();
            copy.seal().unwrap()
        };
        let candidate_a = make_candidate("candidate-a.txt");
        let candidate_b = make_candidate("candidate-b.txt");
        let first_backend = Arc::new(
            WindowsProtectedApprovalCheckpoint::open(
                temporary.path(),
                "fixture-native-checkpoint",
                WindowsApprovalPersistence::LogonSession,
            )
            .unwrap(),
        );
        let second_backend = Arc::new(second_backend);
        let expected_a = first.checkpoint.clone();
        let expected_b = first.checkpoint.clone();
        let a = std::thread::spawn(move || {
            first_backend.publish(
                &expected_a,
                &candidate_a,
                Instant::now() + Duration::from_secs(5),
            )
        });
        let b = std::thread::spawn(move || {
            second_backend.publish(
                &expected_b,
                &candidate_b,
                Instant::now() + Duration::from_secs(5),
            )
        });
        let results = [a.join().unwrap(), b.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| result
                    .as_ref()
                    .is_err_and(|error| error.code == WasiDiagnosticCode::PluginWasiWriteConflict))
                .count(),
            1
        );
        assert_eq!(
            backend.load().unwrap().unwrap().checkpoint.revision,
            first.checkpoint.revision + 1
        );

        let current = backend.load().unwrap().unwrap();
        let selected = SealedApprovalPolicies {
            bytes: backend
                .read_snapshot(&current.checkpoint, Instant::now() + Duration::from_secs(5))
                .unwrap(),
            checkpoint: current.checkpoint.clone(),
        };
        let recovered = backend
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(recovered.policies().unwrap().len(), 2);
        let current_identity = RememberedPolicyIdentity::for_policy(
            &current.checkpoint.store_id,
            &recovered.policies().unwrap()[0],
        )
        .unwrap();
        second_backend_for_guard(temporary.path())
            .check_current(&current_identity, Instant::now() + Duration::from_secs(5))
            .unwrap();
        recovered.clear().unwrap();
        let forgotten = recovered.seal().unwrap();
        // Simulate a cut after writing an object but before protected selection.
        {
            let _lock = backend
                .lock(Instant::now() + Duration::from_secs(5))
                .unwrap();
            backend
                .save_snapshot(&key, &forgotten, Instant::now() + Duration::from_secs(5))
                .unwrap();
        }
        assert_eq!(
            backend
                .restore_policies(Instant::now() + Duration::from_secs(5))
                .unwrap()
                .unwrap()
                .policies()
                .unwrap()
                .len(),
            2
        );

        // Tampering the selected object must not fall back to an older valid one.
        let mut options = OpenOptions::new();
        options.write(true).follow(FollowSymlinks::No);
        let mut damaged = backend
            .directory
            .open_with(backend.snapshot_name(&current.checkpoint), &options)
            .unwrap();
        damaged.write_all(&[0]).unwrap();
        damaged.sync_all().unwrap();
        drop(damaged);
        assert!(backend
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .is_err());
        assert!(same_snapshot(
            &backend.load().unwrap().unwrap().checkpoint,
            &current.checkpoint
        ));
        // Repair only the synthetic fixture object using its known original bytes.
        backend
            .directory
            .remove_file(backend.snapshot_name(&current.checkpoint))
            .unwrap();
        {
            let _lock = backend
                .lock(Instant::now() + Duration::from_secs(5))
                .unwrap();
            backend
                .save_snapshot(&key, &selected, Instant::now() + Duration::from_secs(5))
                .unwrap();
        }
        backend
            .publish(
                &current.checkpoint,
                &forgotten,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        assert_eq!(
            second_backend_for_guard(temporary.path())
                .check_current(&current_identity, Instant::now() + Duration::from_secs(5))
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiApprovalRevoked
        );
        assert!(backend
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .unwrap()
            .unwrap()
            .policies()
            .unwrap()
            .is_empty());

        backend.remove_test_credential().unwrap();
        cleanup.armed = false;
        assert!(backend.load().unwrap().is_none());
        drop(cleanup);
        drop(loaded);
        drop(recovered);
        drop(backend);
        temporary.close().unwrap();
    }

    fn second_backend_for_guard(root: &Path) -> WindowsProtectedApprovalCheckpoint {
        WindowsProtectedApprovalCheckpoint::open(
            root,
            "fixture-native-checkpoint",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap()
    }
}
