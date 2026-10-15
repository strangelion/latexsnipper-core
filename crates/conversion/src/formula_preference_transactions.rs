//! Trusted-provider transactions. No filesystem, system keystore or user consent UI.
use super::{
    FormulaPreferenceStorageError, FormulaPreferenceStoragePermission,
    FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES,
};
use crate::formula_completion::{
    preferences::FormulaPreferenceStore, scoped::FormulaDefinitionScope,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Weak};

pub const FORMULA_PREFERENCE_MAX_PROTECTED_RECORD_BYTES: usize = 1024 * 1024;
type Result<T> = std::result::Result<T, FormulaPreferenceTransactionError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaPreferenceTransactionError {
    Storage(FormulaPreferenceStorageError),
    InvalidRecord,
    IdentityMismatch,
    NotFound,
    ProviderUnavailable,
    StaleTransaction,
    Conflict,
    NotCommitted,
    Indeterminate,
    RecoveryRequired,
    Exhausted,
}
impl std::fmt::Display for FormulaPreferenceTransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Storage(_) => "Preference transaction storage validation refused.",
            Self::InvalidRecord => "Invalid closed protected preference record.",
            Self::IdentityMismatch => "Protected preference owner or scope does not match.",
            Self::NotFound => "Protected preference state is absent; no implicit initialization.",
            Self::ProviderUnavailable => "Protected preference provider is unavailable.",
            Self::StaleTransaction => "Preference transaction belongs to another owner or base.",
            Self::Conflict => {
                "Protected preference state changed; explicit reconciliation required."
            }
            Self::NotCommitted => "Preference publication was confirmed not committed.",
            Self::Indeterminate => {
                "Preference publication may have committed; explicit reconciliation required."
            }
            Self::RecoveryRequired => {
                "Preference writes require explicit reconciliation or reopen."
            }
            Self::Exhausted => "Preference transaction sequence or reset generation exhausted.",
        })
    }
}
impl std::error::Error for FormulaPreferenceTransactionError {}
impl From<FormulaPreferenceStorageError> for FormulaPreferenceTransactionError {
    fn from(value: FormulaPreferenceStorageError) -> Self {
        Self::Storage(value)
    }
}

/// Stable slot identity: generation stays in the protected head across resets.
#[derive(Clone, PartialEq, Eq)]
pub struct FormulaPreferenceStorageIdentity {
    pub user_sha256: String,
    pub project_sha256: Option<String>,
    pub document_sha256: Option<String>,
}
impl FormulaPreferenceStorageIdentity {
    pub fn from_scope(scope: &FormulaDefinitionScope) -> Self {
        Self {
            user_sha256: scope.user_sha256.clone(),
            project_sha256: scope.project_sha256.clone(),
            document_sha256: scope.document_sha256.clone(),
        }
    }
}

/// Host-generated independent keys. Not Clone/Serialize/Debug or a permission grant.
pub struct FormulaPreferenceHostKeys {
    feedback: [u8; 32],
    storage: [u8; 32],
}
impl FormulaPreferenceHostKeys {
    pub fn new(feedback: [u8; 32], storage: [u8; 32]) -> Result<Self> {
        if feedback == storage || feedback == [0; 32] || storage == [0; 32] {
            return Err(FormulaPreferenceStorageError::InvalidKey.into());
        }
        Ok(Self { feedback, storage })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecordWire {
    schema_version: u16,
    owner_sha256: String,
    #[serde(with = "crate::formula_completion::wire::revision_decimal")]
    revision: u64,
    scope: FormulaDefinitionScope,
    #[serde(with = "crate::formula_completion::wire::revision_decimal")]
    checkpoint: u64,
    feedback_key: [u8; 32],
    storage_key: [u8; 32],
    snapshot_json: String,
}

/// Contains SECRET KEYS. A trusted provider must protect the ENTIRE record and
/// its freshness, not store these encoded bytes in an ordinary snapshot file.
/// No authorization switches, captures or receipts are encoded.
pub struct ProtectedPreferenceRecord {
    wire: RecordWire,
}
impl std::fmt::Debug for ProtectedPreferenceRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProtectedPreferenceRecord([REDACTED])")
    }
}
impl ProtectedPreferenceRecord {
    fn validate(&self) -> Result<()> {
        let w = &self.wire;
        if w.schema_version != 1
            || w.revision == 0
            || w.checkpoint == 0
            || w.owner_sha256.len() != 64
            || !w
                .owner_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || w.snapshot_json.len() > FORMULA_PREFERENCE_MAX_SNAPSHOT_BYTES
        {
            return Err(FormulaPreferenceTransactionError::InvalidRecord);
        }
        FormulaPreferenceHostKeys::new(w.feedback_key, w.storage_key)?;
        FormulaPreferenceStore::new(w.scope.clone(), w.feedback_key)
            .map_err(|_| FormulaPreferenceTransactionError::InvalidRecord)?;
        Ok(())
    }
    /// Sensitive plaintext, for a trusted provider's protected encoding ONLY.
    /// This API provides no heap-erasure or encryption guarantee.
    pub fn encode_for_protected_storage(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(&self.wire)
            .map_err(|_| FormulaPreferenceTransactionError::InvalidRecord)?;
        if bytes.len() > FORMULA_PREFERENCE_MAX_PROTECTED_RECORD_BYTES {
            return Err(FormulaPreferenceTransactionError::InvalidRecord);
        }
        Ok(bytes)
    }
    /// Call only AFTER reading/decrypting current trusted provider state. Parsing
    /// this from an untrusted file does not establish freshness or authenticity.
    pub fn decode_from_protected_storage(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > FORMULA_PREFERENCE_MAX_PROTECTED_RECORD_BYTES {
            return Err(FormulaPreferenceTransactionError::InvalidRecord);
        }
        let record = Self {
            wire: serde_json::from_slice(bytes)
                .map_err(|_| FormulaPreferenceTransactionError::InvalidRecord)?,
        };
        record.validate()?;
        Ok(record)
    }
    pub fn revision(&self) -> u64 {
        self.wire.revision
    }
    pub fn scope(&self) -> &FormulaDefinitionScope {
        &self.wire.scope
    }
    pub fn checkpoint(&self) -> u64 {
        self.wire.checkpoint
    }
    pub fn witness(&self) -> Result<PreferenceStorageWitness> {
        let mut digest = Sha256::new();
        digest.update(b"latexsnipper:preference-protected-record:v1\0");
        digest.update(self.encode_for_protected_storage()?);
        Ok(PreferenceStorageWitness(digest.finalize().into()))
    }
    fn restore(
        &self,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<FormulaPreferenceStore> {
        Ok(FormulaPreferenceStore::restore_checkpoint(
            self.wire.scope.clone(),
            self.wire.feedback_key,
            &self.wire.storage_key,
            self.wire.checkpoint,
            &self.wire.snapshot_json,
            permission,
        )?)
    }
}

/// Exact content identity, not a reusable credential or permission token.
#[derive(Clone, PartialEq, Eq)]
pub struct PreferenceStorageWitness([u8; 32]);
impl std::fmt::Debug for PreferenceStorageWitness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreferenceStorageWitness([REDACTED])")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferencePublication {
    /// The protected head, keys and snapshot were atomically selected together.
    Applied,
    /// Expected content differs. Do not overwrite the winner or silently merge.
    Conflict,
    /// Provider can PROVE that nothing was selected; explicit same-byte retry is safe.
    NotCommitted,
    /// Includes late/failed native writes where publication cannot be determined.
    Indeterminate,
}

/// TRUSTED HOST IMPLEMENTATION, not a guest/plugin interface. The provider must
/// protect keys AND current freshness, atomically CAS all record bytes across
/// cooperating processes, and never recreate missing state or log plaintext.
/// In-process locking alone is not a cross-process/disk implementation.
pub trait FormulaPreferenceCheckpointBackend {
    /// A successful read must be ordered after any earlier publication that
    /// could still select a head. If such a write has not settled, return
    /// ProviderUnavailable instead of reporting an apparently unchanged base.
    /// Otherwise reconciliation could permit a retry before a late write wins.
    fn load(&self) -> Result<Option<ProtectedPreferenceRecord>>;
    /// CAS must remain serialized with load and all competing publications,
    /// including native operations still in flight after an uncertain result.
    /// A timeout does not cancel IO or release that coordination boundary.
    fn compare_exchange(
        &self,
        expected: Option<&PreferenceStorageWitness>,
        candidate: &ProtectedPreferenceRecord,
    ) -> PreferencePublication;
}

/// Explicitly staged immutable bytes; cannot be cloned, deserialized or replayed
/// on another session. A stage is NOT a storage-consent credential.
pub struct PreparedPreferenceCheckpoint {
    owner: Weak<()>,
    base: Option<PreferenceStorageWitness>,
    candidate: ProtectedPreferenceRecord,
    reset: bool,
    consumed: bool,
    event_count: usize,
}
impl PreparedPreferenceCheckpoint {
    pub fn checkpoint(&self) -> u64 {
        self.candidate.checkpoint()
    }
    pub fn revision(&self) -> u64 {
        self.candidate.revision()
    }
    pub fn event_count(&self) -> usize {
        self.event_count
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceReconciliation {
    Applied,
    Unchanged,
}

/// Coordinates host publication without filesystem/keystore IO, automatic
/// retries, merging, feedback replay, or persistent consent. OS providers remain
/// separate; ordinary deterministic completion is independent of this session.
pub struct FormulaPreferenceStorageSession<B> {
    backend: B,
    owner_sha256: String,
    identity: FormulaPreferenceStorageIdentity,
    scope: FormulaDefinitionScope,
    keys: FormulaPreferenceHostKeys,
    core: FormulaPreferenceStore,
    head: Option<PreferenceStorageWitness>,
    revision: u64,
    owner: Arc<()>,
    recovery_required: bool,
}
fn permitted(permission: FormulaPreferenceStoragePermission) -> Result<()> {
    if permission.storage_enabled {
        Ok(())
    } else {
        Err(FormulaPreferenceStorageError::Disabled.into())
    }
}
fn empty_permission() -> FormulaPreferenceStoragePermission {
    // Explicit reset writes only a new empty epoch/tombstone, never history.
    FormulaPreferenceStoragePermission {
        storage_enabled: true,
        allow_sensitive_storage: false,
    }
}
impl<B: FormulaPreferenceCheckpointBackend> FormulaPreferenceStorageSession<B> {
    /// Explicit in-memory initialization. Does not read/write provider state;
    /// first commit uses CAS(None), never overwrites an existing protected root.
    pub fn new(
        backend: B,
        owner_sha256: String,
        scope: FormulaDefinitionScope,
        keys: FormulaPreferenceHostKeys,
    ) -> Result<Self> {
        if owner_sha256.len() != 64
            || !owner_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(FormulaPreferenceTransactionError::IdentityMismatch);
        }
        let core = FormulaPreferenceStore::new(scope.clone(), keys.feedback)
            .map_err(|_| FormulaPreferenceTransactionError::InvalidRecord)?;
        let owner = core.owner.clone();
        Ok(Self {
            backend,
            owner_sha256,
            identity: FormulaPreferenceStorageIdentity::from_scope(&scope),
            scope,
            keys,
            core,
            head: None,
            revision: 0,
            owner,
            recovery_required: false,
        })
    }
    /// Generation/keys/checkpoint come from the CURRENT PROTECTED record, never
    /// from its untrusted snapshot. Identity/namespace come from the trusted host.
    pub fn open(
        backend: B,
        owner_sha256: String,
        identity: FormulaPreferenceStorageIdentity,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<Self> {
        permitted(permission)?;
        let record = backend
            .load()?
            .ok_or(FormulaPreferenceTransactionError::NotFound)?;
        record.validate()?;
        if record.wire.owner_sha256 != owner_sha256
            || FormulaPreferenceStorageIdentity::from_scope(record.scope()) != identity
        {
            return Err(FormulaPreferenceTransactionError::IdentityMismatch);
        }
        let core = record.restore(permission)?;
        let owner = core.owner.clone();
        let head = record.witness()?;
        Ok(Self {
            backend,
            owner_sha256,
            identity,
            scope: record.wire.scope,
            keys: FormulaPreferenceHostKeys {
                feedback: record.wire.feedback_key,
                storage: record.wire.storage_key,
            },
            core,
            head: Some(head),
            revision: record.wire.revision,
            owner,
            recovery_required: false,
        })
    }
    pub fn preferences(&self) -> &FormulaPreferenceStore {
        &self.core
    }
    /// Use prepare_clear for durable reset. Replacing/clearing the raw Core with
    /// unrelated keys/scope makes subsequent staging refuse, never corrupt disk.
    pub fn preferences_mut(&mut self) -> &mut FormulaPreferenceStore {
        &mut self.core
    }
    pub fn recovery_required(&self) -> bool {
        self.recovery_required
    }
    fn healthy(&self) -> Result<()> {
        if self.recovery_required {
            return Err(FormulaPreferenceTransactionError::RecoveryRequired);
        }
        self.consistent()
    }
    fn consistent(&self) -> Result<()> {
        if !Arc::ptr_eq(&self.owner, &self.core.owner)
            || self.core.scope() != &self.scope
            || self.core.key != self.keys.feedback
            || FormulaPreferenceStorageIdentity::from_scope(&self.scope) != self.identity
        {
            return Err(FormulaPreferenceTransactionError::IdentityMismatch);
        }
        Ok(())
    }
    fn stage_record(
        &self,
        wire: RecordWire,
        base: Option<PreferenceStorageWitness>,
        reset: bool,
        event_count: usize,
    ) -> Result<PreparedPreferenceCheckpoint> {
        let candidate = ProtectedPreferenceRecord { wire };
        candidate.encode_for_protected_storage()?;
        Ok(PreparedPreferenceCheckpoint {
            owner: Arc::downgrade(&self.owner),
            base,
            candidate,
            reset,
            consumed: false,
            event_count,
        })
    }
    pub fn prepare(
        &mut self,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<PreparedPreferenceCheckpoint> {
        self.healthy()?;
        permitted(permission)?;
        // Reserve publication identity BEFORE advancing the Core export sequence.
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(FormulaPreferenceTransactionError::Exhausted)?;
        let checkpoint = self
            .core
            .last_checkpoint
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(FormulaPreferenceTransactionError::Exhausted)?;
        let snapshot = self
            .core
            .export_checkpoint(&self.keys.storage, checkpoint, permission)?;
        self.stage_record(
            RecordWire {
                schema_version: 1,
                owner_sha256: self.owner_sha256.clone(),
                revision,
                scope: self.scope.clone(),
                checkpoint,
                feedback_key: self.keys.feedback,
                storage_key: self.keys.storage,
                snapshot_json: snapshot,
            },
            self.head.clone(),
            false,
            self.core.event_count(),
        )
    }
    /// Explicit privacy reset is available even after ordinary storage is disabled.
    /// The new empty epoch/key tombstone prevents old history from resurrecting.
    pub fn prepare_clear(
        &mut self,
        generation: u64,
        keys: FormulaPreferenceHostKeys,
    ) -> Result<PreparedPreferenceCheckpoint> {
        self.consistent()?;
        // Explicit deletion may read trusted metadata even when history storage
        // consent has been revoked. Never restore or expose the old history here.
        let current = self.backend.load()?;
        if current.is_none() && self.head.is_some() {
            self.recovery_required = true;
            return Err(FormulaPreferenceTransactionError::NotFound);
        }
        let mut previous_keys = vec![self.keys.feedback, self.keys.storage];
        let mut latest_generation = self.scope.generation;
        let (base, revision) = if let Some(record) = current {
            record.validate()?;
            if record.wire.owner_sha256 != self.owner_sha256
                || FormulaPreferenceStorageIdentity::from_scope(record.scope()) != self.identity
            {
                return Err(FormulaPreferenceTransactionError::IdentityMismatch);
            }
            latest_generation = latest_generation.max(record.scope().generation);
            previous_keys.extend([record.wire.feedback_key, record.wire.storage_key]);
            (Some(record.witness()?), record.revision())
        } else {
            (None, 0)
        };
        if latest_generation == u64::MAX {
            return Err(FormulaPreferenceTransactionError::Exhausted);
        }
        if generation <= latest_generation
            || previous_keys
                .iter()
                .any(|old| *old == keys.feedback || *old == keys.storage)
        {
            return Err(FormulaPreferenceTransactionError::IdentityMismatch);
        }
        let revision = revision
            .checked_add(1)
            .ok_or(FormulaPreferenceTransactionError::Exhausted)?;
        let mut scope = self.scope.clone();
        scope.generation = generation;
        let mut empty = FormulaPreferenceStore::new(scope.clone(), keys.feedback)
            .map_err(|_| FormulaPreferenceTransactionError::InvalidRecord)?;
        let snapshot = empty.export_checkpoint(&keys.storage, 1, empty_permission())?;
        if base != self.head {
            self.recovery_required = true;
        }
        self.stage_record(
            RecordWire {
                schema_version: 1,
                owner_sha256: self.owner_sha256.clone(),
                revision,
                scope,
                checkpoint: 1,
                feedback_key: keys.feedback,
                storage_key: keys.storage,
                snapshot_json: snapshot,
            },
            base,
            true,
            0,
        )
    }
    fn check_stage(&self, stage: &PreparedPreferenceCheckpoint) -> Result<()> {
        if stage.consumed
            || !Weak::ptr_eq(&stage.owner, &Arc::downgrade(&self.owner))
            || (!stage.reset && stage.base != self.head)
        {
            return Err(FormulaPreferenceTransactionError::StaleTransaction);
        }
        self.consistent()?;
        if FormulaPreferenceStorageIdentity::from_scope(stage.candidate.scope()) != self.identity {
            return Err(FormulaPreferenceTransactionError::IdentityMismatch);
        }
        Ok(())
    }
    fn validate_candidate(
        &self,
        stage: &PreparedPreferenceCheckpoint,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<FormulaPreferenceStore> {
        let mut restored = stage.candidate.restore(if stage.reset {
            empty_permission()
        } else {
            permission
        })?;
        if stage.reset {
            restored
                .set_policy(self.core.policy())
                .map_err(|_| FormulaPreferenceTransactionError::Exhausted)?;
        }
        Ok(restored)
    }
    fn adopt(
        &mut self,
        stage: &mut PreparedPreferenceCheckpoint,
        restored: FormulaPreferenceStore,
        witness: PreferenceStorageWitness,
    ) {
        self.head = Some(witness);
        self.revision = stage.candidate.revision();
        if stage.reset {
            self.scope = stage.candidate.wire.scope.clone();
            self.keys = FormulaPreferenceHostKeys {
                feedback: stage.candidate.wire.feedback_key,
                storage: stage.candidate.wire.storage_key,
            };
            self.core = restored;
            self.owner = self.core.owner.clone();
        }
        stage.consumed = true;
        self.recovery_required = false;
    }
    pub fn commit(
        &mut self,
        stage: &mut PreparedPreferenceCheckpoint,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<()> {
        if !stage.reset {
            self.healthy()?;
        }
        self.check_stage(stage)?;
        let restored = self.validate_candidate(stage, permission)?;
        let witness = stage.candidate.witness()?;
        // Even an unwinding provider may have selected the record. Fence writes
        // BEFORE crossing that boundary; only a definitive outcome clears it.
        self.recovery_required = true;
        match self
            .backend
            .compare_exchange(stage.base.as_ref(), &stage.candidate)
        {
            PreferencePublication::Applied => {
                self.adopt(stage, restored, witness);
                Ok(())
            }
            PreferencePublication::NotCommitted => {
                self.recovery_required = stage.base != self.head;
                Err(FormulaPreferenceTransactionError::NotCommitted)
            }
            outcome => {
                self.recovery_required = true;
                Err(if outcome == PreferencePublication::Conflict {
                    FormulaPreferenceTransactionError::Conflict
                } else {
                    FormulaPreferenceTransactionError::Indeterminate
                })
            }
        }
    }
    /// Read back exact protected identity; never replay publication automatically.
    /// Unchanged permits an EXPLICIT same-stage retry, Applied consumes it, and a
    /// different winner keeps writes suspended until explicit reopen/discard.
    pub fn reconcile(
        &mut self,
        stage: &mut PreparedPreferenceCheckpoint,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<PreferenceReconciliation> {
        self.check_stage(stage)?;
        let restored = self.validate_candidate(stage, permission)?;
        let current = self.backend.load()?;
        let witness = current
            .as_ref()
            .map(ProtectedPreferenceRecord::witness)
            .transpose()?;
        let candidate = stage.candidate.witness()?;
        if witness.as_ref() == Some(&candidate) {
            self.adopt(stage, restored, candidate);
            Ok(PreferenceReconciliation::Applied)
        } else if witness == stage.base {
            self.recovery_required = stage.base != self.head;
            Ok(PreferenceReconciliation::Unchanged)
        } else {
            self.recovery_required = true;
            Err(FormulaPreferenceTransactionError::Conflict)
        }
    }
}
