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
fn optional_signatures_keep_omission_incomplete_arguments_and_typed_slots_distinct() {
    let mut catalog = catalog();
    catalog.definitions[0]
        .arguments
        .insert(0, FormulaDefinitionArgument::OptionalMath);
    for (source, slot, text) in [
        (
            r"\custompair[▮]{x}{caption}",
            FormulaSlot::MathArgument,
            false,
        ),
        (
            r"\custompair[\frac{a}{▮b}]{x}{caption}",
            FormulaSlot::FractionDenominator,
            false,
        ),
        (
            r"\custompair{▮x}{caption}",
            FormulaSlot::MathArgument,
            false,
        ),
        (r"\custompair[2]{x}{\fr▮}", FormulaSlot::Text, true),
    ] {
        let reply = query(source, &catalog);
        assert_eq!(reply.context.slot, slot, "{source}");
        assert_eq!(reply.context.in_text, text, "{source}");
        assert!(reply.context.diagnostics.is_empty(), "{source}");
    }
    let unfinished = query(r"\custompair[\fr▮", &catalog);
    assert!(unfinished
        .context
        .diagnostics
        .iter()
        .any(|item| item.code == "UNCLOSED_GROUP"));
    assert!(!unfinished
        .context
        .nodes
        .iter()
        .any(|node| node.slot == Some(FormulaSlot::Text)));
    catalog.definitions[0].arguments[0] = FormulaDefinitionArgument::OptionalText;
    let literal = query(r"\custompair[\fr▮]{x}{caption}", &catalog);
    assert!(literal.context.in_text && literal.candidates.is_empty());
    assert_eq!(literal.context.slot, FormulaSlot::Text);
}

#[test]
fn optional_candidates_offer_explicit_brackets_without_duplicating_existing_lists() {
    let mut catalog = catalog();
    catalog.definitions[0]
        .arguments
        .insert(0, FormulaDefinitionArgument::OptionalMath);
    let source = "中\\cust";
    let reply = complete_formula_scoped(source, source.len(), 7, &catalog).unwrap();
    for (id, suffix, stops) in [
        ("definition:custom-pair", r"\custompair{}{}", 2),
        ("definition:custom-pair:optional", r"\custompair[]{}{}", 3),
    ] {
        let applied =
            apply_formula_candidate_scoped(source, source.len(), 7, &catalog, &reply.snapshot, id)
                .unwrap();
        assert_eq!(applied.source, format!("中{suffix}"));
        assert_eq!(applied.tab_stops.len(), stops);
        assert!(applied
            .tab_stops
            .iter()
            .all(|stop| applied.source.is_char_boundary(stop.start) && stop.start == stop.end));
        assert_eq!(applied.cursor, applied.tab_stops[0].start);
    }
    for source in [
        r"\cust[2]{x}{caption}",
        r"\cust[",
        r"\cust{x}",
        "\\cust % header\n[2]",
    ] {
        let reply = complete_formula_scoped(source, 5, 7, &catalog).unwrap();
        let custom: Vec<_> = reply
            .candidates
            .iter()
            .filter(|item| item.definition.is_some())
            .collect();
        assert_eq!(custom.len(), 1, "{source}");
        assert_eq!(custom[0].edit.insert, r"\custompair");
        let applied = apply_formula_candidate_scoped(
            source,
            5,
            7,
            &catalog,
            &reply.snapshot,
            &custom[0].edit.id,
        )
        .unwrap();
        assert_eq!(applied.source, source.replacen(r"\cust", r"\custompair", 1));
        assert!(applied.tab_stops.is_empty());
    }
    catalog.definitions[0].arguments = vec![FormulaDefinitionArgument::OptionalText];
    let reply = query(r"\cust▮", &catalog);
    assert!(reply
        .candidates
        .iter()
        .any(|item| item.edit.insert == r"\custompair" && item.edit.tab_stops.is_empty()));
    assert!(reply
        .candidates
        .iter()
        .any(|item| item.edit.insert == r"\custompair[]" && item.edit.tab_stops.len() == 1));
}

#[test]
fn optional_arguments_respect_tex_token_boundaries_and_do_not_expand_macros() {
    let mut catalog = catalog();
    catalog.definitions[0].arguments = vec![
        FormulaDefinitionArgument::OptionalMath,
        FormulaDefinitionArgument::Math,
    ];
    for marked in [
        r"\custompair[{[x]}]{▮y}",
        r"\custompair[\]x]{▮y}",
        "\\custompair[x % ignored ] }\n]{▮y}",
        r"\custompair[\text{]}]{▮y}",
    ] {
        let reply = query(marked, &catalog);
        assert!(
            reply.context.diagnostics.is_empty(),
            "{marked}: {:?}",
            reply.context.diagnostics
        );
        assert_eq!(reply.context.slot, FormulaSlot::MathArgument, "{marked}");
    }
    for (source, expected) in [
        (r"\custompair[[x]]{y}", "[x"),
        (r"\custompair[\left(x\right]]{y}", r"\left(x\right"),
        (r"\custompair[\verb|]|]{y}", r"\verb|"),
    ] {
        let reply = complete_formula_scoped(source, 0, 7, &catalog).unwrap();
        let node = reply
            .context
            .nodes
            .iter()
            .find(|node| {
                node.range.start == r"\custompair".len()
                    && node.slot == Some(FormulaSlot::MathArgument)
            })
            .unwrap();
        assert_eq!(
            &source[node.content.start..node.content.end],
            expected,
            "{source}"
        );
        assert!(node.closed);
        assert!(!reply.context.diagnostics.is_empty(), "{source}");
    }
    let reply = query(r"\frac{\custompair[x}{▮y}", &catalog);
    assert_eq!(reply.context.slot, FormulaSlot::FractionDenominator);
    assert!(reply
        .context
        .diagnostics
        .iter()
        .any(|item| item.code == "UNCLOSED_GROUP"));
}

#[test]
fn optional_signature_shapes_are_bounded_filtered_and_bound_to_the_snapshot() {
    let mut catalog = catalog();
    for arguments in [
        vec![
            FormulaDefinitionArgument::Math,
            FormulaDefinitionArgument::OptionalMath,
        ],
        vec![
            FormulaDefinitionArgument::OptionalMath,
            FormulaDefinitionArgument::OptionalText,
        ],
    ] {
        catalog.definitions[0].arguments = arguments;
        assert_eq!(
            complete_formula_scoped("", 0, 7, &catalog),
            Err(ScopedFormulaCompletionError::InvalidCatalog)
        );
        let mut foreign = catalog.definitions[0].clone();
        foreign.user_sha256 = "f".repeat(64);
        catalog.definitions[0].arguments = vec![FormulaDefinitionArgument::Math];
        catalog.definitions.push(foreign);
        assert!(complete_formula_scoped("", 0, 7, &catalog).is_ok());
        catalog.definitions.pop();
    }
    catalog.definitions[0].arguments = vec![
        FormulaDefinitionArgument::OptionalMath,
        FormulaDefinitionArgument::Math,
    ];
    let reply = query(r"\cust▮", &catalog);
    catalog.definitions[0].arguments[0] = FormulaDefinitionArgument::OptionalText;
    assert_eq!(
        apply_formula_candidate_scoped(
            r"\cust",
            5,
            u64::MAX,
            &catalog,
            &reply.snapshot,
            "definition:custom-pair:optional"
        ),
        Err(ScopedFormulaCompletionError::StaleContext)
    );
    let encoded = serde_json::to_value(&catalog).unwrap();
    assert_eq!(
        encoded["definitions"][0]["arguments"],
        json!(["optional-text", "math"])
    );
    assert_eq!(
        serde_json::from_value::<FormulaDefinitionCatalog>(encoded).unwrap(),
        catalog
    );
}

#[test]
fn optional_candidate_expansion_respects_the_global_ceiling() {
    let mut catalog = catalog();
    let definition = catalog.definitions[0].clone();
    catalog.definitions = (0..64)
        .map(|index| {
            let mut entry = definition.clone();
            entry.id = format!("entry-{index}");
            entry.command = format!(
                "custom{}{}",
                (b'a' + index / 26) as char,
                (b'a' + index % 26) as char
            );
            entry.arguments = vec![
                FormulaDefinitionArgument::OptionalMath,
                FormulaDefinitionArgument::Math,
            ];
            entry
        })
        .collect();
    let reply = query(r"\▮", &catalog);
    assert_eq!(reply.candidates.len(), FORMULA_COMPLETION_MAX_CANDIDATES);
    assert!(reply
        .candidates
        .iter()
        .all(|entry| entry.definition.is_some()));
    assert_eq!(
        reply
            .candidates
            .iter()
            .filter(|entry| entry.edit.id.ends_with(":optional"))
            .count(),
        64
    );
}

#[test]
fn optional_argument_scanning_keeps_bounded_unicode_source_ranges() {
    let mut catalog = catalog();
    catalog.definitions[0].arguments = vec![
        FormulaDefinitionArgument::OptionalMath,
        FormulaDefinitionArgument::Math,
    ];
    let source = format!("{}x", r"\custompair[".repeat(70));
    assert_eq!(
        complete_formula_scoped(&source, 0, 7, &catalog),
        Err(ScopedFormulaCompletionError::LimitExceeded)
    );
    let pieces = [
        r"\custompair[",
        r"\sqrt[",
        r"\cfrac[",
        "[",
        "]",
        "{",
        "}",
        r"\left[",
        r"\right]",
        r"\]",
        "% ] }\n",
        "α😀",
        r"\frac",
        "x^",
        " & ",
    ];
    let mut state: usize = 42;
    for _ in 0..80 {
        let mut source = String::new();
        for _ in 0..12 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            source.push_str(pieces[state % pieces.len()]);
        }
        for cursor in source
            .char_indices()
            .map(|(index, _)| index)
            .chain([source.len()])
            .step_by(3)
        {
            let reply = complete_formula_scoped(&source, cursor, 7, &catalog).unwrap();
            assert_eq!(reply.context.nodes[0].range.end, source.len());
            for (index, node) in reply.context.nodes.iter().enumerate() {
                assert!(
                    node.range.start <= node.content.start
                        && node.content.start <= node.content.end
                        && node.content.end <= node.range.end
                );
                for offset in [
                    node.range.start,
                    node.range.end,
                    node.content.start,
                    node.content.end,
                ] {
                    assert!(offset <= source.len() && source.is_char_boundary(offset));
                }
                if let Some(parent) = node.parent {
                    assert!(parent < index);
                    assert!(
                        reply.context.nodes[parent].range.start <= node.range.start
                            && node.range.end <= reply.context.nodes[parent].range.end
                    );
                }
            }
        }
    }
}

#[test]
fn scoped_signatures_and_fences_keep_nested_math_and_literal_ownership_separate() {
    let catalog = catalog();
    let reply = query(r"\custompair{\left(\sqrt[3]{▮x}\right)}{caption}", &catalog);
    assert_eq!(reply.context.slot, FormulaSlot::RadicalBody);
    assert!(reply
        .context
        .owner_path
        .iter()
        .any(|id| reply.context.nodes[*id].slot == Some(FormulaSlot::MathArgument)));
    let reply = query(r"\custompair{x}{\left(▮}", &catalog);
    assert!(reply.context.in_text && reply.candidates.is_empty());
    for name in [
        "langle",
        "rangle",
        "lceil",
        "rvert",
        "vert",
        "Vert",
        "backslash",
    ] {
        let mut invalid = catalog.clone();
        invalid.definitions[0].command = name.into();
        assert_eq!(
            complete_formula_scoped("", 0, 0, &invalid),
            Err(ScopedFormulaCompletionError::InvalidCatalog)
        );
    }
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
