use latexsnipper_conversion::formula_completion::{
    apply_formula_candidate, complete_formula, scoped::*, FormulaCompletionError, FormulaSlot,
    FORMULA_COMPLETION_MAX_BYTES, FORMULA_COMPLETION_MAX_CANDIDATES,
};
use serde_json::json;

fn catalog() -> FormulaDefinitionCatalog {
    FormulaDefinitionCatalog {
        scope: FormulaDefinitionScope {
            user_sha256: "a".repeat(64),
            project_sha256: Some("b".repeat(64)),
            document_sha256: Some("c".repeat(64)),
            generation: u64::MAX,
        },
        definitions: vec![FormulaCommandDefinition {
            id: "custom-pair".into(),
            revision: u64::MAX,
            user_sha256: "a".repeat(64),
            project_sha256: Some("b".repeat(64)),
            command: "custompair".into(),
            arguments: vec![
                FormulaDefinitionArgument::Math,
                FormulaDefinitionArgument::Text,
            ],
        }],
    }
}

fn query(marked: &str, catalog: &FormulaDefinitionCatalog) -> ScopedFormulaCompletion {
    assert_eq!(marked.matches('▮').count(), 1);
    complete_formula_scoped(
        &marked.replace('▮', ""),
        marked.find('▮').unwrap(),
        u64::MAX,
        catalog,
    )
    .unwrap()
}

#[test]
fn scoped_signatures_own_arguments_scripts_and_literal_scopes() {
    let catalog = catalog();
    for (source, slot, text) in [
        (r"\custompair{▮}{caption}", FormulaSlot::MathArgument, false),
        (
            r"\custompair{x^{▮}}{caption}",
            FormulaSlot::Superscript,
            false,
        ),
        (r"\custompair{x}{\fr▮}", FormulaSlot::Text, true),
        (r"\custompair ▮x", FormulaSlot::MathArgument, false),
        (r"\custompair{x}▮", FormulaSlot::Text, true),
    ] {
        let reply = query(source, &catalog);
        assert_eq!(reply.context.slot, slot, "{source}");
        assert_eq!(reply.context.in_text, text, "{source}");
        assert_eq!(reply.context.schema_version, 2);
        if text {
            assert!(reply.candidates.is_empty());
        }
    }
    let plain = complete_formula(r"\custompair{x}{caption}", 12, 0).unwrap();
    assert_ne!(plain.context.slot, FormulaSlot::MathArgument);
}

#[test]
fn definition_candidates_preserve_source_and_provide_typed_placeholders() {
    let catalog = catalog();
    let source = "中\\cust";
    let reply = complete_formula_scoped(source, source.len(), u64::MAX, &catalog).unwrap();
    let candidate = reply
        .candidates
        .iter()
        .find(|entry| entry.definition.is_some())
        .unwrap();
    assert_eq!(candidate.edit.id, "definition:custom-pair");
    assert_eq!(candidate.edit.source, "core:definition");
    assert_eq!(candidate.definition.as_ref().unwrap().revision, u64::MAX);
    assert_eq!(candidate.edit.insert, r"\custompair{}{}");
    assert_eq!(candidate.edit.tab_stops.len(), 2);
    let result = apply_formula_candidate_scoped(
        source,
        source.len(),
        u64::MAX,
        &catalog,
        &reply.snapshot,
        &candidate.edit.id,
    )
    .unwrap();
    assert_eq!(result.source, "中\\custompair{}{}");
    assert_eq!(result.cursor, "中\\custompair{".len());
    assert_eq!(result.tab_stops.len(), 2);
    for source in [r"\cust{x}{caption}", "\\cust % comment\n{x}"] {
        let reply = complete_formula_scoped(source, 5, 1, &catalog).unwrap();
        let edit = &reply
            .candidates
            .iter()
            .find(|entry| entry.definition.is_some())
            .unwrap()
            .edit;
        assert_eq!(edit.insert, r"\custompair");
        let result =
            apply_formula_candidate_scoped(source, 5, 1, &catalog, &reply.snapshot, &edit.id)
                .unwrap();
        assert_eq!(result.source, source.replacen(r"\cust", r"\custompair", 1));
        assert!(result.tab_stops.is_empty());
    }
}

#[test]
fn zero_argument_symbols_are_data_not_macro_expansion() {
    let mut catalog = catalog();
    catalog.definitions[0].arguments.clear();
    let reply = query(r"\cust▮", &catalog);
    let candidate = reply
        .candidates
        .iter()
        .find(|entry| entry.definition.is_some())
        .unwrap();
    assert_eq!(candidate.edit.insert, r"\custompair");
    assert!(candidate.edit.tab_stops.is_empty());
    assert!(query(r"\text{\cust▮}", &catalog).candidates.is_empty());
    assert!(query("% \\cust▮", &catalog).candidates.is_empty());
    assert!(query(r"\verb|\cust▮|", &catalog).candidates.is_empty());
    assert!(query(r"\begin{array}{\cust▮}x\end{array}", &catalog)
        .candidates
        .is_empty());
}

#[test]
fn all_snapshot_dimensions_reject_old_edits_and_v1_downgrade() {
    let catalog = catalog();
    let source = r"\cust";
    let reply = complete_formula_scoped(source, source.len(), 9, &catalog).unwrap();
    for field in [
        "revision",
        "signature",
        "command",
        "id",
        "user",
        "project",
        "document",
        "generation",
        "clear",
    ] {
        let mut changed = catalog.clone();
        match field {
            "revision" => changed.definitions[0].revision -= 1,
            "signature" => changed.definitions[0].arguments.clear(),
            "command" => changed.definitions[0].command.push('x'),
            "id" => changed.definitions[0].id.push('x'),
            "user" => changed.scope.user_sha256 = "d".repeat(64),
            "project" => changed.scope.project_sha256 = Some("d".repeat(64)),
            "document" => changed.scope.document_sha256 = Some("d".repeat(64)),
            "generation" => changed.scope.generation -= 1,
            "clear" => changed.definitions.clear(),
            _ => unreachable!(),
        }
        assert_eq!(
            apply_formula_candidate_scoped(
                source,
                source.len(),
                9,
                &changed,
                &reply.snapshot,
                "definition:custom-pair"
            ),
            Err(ScopedFormulaCompletionError::StaleContext),
            "{field}"
        );
    }
    for (source, cursor, revision) in [(r"\custx", 5, 9), (source, 4, 9), (source, 5, 10)] {
        assert_eq!(
            apply_formula_candidate_scoped(
                source,
                cursor,
                revision,
                &catalog,
                &reply.snapshot,
                "definition:custom-pair"
            ),
            Err(ScopedFormulaCompletionError::StaleContext)
        );
    }
    assert_eq!(
        apply_formula_candidate(source, 5, 9, &reply.context.snapshot(), "builtin:frac"),
        Err(FormulaCompletionError::StaleContext)
    );
    assert_eq!(
        apply_formula_candidate_scoped(source, 5, 9, &catalog, &reply.snapshot, "unknown"),
        Err(ScopedFormulaCompletionError::UnknownCandidate)
    );
}

#[test]
fn scope_filtering_never_exposes_foreign_names_or_changes_effective_digest() {
    let catalog = catalog();
    let baseline = query(r"\▮", &catalog);
    for (user, project) in [
        ("d".repeat(64), Some("b".repeat(64))),
        ("a".repeat(64), Some("d".repeat(64))),
    ] {
        let mut changed = catalog.clone();
        let mut foreign = changed.definitions[0].clone();
        foreign.id = "hidden-private-id".into();
        foreign.command = "privateInvalid123".into();
        foreign.user_sha256 = user;
        foreign.project_sha256 = project;
        foreign.arguments = vec![FormulaDefinitionArgument::Math; 20];
        changed.definitions.push(foreign);
        assert_eq!(query(r"\▮", &changed), baseline);
    }
    let mut user_global = catalog.clone();
    user_global.definitions[0].project_sha256 = None;
    user_global.scope.project_sha256 = None;
    assert!(query(r"\cust▮", &user_global)
        .candidates
        .iter()
        .any(|entry| entry.definition.is_some()));
    user_global.definitions[0].project_sha256 = Some("b".repeat(64));
    assert!(query(r"\cust▮", &user_global).candidates.is_empty());
}

#[test]
fn scoped_catalog_order_is_canonical_but_ambiguity_is_refused() {
    let mut catalog = catalog();
    let mut other = catalog.definitions[0].clone();
    other.id = "second".into();
    other.command = "customother".into();
    catalog.definitions.push(other);
    let before = query(r"\cust▮", &catalog);
    catalog.definitions.reverse();
    assert_eq!(query(r"\cust▮", &catalog), before);
    catalog.definitions[0].command = catalog.definitions[1].command.clone();
    catalog.definitions[0].project_sha256 = None;
    assert_eq!(
        complete_formula_scoped(r"\cust", 5, 0, &catalog),
        Err(ScopedFormulaCompletionError::InvalidCatalog)
    );
    catalog.definitions[0].command = "unique".into();
    catalog.definitions[0].id = catalog.definitions[1].id.clone();
    assert_eq!(
        complete_formula_scoped(r"\cust", 5, 0, &catalog),
        Err(ScopedFormulaCompletionError::InvalidCatalog)
    );
}

#[test]
fn declarations_refuse_invalid_control_words_reserved_owners_and_unbounded_payloads() {
    for command in [
        "",
        "mysymbol2",
        "\\custom",
        "命令",
        "custom*",
        "newcommand",
        "frac",
        "dbinom",
        "sqrt",
        "text",
        "verb",
        "begin",
        "end",
        "left",
        "alpha",
        "min",
        "max",
    ] {
        let mut catalog = catalog();
        catalog.definitions[0].command = command.into();
        assert_eq!(
            complete_formula_scoped("", 0, 0, &catalog),
            Err(ScopedFormulaCompletionError::InvalidCatalog),
            "{command}"
        );
    }
    for id in [
        "",
        "id:inject",
        "private/path",
        "\\input{file}",
        &"a".repeat(65),
    ] {
        let mut catalog = catalog();
        catalog.definitions[0].id = id.into();
        assert_eq!(
            complete_formula_scoped("", 0, 0, &catalog),
            Err(ScopedFormulaCompletionError::InvalidCatalog)
        );
    }
    let mut catalog = catalog();
    catalog.definitions[0].arguments = vec![FormulaDefinitionArgument::Math; 5];
    assert_eq!(
        complete_formula_scoped("", 0, 0, &catalog),
        Err(ScopedFormulaCompletionError::LimitExceeded)
    );
    catalog.definitions = vec![catalog.definitions[0].clone(); 65];
    assert_eq!(
        complete_formula_scoped("", 0, 0, &catalog),
        Err(ScopedFormulaCompletionError::LimitExceeded)
    );
    let mut catalog = crate::catalog();
    catalog.scope.user_sha256 = "A".repeat(64);
    assert_eq!(
        complete_formula_scoped("", 0, 0, &catalog),
        Err(ScopedFormulaCompletionError::InvalidCatalog)
    );
}

#[test]
fn finite_source_and_candidate_budgets_and_utf8_boundaries_remain_enforced() {
    let mut catalog = catalog();
    let template = catalog.definitions[0].clone();
    catalog.definitions.clear();
    for suffix in 0..FORMULA_COMPLETION_MAX_DEFINITIONS {
        let mut entry = template.clone();
        entry.id = format!("id-{suffix}");
        entry.command = format!(
            "custom{}{}",
            (b'A' + (suffix / 26) as u8) as char,
            (b'A' + (suffix % 26) as u8) as char
        );
        catalog.definitions.push(entry);
    }
    let all = query(r"\▮", &catalog);
    assert_eq!(all.candidates.len(), FORMULA_COMPLETION_MAX_CANDIDATES);
    assert_eq!(
        all.candidates
            .iter()
            .filter(|entry| entry.definition.is_some())
            .count(),
        FORMULA_COMPLETION_MAX_DEFINITIONS
    );
    assert_eq!(
        complete_formula_scoped("中", 1, 0, &catalog),
        Err(ScopedFormulaCompletionError::InvalidCursor)
    );
    assert_eq!(
        complete_formula_scoped(
            &"x".repeat(FORMULA_COMPLETION_MAX_BYTES + 1),
            0,
            0,
            &catalog
        ),
        Err(ScopedFormulaCompletionError::LimitExceeded)
    );
    let near_limit = format!("%{}\n\\cust", "x".repeat(FORMULA_COMPLETION_MAX_BYTES - 8));
    let reply = complete_formula_scoped(&near_limit, near_limit.len(), 0, &catalog).unwrap();
    assert!(reply.candidates.is_empty());
}

#[test]
fn scoped_wire_is_closed_and_all_versions_are_lossless_strings() {
    let request = ScopedFormulaCompletionRequest {
        source: r"\cust".into(),
        input_format:
            latexsnipper_conversion::formula_completion::FormulaCompletionInputFormat::Latex,
        cursor: 5,
        revision: u64::MAX,
        catalog: catalog(),
    };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["revision"], "18446744073709551615");
    assert_eq!(
        value["catalog"]["scope"]["generation"],
        "18446744073709551615"
    );
    assert_eq!(
        value["catalog"]["definitions"][0]["revision"],
        "18446744073709551615"
    );
    let reply = request.execute().unwrap();
    let mut apply = json!({"source": request.source, "inputFormat": "latex", "cursor": 5,
        "revision": "18446744073709551615", "catalog": value["catalog"],
        "snapshot": reply.snapshot, "candidateId": "definition:custom-pair"});
    let valid: ScopedFormulaCompletionApplyRequest = serde_json::from_value(apply.clone()).unwrap();
    assert_eq!(valid.execute().unwrap().source, r"\custompair{}{}");
    for path in ["scope", "definition", "snapshot", "root"] {
        let mut invalid = apply.clone();
        match path {
            "scope" => invalid["catalog"]["scope"]["extra"] = json!(true),
            "definition" => {
                invalid["catalog"]["definitions"][0]["body"] = json!(r"\input{private}")
            }
            "snapshot" => invalid["snapshot"]["extra"] = json!(true),
            _ => invalid["insert"] = json!("untrusted"),
        }
        assert!(serde_json::from_value::<ScopedFormulaCompletionApplyRequest>(invalid).is_err());
    }
    apply["catalog"]["definitions"][0]["revision"] = json!(42);
    assert!(serde_json::from_value::<ScopedFormulaCompletionApplyRequest>(apply).is_err());
}
