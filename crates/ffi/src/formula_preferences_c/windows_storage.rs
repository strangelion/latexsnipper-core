//! Opt-in trusted-host Windows storage. JSON flags are not consent credentials.
use super::*;
use latexsnipper_conversion::formula_completion::preferences::persistence::transactions::*;
use latexsnipper_preference_storage::{
    windows::{generate_host_keys, WindowsPreferenceCheckpoint, WindowsPreferencePersistence},
    PreferenceProviderError,
};
use std::path::Path;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Open {
    schema_version: u16,
    owner: String,
    identity: Identity,
    persistence: Persistence,
    operation_timeout_ms: u32,
    permission: FormulaPreferenceStoragePermission,
    action: Action,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Identity {
    user_sha256: String,
    project_sha256: Option<String>,
    document_sha256: Option<String>,
}
impl From<Identity> for FormulaPreferenceStorageIdentity {
    fn from(value: Identity) -> Self {
        Self {
            user_sha256: value.user_sha256,
            project_sha256: value.project_sha256,
            document_sha256: value.document_sha256,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Persistence {
    LogonSession,
    ThisUserThisComputer,
}
impl From<Persistence> for WindowsPreferencePersistence {
    fn from(value: Persistence) -> Self {
        match value {
            Persistence::LogonSession => Self::LogonSession,
            Persistence::ThisUserThisComputer => Self::ThisUserThisComputer,
        }
    }
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Action {
    OpenExisting {
        root_path: String,
    },
    InitializeExisting {
        root_path: String,
        generation: Decimal,
    },
    CreateNew {
        parent_path: String,
        directory_name: String,
        generation: Decimal,
    },
}

fn provider_error(value: PreferenceProviderError) -> AdapterError {
    let code = match value {
        PreferenceProviderError::InvalidConfiguration => "INVALID_ARGUMENT",
        PreferenceProviderError::PrivateDirectoryRequired => "PRIVATE_DIRECTORY_REQUIRED",
        PreferenceProviderError::Unavailable => "PREFERENCE_PROVIDER_UNAVAILABLE",
        PreferenceProviderError::InvalidRecord => "INVALID_PROTECTED_RECORD",
        PreferenceProviderError::IntegrityMismatch => "PREFERENCE_INTEGRITY_MISMATCH",
        PreferenceProviderError::DeadlineExceeded => "PREFERENCE_DEADLINE_EXCEEDED",
        PreferenceProviderError::CleanupLimit => "PREFERENCE_CLEANUP_LIMIT",
    };
    error(code, &value.to_string())
}
fn transaction_error(value: FormulaPreferenceTransactionError) -> AdapterError {
    if let FormulaPreferenceTransactionError::Storage(value) = value {
        return storage_error(value);
    }
    let code = match value {
        FormulaPreferenceTransactionError::InvalidRecord => "INVALID_PROTECTED_RECORD",
        FormulaPreferenceTransactionError::IdentityMismatch => "PREFERENCE_IDENTITY_MISMATCH",
        FormulaPreferenceTransactionError::NotFound => "PREFERENCE_STORAGE_NOT_FOUND",
        FormulaPreferenceTransactionError::ProviderUnavailable => "PREFERENCE_PROVIDER_UNAVAILABLE",
        FormulaPreferenceTransactionError::StaleTransaction => "PREFERENCE_STALE_TRANSACTION",
        FormulaPreferenceTransactionError::Conflict => "PREFERENCE_CONFLICT",
        FormulaPreferenceTransactionError::NotCommitted => "PREFERENCE_NOT_COMMITTED",
        FormulaPreferenceTransactionError::Indeterminate => "PREFERENCE_INDETERMINATE",
        FormulaPreferenceTransactionError::RecoveryRequired => "PREFERENCE_RECOVERY_REQUIRED",
        FormulaPreferenceTransactionError::Exhausted => "LIMIT_EXCEEDED",
        FormulaPreferenceTransactionError::Storage(_) => unreachable!(),
    };
    error(code, &value.to_string())
}

type Session = FormulaPreferenceStorageSession<WindowsPreferenceCheckpoint>;
struct Stage {
    core: PreparedPreferenceCheckpoint,
    clear: bool,
}
impl Stage {
    fn info(&self, token: u64) -> Value {
        json!({"stageId":token.to_string(), "revision":self.core.revision().to_string(),
            "checkpoint":self.core.checkpoint().to_string(), "eventCount":self.core.event_count(),
            "clear":self.clear})
    }
}
pub(super) struct ProtectedOwner {
    pub(super) session: Session,
    backend: WindowsPreferenceCheckpoint,
    stages: BTreeMap<u64, Stage>,
}
pub(super) struct Outcome {
    pub(super) value: Value,
    pub(super) cleared: bool,
}

impl Open {
    pub(super) fn open(self) -> Result<ProtectedOwner> {
        if self.schema_version != 1 {
            return Err(error(
                "VERSION_MISMATCH",
                "Unsupported preference storage schema.",
            ));
        }
        // Consent is checked before any directory/provider IO, even provisioning.
        if !self.permission.storage_enabled {
            return Err(storage_error(FormulaPreferenceStorageError::Disabled));
        }
        let identity: FormulaPreferenceStorageIdentity = self.identity.into();
        let persistence = self.persistence.into();
        let timeout = Duration::from_millis(self.operation_timeout_ms.into());
        let (backend, generation) = match self.action {
            Action::OpenExisting { root_path } => (
                WindowsPreferenceCheckpoint::open_verified_private_root(
                    Path::new(&root_path),
                    &self.owner,
                    identity.clone(),
                    persistence,
                    timeout,
                )
                .map_err(provider_error)?,
                None,
            ),
            Action::InitializeExisting {
                root_path,
                generation,
            } => (
                WindowsPreferenceCheckpoint::open_verified_private_root(
                    Path::new(&root_path),
                    &self.owner,
                    identity.clone(),
                    persistence,
                    timeout,
                )
                .map_err(provider_error)?,
                Some(generation.0),
            ),
            Action::CreateNew {
                parent_path,
                directory_name,
                generation,
            } => (
                WindowsPreferenceCheckpoint::create_private_root(
                    Path::new(&parent_path),
                    &directory_name,
                    &self.owner,
                    identity.clone(),
                    persistence,
                    timeout,
                )
                .map_err(provider_error)?,
                Some(generation.0),
            ),
        };
        let owner = backend.owner_sha256().to_owned();
        let session = match generation {
            None => Session::open(backend.clone(), owner, identity, self.permission)
                .map_err(transaction_error)?,
            Some(generation) => {
                // Explicit initialization never falls back from missing/corrupt
                // restore, and CAS(None) still protects the later publication race.
                if backend
                    .read_current_record()
                    .map_err(provider_error)?
                    .is_some()
                {
                    return Err(error(
                        "PREFERENCE_ALREADY_INITIALIZED",
                        "Protected preference state already exists.",
                    ));
                }
                let scope = FormulaDefinitionScope {
                    user_sha256: identity.user_sha256,
                    project_sha256: identity.project_sha256,
                    document_sha256: identity.document_sha256,
                    generation,
                };
                Session::new(
                    backend.clone(),
                    owner,
                    scope,
                    generate_host_keys().map_err(provider_error)?,
                )
                .map_err(transaction_error)?
            }
        };
        Ok(ProtectedOwner {
            session,
            backend,
            stages: BTreeMap::new(),
        })
    }
}

impl ProtectedOwner {
    pub(super) fn info(&self) -> Value {
        json!({"provider":"windows-dpapi", "ownerSha256":self.backend.owner_sha256(),
            "pendingStages":self.stages.len(), "recoveryRequired":self.session.recovery_required(),
            "keysReturned":false})
    }
    pub(super) fn execute(&mut self, command: Command) -> Result<Outcome> {
        let mut cleared = false;
        let value = match command {
            Command::StoragePrepare { permission } => {
                if self.stages.len() >= MAX_PREPARED_STAGES {
                    return Err(error(
                        "LIMIT_EXCEEDED",
                        "Prepared preference stage budget exhausted.",
                    ));
                }
                // Reserve routing identity BEFORE advancing the export sequence.
                let token = next_token(&NEXT_TOKEN)?;
                let stage = Stage {
                    core: self
                        .session
                        .prepare(permission)
                        .map_err(transaction_error)?,
                    clear: false,
                };
                let info = stage.info(token);
                self.stages.insert(token, stage);
                info
            }
            Command::StoragePrepareClear { generation } => {
                let token = next_token(&NEXT_TOKEN)?;
                let core = self
                    .session
                    .prepare_clear(generation.0, generate_host_keys().map_err(provider_error)?)
                    .map_err(transaction_error)?;
                // Only replace pending stages after a successful explicit reset
                // preparation; failed reads/validation retain all old stages.
                let stage = Stage { core, clear: true };
                let info = stage.info(token);
                self.stages.clear();
                self.stages.insert(token, stage);
                info
            }
            Command::StorageCommit {
                stage_id,
                permission,
            } => {
                let stage = self.stages.get_mut(&stage_id.0).ok_or_else(unknown_stage)?;
                self.session
                    .commit(&mut stage.core, permission)
                    .map_err(transaction_error)?;
                cleared = stage.clear;
                // All other stages belong to the old base/epoch. Invalidate
                // them now rather than retaining unusable entries against quota.
                self.stages.clear();
                json!({"stageId":stage_id.0.to_string(), "status":"applied"})
            }
            Command::StorageReconcile {
                stage_id,
                permission,
            } => {
                let stage = self.stages.get_mut(&stage_id.0).ok_or_else(unknown_stage)?;
                let status = match self
                    .session
                    .reconcile(&mut stage.core, permission)
                    .map_err(transaction_error)?
                {
                    PreferenceReconciliation::Applied => {
                        cleared = stage.clear;
                        self.stages.clear();
                        "applied"
                    }
                    PreferenceReconciliation::Unchanged => "unchanged",
                };
                json!({"stageId":stage_id.0.to_string(), "status":status})
            }
            Command::StorageDiscardStage { stage_id } => {
                self.stages.remove(&stage_id.0).ok_or_else(unknown_stage)?;
                // Discard is NOT reconciliation, cancellation or durable undo.
                json!({"stageId":stage_id.0.to_string(), "discarded":true})
            }
            Command::StorageCleanup { maximum } => {
                let result = self
                    .backend
                    .cleanup_unselected(maximum.into())
                    .map_err(provider_error)?;
                json!({"inspected":result.inspected, "removed":result.removed, "more":result.more})
            }
            _ => {
                return Err(error(
                    "INVALID_ARGUMENT",
                    "Not a protected preference storage command.",
                ))
            }
        };
        Ok(Outcome { value, cleared })
    }
}
fn unknown_stage() -> AdapterError {
    error(
        "PREFERENCE_STAGE_NOT_FOUND",
        "Preference stage is unknown, discarded or invalidated.",
    )
}
