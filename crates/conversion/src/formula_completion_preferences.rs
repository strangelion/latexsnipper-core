//! Explicit, bounded in-memory feedback. No automatic history or storage IO.
//!
//! The trusted host owns consent, keys, actual acceptance/undo and persistence.
//! A receipt records feedback only; it grants no editing or plugin permission.
use super::{fragments::*, scoped::*, *};
use std::sync::{Arc, Weak};

pub const FORMULA_PREFERENCE_MAX_EVENTS: usize = 1024;

/// Disabled by default; sensitive learning requires its own explicit host allow.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormulaPreferencePolicy {
    pub ranking_enabled: bool,
    pub feedback_enabled: bool,
    pub allow_sensitive_feedback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaPreferenceError {
    InvalidIdentity,
    ScopeMismatch,
    StaleFeedback,
    LimitExceeded,
    Completion(ScopedFormulaCompletionError),
}
impl std::fmt::Display for FormulaPreferenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidIdentity => "Invalid preference scope or host key.",
            Self::ScopeMismatch => "Preference scope does not match the current request.",
            Self::StaleFeedback => {
                "Feedback is stale, already consumed, or belongs to another store."
            }
            Self::LimitExceeded => "Preference feedback budget or generation exhausted.",
            Self::Completion(_) => "Completion validation refused preference feedback.",
        })
    }
}
impl std::error::Error for FormulaPreferenceError {}
impl From<ScopedFormulaCompletionError> for FormulaPreferenceError {
    fn from(error: ScopedFormulaCompletionError) -> Self {
        Self::Completion(error)
    }
}

/// Non-serializable, instance-bound receipt. Undo can consume it only once.
#[derive(Debug)]
pub struct FormulaPreferenceReceipt {
    owner: Weak<()>,
    event_id: u64,
    epoch: u64,
}

/// Capture before dispatching an acceptance/feedback job. This is not consent;
/// it only prevents late queued feedback from crossing a policy/reset boundary.
#[derive(Debug)]
pub struct FormulaPreferenceCapture {
    owner: Weak<()>,
    epoch: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaPreferenceFeatures {
    pub accepted: u16,
    pub undone: u16,
    /// Event order, not wall time. Zero means no surviving acceptance.
    #[serde(with = "wire::revision_decimal")]
    pub last_accepted_event: u64,
}
impl FormulaPreferenceFeatures {
    pub fn net_acceptances(&self) -> u16 {
        self.accepted.saturating_sub(self.undone)
    }
}

/// Only a permutation of freshly generated Core candidates, never replacement edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreferredFormulaCompletion {
    pub completion: FormulaFragmentCompletion,
    pub order: Vec<usize>,
    /// Aligned with the original candidates, not the reordered indices.
    pub features: Vec<FormulaPreferenceFeatures>,
}

struct FeedbackEvent {
    id: u64,
    key: [u8; 32],
    undone: bool,
}

/// Host-owned volatile state, not Clone/Serialize/Debug. Never stores formula text,
/// candidate names, timestamps, editor revisions or document contents.
pub struct FormulaPreferenceStore {
    scope: FormulaDefinitionScope,
    key: [u8; 32],
    owner: Arc<()>,
    policy: FormulaPreferencePolicy,
    epoch: u64,
    last_event: Option<u64>,
    events: Vec<FeedbackEvent>,
}

impl FormulaPreferenceStore {
    /// Supply a unique, cryptographically random host key for each store/user.
    /// The key is for local pseudonyms, not authentication or encryption.
    pub fn new(
        scope: FormulaDefinitionScope,
        key: [u8; 32],
    ) -> Result<Self, FormulaPreferenceError> {
        let valid_hash = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        };
        if !valid_hash(&scope.user_sha256)
            || !scope.project_sha256.as_deref().is_none_or(valid_hash)
            || !scope.document_sha256.as_deref().is_none_or(valid_hash)
            || key.iter().all(|byte| *byte == 0)
        {
            return Err(FormulaPreferenceError::InvalidIdentity);
        }
        Ok(Self {
            scope,
            key,
            owner: Arc::new(()),
            policy: FormulaPreferencePolicy::default(),
            epoch: 0,
            last_event: None,
            events: Vec::new(),
        })
    }

    pub fn policy(&self) -> FormulaPreferencePolicy {
        self.policy
    }
    pub fn scope(&self) -> &FormulaDefinitionScope {
        &self.scope
    }
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn capture_context(&self) -> FormulaPreferenceCapture {
        FormulaPreferenceCapture {
            owner: Arc::downgrade(&self.owner),
            epoch: self.epoch,
        }
    }

    /// Any policy transition invalidates outstanding receipts, including ABA.
    /// Ranking/capture switches are independent; disabling both retains history
    /// without exposing scores or capturing new feedback.
    pub fn set_policy(
        &mut self,
        policy: FormulaPreferencePolicy,
    ) -> Result<(), FormulaPreferenceError> {
        if policy != self.policy {
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or(FormulaPreferenceError::LimitExceeded)?;
            self.epoch = epoch;
            self.policy = policy;
        }
        Ok(())
    }

    /// Clear is always available, even when disabled. A strictly newer scope epoch
    /// and a fresh host key prevent old queued feedback/pseudonyms from returning.
    pub fn clear(&mut self, generation: u64, key: [u8; 32]) -> Result<(), FormulaPreferenceError> {
        if generation <= self.scope.generation
            || key == self.key
            || key.iter().all(|byte| *byte == 0)
        {
            return Err(FormulaPreferenceError::InvalidIdentity);
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or(FormulaPreferenceError::LimitExceeded)?;
        self.events.clear();
        self.scope.generation = generation;
        self.key = key;
        self.epoch = epoch;
        self.last_event = None;
        Ok(())
    }

    fn check_scope(&self, scope: &FormulaDefinitionScope) -> Result<(), FormulaPreferenceError> {
        if scope != &self.scope {
            Err(FormulaPreferenceError::ScopeMismatch)
        } else {
            Ok(())
        }
    }

    fn candidate_key(
        &self,
        candidate: &FormulaFragmentCandidate,
        catalog_sha256: &str,
    ) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(b"latexsnipper:volatile-preference:v1\0");
        digest.update(self.key);
        // Length-frame all variable fields. A body/signature revision change must
        // not inherit an unrelated candidate's feedback under a reused ID.
        for field in [
            &self.scope.user_sha256,
            self.scope.project_sha256.as_deref().unwrap_or(""),
            self.scope.document_sha256.as_deref().unwrap_or(""),
            &candidate.edit.id,
            &candidate.edit.insert,
            catalog_sha256,
        ] {
            digest.update((field.len() as u64).to_be_bytes());
            digest.update(field.as_bytes());
        }
        digest.update(self.scope.generation.to_be_bytes());
        for reference in [
            candidate.definition.as_ref().map(|value| value.revision),
            candidate.fragment.as_ref().map(|value| value.revision),
        ] {
            digest.update([u8::from(reference.is_some())]);
            digest.update(reference.unwrap_or(0).to_be_bytes());
        }
        digest.finalize().into()
    }

    /// Call ONLY after the host actually committed an explicit user acceptance.
    /// Source/snapshot/catalog are regenerated and validated before state changes.
    /// None means capture is disabled; it does not block ordinary completion.
    pub fn record_acceptance_at(
        &mut self,
        capture: &FormulaPreferenceCapture,
        request: &FormulaFragmentCompletionApplyRequest,
        event_id: u64,
    ) -> Result<Option<FormulaPreferenceReceipt>, FormulaPreferenceError> {
        if capture.epoch != self.epoch
            || !Weak::ptr_eq(&capture.owner, &Arc::downgrade(&self.owner))
        {
            return Err(FormulaPreferenceError::StaleFeedback);
        }
        self.check_scope(&request.catalog.commands.scope)?;
        if !self.policy.feedback_enabled
            || (request.catalog.policy.sensitive_document && !self.policy.allow_sensitive_feedback)
        {
            return Ok(None);
        }
        if event_id == 0 || self.last_event.is_some_and(|previous| event_id <= previous) {
            return Err(FormulaPreferenceError::StaleFeedback);
        }
        if self.events.len() >= FORMULA_PREFERENCE_MAX_EVENTS {
            return Err(FormulaPreferenceError::LimitExceeded);
        }
        request.execute()?;
        let current = complete_formula_fragments(
            &request.source,
            request.cursor,
            request.revision,
            &request.catalog,
        )?;
        let candidate = current
            .candidates
            .iter()
            .find(|value| value.edit.id == request.candidate_id)
            .ok_or(FormulaPreferenceError::Completion(
                ScopedFormulaCompletionError::UnknownCandidate,
            ))?;
        let key = self.candidate_key(candidate, &current.snapshot.catalog_sha256);
        self.events.push(FeedbackEvent {
            id: event_id,
            key,
            undone: false,
        });
        self.last_event = Some(event_id);
        Ok(Some(FormulaPreferenceReceipt {
            owner: Arc::downgrade(&self.owner),
            event_id,
            epoch: self.epoch,
        }))
    }

    /// Host reports a real undo. No source text or caller-supplied candidate key.
    pub fn record_undo(
        &mut self,
        receipt: &FormulaPreferenceReceipt,
    ) -> Result<(), FormulaPreferenceError> {
        if !self.policy.feedback_enabled
            || receipt.epoch != self.epoch
            || !Weak::ptr_eq(&receipt.owner, &Arc::downgrade(&self.owner))
        {
            return Err(FormulaPreferenceError::StaleFeedback);
        }
        let event = self
            .events
            .iter_mut()
            .find(|value| value.id == receipt.event_id && !value.undone)
            .ok_or(FormulaPreferenceError::StaleFeedback)?;
        event.undone = true;
        Ok(())
    }

    fn features(&self, key: [u8; 32]) -> FormulaPreferenceFeatures {
        let mut features = FormulaPreferenceFeatures::default();
        for event in self.events.iter().filter(|value| value.key == key) {
            features.accepted += 1;
            if event.undone {
                features.undone += 1;
            } else {
                features.last_accepted_event = event.id;
            }
        }
        features
    }

    /// Deterministic frequency/recency baseline over current Core candidates.
    /// It cannot add candidates, edit insertions, apply source or grant permission.
    pub fn rank_fragments(
        &self,
        request: &FormulaFragmentCompletionRequest,
    ) -> Result<PreferredFormulaCompletion, FormulaPreferenceError> {
        self.check_scope(&request.catalog.commands.scope)?;
        let completion = request.execute()?;
        let enabled = self.policy.ranking_enabled
            && (!request.catalog.policy.sensitive_document || self.policy.allow_sensitive_feedback);
        let features: Vec<_> = completion
            .candidates
            .iter()
            .map(|candidate| {
                if enabled {
                    self.features(
                        self.candidate_key(candidate, &completion.snapshot.catalog_sha256),
                    )
                } else {
                    FormulaPreferenceFeatures::default()
                }
            })
            .collect();
        let mut order: Vec<_> = (0..completion.candidates.len()).collect();
        order.sort_by_key(|index| {
            (
                std::cmp::Reverse(features[*index].net_acceptances()),
                std::cmp::Reverse(features[*index].last_accepted_event),
                *index,
            )
        });
        Ok(PreferredFormulaCompletion {
            completion,
            order,
            features,
        })
    }
}
