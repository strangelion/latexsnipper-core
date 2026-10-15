use latexsnipper_conversion::formula_completion::{
    analyze_formula_context, apply_formula_completion, complete_formula, scoped::*,
    FormulaCompletionContext, FormulaCompletionError, FormulaEditKind, FormulaSlot,
};

fn context(marked: &str) -> FormulaCompletionContext {
    analyze_formula_context(&marked.replace('▮', ""), marked.find('▮').unwrap(), 7).unwrap()
}

#[test]
fn common_builtin_signatures_keep_nested_math_arguments_and_literal_wrappers_separate() {
    for name in [
        "dot",
        "ddot",
        "dddot",
        "ddddot",
        "tilde",
        "widetilde",
        "widehat",
        "check",
        "breve",
        "acute",
        "grave",
        "mathring",
        "overrightarrow",
        "overleftarrow",
        "overleftrightarrow",
        "underleftarrow",
        "underrightarrow",
        "underleftrightarrow",
        "mathsf",
        "mathtt",
        "mathfrak",
        "mathnormal",
        "mathscr",
        "boldsymbol",
        "bm",
        "boxed",
        "ensuremath",
        "phantom",
        "hphantom",
        "vphantom",
    ] {
        let marked = format!("\\{name}{{▮x}}");
        let ctx = context(&marked);
        assert_eq!(ctx.slot, FormulaSlot::MathArgument, "{marked}");
        assert!(!ctx.in_text && ctx.diagnostics.is_empty(), "{marked}");
        let nested = context(&format!("\\{name}{{\\frac{{a}}{{▮b}}}}"));
        assert_eq!(nested.slot, FormulaSlot::FractionDenominator);
        assert!(nested
            .owner_path
            .iter()
            .any(|id| nested.nodes[*id].slot == Some(FormulaSlot::MathArgument)));
    }
    for name in [
        "textnormal",
        "textmd",
        "textsc",
        "textsl",
        "textup",
        "mbox",
        "fbox",
    ] {
        let marked = format!("\\{name}{{\\fr▮}}");
        let ctx = context(&marked);
        assert!(ctx.in_text && ctx.command_prefix.is_none(), "{marked}");
        let source = marked.replace('▮', "");
        assert!(complete_formula(&source, marked.find('▮').unwrap(), 7)
            .unwrap()
            .candidates
            .is_empty());
    }
    for marked in [
        r"\stackrel{▮a}{b}",
        r"\stackrel{a}{▮b}",
        r"\operatorname*{\hat{▮x}}",
    ] {
        assert_eq!(context(marked).slot, FormulaSlot::MathArgument, "{marked}");
    }
    assert_eq!(
        context(
            r"\operatorname % header
*{▮lim}"
        )
        .slot,
        FormulaSlot::MathArgument
    );
}

#[test]
fn operator_star_completion_preserves_existing_stars_arguments_and_source_trivia() {
    for (source, cursor, id, expected) in [
        (r"\oper", 5, "builtin:operatorname", r"\operatorname{}"),
        (
            r"\oper",
            5,
            "builtin:operatorname-star",
            r"\operatorname*{}",
        ),
        (
            r"\oper{x}",
            5,
            "builtin:operatorname-star",
            r"\operatorname*{x}",
        ),
        (
            r"\oper*{x}",
            5,
            "builtin:operatorname",
            r"\operatorname*{x}",
        ),
        (
            "\\oper % header\n*{x}",
            5,
            "builtin:operatorname",
            "\\operatorname % header\n*{x}",
        ),
    ] {
        let result = complete_formula(source, cursor, 7).unwrap();
        let applied = apply_formula_completion(source, cursor, 7, &result, id).unwrap();
        assert_eq!(applied.source, expected, "{source}");
        assert!(analyze_formula_context(&applied.source, applied.cursor, 8)
            .unwrap()
            .diagnostics
            .is_empty());
    }
    let result = complete_formula(r"\oper", 5, 7).unwrap();
    let applied =
        apply_formula_completion(r"\oper", 5, 7, &result, "builtin:operatorname-star").unwrap();
    assert_eq!(applied.tab_stops.len(), 1);
    assert_eq!(&applied.source[..applied.cursor], r"\operatorname*{");
    assert_eq!(
        apply_formula_completion(r"\oper", 5, 8, &result, "builtin:operatorname-star"),
        Err(FormulaCompletionError::StaleContext)
    );
    let marked = r"\operatorname*{\fr▮}";
    let ctx = context(marked);
    assert!(!ctx.in_text && ctx.command_prefix.unwrap().prefix == "fr");
}

#[test]
fn sized_pairs_keep_height_selection_and_own_their_body_and_script_base() {
    for (left, right, middle) in [
        ("bigl", "bigr", "bigm"),
        ("Bigl", "Bigr", "Bigm"),
        ("biggl", "biggr", "biggm"),
        ("Biggl", "Biggr", "Biggm"),
    ] {
        let marked = format!("\\{left}(▮x\\{middle}|y\\{right})");
        let ctx = context(&marked);
        assert_eq!(ctx.slot, FormulaSlot::Group);
        assert!(ctx.nodes[*ctx.owner_path.last().unwrap()].closed);
        assert!(
            ctx.diagnostics.is_empty(),
            "{marked}: {:?}",
            ctx.diagnostics
        );
        let source = format!("\\{left}(x");
        let result = complete_formula(&source, source.len(), 7).unwrap();
        let applied = apply_formula_completion(
            &source,
            source.len(),
            7,
            &result,
            "structure:close-delimiter",
        )
        .unwrap();
        assert_eq!(applied.source, format!("{source}\\{right})"));
        let pending = format!("{source}\\{right}");
        let result = complete_formula(&pending, pending.len(), 7).unwrap();
        let applied = apply_formula_completion(
            &pending,
            pending.len(),
            7,
            &result,
            "structure:close-delimiter",
        )
        .unwrap();
        assert_eq!(applied.source, format!("{pending})"));
        let prefix = format!("\\{left}");
        let result = complete_formula(&prefix, prefix.len(), 7).unwrap();
        let applied = apply_formula_completion(
            &prefix,
            prefix.len(),
            7,
            &result,
            &format!("builtin:{left}"),
        )
        .unwrap();
        assert_eq!(applied.source, format!("\\{left}(\\{right})"));
        assert_eq!(applied.tab_stops.len(), 1);
        assert_eq!(&applied.source[..applied.cursor], format!("\\{left}("));
    }
    for source in [r"\bigl(x\Biggm|y\Biggr)", r"\Biggl[x\bigr)"] {
        assert!(analyze_formula_context(source, 0, 0)
            .unwrap()
            .diagnostics
            .is_empty());
    }
    let ctx = context(r"\Bigl(x\Bigr)^{▮2}");
    let base = ctx
        .owner_path
        .iter()
        .find_map(|id| match ctx.nodes[*id].kind {
            FormulaEditKind::Script { base } => base,
            _ => None,
        })
        .unwrap();
    assert_eq!(ctx.nodes[base].kind, FormulaEditKind::Group);
}

#[test]
fn neutral_sizes_are_single_marker_commands_not_invented_paired_groups() {
    for name in [
        "big", "Big", "bigg", "Bigg", "bigm", "Bigm", "biggm", "Biggm",
    ] {
        let source = format!("\\{name}|_0^1");
        let ctx = analyze_formula_context(&source, source.len(), 0).unwrap();
        assert!(
            ctx.diagnostics.is_empty(),
            "{source}: {:?}",
            ctx.diagnostics
        );
        assert!(!ctx
            .nodes
            .iter()
            .any(|node| node.kind == FormulaEditKind::Group));
        let command = ctx.nodes.iter().find(|node| matches!(&node.kind, FormulaEditKind::Command { name: value } if value == name)).unwrap();
        assert_eq!(
            &source[command.range.start..command.range.end],
            format!("\\{name}|")
        );
        let missing = analyze_formula_context(&format!("\\{name}"), 0, 0).unwrap();
        assert!(missing
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MISSING_DELIMITER"));
    }
    for marked in [
        r"\begin{matrix}\Bigl(a&▮b\Bigr)\end{matrix}",
        r"\sqrt[\Bigl(x]{▮y}",
    ] {
        let ctx = context(marked);
        if marked.starts_with(r"\begin") {
            assert_eq!(ctx.cell.unwrap().column, 1);
        } else {
            assert_eq!(ctx.slot, FormulaSlot::RadicalBody);
        }
    }
    let ctx = context(r"\text{\Bigl(\Biggr)▮}");
    assert!(ctx.in_text && ctx.diagnostics.is_empty());
    for source in [r"\left(x\bigr)", r"\Bigl(x\right)"] {
        assert!(!analyze_formula_context(source, 0, 0)
            .unwrap()
            .diagnostics
            .is_empty());
    }
}

#[test]
fn scoped_declarations_cannot_replace_shared_builtin_signatures_or_size_markers() {
    let mut catalog: FormulaDefinitionCatalog = serde_json::from_value(serde_json::json!({
        "scope": {"userSha256": "a".repeat(64), "projectSha256": null, "documentSha256": null, "generation": "0"},
        "definitions": [{"id": "custom", "revision": "0", "userSha256": "a".repeat(64), "projectSha256": null,
            "command": "custom", "arguments": ["math"]}]})).unwrap();
    for name in [
        "operatorname",
        "boxed",
        "dot",
        "bm",
        "textnormal",
        "mbox",
        "stackrel",
        "big",
        "bigl",
        "Bigr",
        "Biggm",
        "middle",
    ] {
        catalog.definitions[0].command = name.into();
        assert_eq!(
            complete_formula_scoped("", 0, 0, &catalog),
            Err(ScopedFormulaCompletionError::InvalidCatalog),
            "{name}"
        );
    }
}

#[test]
fn finite_signature_catalog_provides_valid_templates_for_each_supported_common_family() {
    for name in [
        "dbinom",
        "tbinom",
        "dot",
        "ddot",
        "dddot",
        "ddddot",
        "tilde",
        "widetilde",
        "widehat",
        "check",
        "breve",
        "acute",
        "grave",
        "mathring",
        "overrightarrow",
        "overleftarrow",
        "overleftrightarrow",
        "underleftarrow",
        "underrightarrow",
        "underleftrightarrow",
        "mathsf",
        "mathtt",
        "mathfrak",
        "mathnormal",
        "mathscr",
        "boldsymbol",
        "bm",
        "boxed",
        "ensuremath",
        "phantom",
        "hphantom",
        "vphantom",
        "stackrel",
        "textrm",
        "textbf",
        "textit",
        "textsf",
        "texttt",
        "textnormal",
        "textmd",
        "textsc",
        "textsl",
        "textup",
        "mbox",
        "fbox",
    ] {
        let prefix = format!("\\{}", &name[..name.len() - 1]);
        let result = complete_formula(&prefix, prefix.len(), 7).unwrap();
        let applied = apply_formula_completion(
            &prefix,
            prefix.len(),
            7,
            &result,
            &format!("builtin:{name}"),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(!applied.tab_stops.is_empty(), "{name}");
        assert!(
            analyze_formula_context(&applied.source, applied.cursor, 8)
                .unwrap()
                .diagnostics
                .is_empty(),
            "{name}"
        );
    }
}
