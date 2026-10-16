//! Opt-in trusted-host Windows storage. JSON flags are not consent credentials.
use super::*;
use latexsnipper_conversion::formula_completion::preferences::persistence::transactions::*;
use latexsnipper_preference_storage::{
    windows::{
        generate_host_keys, WindowsPreferenceCheckpoint, WindowsPreferenceOpenRequest,
        WindowsPreferenceSessionError,
    },
    PreferenceProviderError,
};

#[derive(Deserialize)]
#[serde(transparent)]
pub(super) struct Open(WindowsPreferenceOpenRequest);

fn session_error(value: WindowsPreferenceSessionError) -> AdapterError {
    error(value.code(), &value.to_string())
}

fn provider_error(value: PreferenceProviderError) -> AdapterError {
    session_error(value.into())
}
fn transaction_error(value: FormulaPreferenceTransactionError) -> AdapterError {
    session_error(value.into())
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
        let opened = self.0.open().map_err(session_error)?;
        Ok(ProtectedOwner {
            session: opened.session,
            backend: opened.backend,
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
