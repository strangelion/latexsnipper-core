use latexsnipper_conversion::formula_completion::{
    analyze_formula_context, apply_formula_completion, complete_formula, FormulaCompletionContext,
    FormulaCompletionError, FormulaEditKind, FormulaSlot, FORMULA_COMPLETION_MAX_BYTES,
};

fn context(marked: &str) -> FormulaCompletionContext {
    let cursor = marked.find('▮').unwrap();
    analyze_formula_context(&marked.replace('▮', ""), cursor, 7).unwrap()
}

#[test]
fn fences_retain_nested_body_ownership_and_script_bases_without_new_node_tags() {
    for marked in [
        r"f(▮x)",
        r"[▮x)",
        r"\{▮x\}",
        r"\langle▮ x\rangle",
        "⟨▮x⟩",
        r"\left[▮x\right)",
        r"\left.▮x\right|",
        r"\left\lVert▮ x\right\rVert",
    ] {
        let ctx = context(marked);
        assert_eq!(ctx.slot, FormulaSlot::Group, "{marked}");
        let owner = &ctx.nodes[*ctx.owner_path.last().unwrap()];
        assert_eq!(owner.kind, FormulaEditKind::Group);
        assert!(owner.closed, "{marked}");
        assert!(
            ctx.diagnostics.is_empty(),
            "{marked}: {:?}",
            ctx.diagnostics
        );
    }
    let ctx = context(r"\left(\frac{a}{[▮b]}\right)");
    assert!(ctx
        .owner_path
        .iter()
        .any(|id| ctx.nodes[*id].slot == Some(FormulaSlot::FractionDenominator)));
    let ctx = context(r"(x)^{▮2}");
    let base = ctx
        .owner_path
        .iter()
        .find_map(|id| match ctx.nodes[*id].kind {
            FormulaEditKind::Script { base } => base,
            _ => None,
        })
        .unwrap();
    assert_eq!(ctx.nodes[base].kind, FormulaEditKind::Group);
    assert_eq!(
        (ctx.nodes[base].range.start, ctx.nodes[base].range.end),
        (0, 3)
    );
    let source = r"\left(x\middle|y\right]";
    let ctx = analyze_formula_context(source, 7, 7).unwrap();
    let owner = &ctx.nodes[*ctx.owner_path.last().unwrap()];
    assert_eq!(
        &source[owner.content.start..owner.content.end],
        r"x\middle|y"
    );
    assert!(owner.closed && ctx.diagnostics.is_empty());
    for source in ["|x|", "x<y", r"\vert x\vert", ".x."] {
        let ctx = analyze_formula_context(source, 0, 0).unwrap();
        assert!(
            !ctx.nodes
                .iter()
                .any(|node| node.kind == FormulaEditKind::Group),
            "{source}"
        );
    }
}

#[test]
fn explicit_closure_edits_handle_missing_markers_comments_and_nested_owners() {
    for (source, suffix) in [
        ("(x", ")"),
        ("[x", "]"),
        (r"\{x", r"\}"),
        (r"\langle x", r"\rangle"),
        ("⟨x", "⟩"),
        (r"\left(x", r"\right)"),
        (r"\left.x", r"\right."),
        (r"\left\langle x", r"\right\rangle"),
        ("\\left % header\n(x", r"\right)"),
        (r"\left(x\right", ")"),
        ("\\left(x\\right % footer\n", ")"),
    ] {
        let result = complete_formula(source, source.len(), 7).unwrap();
        let edit = result
            .candidates
            .iter()
            .find(|item| item.id == "structure:close-delimiter")
            .unwrap_or_else(|| panic!("{source}: {result:?}"));
        assert_eq!(edit.insert, suffix, "{source}");
        let applied = apply_formula_completion(source, source.len(), 7, &result, &edit.id).unwrap();
        assert_eq!(applied.source, format!("{source}{suffix}"));
        assert_eq!(applied.cursor, applied.source.len());
        assert!(
            analyze_formula_context(&applied.source, applied.cursor, 8)
                .unwrap()
                .diagnostics
                .is_empty(),
            "{}",
            applied.source
        );
    }
    for source in [
        r"\left(x^",
        r"\left(\frac{a}",
        r"\left\private",
        r"\left(",
        "(",
    ] {
        let result = complete_formula(source, source.len(), 7).unwrap();
        if !matches!(source, r"\left(" | "(") {
            assert!(
                !result
                    .candidates
                    .iter()
                    .any(|edit| edit.id == "structure:close-delimiter"),
                "{source}"
            );
        }
    }
    let source = r"\left({x";
    let result = complete_formula(source, source.len(), 7).unwrap();
    assert_eq!(
        result
            .candidates
            .iter()
            .find(|item| item.id == "structure:close-group")
            .unwrap()
            .insert,
        "}"
    );
    assert!(!result
        .candidates
        .iter()
        .any(|item| item.id == "structure:close-delimiter"));
}

#[test]
fn delimiter_templates_preserve_existing_bodies_and_place_the_cursor_inside() {
    let result = complete_formula(r"\lang", 5, 7).unwrap();
    let applied = apply_formula_completion(r"\lang", 5, 7, &result, "builtin:langle").unwrap();
    assert_eq!(applied.source, r"\langle\rangle");
    assert_eq!(applied.cursor, 7);
    assert_eq!(applied.tab_stops.len(), 1);
    for source in [r"\lang x\rangle", r"\lang\rangle"] {
        let result = complete_formula(source, 5, 7).unwrap();
        let applied = apply_formula_completion(source, 5, 7, &result, "builtin:langle").unwrap();
        assert_eq!(applied.source, source.replacen(r"\lang", r"\langle", 1));
        assert!(analyze_formula_context(&applied.source, 0, 8)
            .unwrap()
            .diagnostics
            .is_empty());
    }
    for (source, expected) in [
        (r"\langle x\ran", r"\langle x\rangle"),
        (r"\left(x\ri", r"\left(x\right)"),
    ] {
        let result = complete_formula(source, source.len(), 7).unwrap();
        let applied = apply_formula_completion(
            source,
            source.len(),
            7,
            &result,
            "structure:close-delimiter-prefix",
        )
        .unwrap();
        assert_eq!(applied.source, expected);
        assert_eq!(
            apply_formula_completion(
                source,
                source.len(),
                8,
                &result,
                "structure:close-delimiter-prefix"
            ),
            Err(FormulaCompletionError::StaleContext)
        );
    }
    for source in [r"\left(x^\ri", r"\left(\frac\ri", r"\left(x_{\ri"] {
        let result = complete_formula(source, source.len(), 7).unwrap();
        assert!(
            !result
                .candidates
                .iter()
                .any(|item| item.id == "structure:close-delimiter-prefix"),
            "{source}"
        );
    }
}

#[test]
fn presentation_fences_never_swallow_matrix_or_lexical_argument_boundaries() {
    for (marked, row, column) in [
        (r"\begin{matrix}(a&▮b)\end{matrix}", 0, 1),
        (r"\begin{matrix}\left(a\\▮b\right)\end{matrix}", 1, 0),
        (r"\begin{matrix}(a\end{matrix}▮", 0, 0),
    ] {
        let ctx = context(marked);
        if let Some(cell) = ctx.cell {
            assert_eq!((cell.row, cell.column), (row, column), "{marked}");
        } else {
            assert!(
                marked.ends_with("▮")
                    && ctx.nodes.iter().any(|node| matches!(
                        node.kind,
                        FormulaEditKind::Environment { .. }
                    ) && node.closed)
            );
        }
        assert!(
            ctx.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "UNCLOSED_DELIMITER"),
            "{marked}"
        );
    }
    assert_eq!(context(r"\sqrt[(x]{▮y}").slot, FormulaSlot::RadicalBody);
    assert!(context(r"\sqrt[{[x]}]{▮y}").diagnostics.is_empty());
    // A raw ] ends a TeX optional argument; presentation pairing cannot hide it.
    assert!(!context(r"\sqrt[[x]]{▮y}").diagnostics.is_empty());
    for (source, slot) in [
        (r"\frac(x)y", FormulaSlot::FractionNumerator),
        (r"x^(y)", FormulaSlot::Superscript),
    ] {
        let ctx = analyze_formula_context(source, 0, 0).unwrap();
        assert!(ctx
            .nodes
            .iter()
            .any(|node| node.kind == FormulaEditKind::Argument
                && node.slot == Some(slot)
                && &source[node.range.start..node.range.end] == "("));
        assert!(!ctx
            .nodes
            .iter()
            .any(|node| node.kind == FormulaEditKind::Group
                && &source[node.range.start..node.range.end] == "(x)"));
    }
    for source in [r"\text{(\left[▮}", r"\verb|(\left[▮|", "% (\\left[▮"] {
        let ctx = context(source);
        assert!(ctx.in_text || ctx.in_comment);
        assert!(!ctx
            .nodes
            .iter()
            .any(|node| node.slot == Some(FormulaSlot::Group)));
    }
}

#[test]
fn incomplete_and_stray_delimiters_diagnose_without_normalizing_source_or_prefixes() {
    for (source, code) in [
        (r"\left", "MISSING_DELIMITER"),
        (r"\left\private", "MISSING_DELIMITER"),
        (r"\left(x\right", "MISSING_DELIMITER"),
        (r"\right)", "UNMATCHED_DELIMITER"),
        ("x)", "UNMATCHED_DELIMITER"),
        (r"\langle x", "UNCLOSED_DELIMITER"),
    ] {
        let ctx = analyze_formula_context(source, source.len(), 0).unwrap();
        assert!(
            ctx.diagnostics.iter().any(|item| item.code == code),
            "{source}: {:?}",
            ctx.diagnostics
        );
    }
    let ctx = context(r"\leftward+\rightward+\fr▮");
    assert_eq!(ctx.command_prefix.unwrap().prefix, "fr");
    assert!(ctx.diagnostics.is_empty());
    let ctx = context("\\left(x % \\right)\n+▮y\\right]");
    assert!(ctx.diagnostics.is_empty());
}

#[test]
fn delimiter_depth_source_growth_and_generated_range_invariants_are_bounded() {
    for source in [
        format!("{}x{}", "(".repeat(70), ")".repeat(70)),
        format!("{}x{}", r"\left(".repeat(70), r"\right)".repeat(70)),
        format!("{}x{}", r"\Bigl(".repeat(70), r"\bigr)".repeat(70)),
    ] {
        assert_eq!(
            analyze_formula_context(&source, 0, 0),
            Err(FormulaCompletionError::LimitExceeded)
        );
    }
    let source = format!("%{}\n(", "x".repeat(FORMULA_COMPLETION_MAX_BYTES - 3));
    assert_eq!(source.len(), FORMULA_COMPLETION_MAX_BYTES);
    assert!(!complete_formula(&source, source.len(), 0)
        .unwrap()
        .candidates
        .iter()
        .any(|item| item.id == "structure:close-delimiter"));
    let pieces = [
        "(",
        ")",
        "[",
        "]",
        r"\left(",
        r"\right]",
        r"\Bigl(",
        r"\bigr]",
        r"\Biggm|",
        r"\big|",
        r"\operatorname*",
        r"\mbox",
        r"\langle",
        r"\rangle",
        "{",
        "}",
        r"\frac",
        "x^",
        "α",
        " & ",
        r"\\",
        "% )\n",
        r"\verb|(|",
    ];
    let mut state: usize = 42;
    for _ in 0..150 {
        let mut source = String::new();
        for _ in 0..14 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            source.push_str(pieces[state % pieces.len()]);
        }
        for cursor in source
            .char_indices()
            .map(|(index, _)| index)
            .chain([source.len()])
            .step_by(3)
        {
            let ctx = analyze_formula_context(&source, cursor, 7).unwrap();
            for (index, node) in ctx.nodes.iter().enumerate() {
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
                        ctx.nodes[parent].range.start <= node.range.start
                            && node.range.end <= ctx.nodes[parent].range.end
                    );
                }
            }
        }
    }
}
