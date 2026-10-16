use latexsnipper_conversion::formula_completion::{fragments::*, preferences::*};
use serde_json::json;

// Deterministic public test keys, not production secrets.
const FEEDBACK_KEY: [u8; 32] = [19; 32];
fn policy() -> FormulaPreferencePolicy {
    FormulaPreferencePolicy {
        ranking_enabled: true,
        feedback_enabled: true,
        allow_sensitive_feedback: false,
    }
}
fn query() -> FormulaFragmentCompletionRequest {
    serde_json::from_value(json!({"source":"\\local","inputFormat":"latex","cursor":6,"revision":"18446744073709551615",
        "catalog":{"commands":{"scope":{"userSha256":"a".repeat(64),"projectSha256":"b".repeat(64),"documentSha256":"c".repeat(64),"generation":"0"},"definitions":[]},
        "policy":{"retrievalEnabled":true,"sensitiveDocument":false,"allowInSensitiveDocument":false},
        "fragments":[{"id":"a","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localfirst","label":"PRIVATE_LABEL_A","source":"x+1"},
        {"id":"z","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localsecond","label":"PRIVATE_LABEL_Z","source":"\\frac{PRIVATE}{BODY}"}]}})).unwrap()
}
fn owner() -> FormulaPreferenceStore {
    let mut store =
        FormulaPreferenceStore::new(query().catalog.commands.scope, FEEDBACK_KEY).unwrap();
    store.set_policy(policy()).unwrap();
    store
}
fn record(
    store: &mut FormulaPreferenceStore,
    query: &FormulaFragmentCompletionRequest,
    candidate_id: &str,
    event: u64,
) -> FormulaPreferenceReceipt {
    let apply = FormulaFragmentCompletionApplyRequest {
        source: query.source.clone(),
        input_format: query.input_format,
        cursor: query.cursor,
        revision: query.revision,
        catalog: query.catalog.clone(),
        snapshot: query.execute().unwrap().snapshot,
        candidate_id: candidate_id.into(),
    };
    store
        .record_acceptance_at(&store.capture_context(), &apply, event)
        .unwrap()
        .unwrap()
}

#[test]
fn strategies_only_permute_never_replace_candidates() {
    let mut store = owner();
    record(&mut store, &query(), "fragment:z", 1);
    record(&mut store, &query(), "fragment:z", 2);
    let request = query();

    let baseline = store.rank_fragments(&request).unwrap();
    for strategy in [
        FormulaRankingStrategy::Fixed,
        FormulaRankingStrategy::Frequency,
        FormulaRankingStrategy::Recency,
    ] {
        let ranked = store.rank_fragments_with(&request, strategy).unwrap();
        assert_eq!(ranked.completion, baseline.completion);
        assert_eq!(ranked.features, baseline.features);
        let mut sorted = ranked.order.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            vec![0, 1],
            "strategy {strategy:?} must keep both candidates"
        );
    }
}

#[test]
fn accepted_candidate_surfaces_first_under_feedback_strategies() {
    let mut store = owner();
    let request = query();
    record(&mut store, &request, "fragment:z", 1);
    record(&mut store, &request, "fragment:z", 2);
    record(&mut store, &request, "fragment:z", 3);

    assert_eq!(
        store
            .rank_fragments_with(&request, FormulaRankingStrategy::Fixed)
            .unwrap()
            .order,
        vec![0, 1],
        "fixed rule preserves original order"
    );
    assert_eq!(
        store
            .rank_fragments_with(&request, FormulaRankingStrategy::Frequency)
            .unwrap()
            .order,
        vec![1, 0]
    );
    assert_eq!(
        store
            .rank_fragments_with(&request, FormulaRankingStrategy::Recency)
            .unwrap()
            .order,
        vec![1, 0]
    );
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![1, 0]);
}

#[test]
fn frequency_and_recency_diverge_when_history_disagrees() {
    let mut store = owner();
    let request = query();
    // z is accepted three times, then a is accepted once most recently:
    // frequency favors z (3 > 1), recency favors a (event 4 > 3).
    record(&mut store, &request, "fragment:z", 1);
    record(&mut store, &request, "fragment:z", 2);
    record(&mut store, &request, "fragment:z", 3);
    record(&mut store, &request, "fragment:a", 4);

    assert_eq!(
        store
            .rank_fragments_with(&request, FormulaRankingStrategy::Frequency)
            .unwrap()
            .order,
        vec![1, 0]
    );
    assert_eq!(
        store
            .rank_fragments_with(&request, FormulaRankingStrategy::Recency)
            .unwrap()
            .order,
        vec![0, 1]
    );
    // Baseline lexicographically prefers frequency first.
    assert_eq!(store.rank_fragments(&request).unwrap().order, vec![1, 0]);
}

#[test]
fn undo_reduces_but_keeps_frequency_signal() {
    let mut store = owner();
    let request = query();
    let r1 = record(&mut store, &request, "fragment:z", 1);
    let r2 = record(&mut store, &request, "fragment:z", 2);
    record(&mut store, &request, "fragment:z", 3);
    store.record_undo(&r1).unwrap();
    store.record_undo(&r2).unwrap();

    let ranked = store
        .rank_fragments_with(&request, FormulaRankingStrategy::Frequency)
        .unwrap();
    assert_eq!(ranked.order, vec![1, 0]);
    assert_eq!(ranked.features[1].net_acceptances(), 1);
    assert_eq!(ranked.features[0].net_acceptances(), 0);
}

#[test]
fn disabled_ranking_yields_original_order_for_every_strategy() {
    let mut store = owner();
    store
        .set_policy(FormulaPreferencePolicy {
            ranking_enabled: false,
            feedback_enabled: true,
            allow_sensitive_feedback: false,
        })
        .unwrap();
    let request = query();
    record(&mut store, &request, "fragment:z", 1);
    record(&mut store, &request, "fragment:z", 2);

    for strategy in [
        FormulaRankingStrategy::Baseline,
        FormulaRankingStrategy::Fixed,
        FormulaRankingStrategy::Frequency,
        FormulaRankingStrategy::Recency,
    ] {
        let ranked = store.rank_fragments_with(&request, strategy).unwrap();
        assert_eq!(ranked.order, vec![0, 1], "strategy {strategy:?}");
        assert!(ranked
            .features
            .iter()
            .all(|f| f == &FormulaPreferenceFeatures::default()));
    }
}
