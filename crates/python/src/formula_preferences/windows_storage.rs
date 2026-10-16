//! Instance-owned stages. No process-global handles or automatic publication.
use super::*;
use latexsnipper_conversion::formula_completion::preferences::persistence::transactions::*;
use latexsnipper_preference_storage::windows::{
    generate_host_keys, OpenedWindowsPreferenceStorage, WindowsPreferenceOpenRequest,
    WindowsPreferenceSessionError,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Weak};

fn native_error(value: impl Into<WindowsPreferenceSessionError>) -> BindingError {
    let value = value.into();
    error(value.code(), &value.to_string())
}
struct Stage {
    core: PreparedPreferenceCheckpoint,
}
pub(super) struct ProtectedOwner {
    pub(super) storage: OpenedWindowsPreferenceStorage,
    identity: Arc<()>,
    next: u64,
    stages: BTreeMap<u64, Stage>,
}
impl ProtectedOwner {
    pub(super) fn open(encoded: &str) -> Result<Self, BindingError> {
        if encoded.is_empty() || encoded.len() > MAX_STORAGE_OPEN_BYTES {
            return Err(BindingError::invalid(
                "Protected preference open request exceeds 16 KiB.",
            ));
        }
        let request: WindowsPreferenceOpenRequest = serde_json::from_str(encoded)
            .map_err(|_| BindingError::invalid("Invalid closed protected preference metadata."))?;
        Ok(Self {
            storage: request.open().map_err(native_error)?,
            identity: Arc::new(()),
            next: 0,
            stages: BTreeMap::new(),
        })
    }
    pub(super) fn info(&self) -> serde_json::Value {
        json!({"provider":"windows-dpapi","ownerSha256":self.storage.backend.owner_sha256(),
            "pendingStages":self.stages.len(),"maxPreparedStages":MAX_PREPARED_STAGES,
            "recoveryRequired":self.storage.session.recovery_required(),"keysReturned":false})
    }
    fn reserve(&mut self, clear: bool) -> Result<u64, BindingError> {
        if !clear && self.stages.len() >= MAX_PREPARED_STAGES {
            return Err(error(
                "LIMIT_EXCEEDED",
                "Prepared preference stage budget exhausted.",
            ));
        }
        self.next = self.next.checked_add(1).ok_or_else(|| {
            error(
                "LIMIT_EXCEEDED",
                "Preference stage identity space exhausted.",
            )
        })?;
        Ok(self.next)
    }
    fn insert(
        &mut self,
        id: u64,
        core: PreparedPreferenceCheckpoint,
        clear: bool,
    ) -> PreferenceStorageStage {
        let token = PreferenceStorageStage {
            id,
            owner: Arc::downgrade(&self.identity),
            checkpoint: core.checkpoint(),
            revision: core.revision(),
            event_count: core.event_count(),
            clear,
        };
        if clear {
            self.stages.clear();
        }
        self.stages.insert(id, Stage { core });
        token
    }
    pub(super) fn prepare(
        &mut self,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<PreferenceStorageStage, BindingError> {
        let id = self.reserve(false)?;
        let core = self
            .storage
            .session
            .prepare(permission)
            .map_err(native_error)?;
        Ok(self.insert(id, core, false))
    }
    pub(super) fn prepare_clear(
        &mut self,
        generation: u64,
    ) -> Result<PreferenceStorageStage, BindingError> {
        let id = self.reserve(true)?;
        let keys = generate_host_keys().map_err(native_error)?;
        let core = self
            .storage
            .session
            .prepare_clear(generation, keys)
            .map_err(native_error)?;
        Ok(self.insert(id, core, true))
    }
    fn check(&self, stage: &PreferenceStorageStage) -> Result<(), BindingError> {
        if !Weak::ptr_eq(&stage.owner, &Arc::downgrade(&self.identity))
            || !self.stages.contains_key(&stage.id)
        {
            return Err(error(
                "PREFERENCE_STAGE_NOT_FOUND",
                "Preference stage belongs to another, closed or invalidated owner.",
            ));
        }
        Ok(())
    }
    pub(super) fn commit(
        &mut self,
        stage: &PreferenceStorageStage,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<(), BindingError> {
        self.check(stage)?;
        self.storage
            .session
            .commit(
                &mut self.stages.get_mut(&stage.id).unwrap().core,
                permission,
            )
            .map_err(native_error)?;
        self.stages.clear();
        Ok(())
    }
    pub(super) fn reconcile(
        &mut self,
        stage: &PreferenceStorageStage,
        permission: FormulaPreferenceStoragePermission,
    ) -> Result<&'static str, BindingError> {
        self.check(stage)?;
        let outcome = self
            .storage
            .session
            .reconcile(
                &mut self.stages.get_mut(&stage.id).unwrap().core,
                permission,
            )
            .map_err(native_error)?;
        match outcome {
            PreferenceReconciliation::Applied => {
                self.stages.clear();
                Ok("applied")
            }
            PreferenceReconciliation::Unchanged => Ok("unchanged"),
        }
    }
    pub(super) fn discard(&mut self, stage: &PreferenceStorageStage) -> Result<(), BindingError> {
        self.check(stage)?;
        self.stages.remove(&stage.id);
        Ok(())
    }
    pub(super) fn cleanup(&self, maximum: u64) -> Result<String, BindingError> {
        if !(1..=128).contains(&maximum) {
            return Err(BindingError::invalid(
                "Preference cleanup maximum must be 1..128.",
            ));
        }
        let cleaned = self
            .storage
            .backend
            .cleanup_unselected(maximum as usize)
            .map_err(native_error)?;
        encode_result(
            &json!({"inspected":cleaned.inspected,"removed":cleaned.removed,"more":cleaned.more}),
        )
    }
}
