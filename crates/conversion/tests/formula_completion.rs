use latexsnipper_conversion::formula_completion::{
    analyze_formula_context, FormulaCompletionContext, FormulaCompletionError, FormulaEditKind,
    FormulaSlot,
};
use sha2::{Digest, Sha256};

#[test]
fn completion_wire_preserves_all_revision_bits_and_rejects_ambiguous_requests() {
    use latexsnipper_conversion::formula_completion::{
        FormulaCompletionApplyRequest, FormulaCompletionRequest,
    };
    use serde_json::json;
    for revision in ["0", "9007199254740993", "18446744073709551615"] {
        let request =
            json!({"source": "\\fr", "cursor": 3, "revision": revision, "inputFormat": "latex"});
        let request: FormulaCompletionRequest = serde_json::from_value(request).unwrap();
        let reply = request.execute().unwrap();
        let value = serde_json::to_value(&reply).unwrap();
        assert_eq!(value["snapshot"]["revision"], revision);
        assert_eq!(value["completion"]["context"]["revision"], revision);
        let apply = json!({"source": "\\fr", "cursor": 3, "revision": revision,
            "inputFormat": "latex", "snapshot": value["snapshot"], "candidateId": "builtin:frac"});
        let request: FormulaCompletionApplyRequest = serde_json::from_value(apply.clone()).unwrap();
        assert_eq!(request.execute().unwrap().source, r"\frac{}{}");
        for extra in ["insert", "replace", "timeoutMs", "backend"] {
            let mut invalid = apply.clone();
            invalid[extra] = json!("untrusted");
            assert!(serde_json::from_value::<FormulaCompletionApplyRequest>(invalid).is_err());
        }
        let mut invalid = apply;
        invalid["snapshot"]["extra"] = json!(true);
        assert!(serde_json::from_value::<FormulaCompletionApplyRequest>(invalid).is_err());
    }
    for revision in [
        json!(0),
        json!(1.5),
        json!(null),
        json!(""),
        json!("01"),
        json!("-1"),
        json!("+1"),
        json!("1e3"),
        json!(" 1"),
        json!("１"),
        json!("18446744073709551616"),
    ] {
        let request =
            json!({"source": "\\fr", "cursor": 3, "revision": revision, "inputFormat": "latex"});
        assert!(serde_json::from_value::<FormulaCompletionRequest>(request).is_err());
    }
    for input in ["typst", "unicode-math", "LATEX"] {
        assert!(serde_json::from_value::<FormulaCompletionRequest>(json!({
            "source": "x", "cursor": 1, "revision": "0", "inputFormat": input
        }))
        .is_err());
    }
}

#[test]
fn concise_snapshots_bind_source_cursor_version_and_revision() {
    use latexsnipper_conversion::formula_completion::{apply_formula_candidate, complete_formula};
    let source = "中\\fr";
    let reply = complete_formula(source, source.len(), u64::MAX).unwrap();
    let token = reply.context.snapshot();
    assert_eq!(
        apply_formula_candidate(source, source.len(), u64::MAX, &token, "builtin:frac")
            .unwrap()
            .source,
        "中\\frac{}{}"
    );
    for field in ["source", "cursor", "revision", "schemaVersion"] {
        let mut changed = token.clone();
        match field {
            "source" => changed.source_sha256 = "0".repeat(64),
            "cursor" => changed.cursor -= 1,
            "revision" => changed.revision -= 1,
            _ => changed.schema_version += 1,
        }
        assert_eq!(
            apply_formula_candidate(source, source.len(), u64::MAX, &changed, "builtin:frac"),
            Err(FormulaCompletionError::StaleContext)
        );
    }
    assert_eq!(
        apply_formula_candidate(source, 1, u64::MAX, &token, "builtin:frac"),
        Err(FormulaCompletionError::InvalidCursor)
    );
    assert_eq!(
        apply_formula_candidate(source, source.len(), u64::MAX, &token, "arbitrary"),
        Err(FormulaCompletionError::UnknownCandidate)
    );
}

fn context(marked: &str) -> FormulaCompletionContext {
    assert_eq!(marked.matches('▮').count(), 1);
    let cursor = marked.find('▮').unwrap();
    let source = marked.replacen('▮', "", 1);
    analyze_formula_context(&source, cursor, 17).unwrap()
}

#[test]
fn cursor_has_typed_nested_owners_for_complete_and_incomplete_operands() {
    for (source, slot) in [
        (r"\frac{▮x}{y}", FormulaSlot::FractionNumerator),
        (r"\frac{x}{▮y}", FormulaSlot::FractionDenominator),
        (r"\frac{x}▮", FormulaSlot::FractionDenominator),
        (r"\frac▮", FormulaSlot::FractionNumerator),
        (r"\frac{a▮", FormulaSlot::FractionNumerator),
        (r"\frac{\sqrt[▮3]{x}}{b}", FormulaSlot::RadicalIndex),
        (r"\sqrt[n▮", FormulaSlot::RadicalIndex),
        (r"\sqrt[n]{▮x}", FormulaSlot::RadicalBody),
        (r"x^{▮}", FormulaSlot::Superscript),
        (r"x_▮", FormulaSlot::Subscript),
        (r"x_{\frac{▮a}{b}}", FormulaSlot::FractionNumerator),
        (r"\binom{n}{▮k}", FormulaSlot::BinomialLower),
        (r"\cfrac[l]{▮x}{y}", FormulaSlot::FractionNumerator),
        (r"\cfrac[▮l]{x}{y}", FormulaSlot::FractionAlignment),
        (r"\mathbf{▮v}", FormulaSlot::MathArgument),
        (r"\frac{a}{b}▮", FormulaSlot::Formula),
        (r"\frac12▮", FormulaSlot::FractionDenominator),
        (r"{▮a}", FormulaSlot::Group),
    ] {
        let result = context(source);
        assert_eq!(result.slot, slot, "{source}: {result:?}");
        assert_eq!(result.owner_path.first(), Some(&0));
        for pair in result.owner_path.windows(2) {
            assert_eq!(result.nodes[pair[1]].parent, Some(pair[0]));
        }
    }
    let result = context(r"x_{\frac{▮a}{b}}");
    assert!(result
        .owner_path
        .iter()
        .any(|id| result.nodes[*id].slot == Some(FormulaSlot::Subscript)));
}

#[test]
fn matrix_positions_are_local_and_ignore_nested_or_escaped_separators() {
    for (source, environment, row, column, slot) in [
        (
            r"\begin{matrix}a&\frac{x}{▮y}\\b&c\end{matrix}",
            "matrix",
            0,
            1,
            FormulaSlot::FractionDenominator,
        ),
        (
            r"\begin{pmatrix}a&b\\c&▮d\end{pmatrix}",
            "pmatrix",
            1,
            1,
            FormulaSlot::MatrixCell,
        ),
        (
            r"\begin{pmatrix}a&\begin{matrix}b&▮c\end{matrix}\\d&e\end{pmatrix}",
            "matrix",
            0,
            1,
            FormulaSlot::MatrixCell,
        ),
        (
            r"\begin{matrix}a\&b&{x&y}▮\\c&d\end{matrix}",
            "matrix",
            0,
            1,
            FormulaSlot::MatrixCell,
        ),
        (
            r"\begin{cases}\text{a&b} & x>0\\▮0 & x=0\end{cases}",
            "cases",
            1,
            0,
            FormulaSlot::MatrixCell,
        ),
        (
            r"\begin{align*}a&=b\\c&=▮d\end{align*}",
            "align*",
            1,
            1,
            FormulaSlot::MatrixCell,
        ),
    ] {
        let result = context(source);
        assert_eq!(result.slot, slot, "{source}");
        let cell = result.cell.unwrap();
        assert_eq!(
            (cell.environment.as_str(), cell.row, cell.column),
            (environment, row, column),
            "{source}"
        );
    }
    let spec = context(r"\begin{array}{▮cc}a&b\end{array}");
    assert_eq!(spec.slot, FormulaSlot::ArrayColumnSpec);
    assert!(spec.cell.is_none());
    let body = context(r"\begin{array}{cc}a&▮b\end{array}");
    assert_eq!(body.cell.unwrap().column, 1);
}

#[test]
fn comments_text_and_verbatim_do_not_create_math_owners_or_prefixes() {
    for source in [
        "x % \\frac{▮x\ny",
        r"\text{\frac{▮a}{b}}",
        r"\verb|\frac{▮%a}|",
        r"\verb*+\sq▮+",
        r"\verb|\sq▮",
    ] {
        let result = context(source);
        assert!(result.in_comment || result.in_text, "{source}");
        assert!(result.command_prefix.is_none(), "{source}");
        assert!(
            !result
                .nodes
                .iter()
                .any(|node| node.slot == Some(FormulaSlot::FractionNumerator)),
            "{source}"
        );
    }
    let escaped = context(r"x\%+\fr▮");
    assert!(!escaped.in_comment);
    assert_eq!(escaped.command_prefix.unwrap().prefix, "fr");
    let next_line = context("x % \\frac{a}\n\\sq▮");
    assert!(!next_line.in_comment);
    assert_eq!(next_line.command_prefix.unwrap().prefix, "sq");
}

#[test]
fn source_revision_utf8_and_middle_of_command_ranges_are_preserved() {
    let source = "α+\\fraction+x";
    let cursor = "α+\\fra".len();
    let result = analyze_formula_context(source, cursor, 32).unwrap();
    assert_eq!(result.revision, 32);
    assert_eq!(
        result.source_sha256,
        format!("{:x}", Sha256::digest(source.as_bytes()))
    );
    let prefix = result.command_prefix.unwrap();
    assert_eq!(prefix.prefix, "fra");
    assert_eq!(
        &source[prefix.replace.start..prefix.replace.end],
        "\\fraction"
    );
    for node in result.nodes {
        for offset in [
            node.range.start,
            node.range.end,
            node.content.start,
            node.content.end,
        ] {
            assert!(offset <= source.len() && source.is_char_boundary(offset));
        }
        assert!(node.range.start <= node.content.start && node.content.end <= node.range.end);
    }
    assert_eq!(
        analyze_formula_context("α", 1, 0).unwrap_err(),
        FormulaCompletionError::InvalidCursor
    );
    assert_eq!(
        analyze_formula_context("x", 2, 0).unwrap_err(),
        FormulaCompletionError::InvalidCursor
    );
    assert_eq!(
        analyze_formula_context("", 0, 0).unwrap().slot,
        FormulaSlot::Formula
    );
}

#[test]
fn incomplete_environments_and_mismatches_keep_source_and_diagnostics() {
    let result = context(r"\begin{matrix}a&▮");
    assert_eq!(result.cell.unwrap().column, 1);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "UNCLOSED_ENVIRONMENT"));
    let result = context(r"\begin{matrix}\begin{cases}a\end{matrix}▮");
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "ENVIRONMENT_MISMATCH"));
    assert!(result.nodes.iter().any(
        |node| matches!(&node.kind, FormulaEditKind::Environment { name } if name == "matrix")
            && node.closed
    ));
    let result = context(r"\end{matrix}▮}");
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "UNMATCHED_ENVIRONMENT_END"));
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "UNMATCHED_GROUP"));
    assert_eq!(context(r"\begin{pm▮").slot, FormulaSlot::EnvironmentName);
}

#[test]
fn parser_limits_are_enforced_without_returning_a_partial_tree() {
    assert_eq!(
        analyze_formula_context(&"x".repeat(65_537), 0, 0).unwrap_err(),
        FormulaCompletionError::LimitExceeded
    );
    assert_eq!(
        analyze_formula_context(&"{x".repeat(70), 0, 0).unwrap_err(),
        FormulaCompletionError::LimitExceeded
    );
    assert_eq!(
        analyze_formula_context(&"\\alpha ".repeat(4096), 0, 0).unwrap_err(),
        FormulaCompletionError::LimitExceeded
    );
}

#[test]
fn generated_cursor_queries_always_have_valid_parent_paths_and_source_ranges() {
    for source in [
        r"\frac{α}{\sqrt[3]{x_i}}",
        r"\begin{matrix}a&b\\c&d\end{matrix}",
        r"\text{a\%b}%ignored",
        r"\frac{\unknown{a}",
        "α{x}^2}\\sq",
    ] {
        for cursor in (0..=source.len()).filter(|at| source.is_char_boundary(*at)) {
            let result = analyze_formula_context(source, cursor, 1).unwrap();
            for (id, node) in result.nodes.iter().enumerate() {
                assert!(node.parent.is_none_or(|parent| parent < id));
                assert!(
                    node.range.start <= node.content.start && node.content.end <= node.range.end
                );
                assert!(node.range.end <= source.len());
                assert!(
                    source.is_char_boundary(node.range.start)
                        && source.is_char_boundary(node.range.end)
                );
            }
        }
    }
}

#[test]
fn script_bases_and_environment_footer_names_have_structural_ownership() {
    let source = r"\frac{a}{b} ^2_i";
    let result = analyze_formula_context(source, source.len(), 0).unwrap();
    let scripts: Vec<_> = result
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(id, node)| {
            if let FormulaEditKind::Script { base } = node.kind {
                Some((id, base))
            } else {
                None
            }
        })
        .collect();
    let base = scripts[0].1.unwrap();
    assert!(
        matches!(&result.nodes[base].kind, FormulaEditKind::Command { name } if name == "frac")
    );
    assert_eq!(scripts[1].1, Some(scripts[0].0));
    let result = context(r"\begin{matrix}a\end{ma▮trix}");
    assert_eq!(result.slot, FormulaSlot::EnvironmentName);
    assert_eq!(result.active_environment.as_deref(), Some("matrix"));
    assert!(result.cell.is_none());
    assert!(context(r"\begin m▮atrix").active_environment.is_none());
    assert!(context(r"^▮x")
        .diagnostics
        .iter()
        .any(|d| d.code == "MISSING_SCRIPT_BASE"));
}

#[test]
fn core_scaffolds_apply_once_and_expose_structural_tab_stops() {
    use latexsnipper_conversion::formula_completion::{apply_formula_completion, complete_formula};
    for (source, id, expected, stops) in [
        (r"\fr", "builtin:frac", r"\frac{}{}", vec![6, 8]),
        (r"\sq", "builtin:sqrt", r"\sqrt{}", vec![6]),
        (r"\sq", "builtin:sqrt-index", r"\sqrt[]{}", vec![6, 8]),
        (r"α+\bi", "builtin:binom", r"α+\binom{}{}", vec![10, 12]),
    ] {
        let result = complete_formula(source, source.len(), 8).unwrap();
        let applied = apply_formula_completion(source, source.len(), 8, &result, id).unwrap();
        assert_eq!(applied.source, expected);
        assert_eq!(
            applied
                .tab_stops
                .iter()
                .map(|range| range.start)
                .collect::<Vec<_>>(),
            stops
        );
        assert_eq!(applied.cursor, stops[0]);
        assert!(analyze_formula_context(&applied.source, applied.cursor, 9)
            .unwrap()
            .diagnostics
            .is_empty());
    }
    let source = r"α+\fraction{a}{b}+z";
    let cursor = "α+\\fra".len();
    let result = complete_formula(source, cursor, 1).unwrap();
    let applied = apply_formula_completion(source, cursor, 1, &result, "builtin:frac").unwrap();
    assert_eq!(applied.source, r"α+\frac{a}{b}+z");
    assert!(applied.tab_stops.is_empty());
    let source = r"\sq [3]{x}";
    let result = complete_formula(source, 3, 1).unwrap();
    assert_eq!(result.candidates.len(), 1);
    assert_eq!(
        apply_formula_completion(source, 3, 1, &result, "builtin:sqrt")
            .unwrap()
            .source,
        r"\sqrt [3]{x}"
    );
}

#[test]
fn stale_responses_and_modified_candidate_payloads_cannot_change_the_edit() {
    use latexsnipper_conversion::formula_completion::{apply_formula_completion, complete_formula};
    let source = r"\fr";
    let mut result = complete_formula(source, 3, 4).unwrap();
    assert_eq!(
        apply_formula_completion(source, 3, 5, &result, "builtin:frac").unwrap_err(),
        FormulaCompletionError::StaleContext
    );
    assert_eq!(
        apply_formula_completion(r"\fr+x", 3, 4, &result, "builtin:frac").unwrap_err(),
        FormulaCompletionError::StaleContext
    );
    assert_eq!(
        apply_formula_completion(source, 1, 4, &result, "builtin:frac").unwrap_err(),
        FormulaCompletionError::StaleContext
    );
    assert_eq!(
        apply_formula_completion(source, 3, 4, &result, "unknown").unwrap_err(),
        FormulaCompletionError::UnknownCandidate
    );
    result.candidates[0].insert = r"\input{private}".into();
    result.candidates[0].replace.start = 65_536;
    assert_eq!(
        apply_formula_completion(source, 3, 4, &result, "builtin:frac")
            .unwrap()
            .source,
        r"\frac{}{}"
    );
    let serialized = serde_json::to_string(&result).unwrap();
    let decoded = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        apply_formula_completion(source, 3, 4, &decoded, "builtin:frac")
            .unwrap()
            .source,
        r"\frac{}{}"
    );
}

#[test]
fn environment_templates_and_structural_edits_keep_local_ranges_and_slots() {
    use latexsnipper_conversion::formula_completion::{apply_formula_completion, complete_formula};
    for source in [r"\beg", r"\begin{pm"] {
        let result = complete_formula(source, source.len(), 0).unwrap();
        let applied =
            apply_formula_completion(source, source.len(), 0, &result, "environment:pmatrix")
                .unwrap();
        let tree = analyze_formula_context(&applied.source, applied.cursor, 1).unwrap();
        assert_eq!(tree.active_environment.as_deref(), Some("pmatrix"));
        assert!(
            tree.diagnostics.is_empty(),
            "{}: {:?}",
            applied.source,
            tree.diagnostics
        );
        assert_eq!(applied.tab_stops.len(), 4);
    }
    let source = r"\begin{matrix}a&b\end{ma";
    let result = complete_formula(source, source.len(), 0).unwrap();
    let applied =
        apply_formula_completion(source, source.len(), 0, &result, "close-environment:matrix")
            .unwrap();
    assert_eq!(applied.source, r"\begin{matrix}a&b\end{matrix}");
    for (source, id, expected) in [
        (r"\frac{a", "structure:close-group", r"\frac{a}"),
        (r"\begin{matrix}a", "structure:cell", r"\begin{matrix}a&"),
        (r"\begin{matrix}a", "structure:row", r"\begin{matrix}a\\"),
    ] {
        let result = complete_formula(source, source.len(), 0).unwrap();
        assert_eq!(
            apply_formula_completion(source, source.len(), 0, &result, id)
                .unwrap()
                .source,
            expected
        );
    }
    for source in [
        r"\text{\fr",
        r"\verb|\fr",
        r"%\fr",
        r"\begin{array}{\fr",
        r"\cfrac[\fr",
    ] {
        assert!(complete_formula(source, source.len(), 0)
            .unwrap()
            .candidates
            .is_empty());
    }
}

#[test]
fn empty_slot_scaffolds_follow_structure_without_guessing_at_filled_operands() {
    use latexsnipper_conversion::formula_completion::{apply_formula_completion, complete_formula};
    for (source, id, expected) in [
        (r"x^", "slot:power-two", r"x^2"),
        (r"x_", "slot:index-i", r"x_i"),
        (r"\frac{", "slot:radical", r"\frac{\sqrt{}"),
        (r"\sqrt[", "slot:root-three", r"\sqrt[3"),
    ] {
        let result = complete_formula(source, source.len(), 0).unwrap();
        assert_eq!(
            apply_formula_completion(source, source.len(), 0, &result, id)
                .unwrap()
                .source,
            expected
        );
    }
    let source = r"\frac{x";
    let result = complete_formula(source, source.len(), 0).unwrap();
    assert!(!result
        .candidates
        .iter()
        .any(|candidate| candidate.id.starts_with("slot:")));
}

#[test]
fn environment_header_comments_cannot_close_scopes_or_move_matrix_cells() {
    let marked = "\\begin% }\n{mat% }\n rix}a % & \\\\\n&▮b\\end% }\n{mat% }\n rix}";
    let result = context(marked);
    assert_eq!(result.cell.unwrap().column, 1);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert!(result.nodes.iter().any(
        |node| matches!(&node.kind, FormulaEditKind::Environment { name } if name == "matrix")
            && node.closed
    ));
}

#[test]
fn candidate_catalogue_and_growth_respect_edit_tree_and_output_budgets() {
    use latexsnipper_conversion::formula_completion::{apply_formula_completion, complete_formula};
    let result = complete_formula("\\", 1, 0).unwrap();
    assert!(result.candidates.len() > 60 && result.candidates.len() <= 128);
    for candidate in &result.candidates {
        let applied = apply_formula_completion("\\", 1, 0, &result, &candidate.id).unwrap();
        let tree = analyze_formula_context(&applied.source, applied.cursor, 1).unwrap();
        assert!(
            tree.diagnostics.is_empty(),
            "{}: {:?}",
            candidate.id,
            tree.diagnostics
        );
        assert!(applied.tab_stops.iter().all(|stop| stop.start <= stop.end
            && stop.end <= applied.source.len()
            && applied.source.is_char_boundary(stop.start)
            && applied.source.is_char_boundary(stop.end)));
    }
    let source = format!("%{}\n\\fr", "x".repeat(65_530));
    assert_eq!(source.len(), 65_535);
    assert!(complete_formula(&source, source.len(), 0)
        .unwrap()
        .candidates
        .is_empty());
}
