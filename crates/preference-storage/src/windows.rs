//! Current-user DPAPI objects selected by a Credential Manager head under an OS
//! file lock. Trusted hosts must exclude the private root from all guest grants.
use crate::PreferenceProviderError as Error;
use cap_fs_ext::{FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::fs::{Dir, OpenOptions};
use latexsnipper_conversion::formula_completion::preferences::persistence::transactions::{
    FormulaPreferenceCheckpointBackend, FormulaPreferenceHostKeys,
    FormulaPreferenceStorageIdentity, FormulaPreferenceTransactionError, PreferencePublication,
    PreferenceStorageWitness, ProtectedPreferenceRecord,
    FORMULA_PREFERENCE_MAX_PROTECTED_RECORD_BYTES,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::os::windows::{fs::MetadataExt as _, io::AsRawHandle};
use std::path::Path;
use std::ptr::{null, null_mut, NonNull};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::{GetLastError, LocalFree, ERROR_NOT_FOUND},
    Security::{
        Credentials::{
            CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_MAX_CREDENTIAL_BLOB_SIZE,
            CRED_PERSIST_LOCAL_MACHINE, CRED_PERSIST_SESSION, CRED_TYPE_GENERIC,
        },
        Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    },
};
use zeroize::{Zeroize, Zeroizing};

mod isolation;
mod private_root;
mod session;
pub use session::{
    OpenedWindowsPreferenceStorage, WindowsPreferenceOpenRequest, WindowsPreferenceSessionError,
};
#[cfg(test)]
mod tests;
type Result<T> = std::result::Result<T, Error>;
const MAX_ENCRYPTED_BYTES: usize = FORMULA_PREFERENCE_MAX_PROTECTED_RECORD_BYTES + 64 * 1024;

/// Both options are current-user scopes, not machine-wide authority.
#[derive(Debug, Clone, Copy)]
pub enum WindowsPreferencePersistence {
    LogonSession,
    ThisUserThisComputer,
}
impl WindowsPreferencePersistence {
    fn raw(self) -> u32 {
        match self {
            Self::LogonSession => CRED_PERSIST_SESSION,
            Self::ThisUserThisComputer => CRED_PERSIST_LOCAL_MACHINE,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Head {
    schema_version: u16,
    owner_sha256: String,
    object_sha256: String,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicationCut {
    ObjectCreated,
    ChunkWritten,
    ObjectSynced,
    BeforeSelection,
    AfterSelection,
}

struct Inner {
    directory: Dir,
    owner_sha256: String,
    identity: FormulaPreferenceStorageIdentity,
    target: Vec<u16>,
    persistence: u32,
    timeout: Duration,
    #[cfg(test)]
    probe: Option<Arc<dyn Fn(PublicationCut) + Send + Sync>>,
}

/// An explicit trusted backend; opening does not initialize credentials or enable
/// storage/learning consent. Synchronous native IO is not forcibly cancellable.
#[derive(Clone)]
pub struct WindowsPreferenceCheckpoint {
    inner: Arc<Inner>,
}

/// Bounded, explicitly requested cleanup. The protected head/tombstone and lock
/// are never deleted, and the currently selected object is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreferenceObjectCleanup {
    pub inspected: usize,
    pub removed: usize,
    pub more: bool,
}

pub fn generate_host_keys() -> Result<FormulaPreferenceHostKeys> {
    let mut random = Zeroizing::new([0_u8; 64]);
    getrandom::fill(random.as_mut()).map_err(|_| Error::Unavailable)?;
    let (feedback, storage) = random.split_at(32);
    FormulaPreferenceHostKeys::new(feedback.try_into().unwrap(), storage.try_into().unwrap())
        .map_err(|_| Error::Unavailable)
}

fn hash(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn validate_config(
    owner: &str,
    identity: &FormulaPreferenceStorageIdentity,
    timeout: Duration,
) -> Result<()> {
    if owner.is_empty()
        || owner.len() > 4096
        || owner.chars().any(char::is_control)
        || !valid_hash(&identity.user_sha256)
        || !identity.project_sha256.as_deref().is_none_or(valid_hash)
        || !identity.document_sha256.as_deref().is_none_or(valid_hash)
        || timeout.is_zero()
        || timeout > Duration::from_secs(30)
    {
        return Err(Error::InvalidConfiguration);
    }
    Ok(())
}
fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(Error::DeadlineExceeded)
    } else {
        Ok(())
    }
}

impl WindowsPreferenceCheckpoint {
    /// Verifies an existing private root without repairing ACLs, choosing consent
    /// or recreating a missing protected head. Namespace uses physical root ID.
    pub fn open_verified_private_root(
        root: &Path,
        owner: &str,
        identity: FormulaPreferenceStorageIdentity,
        persistence: WindowsPreferencePersistence,
        operation_timeout: Duration,
    ) -> Result<Self> {
        validate_config(owner, &identity, operation_timeout)?;
        let metadata = std::fs::symlink_metadata(root).map_err(|_| Error::Unavailable)?;
        if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::PrivateDirectoryRequired);
        }
        let directory = Dir::open_ambient_dir(root, cap_std::ambient_authority())
            .map_err(|_| Error::Unavailable)?;
        private_root::verify(directory.as_raw_handle(), true)?;
        isolation::local_path(&directory)?;
        let metadata = directory.dir_metadata().map_err(|_| Error::Unavailable)?;
        if !metadata.is_dir() || metadata.ino() == 0 {
            return Err(Error::PrivateDirectoryRequired);
        }
        let namespace = serde_json::to_vec(&(
            "latexsnipper:windows-preference-provider:v1",
            metadata.dev(),
            metadata.ino(),
            owner,
            &identity.user_sha256,
            &identity.project_sha256,
            &identity.document_sha256,
            persistence.raw(),
        ))
        .map_err(|_| Error::InvalidConfiguration)?;
        let owner_sha256 = hash(&namespace);
        let target = format!("LaTeXSnipperCore.Preferences.v1/{owner_sha256}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        Ok(Self {
            inner: Arc::new(Inner {
                directory,
                owner_sha256,
                identity,
                target,
                persistence: persistence.raw(),
                timeout: operation_timeout,
                #[cfg(test)]
                probe: None,
            }),
        })
    }

    /// Creates only one fresh child with a restrictive descriptor at creation.
    /// Existing children/parent ACLs are never repaired or deleted on failure.
    pub fn create_private_root(
        parent: &Path,
        name: &str,
        owner: &str,
        identity: FormulaPreferenceStorageIdentity,
        persistence: WindowsPreferencePersistence,
        operation_timeout: Duration,
    ) -> Result<Self> {
        validate_config(owner, &identity, operation_timeout)?;
        let root = private_root::create(parent, name)?;
        Self::open_verified_private_root(&root, owner, identity, persistence, operation_timeout)
    }

    pub fn owner_sha256(&self) -> &str {
        &self.inner.owner_sha256
    }
    /// Trusted-host read with redacted native diagnostic categories. This is not
    /// a consent grant; ordinary callers should restore through the session.
    pub fn read_current_record(&self) -> Result<Option<ProtectedPreferenceRecord>> {
        let deadline = self.deadline();
        let _lock = self.lock(deadline)?;
        self.load_locked(deadline)
    }
    fn deadline(&self) -> Instant {
        Instant::now() + self.inner.timeout
    }
    fn check_private_root(&self) -> Result<()> {
        private_root::verify(self.inner.directory.as_raw_handle(), true)
    }
    fn lock_name(&self) -> String {
        format!("pref-{}.lock", self.owner_sha256())
    }
    fn object_prefix(&self) -> String {
        format!("pref-{}-", self.owner_sha256())
    }
    fn object_name(&self, digest: &str) -> String {
        format!("{}{digest}.dpapi", self.object_prefix())
    }
    fn lock(&self, deadline: Instant) -> Result<PublicationLock> {
        check_deadline(deadline)?;
        self.check_private_root()?;
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .follow(FollowSymlinks::No);
        let file = self
            .inner
            .directory
            .open_with(self.lock_name(), &options)
            .map_err(|_| Error::Unavailable)?;
        private_root::verify(file.as_raw_handle(), false)?;
        let meta = file.metadata().map_err(|_| Error::Unavailable)?;
        if !meta.is_file() || meta.len() != 0 || meta.nlink() != 1 {
            return Err(Error::PrivateDirectoryRequired);
        }
        let file = file.into_std();
        loop {
            check_deadline(deadline)?;
            match fs2::FileExt::try_lock_exclusive(&file) {
                Ok(()) => return Ok(PublicationLock(file)),
                Err(error)
                    if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() =>
                {
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(_) => return Err(Error::Unavailable),
            }
        }
    }
    fn head(&self, deadline: Instant) -> Result<Option<Head>> {
        check_deadline(deadline)?;
        self.check_private_root()?;
        let mut raw = null_mut();
        // SAFETY: target is terminated; writable output points to one OS allocation.
        if unsafe { CredReadW(self.inner.target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0 {
            let code = unsafe { GetLastError() };
            check_deadline(deadline)?;
            return if code == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(Error::Unavailable)
            };
        }
        let allocation = CredentialAllocation(NonNull::new(raw).ok_or(Error::InvalidRecord)?);
        // SAFETY: successful CredReadW returned this owned CREDENTIALW allocation.
        let credential = unsafe { allocation.0.as_ref() };
        if credential.Type != CRED_TYPE_GENERIC
            || credential.Persist != self.inner.persistence
            || credential.CredentialBlob.is_null()
            || credential.CredentialBlobSize == 0
            || credential.CredentialBlobSize > CRED_MAX_CREDENTIAL_BLOB_SIZE
        {
            return Err(Error::InvalidRecord);
        }
        // SAFETY: allocation owns the bounded, nonnull credential blob.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        };
        let head: Head = serde_json::from_slice(bytes).map_err(|_| Error::InvalidRecord)?;
        if head.schema_version != 1
            || head.owner_sha256 != self.owner_sha256()
            || !valid_hash(&head.object_sha256)
        {
            return Err(Error::InvalidRecord);
        }
        check_deadline(deadline)?;
        Ok(Some(head))
    }
    fn read_object(&self, digest: &str, deadline: Instant) -> Result<Vec<u8>> {
        check_deadline(deadline)?;
        self.check_private_root()?;
        if !valid_hash(digest) {
            return Err(Error::InvalidRecord);
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let mut file = self
            .inner
            .directory
            .open_with(self.object_name(digest), &options)
            .map_err(|_| Error::InvalidRecord)?;
        private_root::verify(file.as_raw_handle(), false)?;
        let meta = file.metadata().map_err(|_| Error::Unavailable)?;
        if !meta.is_file()
            || meta.nlink() != 1
            || meta.len() == 0
            || meta.len() > MAX_ENCRYPTED_BYTES as u64
        {
            return Err(Error::InvalidRecord);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_ENCRYPTED_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Unavailable)?;
        if bytes.len() > MAX_ENCRYPTED_BYTES || hash(&bytes) != digest {
            return Err(Error::IntegrityMismatch);
        }
        check_deadline(deadline)?;
        Ok(bytes)
    }
    fn validate_identity(&self, record: &ProtectedPreferenceRecord) -> Result<()> {
        if record.owner_sha256() != self.owner_sha256()
            || FormulaPreferenceStorageIdentity::from_scope(record.scope()) != self.inner.identity
        {
            return Err(Error::InvalidRecord);
        }
        record
            .validate_checkpoint_integrity()
            .map_err(|_| Error::IntegrityMismatch)?;
        Ok(())
    }
    fn load_locked(&self, deadline: Instant) -> Result<Option<ProtectedPreferenceRecord>> {
        let Some(head) = self.head(deadline)? else {
            return Ok(None);
        };
        let cipher = self.read_object(&head.object_sha256, deadline)?;
        let plaintext = self.crypt(&cipher, false)?;
        if plaintext.len() > FORMULA_PREFERENCE_MAX_PROTECTED_RECORD_BYTES {
            return Err(Error::InvalidRecord);
        }
        let record = ProtectedPreferenceRecord::decode_from_protected_storage(&plaintext)
            .map_err(|_| Error::InvalidRecord)?;
        self.validate_identity(&record)?;
        check_deadline(deadline)?;
        Ok(Some(record))
    }
    fn crypt(&self, bytes: &[u8], protect: bool) -> Result<Zeroizing<Vec<u8>>> {
        if bytes.is_empty() || bytes.len() > MAX_ENCRYPTED_BYTES {
            return Err(Error::InvalidRecord);
        }
        let input = CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr().cast_mut(),
        };
        let entropy_bytes = format!("latexsnipper:preference-dpapi:v1:{}", self.owner_sha256());
        let entropy = CRYPT_INTEGER_BLOB {
            cbData: entropy_bytes.len() as u32,
            pbData: entropy_bytes.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: inputs remain live; no prompt or machine-wide flag is supplied.
        // These synchronous calls finish before the publication lock can drop.
        let success = unsafe {
            if protect {
                CryptProtectData(
                    &input,
                    null(),
                    &entropy,
                    null(),
                    null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    null_mut(),
                    &entropy,
                    null(),
                    null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        let allocation = LocalBlob(output);
        if success == 0 {
            return Err(Error::Unavailable);
        }
        if allocation.0.pbData.is_null()
            || allocation.0.cbData == 0
            || allocation.0.cbData as usize > MAX_ENCRYPTED_BYTES
        {
            return Err(Error::InvalidRecord);
        }
        // SAFETY: successful DPAPI output is bounded and owned until LocalFree.
        Ok(Zeroizing::new(
            unsafe {
                std::slice::from_raw_parts(allocation.0.pbData, allocation.0.cbData as usize)
            }
            .to_vec(),
        ))
    }
    fn save_object(&self, cipher: &[u8], deadline: Instant) -> Result<String> {
        check_deadline(deadline)?;
        self.check_private_root()?;
        let digest = hash(cipher);
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let mut file = self
            .inner
            .directory
            .open_with(self.object_name(&digest), &options)
            .map_err(|_| Error::Unavailable)?;
        #[cfg(test)]
        self.probe(PublicationCut::ObjectCreated);
        private_root::verify(file.as_raw_handle(), false)?;
        for chunk in cipher.chunks(64 * 1024) {
            check_deadline(deadline)?;
            file.write_all(chunk).map_err(|_| Error::Unavailable)?;
            #[cfg(test)]
            self.probe(PublicationCut::ChunkWritten);
        }
        file.sync_all().map_err(|_| Error::Unavailable)?;
        #[cfg(test)]
        self.probe(PublicationCut::ObjectSynced);
        drop(file);
        if self.read_object(&digest, deadline)? != cipher {
            return Err(Error::IntegrityMismatch);
        }
        Ok(digest)
    }
    fn write_head(&self, digest: String) -> Result<()> {
        self.check_private_root()?;
        let mut bytes = serde_json::to_vec(&Head {
            schema_version: 1,
            owner_sha256: self.owner_sha256().into(),
            object_sha256: digest,
        })
        .map_err(|_| Error::InvalidRecord)?;
        if bytes.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
            return Err(Error::InvalidRecord);
        }
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: self.inner.target.as_ptr().cast_mut(),
            CredentialBlobSize: bytes.len() as u32,
            CredentialBlob: bytes.as_mut_ptr(),
            Persist: self.inner.persistence,
            ..CREDENTIALW::default()
        };
        // SAFETY: borrowed buffers remain live; this copies only our exact head.
        if unsafe { CredWriteW(&credential, 0) } == 0 {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    #[cfg(test)]
    fn probe(&self, cut: PublicationCut) {
        if let Some(probe) = &self.inner.probe {
            probe(cut);
        }
    }

    /// Removes only bounded, exact namespace object names, never the head or lock.
    /// Missing/corrupt selected state blocks cleanup instead of choosing an orphan.
    pub fn cleanup_unselected(&self, maximum: usize) -> Result<PreferenceObjectCleanup> {
        if maximum == 0 || maximum > 128 {
            return Err(Error::InvalidConfiguration);
        }
        let deadline = self.deadline();
        let _lock = self.lock(deadline)?;
        let selected = self.head(deadline)?.ok_or(Error::InvalidRecord)?;
        self.load_locked(deadline)?.ok_or(Error::InvalidRecord)?;
        let mut result = PreferenceObjectCleanup {
            inspected: 0,
            removed: 0,
            more: false,
        };
        let prefix = self.object_prefix();
        for entry in self
            .inner
            .directory
            .entries()
            .map_err(|_| Error::Unavailable)?
        {
            check_deadline(deadline)?;
            result.inspected += 1;
            if result.inspected > 4096 {
                return Err(Error::CleanupLimit);
            }
            let entry = entry.map_err(|_| Error::Unavailable)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(digest) = name
                .strip_prefix(&prefix)
                .and_then(|value| value.strip_suffix(".dpapi"))
            else {
                continue;
            };
            if !valid_hash(digest) || digest == selected.object_sha256 {
                continue;
            }
            if result.removed == maximum {
                result.more = true;
                break;
            }
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            let file = self
                .inner
                .directory
                .open_with(name, &options)
                .map_err(|_| Error::Unavailable)?;
            private_root::verify(file.as_raw_handle(), false)?;
            let metadata = file.metadata().map_err(|_| Error::Unavailable)?;
            if !metadata.is_file() || metadata.nlink() != 1 {
                return Err(Error::PrivateDirectoryRequired);
            }
            drop(file);
            self.inner
                .directory
                .remove_file(name)
                .map_err(|_| Error::Unavailable)?;
            result.removed += 1;
        }
        check_deadline(deadline)?;
        Ok(result)
    }
}

impl FormulaPreferenceCheckpointBackend for WindowsPreferenceCheckpoint {
    fn load(
        &self,
    ) -> std::result::Result<Option<ProtectedPreferenceRecord>, FormulaPreferenceTransactionError>
    {
        self.read_current_record()
            .map_err(|_| FormulaPreferenceTransactionError::ProviderUnavailable)
    }
    fn compare_exchange(
        &self,
        expected: Option<&PreferenceStorageWitness>,
        candidate: &ProtectedPreferenceRecord,
    ) -> PreferencePublication {
        let deadline = self.deadline();
        let Ok(_lock) = self.lock(deadline) else {
            return PreferencePublication::NotCommitted;
        };
        let Ok(current) = self.load_locked(deadline) else {
            return PreferencePublication::Indeterminate;
        };
        let Ok(witness) = current
            .as_ref()
            .map(ProtectedPreferenceRecord::witness)
            .transpose()
        else {
            return PreferencePublication::Indeterminate;
        };
        if witness.as_ref() != expected {
            return PreferencePublication::Conflict;
        }
        if self.validate_identity(candidate).is_err() {
            return PreferencePublication::NotCommitted;
        }
        let valid_revision = match current.as_ref() {
            Some(current) => {
                current.revision().checked_add(1) == Some(candidate.revision())
                    && candidate.scope().generation >= current.scope().generation
            }
            None => candidate.revision() == 1,
        };
        if !valid_revision {
            return PreferencePublication::NotCommitted;
        }
        let Ok(plaintext) = candidate.encode_for_protected_storage().map(Zeroizing::new) else {
            return PreferencePublication::NotCommitted;
        };
        let Ok(cipher) = self.crypt(&plaintext, true) else {
            return PreferencePublication::NotCommitted;
        };
        let Ok(digest) = self.save_object(&cipher, deadline) else {
            return PreferencePublication::NotCommitted;
        };
        if check_deadline(deadline).is_err() {
            return PreferencePublication::NotCommitted;
        }
        #[cfg(test)]
        self.probe(PublicationCut::BeforeSelection);
        // Beyond this boundary a failed/late native write may have selected it.
        if self.write_head(digest).is_err() {
            return PreferencePublication::Indeterminate;
        }
        #[cfg(test)]
        self.probe(PublicationCut::AfterSelection);
        if check_deadline(deadline).is_err() {
            return PreferencePublication::Indeterminate;
        }
        let Ok(Some(selected)) = self.load_locked(deadline) else {
            return PreferencePublication::Indeterminate;
        };
        match (selected.witness(), candidate.witness()) {
            (Ok(selected), Ok(candidate)) if selected == candidate => {
                PreferencePublication::Applied
            }
            _ => PreferencePublication::Indeterminate,
        }
    }
}

struct PublicationLock(std::fs::File);
impl Drop for PublicationLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}
struct LocalBlob(CRYPT_INTEGER_BLOB);
impl Drop for LocalBlob {
    fn drop(&mut self) {
        if !self.0.pbData.is_null() {
            // SAFETY: DPAPI returned this owned LocalFree allocation. Bound any
            // wipe; malformed lengths do not cause unbounded memory access.
            unsafe {
                if self.0.cbData as usize <= MAX_ENCRYPTED_BYTES {
                    std::slice::from_raw_parts_mut(self.0.pbData, self.0.cbData as usize).zeroize();
                }
                LocalFree(self.0.pbData.cast());
            }
        }
    }
}
struct CredentialAllocation(NonNull<CREDENTIALW>);
impl Drop for CredentialAllocation {
    fn drop(&mut self) {
        // SAFETY: owns exactly one successful CredReadW allocation.
        unsafe {
            CredFree(self.0.as_ptr().cast());
        }
    }
}
