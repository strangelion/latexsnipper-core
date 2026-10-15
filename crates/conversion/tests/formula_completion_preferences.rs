use latexsnipper_conversion::formula_completion::{fragments::*, preferences::*, scoped::*};
use serde_json::json;

// Convenience for synchronous cases. Boundary tests pass an explicitly captured
// context to the public API so they can simulate genuinely late feedback.
trait RecordAcceptance {
    fn record_acceptance(
        &mut self,
        request: &FormulaFragmentCompletionApplyRequest,
        event_id: u64,
    ) -> Result<Option<FormulaPreferenceReceipt>, FormulaPreferenceError>;
}
impl RecordAcceptance for FormulaPreferenceStore {
    fn record_acceptance(
        &mut self,
        request: &FormulaFragmentCompletionApplyRequest,
        event_id: u64,
    ) -> Result<Option<FormulaPreferenceReceipt>, FormulaPreferenceError> {
        self.record_acceptance_at(&self.capture_context(), request, event_id)
    }
}

fn catalog() -> FormulaFragmentCatalog {
    serde_json::from_value(json!({
        "commands": {"scope": {"userSha256": "a".repeat(64), "projectSha256": "b".repeat(64),
            "documentSha256": "c".repeat(64), "generation": "0"}, "definitions": []},
        "policy": {"retrievalEnabled": true, "sensitiveDocument": false, "allowInSensitiveDocument": false},
        "fragments": [
            {"id": "a-first", "revision": "0", "userSha256": "a".repeat(64), "projectSha256": null,
                "documentSha256": null, "trigger": "localfirst", "label": "PRIVATE_FIRST", "source": "x+1"},
            {"id": "b-second", "revision": "0", "userSha256": "a".repeat(64), "projectSha256": null,
                "documentSha256": null, "trigger": "localsecond", "label": "PRIVATE_SECOND", "source": r"\frac{}{}"}
        ]
    })).unwrap()
}
fn query() -> FormulaFragmentCompletionRequest {
    FormulaFragmentCompletionRequest {
        source: r"\local".into(),
        input_format: serde_json::from_value(json!("latex")).unwrap(),
        cursor: 6,
        revision: u64::MAX,
        catalog: catalog(),
    }
}
fn acceptance(
    request: &FormulaFragmentCompletionRequest,
    id: &str,
) -> FormulaFragmentCompletionApplyRequest {
    FormulaFragmentCompletionApplyRequest {
        source: request.source.clone(),
        input_format: request.input_format,
        cursor: request.cursor,
        revision: request.revision,
        catalog: request.catalog.clone(),
        snapshot: request.execute().unwrap().snapshot,
        candidate_id: id.into(),
    }
}
fn store(request: &FormulaFragmentCompletionRequest) -> FormulaPreferenceStore {
    FormulaPreferenceStore::new(request.catalog.commands.scope.clone(), [42; 32]).unwrap()
}
fn enabled() -> FormulaPreferencePolicy {
    FormulaPreferencePolicy {
        ranking_enabled: true,
        feedback_enabled: true,
        allow_sensitive_feedback: false,
    }
}
fn enabled_store(request: &FormulaFragmentCompletionRequest) -> FormulaPreferenceStore {
    let mut store = store(request);
    store.set_policy(enabled()).unwrap();
    store
}

#[test]
fn default_off_is_deterministic_and_captures_nothing() {
    let request = query();
    let mut store = store(&request);
    assert_eq!(store.policy(), FormulaPreferencePolicy::default());
    assert!(store
        .record_acceptance(&acceptance(&request, "fragment:b-second"), 1)
        .unwrap()
        .is_none());
    assert_eq!(store.event_count(), 0);
    let ranked = store.rank_fragments(&request).unwrap();
    assert_eq!(ranked.completion, request.execute().unwrap());
    assert_eq!(ranked.order, vec![0, 1]);
    assert_eq!(
        ranked.features,
        vec![FormulaPreferenceFeatures::default(); 2]
    );
}

#[test]
fn frequency_then_recency_only_permute_current_validated_candidates() {
    let request = query();
    let mut store = enabled_store(&request);
    let original = request.execute().unwrap();
    let second = acceptance(&request, "fragment:b-second");
    let receipt = store.record_acceptance(&second, 1).unwrap().unwrap();
    let ranked = store.rank_fragments(&request).unwrap();
    assert_eq!(ranked.order, vec![1, 0]);
    assert_eq!(ranked.completion, original);
    assert_eq!(ranked.features[1].accepted, 1);
    store
        .record_acceptance(&acceptance(&request, "fragment:a-first"), 2)
        .unwrap();
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![0, 1]);
    store.record_acceptance(&second, 3).unwrap();
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![1, 0]);
    store.record_undo(&receipt).unwrap();
    let ranked = store.rank_fragments(&request).unwrap();
    assert_eq!(ranked.features[1].net_acceptances(), 1);
    assert_eq!(ranked.features[1].undone, 1);
    assert_eq!(ranked.order, vec![1, 0]);
}

#[test]
fn scores_follow_candidates_across_editor_revisions_not_source_positions() {
    let request = query();
    let mut store = enabled_store(&request);
    store
        .record_acceptance(&acceptance(&request, "fragment:b-second"), 1)
        .unwrap();
    let mut edited = request;
    edited.source = "中\\loc".into();
    edited.cursor = edited.source.len();
    edited.revision = 1;
    let ranked = store.rank_fragments(&edited).unwrap();
    assert_eq!(ranked.order, vec![1, 0]);
    assert_eq!(ranked.features[1].accepted, 1);
    assert_eq!(ranked.completion, edited.execute().unwrap());
}

#[test]
fn out_of_order_undo_restores_surviving_recency_and_cannot_replay() {
    let request = query();
    let mut store = enabled_store(&request);
    let first = acceptance(&request, "fragment:a-first");
    let second = acceptance(&request, "fragment:b-second");
    let a1 = store.record_acceptance(&first, 1).unwrap().unwrap();
    store.record_acceptance(&second, 2).unwrap();
    let a3 = store.record_acceptance(&first, 3).unwrap().unwrap();
    store.record_undo(&a3).unwrap();
    let ranked = store.rank_fragments(&request).unwrap();
    assert_eq!(ranked.order, vec![1, 0]);
    assert_eq!(ranked.features[0].last_accepted_event, 1);
    store.record_undo(&a1).unwrap();
    assert_eq!(
        store.rank_fragments(&request).unwrap().features[0].last_accepted_event,
        0
    );
    assert_eq!(
        store.record_undo(&a1),
        Err(FormulaPreferenceError::StaleFeedback)
    );
    assert_eq!(
        store.rank_fragments(&request).unwrap().features[0].undone,
        2
    );
}

#[test]
fn scope_user_project_document_and_generation_are_exact() {
    let request = query();
    let mut store = enabled_store(&request);
    for field in [
        "userSha256",
        "projectSha256",
        "documentSha256",
        "generation",
    ] {
        let mut changed = serde_json::to_value(&request).unwrap();
        changed["catalog"]["commands"]["scope"][field] = if field == "generation" {
            json!("1")
        } else {
            json!("d".repeat(64))
        };
        let changed: FormulaFragmentCompletionRequest = serde_json::from_value(changed).unwrap();
        assert_eq!(
            store.rank_fragments(&changed).unwrap_err(),
            FormulaPreferenceError::ScopeMismatch
        );
        // Use the original snapshot: scope refusal must precede feedback changes.
        let mut accept = acceptance(&request, "fragment:a-first");
        accept.catalog = changed.catalog;
        assert_eq!(
            store.record_acceptance(&accept, 1).unwrap_err(),
            FormulaPreferenceError::ScopeMismatch
        );
    }
    assert_eq!(store.event_count(), 0);
}

#[test]
fn invalid_stale_and_unknown_acceptance_cannot_poison_feedback() {
    let request = query();
    let mut store = enabled_store(&request);
    let accept = acceptance(&request, "fragment:b-second");
    for field in ["revision", "body", "cursor", "snapshot", "candidate"] {
        let mut changed = accept.clone();
        match field {
            "revision" => changed.revision = 0,
            "body" => changed.catalog.fragments[1].source = "x".into(),
            "cursor" => changed.cursor = 1,
            "snapshot" => changed.snapshot.schema_version = 1,
            _ => changed.candidate_id = "untrusted".into(),
        }
        assert!(matches!(
            store.record_acceptance(&changed, 1),
            Err(FormulaPreferenceError::Completion(_))
        ));
        assert_eq!(store.event_count(), 0);
    }
    store.record_acceptance(&accept, 1).unwrap();
    for id in [0, 1] {
        assert_eq!(
            store.record_acceptance(&accept, id).unwrap_err(),
            FormulaPreferenceError::StaleFeedback
        );
    }
    assert_eq!(store.event_count(), 1);
}

#[test]
fn receipts_belong_to_exact_store_even_with_identical_scope_key_and_event_ids() {
    let request = query();
    let mut first = enabled_store(&request);
    let mut second = enabled_store(&request);
    let accept = acceptance(&request, "fragment:b-second");
    let ticket = first.record_acceptance(&accept, 1).unwrap().unwrap();
    second.record_acceptance(&accept, 1).unwrap();
    assert_eq!(
        second.record_undo(&ticket),
        Err(FormulaPreferenceError::StaleFeedback)
    );
    assert_eq!(
        second.rank_fragments(&request).unwrap().features[1].undone,
        0
    );
    first.record_undo(&ticket).unwrap();
}

#[test]
fn policy_revocation_and_aba_do_not_resurrect_outstanding_receipts() {
    let request = query();
    let mut store = enabled_store(&request);
    let ticket = store
        .record_acceptance(&acceptance(&request, "fragment:b-second"), 1)
        .unwrap()
        .unwrap();
    store.set_policy(enabled()).unwrap();
    // No-op update preserves the receipt; actual transitions do not.
    store.record_undo(&ticket).unwrap();
    let ticket = store
        .record_acceptance(&acceptance(&request, "fragment:b-second"), 2)
        .unwrap()
        .unwrap();
    store
        .set_policy(FormulaPreferencePolicy::default())
        .unwrap();
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![0, 1]);
    assert_eq!(
        store.rank_fragments(&request).unwrap().features[1].accepted,
        0
    );
    store.set_policy(enabled()).unwrap();
    assert_eq!(
        store.record_undo(&ticket),
        Err(FormulaPreferenceError::StaleFeedback)
    );
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![1, 0]);
}

#[test]
fn queued_acceptance_cannot_cross_policy_aba_clear_or_instance_boundaries() {
    let request = query();
    let mut store = enabled_store(&request);
    let accept = acceptance(&request, "fragment:b-second");
    let capture = store.capture_context();
    store
        .set_policy(FormulaPreferencePolicy::default())
        .unwrap();
    store.set_policy(enabled()).unwrap();
    assert_eq!(
        store
            .record_acceptance_at(&capture, &accept, 1)
            .unwrap_err(),
        FormulaPreferenceError::StaleFeedback
    );
    let other = enabled_store(&request);
    assert_eq!(
        store
            .record_acceptance_at(&other.capture_context(), &accept, 1)
            .unwrap_err(),
        FormulaPreferenceError::StaleFeedback
    );
    let capture = store.capture_context();
    store.clear(1, [43; 32]).unwrap();
    assert_eq!(
        store
            .record_acceptance_at(&capture, &accept, 1)
            .unwrap_err(),
        FormulaPreferenceError::StaleFeedback
    );
    assert_eq!(store.event_count(), 0);
}

#[test]
fn exhausted_scope_generation_cannot_wrap_or_clear_into_an_old_epoch() {
    let mut request = query();
    request.catalog.commands.scope.generation = u64::MAX;
    let mut store = enabled_store(&request);
    store
        .record_acceptance(&acceptance(&request, "fragment:b-second"), 1)
        .unwrap();
    for generation in [0, 1, u64::MAX] {
        assert_eq!(
            store.clear(generation, [43; 32]),
            Err(FormulaPreferenceError::InvalidIdentity)
        );
        assert_eq!(store.event_count(), 1);
    }
    assert_eq!(
        store.rank_fragments(&request).unwrap().features[1].accepted,
        1
    );
}

#[test]
fn ranking_and_capture_switches_are_independent() {
    let request = query();
    let mut store = enabled_store(&request);
    store
        .set_policy(FormulaPreferencePolicy {
            ranking_enabled: false,
            ..enabled()
        })
        .unwrap();
    store
        .record_acceptance(&acceptance(&request, "fragment:b-second"), 1)
        .unwrap();
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![0, 1]);
    store
        .set_policy(FormulaPreferencePolicy {
            feedback_enabled: false,
            ..enabled()
        })
        .unwrap();
    assert!(store
        .record_acceptance(&acceptance(&request, "fragment:b-second"), 2)
        .unwrap()
        .is_none());
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![1, 0]);
    assert_eq!(store.event_count(), 1);
}

#[test]
fn sensitive_retrieval_allow_is_not_learning_consent() {
    let mut request = query();
    request.catalog.policy.sensitive_document = true;
    request.catalog.policy.allow_in_sensitive_document = true;
    let mut store = enabled_store(&request);
    let accept = acceptance(&request, "fragment:b-second");
    assert!(store.record_acceptance(&accept, 1).unwrap().is_none());
    assert_eq!(store.event_count(), 0);
    store
        .set_policy(FormulaPreferencePolicy {
            allow_sensitive_feedback: true,
            ..enabled()
        })
        .unwrap();
    store.record_acceptance(&accept, 1).unwrap();
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![1, 0]);
    store.set_policy(enabled()).unwrap();
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![0, 1]);
    assert_eq!(
        store.rank_fragments(&request).unwrap().features[1].accepted,
        0
    );
}

#[test]
fn clearing_requires_fresh_generation_and_key_and_prevents_old_feedback_returning() {
    let request = query();
    let mut store = enabled_store(&request);
    let accept = acceptance(&request, "fragment:b-second");
    let ticket = store.record_acceptance(&accept, 1).unwrap().unwrap();
    for (generation, key) in [(0, [43; 32]), (1, [42; 32]), (1, [0; 32])] {
        assert_eq!(
            store.clear(generation, key),
            Err(FormulaPreferenceError::InvalidIdentity)
        );
        assert_eq!(store.event_count(), 1);
    }
    store
        .set_policy(FormulaPreferencePolicy::default())
        .unwrap();
    store.clear(1, [43; 32]).unwrap();
    assert_eq!(store.event_count(), 0);
    assert_eq!(
        store.record_undo(&ticket),
        Err(FormulaPreferenceError::StaleFeedback)
    );
    assert_eq!(
        store.record_acceptance(&accept, 2).unwrap_err(),
        FormulaPreferenceError::ScopeMismatch
    );
    let mut current = request;
    current.catalog.commands.scope.generation = 1;
    store.set_policy(enabled()).unwrap();
    assert_eq!(store.rank_fragments(&current).unwrap().order, vec![0, 1]);
    let mut stale = accept;
    stale.catalog = current.catalog.clone();
    assert_eq!(
        store.record_acceptance(&stale, 1).unwrap_err(),
        FormulaPreferenceError::Completion(ScopedFormulaCompletionError::StaleContext)
    );
    store
        .record_acceptance(&acceptance(&current, "fragment:b-second"), 1)
        .unwrap();
    assert_eq!(store.rank_fragments(&current).unwrap().order, vec![1, 0]);
}

#[test]
fn catalog_and_candidate_changes_never_inherit_old_scores_under_reused_ids() {
    let request = query();
    let mut store = enabled_store(&request);
    store
        .record_acceptance(&acceptance(&request, "fragment:b-second"), 1)
        .unwrap();
    for field in ["source", "revision", "label", "policy"] {
        let mut changed = request.clone();
        match field {
            "source" => changed.catalog.fragments[1].source = "y".into(),
            "revision" => changed.catalog.fragments[1].revision = 1,
            "label" => changed.catalog.fragments[1].label = "Changed".into(),
            _ => changed.catalog.policy.allow_in_sensitive_document = true,
        }
        assert_eq!(
            store.rank_fragments(&changed).unwrap().order,
            vec![0, 1],
            "{field}"
        );
        assert_eq!(
            store.rank_fragments(&changed).unwrap().features[1].accepted,
            0
        );
    }
}

#[test]
fn positive_u64_event_ids_remain_exact_and_features_do_not_include_private_data() {
    let request = query();
    let mut store = enabled_store(&request);
    let accept = acceptance(&request, "fragment:b-second");
    assert_eq!(
        store.record_acceptance(&accept, 0).unwrap_err(),
        FormulaPreferenceError::StaleFeedback
    );
    let receipt = store.record_acceptance(&accept, u64::MAX).unwrap().unwrap();
    let features =
        serde_json::to_string(&store.rank_fragments(&request).unwrap().features).unwrap();
    assert!(features.contains("18446744073709551615"));
    for private in ["PRIVATE_SECOND", "b-second", r"\frac", &"a".repeat(64)] {
        assert!(!features.contains(private));
    }
    assert!(!format!("{receipt:?}").contains("PRIVATE"));
    assert_eq!(
        store.record_acceptance(&accept, u64::MAX).unwrap_err(),
        FormulaPreferenceError::StaleFeedback
    );
    store.record_undo(&receipt).unwrap();
}

#[test]
fn bounded_ledger_refuses_full_without_evicting_receipts_or_corrupting_completion() {
    let request = query();
    let mut store = enabled_store(&request);
    let accept = acceptance(&request, "fragment:b-second");
    let receipt = store.record_acceptance(&accept, 1).unwrap().unwrap();
    for id in 2..=FORMULA_PREFERENCE_MAX_EVENTS as u64 {
        store.record_acceptance(&accept, id).unwrap();
    }
    assert_eq!(
        store.record_acceptance(&accept, 1025).unwrap_err(),
        FormulaPreferenceError::LimitExceeded
    );
    assert_eq!(store.event_count(), 1024);
    store.record_undo(&receipt).unwrap();
    assert_eq!(
        store.rank_fragments(&request).unwrap().features[1].net_acceptances(),
        1023
    );
    assert_eq!(request.execute().unwrap().candidates.len(), 2);
}

#[test]
fn invalid_identity_and_policy_wire_shape_refuse_without_private_error_details() {
    let scope = query().catalog.commands.scope;
    assert!(matches!(
        FormulaPreferenceStore::new(scope.clone(), [0; 32]),
        Err(FormulaPreferenceError::InvalidIdentity)
    ));
    let mut bad = scope;
    bad.user_sha256 = "PRIVATE_USER".into();
    assert!(matches!(
        FormulaPreferenceStore::new(bad, [1; 32]),
        Err(FormulaPreferenceError::InvalidIdentity)
    ));
    for value in [
        json!({}),
        json!({"rankingEnabled": true, "feedbackEnabled": true, "allowSensitiveFeedback": 1}),
        json!({"rankingEnabled": true, "feedbackEnabled": true, "allowSensitiveFeedback": false, "upload": true}),
    ] {
        assert!(serde_json::from_value::<FormulaPreferencePolicy>(value).is_err());
    }
    assert_eq!(
        FormulaPreferenceFeatures {
            accepted: 0,
            undone: 1,
            last_accepted_event: 0
        }
        .net_acceptances(),
        0
    );
    fn send_sync<T: Send + Sync>() {}
    send_sync::<FormulaPreferenceStore>();
    send_sync::<FormulaPreferenceReceipt>();
}
