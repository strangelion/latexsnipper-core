use latexsnipper_conversion::formula_completion::{
    fragments::*, preferences::persistence::transactions::*, preferences::persistence::*,
    preferences::*,
};
use serde_json::{json, Value};
use std::sync::{Arc, Barrier, Mutex};

// Public fixture keys only. This test provider is NOT protected OS storage.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Healthy,
    Refused,
    UnknownBefore,
    UnknownAfter,
    Pending,
    PanicAfter,
}
struct State {
    bytes: Option<Vec<u8>>,
    mode: Mode,
    loads: usize,
    writes: usize,
    unavailable: bool,
    pending: Option<Vec<u8>>,
}
#[derive(Clone)]
struct Backend(Arc<Mutex<State>>);
impl Backend {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(State {
            bytes: None,
            mode: Mode::Healthy,
            loads: 0,
            writes: 0,
            unavailable: false,
            pending: None,
        })))
    }
    fn mode(&self, mode: Mode) {
        self.0.lock().unwrap().mode = mode;
    }
    fn counts(&self) -> (usize, usize) {
        let state = self.0.lock().unwrap();
        (state.loads, state.writes)
    }
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().bytes.clone().unwrap()
    }
    fn replace_fixture(&self, bytes: Vec<u8>) {
        self.0.lock().unwrap().bytes = Some(bytes);
    }
    fn settle(&self, applied: bool) {
        let mut state = self.0.lock().unwrap();
        let candidate = state.pending.take().unwrap();
        if applied {
            state.bytes = Some(candidate);
        }
        state.mode = Mode::Healthy;
    }
}
impl FormulaPreferenceCheckpointBackend for Backend {
    fn load(&self) -> Result<Option<ProtectedPreferenceRecord>, FormulaPreferenceTransactionError> {
        let mut state = self.0.lock().unwrap();
        state.loads += 1;
        if state.unavailable || state.pending.is_some() {
            return Err(FormulaPreferenceTransactionError::ProviderUnavailable);
        }
        state
            .bytes
            .as_deref()
            .map(ProtectedPreferenceRecord::decode_from_protected_storage)
            .transpose()
    }
    fn compare_exchange(
        &self,
        expected: Option<&PreferenceStorageWitness>,
        candidate: &ProtectedPreferenceRecord,
    ) -> PreferencePublication {
        let mut state = self.0.lock().unwrap();
        state.writes += 1;
        if state.pending.is_some() {
            return PreferencePublication::Indeterminate;
        }
        let current = state.bytes.as_deref().map(|bytes| {
            ProtectedPreferenceRecord::decode_from_protected_storage(bytes)
                .unwrap()
                .witness()
                .unwrap()
        });
        if current.as_ref() != expected {
            return PreferencePublication::Conflict;
        }
        match state.mode {
            Mode::Refused => return PreferencePublication::NotCommitted,
            Mode::UnknownBefore => return PreferencePublication::Indeterminate,
            Mode::Pending => {
                state.pending = Some(candidate.encode_for_protected_storage().unwrap());
                return PreferencePublication::Indeterminate;
            }
            _ => {}
        }
        state.bytes = Some(candidate.encode_for_protected_storage().unwrap());
        let mode = state.mode;
        drop(state);
        if mode == Mode::PanicAfter {
            panic!("fixture provider interrupted after selection");
        }
        if mode == Mode::UnknownAfter {
            PreferencePublication::Indeterminate
        } else {
            PreferencePublication::Applied
        }
    }
}
type Session = FormulaPreferenceStorageSession<Backend>;
type Tx = FormulaPreferenceTransactionError;
fn permission() -> FormulaPreferenceStoragePermission {
    FormulaPreferenceStoragePermission {
        storage_enabled: true,
        allow_sensitive_storage: false,
    }
}
fn off() -> FormulaPreferenceStoragePermission {
    FormulaPreferenceStoragePermission::default()
}
fn keys() -> FormulaPreferenceHostKeys {
    FormulaPreferenceHostKeys::new([19; 32], [27; 32]).unwrap()
}
fn fresh_keys() -> FormulaPreferenceHostKeys {
    FormulaPreferenceHostKeys::new([31; 32], [37; 32]).unwrap()
}
fn enabled(sensitive: bool) -> FormulaPreferencePolicy {
    FormulaPreferencePolicy {
        ranking_enabled: true,
        feedback_enabled: true,
        allow_sensitive_feedback: sensitive,
    }
}
fn query() -> FormulaFragmentCompletionRequest {
    serde_json::from_value(json!({"source":"中😀\\lo","inputFormat":"latex","cursor":10,"revision":"18446744073709551615",
      "catalog":{"commands":{"scope":{"userSha256":"a".repeat(64),"projectSha256":"b".repeat(64),"documentSha256":"c".repeat(64),"generation":"0"},"definitions":[]},
        "policy":{"retrievalEnabled":true,"sensitiveDocument":false,"allowInSensitiveDocument":false},
        "fragments":[{"id":"a","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localfirst","label":"PRIVATE_LABEL","source":"x"},
          {"id":"z","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localsecond","label":"PRIVATE_LABEL","source":"y^2"}]}})).unwrap()
}
fn new(backend: Backend) -> Session {
    Session::new(
        backend,
        "d".repeat(64),
        query().catalog.commands.scope,
        keys(),
    )
    .unwrap()
}
fn open(backend: Backend, permission: FormulaPreferenceStoragePermission) -> Result<Session, Tx> {
    Session::open(
        backend,
        "d".repeat(64),
        FormulaPreferenceStorageIdentity::from_scope(&query().catalog.commands.scope),
        permission,
    )
}
fn error<T>(result: Result<T, Tx>, expected: Tx) {
    assert_eq!(result.err().unwrap(), expected);
}
fn record(
    session: &mut Session,
    query: &FormulaFragmentCompletionRequest,
    event: u64,
    candidate: &str,
) -> FormulaPreferenceReceipt {
    let core = session.preferences_mut();
    let apply = FormulaFragmentCompletionApplyRequest {
        source: query.source.clone(),
        cursor: query.cursor,
        revision: query.revision,
        input_format: query.input_format,
        catalog: query.catalog.clone(),
        snapshot: query.execute().unwrap().snapshot,
        candidate_id: candidate.into(),
    };
    core.record_acceptance_at(&core.capture_context(), &apply, event)
        .unwrap()
        .unwrap()
}
fn initialize(backend: &Backend) -> Session {
    let mut session = new(backend.clone());
    let mut stage = session.prepare(permission()).unwrap();
    session.commit(&mut stage, permission()).unwrap();
    session
}

#[test]
fn publication_is_explicit_exact_and_does_not_restore_consent_or_old_tokens() {
    let backend = Backend::new();
    let mut session = new(backend.clone());
    assert_eq!(backend.counts(), (0, 0));
    error(
        session.prepare(off()),
        Tx::Storage(FormulaPreferenceStorageError::Disabled),
    );
    session
        .preferences_mut()
        .set_policy(enabled(false))
        .unwrap();
    let receipt = record(&mut session, &query(), 1, "fragment:z");
    let capture = session.preferences().capture_context();
    let mut stage = session.prepare(permission()).unwrap();
    assert_eq!(
        (stage.checkpoint(), stage.revision(), stage.event_count()),
        (1, 1, 1)
    );
    assert_eq!(backend.counts(), (0, 0));
    error(
        session.commit(&mut stage, off()),
        Tx::Storage(FormulaPreferenceStorageError::Disabled),
    );
    session.commit(&mut stage, permission()).unwrap();
    error(
        session.commit(&mut stage, permission()),
        Tx::StaleTransaction,
    );
    let mut restored = open(backend.clone(), permission()).unwrap();
    assert_eq!(
        restored.preferences().policy(),
        FormulaPreferencePolicy::default()
    );
    restored
        .preferences_mut()
        .set_policy(enabled(false))
        .unwrap();
    let ranked = restored.preferences().rank_fragments(&query()).unwrap();
    let index = ranked
        .completion
        .candidates
        .iter()
        .position(|item| item.edit.id == "fragment:z")
        .unwrap();
    assert_eq!(ranked.order.first(), Some(&index));
    assert!(restored.preferences_mut().record_undo(&receipt).is_err());
    let mut unrelated = new(backend.clone());
    error(
        unrelated.commit(&mut session.prepare(permission()).unwrap(), permission()),
        Tx::StaleTransaction,
    );
    let mut initial = unrelated.prepare(permission()).unwrap();
    error(unrelated.commit(&mut initial, permission()), Tx::Conflict);
    assert!(unrelated.recovery_required());
    drop(capture);
}

#[test]
fn stage_keeps_snapshot_bytes_without_overwriting_later_local_feedback() {
    let backend = Backend::new();
    let mut session = initialize(&backend);
    session
        .preferences_mut()
        .set_policy(enabled(false))
        .unwrap();
    record(&mut session, &query(), 1, "fragment:z");
    let mut stage = session.prepare(permission()).unwrap();
    record(&mut session, &query(), 2, "fragment:z");
    session.commit(&mut stage, permission()).unwrap();
    assert_eq!(session.preferences().event_count(), 2);
    assert_eq!(
        open(backend.clone(), permission())
            .unwrap()
            .preferences()
            .event_count(),
        1
    );
    let mut later = session.prepare(permission()).unwrap();
    assert_eq!(later.checkpoint(), 3);
    session.commit(&mut later, permission()).unwrap();
    assert_eq!(
        open(backend, permission())
            .unwrap()
            .preferences()
            .event_count(),
        2
    );
}

#[test]
fn ambiguous_or_panicking_publication_is_fenced_until_exact_readback_without_replay() {
    for mode in [Mode::UnknownBefore, Mode::UnknownAfter, Mode::PanicAfter] {
        let backend = Backend::new();
        let mut session = initialize(&backend);
        session
            .preferences_mut()
            .set_policy(enabled(false))
            .unwrap();
        record(&mut session, &query(), 1, "fragment:z");
        let mut stage = session.prepare(permission()).unwrap();
        backend.mode(mode);
        if mode == Mode::PanicAfter {
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || session.commit(&mut stage, permission())
            ))
            .is_err());
        } else {
            error(session.commit(&mut stage, permission()), Tx::Indeterminate);
        }
        assert!(session.recovery_required());
        error(session.prepare(permission()), Tx::RecoveryRequired);
        let writes = backend.counts().1;
        backend.0.lock().unwrap().unavailable = true;
        error(
            session.reconcile(&mut stage, permission()),
            Tx::ProviderUnavailable,
        );
        assert!(session.recovery_required());
        backend.0.lock().unwrap().unavailable = false;
        backend.mode(Mode::Healthy);
        let result = session.reconcile(&mut stage, permission()).unwrap();
        assert_eq!(backend.counts().1, writes);
        if mode == Mode::UnknownBefore {
            assert_eq!(result, PreferenceReconciliation::Unchanged);
            session.commit(&mut stage, permission()).unwrap();
        } else {
            assert_eq!(result, PreferenceReconciliation::Applied);
            error(
                session.commit(&mut stage, permission()),
                Tx::StaleTransaction,
            );
        }
        assert!(!session.recovery_required());
        assert_eq!(session.preferences().event_count(), 1);
        assert_eq!(
            open(backend, permission())
                .unwrap()
                .preferences()
                .event_count(),
            1
        );
    }
}

#[test]
fn proven_noncommit_retries_the_same_stage_and_storage_permissions_are_rechecked() {
    let backend = Backend::new();
    let mut session = initialize(&backend);
    session.preferences_mut().set_policy(enabled(true)).unwrap();
    let mut private = query();
    private.catalog.policy.sensitive_document = true;
    private.catalog.policy.allow_in_sensitive_document = true;
    record(&mut session, &private, 1, "fragment:z");
    let allowed = FormulaPreferenceStoragePermission {
        storage_enabled: true,
        allow_sensitive_storage: true,
    };
    error(
        session.prepare(permission()),
        Tx::Storage(FormulaPreferenceStorageError::SensitiveStorageDenied),
    );
    let mut stage = session.prepare(allowed).unwrap();
    error(
        session.commit(&mut stage, permission()),
        Tx::Storage(FormulaPreferenceStorageError::SensitiveStorageDenied),
    );
    assert_eq!(backend.counts().1, 1);
    backend.mode(Mode::Refused);
    error(session.commit(&mut stage, allowed), Tx::NotCommitted);
    assert!(!session.recovery_required());
    assert_eq!(stage.checkpoint(), 2);
    backend.mode(Mode::Healthy);
    session.commit(&mut stage, allowed).unwrap();
    error(
        open(backend.clone(), permission()),
        Tx::Storage(FormulaPreferenceStorageError::SensitiveStorageDenied),
    );
    assert_eq!(
        open(backend, allowed).unwrap().preferences().policy(),
        FormulaPreferencePolicy::default()
    );
}

#[test]
fn concurrent_cas_winner_is_never_overwritten_and_loser_keeps_local_feedback() {
    let backend = Backend::new();
    initialize(&backend);
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = ["fragment:a", "fragment:z"]
        .into_iter()
        .map(|id| {
            let backend = backend.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut session = open(backend, permission()).unwrap();
                session
                    .preferences_mut()
                    .set_policy(enabled(false))
                    .unwrap();
                record(&mut session, &query(), 1, id);
                let mut stage = session.prepare(permission()).unwrap();
                barrier.wait();
                let result = session.commit(&mut stage, permission());
                (session, stage, result)
            })
        })
        .collect();
    let mut successes = 0;
    let mut conflicts = 0;
    for handle in handles {
        let (mut session, mut stage, result) = handle.join().unwrap();
        assert_eq!(session.preferences().event_count(), 1);
        match result {
            Ok(()) => successes += 1,
            Err(failure) => {
                assert_eq!(failure, Tx::Conflict);
                conflicts += 1;
                error(session.reconcile(&mut stage, permission()), Tx::Conflict);
                error(session.prepare(permission()), Tx::RecoveryRequired);
            }
        }
    }
    assert_eq!((successes, conflicts), (1, 1));
    assert_eq!(
        open(backend, permission())
            .unwrap()
            .preferences()
            .event_count(),
        1
    );
}

#[test]
fn explicit_clear_rotates_both_keys_keeps_a_tombstone_and_invalidates_old_writers() {
    let backend = Backend::new();
    let mut session = initialize(&backend);
    session
        .preferences_mut()
        .set_policy(enabled(false))
        .unwrap();
    let receipt = record(&mut session, &query(), 1, "fragment:z");
    let capture = session.preferences().capture_context();
    let mut old = session.prepare(permission()).unwrap();
    session.commit(&mut old, permission()).unwrap();
    let mut loser = open(backend.clone(), permission()).unwrap();
    let mut stale = loser.prepare(permission()).unwrap();
    let mut reset = session.prepare_clear(1, fresh_keys()).unwrap();
    assert_eq!((reset.checkpoint(), reset.event_count()), (1, 0));
    session.commit(&mut reset, off()).unwrap();
    assert_eq!(session.preferences().scope().generation, 1);
    assert_eq!(session.preferences().policy(), enabled(false));
    assert!(session.preferences_mut().record_undo(&receipt).is_err());
    let apply = FormulaFragmentCompletionApplyRequest {
        source: query().source,
        cursor: query().cursor,
        revision: query().revision,
        input_format: query().input_format,
        catalog: query().catalog,
        snapshot: query().execute().unwrap().snapshot,
        candidate_id: "fragment:z".into(),
    };
    assert!(session
        .preferences_mut()
        .record_acceptance_at(&capture, &apply, 2)
        .is_err());
    error(loser.commit(&mut stale, permission()), Tx::Conflict);
    error(loser.reconcile(&mut stale, permission()), Tx::Conflict);
    let reopened = open(backend, permission()).unwrap();
    assert_eq!(reopened.preferences().scope().generation, 1);
    assert_eq!(reopened.preferences().event_count(), 0);
    assert_eq!(
        reopened.preferences().policy(),
        FormulaPreferencePolicy::default()
    );
}

#[test]
fn clear_after_lost_write_does_not_require_reenabling_storage_or_sensitive_history() {
    let backend = Backend::new();
    let mut session = initialize(&backend);
    session.preferences_mut().set_policy(enabled(true)).unwrap();
    let mut private = query();
    private.catalog.policy.sensitive_document = true;
    private.catalog.policy.allow_in_sensitive_document = true;
    record(&mut session, &private, 1, "fragment:z");
    let allowed = FormulaPreferenceStoragePermission {
        storage_enabled: true,
        allow_sensitive_storage: true,
    };
    let mut pending = session.prepare(allowed).unwrap();
    backend.mode(Mode::UnknownAfter);
    error(session.commit(&mut pending, allowed), Tx::Indeterminate);
    error(
        session.reconcile(&mut pending, off()),
        Tx::Storage(FormulaPreferenceStorageError::Disabled),
    );
    let mut reset = session.prepare_clear(1, fresh_keys()).unwrap();
    backend.mode(Mode::Healthy);
    session.commit(&mut reset, off()).unwrap();
    error(
        session.reconcile(&mut pending, allowed),
        Tx::StaleTransaction,
    );
    assert_eq!(
        open(backend, permission())
            .unwrap()
            .preferences()
            .event_count(),
        0
    );
}

#[test]
fn missing_provider_data_and_wrong_owner_never_create_or_merge_state() {
    let backend = Backend::new();
    error(
        open(backend.clone(), off()),
        Tx::Storage(FormulaPreferenceStorageError::Disabled),
    );
    assert_eq!(backend.counts(), (0, 0));
    error(open(backend.clone(), permission()), Tx::NotFound);
    let mut session = initialize(&backend);
    let identity = FormulaPreferenceStorageIdentity::from_scope(&query().catalog.commands.scope);
    error(
        Session::open(backend.clone(), "e".repeat(64), identity, permission()),
        Tx::IdentityMismatch,
    );
    let mut changed = FormulaPreferenceStorageIdentity::from_scope(&query().catalog.commands.scope);
    changed.project_sha256 = None;
    error(
        Session::open(backend.clone(), "d".repeat(64), changed, permission()),
        Tx::IdentityMismatch,
    );
    backend.0.lock().unwrap().bytes = None;
    error(session.prepare_clear(1, fresh_keys()), Tx::NotFound);
    assert!(session.recovery_required());
    assert_eq!(backend.counts().1, 1);
}

#[test]
fn protected_encoding_is_closed_bounded_precision_preserving_and_explicitly_sensitive() {
    let backend = Backend::new();
    initialize(&backend);
    let bytes = backend.bytes();
    let record = ProtectedPreferenceRecord::decode_from_protected_storage(&bytes).unwrap();
    assert_eq!(
        format!("{record:?}"),
        "ProtectedPreferenceRecord([REDACTED])"
    );
    assert_eq!(
        format!("{:?}", record.witness().unwrap()),
        "PreferenceStorageWitness([REDACTED])"
    );
    let base: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(base["revision"], "1");
    assert_eq!(base["checkpoint"], "1");
    // Only a protected host provider may receive this plaintext representation.
    assert!(base.get("feedbackKey").is_some() && base.get("storageKey").is_some());
    assert!(base.get("policy").is_none());
    for (field, value) in [
        ("PRIVATE_SECRET", json!(true)),
        ("revision", json!(1)),
        ("revision", json!("01")),
        ("checkpoint", json!(0)),
        ("schemaVersion", json!(2)),
        ("snapshotJson", json!(" ".repeat(256 * 1024 + 1))),
    ] {
        let mut invalid = base.clone();
        invalid[field] = value;
        error(
            ProtectedPreferenceRecord::decode_from_protected_storage(
                &serde_json::to_vec(&invalid).unwrap(),
            ),
            Tx::InvalidRecord,
        );
    }
    let duplicate = String::from_utf8(bytes.clone()).unwrap().replacen(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"schemaVersion\":1",
        1,
    );
    error(
        ProtectedPreferenceRecord::decode_from_protected_storage(duplicate.as_bytes()),
        Tx::InvalidRecord,
    );
    error(
        ProtectedPreferenceRecord::decode_from_protected_storage(&vec![
            b' ';
            FORMULA_PREFERENCE_MAX_PROTECTED_RECORD_BYTES
                + 1
        ]),
        Tx::InvalidRecord,
    );
    let mut invalid = base;
    let mut snapshot: Value =
        serde_json::from_str(invalid["snapshotJson"].as_str().unwrap()).unwrap();
    snapshot["body"]["checkpoint"] = json!("2");
    invalid["snapshotJson"] = json!(snapshot.to_string());
    backend.replace_fixture(serde_json::to_vec(&invalid).unwrap());
    error(
        open(backend, permission()),
        Tx::Storage(FormulaPreferenceStorageError::AuthenticationFailed),
    );
}

#[test]
fn exhausted_sequences_refuse_without_wrapping_and_raw_scope_mutation_cannot_be_persisted() {
    let backend = Backend::new();
    initialize(&backend);
    let mut root: Value = serde_json::from_slice(&backend.bytes()).unwrap();
    root["revision"] = json!(u64::MAX.to_string());
    backend.replace_fixture(serde_json::to_vec(&root).unwrap());
    let mut session = open(backend.clone(), permission()).unwrap();
    error(session.prepare(permission()), Tx::Exhausted);
    assert!(session
        .preferences_mut()
        .export_checkpoint(&[27; 32], 2, permission())
        .is_ok());
    error(session.prepare_clear(1, fresh_keys()), Tx::Exhausted);
    assert_eq!(backend.counts().1, 1);
    let backend = Backend::new();
    let mut session = initialize(&backend);
    session.preferences_mut().clear(1, [41; 32]).unwrap();
    error(session.prepare(permission()), Tx::IdentityMismatch);
    assert_eq!(backend.counts().1, 1);
    let backend = Backend::new();
    let mut session = initialize(&backend);
    *session.preferences_mut() =
        FormulaPreferenceStore::new(query().catalog.commands.scope, [19; 32]).unwrap();
    error(session.prepare(permission()), Tx::IdentityMismatch);
    assert_eq!(backend.counts().1, 1);
}

#[test]
fn clear_reconciliation_stays_available_with_storage_off_and_a_newer_protected_base() {
    for mode in [Mode::UnknownBefore, Mode::UnknownAfter, Mode::PanicAfter] {
        let backend = Backend::new();
        let mut stale = initialize(&backend);
        let mut writer = open(backend.clone(), permission()).unwrap();
        writer.preferences_mut().set_policy(enabled(false)).unwrap();
        record(&mut writer, &query(), 1, "fragment:z");
        let mut latest = writer.prepare(permission()).unwrap();
        writer.commit(&mut latest, permission()).unwrap();
        let mut reset = stale.prepare_clear(1, fresh_keys()).unwrap();
        assert!(stale.recovery_required());
        backend.mode(mode);
        if mode == Mode::PanicAfter {
            assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || stale.commit(&mut reset, off())
            ))
            .is_err());
        } else {
            error(stale.commit(&mut reset, off()), Tx::Indeterminate);
        }
        let writes = backend.counts().1;
        let outcome = stale.reconcile(&mut reset, off()).unwrap();
        assert_eq!(backend.counts().1, writes);
        if mode == Mode::UnknownBefore {
            assert_eq!(outcome, PreferenceReconciliation::Unchanged);
            assert!(stale.recovery_required());
            error(stale.prepare(permission()), Tx::RecoveryRequired);
            backend.mode(Mode::Healthy);
            stale.commit(&mut reset, off()).unwrap();
        } else {
            assert_eq!(outcome, PreferenceReconciliation::Applied);
        }
        assert!(!stale.recovery_required());
        let opened = open(backend, permission()).unwrap();
        assert_eq!(opened.preferences().scope().generation, 1);
        assert_eq!(opened.preferences().event_count(), 0);
    }
}

#[test]
fn checkpoint_exhaustion_can_only_restart_after_an_explicit_rotated_empty_epoch() {
    let backend = Backend::new();
    initialize(&backend);
    let mut root: Value = serde_json::from_slice(&backend.bytes()).unwrap();
    let scope = query().catalog.commands.scope;
    let mut core = FormulaPreferenceStore::restore_checkpoint(
        scope,
        [19; 32],
        &[27; 32],
        1,
        root["snapshotJson"].as_str().unwrap(),
        permission(),
    )
    .unwrap();
    root["snapshotJson"] = json!(core
        .export_checkpoint(&[27; 32], u64::MAX, permission())
        .unwrap());
    root["checkpoint"] = json!(u64::MAX.to_string());
    backend.replace_fixture(serde_json::to_vec(&root).unwrap());
    let mut session = open(backend.clone(), permission()).unwrap();
    error(session.prepare(permission()), Tx::Exhausted);
    let mut reset = session.prepare_clear(1, fresh_keys()).unwrap();
    assert_eq!(reset.checkpoint(), 1);
    session.commit(&mut reset, off()).unwrap();
    let mut next = session.prepare(permission()).unwrap();
    assert_eq!(next.checkpoint(), 2);
    session.commit(&mut next, permission()).unwrap();
    assert_eq!(
        open(backend, permission())
            .unwrap()
            .preferences()
            .scope()
            .generation,
        1
    );
}

#[test]
fn abandoned_stages_skip_checkpoints_and_only_one_same_base_stage_can_be_adopted() {
    let backend = Backend::new();
    let mut session = initialize(&backend);
    let mut older = session.prepare(permission()).unwrap();
    let mut newer = session.prepare(permission()).unwrap();
    assert_eq!((older.revision(), newer.revision()), (2, 2));
    assert_eq!((older.checkpoint(), newer.checkpoint()), (2, 3));
    session.commit(&mut newer, permission()).unwrap();
    error(
        session.commit(&mut older, permission()),
        Tx::StaleTransaction,
    );
    error(
        session.reconcile(&mut older, permission()),
        Tx::StaleTransaction,
    );
    assert!(!session.recovery_required());
    let next = session.prepare(permission()).unwrap();
    assert_eq!((next.revision(), next.checkpoint()), (3, 4));
    assert_eq!(backend.counts().1, 2);
    assert_eq!(
        ProtectedPreferenceRecord::decode_from_protected_storage(&backend.bytes())
            .unwrap()
            .checkpoint(),
        3
    );
}

#[test]
fn reset_requires_a_new_generation_and_independent_keys_from_both_current_epochs() {
    for (feedback, storage) in [
        ([0; 32], [27; 32]),
        ([19; 32], [0; 32]),
        ([19; 32], [19; 32]),
    ] {
        error(
            FormulaPreferenceHostKeys::new(feedback, storage),
            Tx::Storage(FormulaPreferenceStorageError::InvalidKey),
        );
    }
    let backend = Backend::new();
    let mut stale = initialize(&backend);
    error(stale.prepare_clear(0, fresh_keys()), Tx::IdentityMismatch);
    for (feedback, storage) in [
        ([19; 32], [37; 32]),
        ([31; 32], [27; 32]),
        ([27; 32], [19; 32]),
    ] {
        error(
            stale.prepare_clear(
                1,
                FormulaPreferenceHostKeys::new(feedback, storage).unwrap(),
            ),
            Tx::IdentityMismatch,
        );
    }
    let mut writer = open(backend.clone(), permission()).unwrap();
    let mut reset = writer.prepare_clear(1, fresh_keys()).unwrap();
    writer.commit(&mut reset, off()).unwrap();
    // A stale owner must not reuse either newly protected key in either role.
    for (feedback, storage) in [
        ([31; 32], [43; 32]),
        ([43; 32], [37; 32]),
        ([37; 32], [31; 32]),
    ] {
        error(
            stale.prepare_clear(
                2,
                FormulaPreferenceHostKeys::new(feedback, storage).unwrap(),
            ),
            Tx::IdentityMismatch,
        );
    }
    error(stale.prepare_clear(1, keys()), Tx::IdentityMismatch);
    assert_eq!(backend.counts().1, 2);
    let mut root: Value = serde_json::from_slice(&backend.bytes()).unwrap();
    root["scope"]["generation"] = json!(u64::MAX.to_string());
    backend.replace_fixture(serde_json::to_vec(&root).unwrap());
    error(stale.prepare_clear(u64::MAX, keys()), Tx::Exhausted);
    assert_eq!(backend.counts().1, 2);
}

#[test]
fn in_flight_native_writes_cannot_be_reconciled_as_an_unchanged_base() {
    for applied in [false, true] {
        let backend = Backend::new();
        let mut session = initialize(&backend);
        session
            .preferences_mut()
            .set_policy(enabled(false))
            .unwrap();
        record(&mut session, &query(), 1, "fragment:z");
        let mut stage = session.prepare(permission()).unwrap();
        backend.mode(Mode::Pending);
        error(session.commit(&mut stage, permission()), Tx::Indeterminate);
        // The old root still exists, but reading it would not establish a safe
        // retry boundary until the pending provider operation has settled.
        assert_eq!(
            ProtectedPreferenceRecord::decode_from_protected_storage(&backend.bytes())
                .unwrap()
                .checkpoint(),
            1
        );
        let writes = backend.counts().1;
        error(
            session.reconcile(&mut stage, permission()),
            Tx::ProviderUnavailable,
        );
        error(
            session.prepare_clear(1, fresh_keys()),
            Tx::ProviderUnavailable,
        );
        error(
            session.commit(&mut stage, permission()),
            Tx::RecoveryRequired,
        );
        assert!(session.recovery_required());
        assert_eq!(backend.counts().1, writes);
        backend.settle(applied);
        assert_eq!(
            session.reconcile(&mut stage, permission()).unwrap(),
            if applied {
                PreferenceReconciliation::Applied
            } else {
                PreferenceReconciliation::Unchanged
            }
        );
        if !applied {
            session.commit(&mut stage, permission()).unwrap();
        }
        assert_eq!(
            open(backend, permission())
                .unwrap()
                .preferences()
                .event_count(),
            1
        );
    }
}
