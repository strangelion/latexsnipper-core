use latexsnipper_conversion::formula_completion::{
    apply_formula_candidate, fragments::*, scoped::*, FormulaCompletionError,
    FormulaCompletionSnapshot, FormulaSlot, FORMULA_COMPLETION_MAX_CANDIDATES,
};
use serde_json::json;

fn catalog() -> FormulaFragmentCatalog {
    serde_json::from_value(json!({
        "commands": {"scope": {"userSha256": "a".repeat(64), "projectSha256": "b".repeat(64),
            "documentSha256": "c".repeat(64), "generation": "18446744073709551615"},
            "definitions": [{"id": "private-pair", "revision": "18446744073709551615", "userSha256": "a".repeat(64),
                "projectSha256": null, "command": "privatepair", "arguments": ["math", "text"]}]},
        "policy": {"retrievalEnabled": true, "sensitiveDocument": false, "allowInSensitiveDocument": false},
        "fragments": [{"id": "saved-bayes", "revision": "18446744073709551615", "userSha256": "a".repeat(64),
            "projectSha256": "b".repeat(64), "documentSha256": "c".repeat(64), "trigger": "bayes", "label": "Saved formula",
            "source": r"\privatepair{}{caption}+\frac{}{}"}]
    })).unwrap()
}

fn query(source: &str, catalog: &FormulaFragmentCatalog) -> FormulaFragmentCompletion {
    complete_formula_fragments(source, source.len(), u64::MAX, catalog).unwrap()
}

#[test]
fn local_fragments_preserve_source_provenance_and_typed_tab_stops() {
    let catalog = catalog();
    let source = "中\\bay";
    let reply = query(source, &catalog);
    assert_eq!(reply.context.schema_version, 3);
    assert_eq!(reply.snapshot.schema_version, 3);
    let selected = &reply.candidates[0];
    assert_eq!(selected.edit.id, "fragment:saved-bayes");
    assert_eq!(selected.edit.source, "core:fragment");
    assert_eq!(selected.edit.label, "Saved formula");
    assert!(selected.definition.is_none());
    assert_eq!(selected.fragment.as_ref().unwrap().revision, u64::MAX);
    let applied = apply_formula_candidate_fragments(
        source,
        source.len(),
        u64::MAX,
        &catalog,
        &reply.snapshot,
        &selected.edit.id,
    )
    .unwrap();
    assert_eq!(applied.source, format!("中{}", catalog.fragments[0].source));
    assert_eq!(applied.tab_stops.len(), 3);
    assert_eq!(applied.cursor, applied.tab_stops[0].start);
    assert!(applied
        .tab_stops
        .iter()
        .all(|stop| stop.start == stop.end && applied.source.is_char_boundary(stop.start)));
    let encoded = serde_json::to_value(reply).unwrap();
    assert_eq!(encoded["snapshot"]["revision"], "18446744073709551615");
    assert_eq!(
        encoded["candidates"][0]["fragment"]["revision"],
        "18446744073709551615"
    );
}

#[test]
fn a_fragment_replaces_only_the_command_token_and_never_the_surrounding_formula() {
    let catalog = catalog();
    let marked = r"\frac{x}{\bay▮junk}+z";
    let source = marked.replace('▮', "");
    let cursor = marked.find('▮').unwrap();
    let reply = complete_formula_fragments(&source, cursor, 7, &catalog).unwrap();
    assert_eq!(reply.context.slot, FormulaSlot::FractionDenominator);
    let applied = apply_formula_candidate_fragments(
        &source,
        cursor,
        7,
        &catalog,
        &reply.snapshot,
        "fragment:saved-bayes",
    )
    .unwrap();
    assert_eq!(
        applied.source,
        format!("\\frac{{x}}{{{}}}+z", catalog.fragments[0].source)
    );
    let mut data = serde_json::to_value(FormulaFragmentCompletionApplyRequest {
        source,
        input_format:
            latexsnipper_conversion::formula_completion::FormulaCompletionInputFormat::Latex,
        cursor,
        revision: 7,
        catalog,
        snapshot: reply.snapshot,
        candidate_id: "fragment:saved-bayes".into(),
    })
    .unwrap();
    data["insert"] = json!("untrusted replacement");
    assert!(serde_json::from_value::<FormulaFragmentCompletionApplyRequest>(data).is_err());
}

#[test]
fn policies_suppress_private_data_before_semantic_validation_and_keep_builtins_available() {
    for policy in [
        FormulaFragmentPolicy {
            retrieval_enabled: false,
            sensitive_document: false,
            allow_in_sensitive_document: true,
        },
        FormulaFragmentPolicy {
            retrieval_enabled: true,
            sensitive_document: true,
            allow_in_sensitive_document: false,
        },
    ] {
        let mut catalog = catalog();
        catalog.policy = policy;
        catalog.fragments[0].id = "private bad ID".into();
        catalog.fragments[0].label = "PRIVATE_SECRET".into();
        catalog.fragments[0].source = "x".repeat(FORMULA_COMPLETION_MAX_FRAGMENT_BYTES + 1);
        let reply = query(r"\bay", &catalog);
        assert!(reply.candidates.is_empty());
        assert!(!serde_json::to_string(&reply)
            .unwrap()
            .contains("PRIVATE_SECRET"));
        assert!(query(r"\fr", &catalog)
            .candidates
            .iter()
            .any(|entry| entry.edit.id == "builtin:frac"));
    }
    let mut catalog = catalog();
    catalog.policy.sensitive_document = true;
    catalog.policy.allow_in_sensitive_document = true;
    assert!(query(r"\bay", &catalog).candidates[0].fragment.is_some());
}

#[test]
fn foreign_user_project_and_document_fragments_never_enter_validation_or_the_digest() {
    let base = catalog();
    let digest = query(r"\bay", &base).snapshot.catalog_sha256;
    for field in ["user", "project", "document"] {
        let mut catalog = base.clone();
        let mut foreign = catalog.fragments[0].clone();
        match field {
            "user" => foreign.user_sha256 = "d".repeat(64),
            "project" => foreign.project_sha256 = Some("d".repeat(64)),
            _ => foreign.document_sha256 = Some("d".repeat(64)),
        }
        foreign.trigger = "privateInvalid123".into();
        foreign.label = "PRIVATE_SECRET".into();
        foreign.source = r"\frac{".into();
        catalog.fragments.push(foreign);
        let reply = query(r"\bay", &catalog);
        assert_eq!(reply.snapshot.catalog_sha256, digest);
        assert_eq!(reply.candidates.len(), 1);
        assert!(!serde_json::to_string(&reply)
            .unwrap()
            .contains("PRIVATE_SECRET"));
    }
    let mut unbound = base.clone();
    unbound.commands.scope.document_sha256 = None;
    assert!(query(r"\bay", &unbound).candidates.is_empty());
    unbound.fragments[0].document_sha256 = None;
    assert!(query(r"\bay", &unbound).candidates[0].fragment.is_some());
}

#[test]
fn every_effective_catalog_policy_scope_and_editor_change_invalidates_old_edits() {
    let base = catalog();
    let reply = query(r"\bay", &base);
    for field in [
        "body",
        "label",
        "trigger",
        "id",
        "fragment-revision",
        "definition",
        "generation",
        "user",
        "project",
        "document",
        "enabled",
        "sensitive",
        "allow-sensitive",
    ] {
        let mut changed = base.clone();
        match field {
            "body" => changed.fragments[0].source = "x+1".into(),
            "label" => changed.fragments[0].label = "Renamed".into(),
            "trigger" => changed.fragments[0].trigger = "other".into(),
            "id" => changed.fragments[0].id = "changed".into(),
            "fragment-revision" => changed.fragments[0].revision = 0,
            "definition" => changed.commands.definitions[0].arguments.clear(),
            "generation" => changed.commands.scope.generation = 0,
            "user" => changed.commands.scope.user_sha256 = "d".repeat(64),
            "project" => changed.commands.scope.project_sha256 = Some("d".repeat(64)),
            "document" => changed.commands.scope.document_sha256 = Some("d".repeat(64)),
            "enabled" => changed.policy.retrieval_enabled = false,
            "sensitive" => changed.policy.sensitive_document = true,
            _ => changed.policy.allow_in_sensitive_document = true,
        }
        assert_eq!(
            apply_formula_candidate_fragments(
                r"\bay",
                4,
                u64::MAX,
                &changed,
                &reply.snapshot,
                "fragment:saved-bayes"
            ),
            Err(ScopedFormulaCompletionError::StaleContext),
            "{field}"
        );
    }
    for (source, cursor, revision, error) in [
        (
            r"\bayx",
            4,
            u64::MAX,
            ScopedFormulaCompletionError::StaleContext,
        ),
        (
            r"\bay",
            3,
            u64::MAX,
            ScopedFormulaCompletionError::StaleContext,
        ),
        (r"\bay", 4, 0, ScopedFormulaCompletionError::StaleContext),
        (
            "中\\bay",
            1,
            u64::MAX,
            ScopedFormulaCompletionError::InvalidCursor,
        ),
    ] {
        assert_eq!(
            apply_formula_candidate_fragments(
                source,
                cursor,
                revision,
                &base,
                &reply.snapshot,
                "fragment:saved-bayes"
            ),
            Err(error)
        );
    }
    assert_eq!(
        apply_formula_candidate_fragments(
            r"\bay",
            4,
            u64::MAX,
            &base,
            &reply.snapshot,
            "fragment:unknown"
        ),
        Err(ScopedFormulaCompletionError::UnknownCandidate)
    );
}

#[test]
fn literal_and_structural_headers_do_not_receive_private_fragments() {
    let catalog = catalog();
    for source in [
        r"\text{\bay",
        "% \\bay",
        r"\verb|\bay",
        r"\begin{\bay",
        r"\begin{array}{\bay",
        r"\cfrac[\bay",
    ] {
        assert!(
            !query(source, &catalog)
                .candidates
                .iter()
                .any(|entry| entry.fragment.is_some()),
            "{source}"
        );
    }
}

#[test]
fn fragment_templates_are_finite_closed_editing_data_not_executed_macro_bodies() {
    for value in [
        json!({"id":"bad id"}),
        json!({"trigger":"mysymbol2"}),
        json!({"label":"\n"}),
        json!({"source":""}),
        json!({"source":r"\frac{"}),
        json!({"source":"x\u{0000}"}),
    ] {
        let mut data = serde_json::to_value(catalog()).unwrap();
        for (key, value) in value.as_object().unwrap() {
            data["fragments"][0][key] = value.clone();
        }
        let invalid: FormulaFragmentCatalog = serde_json::from_value(data).unwrap();
        assert_eq!(
            complete_formula_fragments(r"\bay", 4, 7, &invalid),
            Err(ScopedFormulaCompletionError::InvalidCatalog)
        );
    }
    let mut catalog = catalog();
    catalog.fragments[0].source = r"\input{not-opened}".into();
    assert!(query(r"\bay", &catalog).candidates[0].fragment.is_some());
    // Source is data here, not permission to render, load a macro, or open a file.
    catalog.fragments[0].source = "x".repeat(FORMULA_COMPLETION_MAX_FRAGMENT_BYTES + 1);
    assert_eq!(
        complete_formula_fragments(r"\bay", 4, 7, &catalog),
        Err(ScopedFormulaCompletionError::LimitExceeded)
    );
}

#[test]
fn bare_fragment_shape_blocks_document_wrappers_and_comments_without_rewriting_source() {
    for source in [
        r"$$x$$",
        r"\(x\)",
        r"\[x\]",
        r"\documentclass{article}\begin{document}x\end{document}",
        "\\begin{doc% comment\nument}x\\end{document}",
        r"\begin{doc ument}x",
        r"\begin{align}x&=y\end{align}",
        "x % trailing comment",
        "% only a comment\n",
        "x\\",
    ] {
        let mut catalog = catalog();
        catalog.fragments[0].source = source.into();
        assert_eq!(
            complete_formula_fragments(r"\bay", 4, 7, &catalog),
            Err(ScopedFormulaCompletionError::InvalidCatalog),
            "{source}"
        );
    }
    for source in [
        "α+\\frac{}{} % kept\n ",
        r"\text{cost \$5}",
        r"\begin{aligned}x&=y\\z&=w\end{aligned}",
        r"\frac{α}{}+\sqrt[3]{}",
    ] {
        let mut catalog = catalog();
        catalog.fragments[0].source = source.into();
        let reply = query("中\\bay", &catalog);
        let applied = apply_formula_candidate_fragments(
            "中\\bay",
            7,
            u64::MAX,
            &catalog,
            &reply.snapshot,
            "fragment:saved-bayes",
        )
        .unwrap();
        assert_eq!(applied.source, format!("中{source}"));
        assert!(applied
            .tab_stops
            .iter()
            .all(|stop| applied.source.is_char_boundary(stop.start)
                && applied.source.is_char_boundary(stop.end)));
    }
}

#[test]
fn effective_ids_are_unique_order_is_irrelevant_and_old_profiles_cannot_accept_fragments() {
    let mut catalog = catalog();
    catalog.fragments.push(catalog.fragments[0].clone());
    assert_eq!(
        complete_formula_fragments(r"\bay", 4, 7, &catalog),
        Err(ScopedFormulaCompletionError::InvalidCatalog)
    );
    catalog.fragments[1].id = "second".into();
    let reply = query(r"\bay", &catalog);
    catalog.fragments.reverse();
    assert_eq!(query(r"\bay", &catalog), reply);
    let old = FormulaCompletionSnapshot {
        schema_version: 3,
        source_sha256: reply.snapshot.source_sha256.clone(),
        revision: u64::MAX,
        cursor: 4,
    };
    assert_eq!(
        apply_formula_candidate(r"\bay", 4, u64::MAX, &old, "fragment:saved-bayes"),
        Err(FormulaCompletionError::StaleContext)
    );
    let old = ScopedFormulaSnapshot {
        schema_version: 3,
        source_sha256: reply.snapshot.source_sha256,
        revision: u64::MAX,
        cursor: 4,
        definitions_sha256: reply.snapshot.catalog_sha256,
    };
    assert_eq!(
        apply_formula_candidate_scoped(
            r"\bay",
            4,
            u64::MAX,
            &catalog.commands,
            &old,
            "fragment:saved-bayes"
        ),
        Err(ScopedFormulaCompletionError::StaleContext)
    );
}

#[test]
fn combined_fragment_and_definition_candidates_keep_the_global_budget() {
    let mut catalog = catalog();
    let entry = catalog.fragments[0].clone();
    catalog.fragments = (0..64)
        .map(|index| {
            let mut entry = entry.clone();
            entry.id = format!("entry-{index}");
            entry
        })
        .collect();
    let command = catalog.commands.definitions[0].clone();
    catalog.commands.definitions.extend((0..63).map(|index| {
        let mut entry = command.clone();
        entry.id = format!("command-{index}");
        entry.command = format!(
            "command{}{}",
            (b'a' + index / 26) as char,
            (b'a' + index % 26) as char
        );
        entry
    }));
    let reply = query(r"\", &catalog);
    assert_eq!(reply.candidates.len(), FORMULA_COMPLETION_MAX_CANDIDATES);
    assert_eq!(
        reply
            .candidates
            .iter()
            .filter(|entry| entry.fragment.is_some())
            .count(),
        64
    );
    assert!(reply.candidates[64..]
        .iter()
        .all(|entry| entry.fragment.is_none()));
    catalog.fragments.push(entry);
    assert_eq!(
        complete_formula_fragments("", 0, 7, &catalog),
        Err(ScopedFormulaCompletionError::LimitExceeded)
    );
}

#[test]
fn definitions_and_builtins_keep_separate_provenance_and_work_when_retrieval_is_disabled() {
    let mut catalog = catalog();
    catalog.policy.retrieval_enabled = false;
    catalog.fragments[0].source = r"PRIVATE_SECRET\frac{".into();
    for (source, id, expected, definition) in [
        (
            r"\pri",
            "definition:private-pair",
            r"\privatepair{}{}",
            true,
        ),
        (r"\fr", "builtin:frac", r"\frac{}{}", false),
    ] {
        let reply = query(source, &catalog);
        let selected = reply
            .candidates
            .iter()
            .find(|entry| entry.edit.id == id)
            .unwrap();
        assert!(selected.fragment.is_none());
        assert_eq!(selected.definition.is_some(), definition);
        let applied = apply_formula_candidate_fragments(
            source,
            source.len(),
            u64::MAX,
            &catalog,
            &reply.snapshot,
            id,
        )
        .unwrap();
        assert_eq!(applied.source, expected);
        assert_eq!(applied.tab_stops.len(), 2);
    }
}

#[test]
fn unavailable_fragment_growth_does_not_fail_other_candidates_or_return_a_partial_tree() {
    let mut catalog = catalog();
    catalog.fragments[0].source = r"\frac{}{}".into();
    let source = format!("%{}\n\\bay", "x".repeat(65_536 - 6));
    assert_eq!(source.len(), 65_536);
    assert!(!query(&source, &catalog)
        .candidates
        .iter()
        .any(|entry| entry.fragment.is_some()));
    assert_eq!(
        complete_formula_fragments(&format!("{source}x"), 0, 7, &catalog),
        Err(ScopedFormulaCompletionError::LimitExceeded)
    );
    catalog.fragments[0].source = format!("{}x", r"\sqrt{".repeat(70));
    assert_eq!(
        complete_formula_fragments(r"\bay", 4, 7, &catalog),
        Err(ScopedFormulaCompletionError::LimitExceeded)
    );
}
