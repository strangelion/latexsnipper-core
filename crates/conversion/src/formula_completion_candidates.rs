//! Deterministic candidate edits; ranking and host interaction are separate.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaCompletionCandidate {
    pub id: String,
    pub label: String,
    pub source: String,
    pub slot: FormulaSlot,
    pub replace: FormulaSourceRange,
    pub insert: String,
    /// UTF-8 ranges relative to `insert`, in structural traversal order.
    pub tab_stops: Vec<FormulaSourceRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaCompletionResult {
    pub context: FormulaCompletionContext,
    pub candidates: Vec<FormulaCompletionCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFormulaCompletion {
    pub source: String,
    pub cursor: usize,
    /// Absolute ranges in the returned source, suitable for host selections.
    pub tab_stops: Vec<FormulaSourceRange>,
}

const SYMBOLS: &[&str] = &[
    "alpha",
    "beta",
    "gamma",
    "delta",
    "epsilon",
    "varepsilon",
    "zeta",
    "eta",
    "theta",
    "vartheta",
    "iota",
    "kappa",
    "lambda",
    "mu",
    "nu",
    "xi",
    "pi",
    "rho",
    "sigma",
    "tau",
    "upsilon",
    "phi",
    "varphi",
    "chi",
    "psi",
    "omega",
    "Gamma",
    "Delta",
    "Theta",
    "Lambda",
    "Xi",
    "Pi",
    "Sigma",
    "Phi",
    "Psi",
    "Omega",
    "sin",
    "cos",
    "tan",
    "log",
    "ln",
    "exp",
    "lim",
    "min",
    "max",
    "det",
    "gcd",
    "infty",
    "partial",
    "nabla",
    "cdot",
    "times",
    "div",
    "pm",
    "neq",
    "leq",
    "geq",
    "approx",
    "in",
    "notin",
    "subset",
    "cup",
    "cap",
    "forall",
    "exists",
    "to",
    "Rightarrow",
    "sum",
    "int",
    "iint",
    "iiint",
    "oint",
    "prod",
];
const STRUCTURES: &[(&str, &str)] = &[
    ("frac", "\\frac{}{}"),
    ("dfrac", "\\dfrac{}{}"),
    ("tfrac", "\\tfrac{}{}"),
    ("cfrac", "\\cfrac{}{}"),
    ("binom", "\\binom{}{}"),
    ("sqrt", "\\sqrt{}"),
    ("sqrt-index", "\\sqrt[]{}"),
    ("vec", "\\vec{}"),
    ("hat", "\\hat{}"),
    ("bar", "\\bar{}"),
    ("overline", "\\overline{}"),
    ("overbrace", "\\overbrace{}"),
    ("underbrace", "\\underbrace{}"),
    ("mathrm", "\\mathrm{}"),
    ("mathbf", "\\mathbf{}"),
    ("mathbb", "\\mathbb{}"),
    ("mathcal", "\\mathcal{}"),
    ("mathit", "\\mathit{}"),
    ("operatorname", "\\operatorname{}"),
    ("text", "\\text{}"),
    ("overset", "\\overset{}{}"),
    ("underset", "\\underset{}{}"),
];
const ENVIRONMENTS: &[&str] = &[
    "matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix", "cases", "aligned",
];

pub(super) fn builtin_command(name: &str) -> bool {
    SYMBOLS.contains(&name)
        || STRUCTURES
            .iter()
            .any(|(id, _)| id.split('-').next() == Some(name))
}

fn splice(source: &str, replace: FormulaSourceRange, insert: &str) -> EditResult<String> {
    if replace.start > replace.end
        || replace.end > source.len()
        || !source.is_char_boundary(replace.start)
        || !source.is_char_boundary(replace.end)
    {
        return Err(FormulaCompletionError::InvalidCursor);
    }
    if source.len() - (replace.end - replace.start) + insert.len() > MAX_BYTES {
        return Err(FormulaCompletionError::LimitExceeded);
    }
    Ok(format!(
        "{}{}{}",
        &source[..replace.start],
        insert,
        &source[replace.end..]
    ))
}

pub(super) fn candidate(
    source: &str,
    context: &FormulaCompletionContext,
    id: &str,
    label: &str,
    replace: FormulaSourceRange,
    insert: &str,
    signatures: &[CommandSignature],
) -> EditResult<FormulaCompletionCandidate> {
    let proposed = splice(source, replace, insert)?;
    let tree = analyze_with_signatures(&proposed, replace.start, context.revision, signatures)?;
    let inserted_end = replace.start + insert.len();
    let tab_stops = tree
        .nodes
        .iter()
        .filter(|node| {
            node.closed
                && node.slot.is_some_and(|slot| {
                    !matches!(
                        slot,
                        FormulaSlot::Formula | FormulaSlot::Group | FormulaSlot::EnvironmentName
                    )
                })
                && replace.start <= node.content.start
                && node.content.end <= inserted_end
                && proposed[node.content.start..node.content.end]
                    .trim()
                    .is_empty()
        })
        .map(|node| FormulaSourceRange {
            start: node.content.start - replace.start,
            end: node.content.end - replace.start,
        })
        .collect();
    Ok(FormulaCompletionCandidate {
        id: id.into(),
        label: label.into(),
        source: "core:builtin".into(),
        slot: context.slot,
        replace,
        insert: insert.into(),
        tab_stops,
    })
}

pub(super) fn existing_group(source: &str, mut pos: usize, optional: bool) -> bool {
    loop {
        match source.as_bytes().get(pos) {
            Some(byte) if byte.is_ascii_whitespace() => pos += 1,
            Some(b'%') => {
                while source
                    .as_bytes()
                    .get(pos)
                    .is_some_and(|byte| !matches!(byte, b'\r' | b'\n'))
                {
                    pos += 1;
                }
            }
            _ => break,
        }
    }
    source.as_bytes().get(pos) == Some(&b'{')
        || optional && source.as_bytes().get(pos) == Some(&b'[')
}

/// Produce finite built-in scaffolds and structural edits, bound to one snapshot.
/// Empty placeholders are editing slots, not a claim of mathematical validity.
pub fn complete_formula(
    source: &str,
    cursor: usize,
    revision: u64,
) -> EditResult<FormulaCompletionResult> {
    complete_with_signatures(source, cursor, revision, &[])
}

pub(super) fn complete_with_signatures(
    source: &str,
    cursor: usize,
    revision: u64,
    signatures: &[CommandSignature],
) -> EditResult<FormulaCompletionResult> {
    let context = analyze_with_signatures(source, cursor, revision, signatures)?;
    let mut candidates = Vec::new();
    if context.in_comment
        || context.in_text
        || matches!(
            context.slot,
            FormulaSlot::ArrayColumnSpec | FormulaSlot::FractionAlignment
        )
    {
        return Ok(FormulaCompletionResult {
            context,
            candidates,
        });
    }
    let mut add =
        |id: &str, label: &str, replace: FormulaSourceRange, insert: &str| -> EditResult<()> {
            // Omit an unavailable candidate rather than fail all available choices.
            match candidate(source, &context, id, label, replace, insert, signatures) {
                Ok(value) => {
                    if candidates.len() < FORMULA_COMPLETION_MAX_CANDIDATES
                        && !candidates.iter().any(|old: &FormulaCompletionCandidate| {
                            old.replace == value.replace && old.insert == value.insert
                        })
                    {
                        candidates.push(value);
                    }
                }
                Err(FormulaCompletionError::LimitExceeded) => {}
                Err(error) => return Err(error),
            }
            Ok(())
        };
    if context.slot == FormulaSlot::EnvironmentName {
        let slot = &context.nodes[*context.owner_path.last().expect("root path")];
        if !slot.closed && cursor == source.len() && slot.content.start <= cursor {
            let prefix = &source[slot.content.start..cursor];
            let command = slot
                .parent
                .and_then(|parent| match &context.nodes[parent].kind {
                    FormulaEditKind::Command { name } => Some(name.as_str()),
                    _ => None,
                });
            let replace = FormulaSourceRange {
                start: slot.content.start,
                end: cursor,
            };
            if command == Some("begin") {
                for name in ENVIRONMENTS.iter().filter(|name| name.starts_with(prefix)) {
                    add(
                        &format!("environment:{name}"),
                        name,
                        replace,
                        &format!("{name}}}\n & \\\\\n & \n\\end{{{name}}}"),
                    )?;
                }
            } else if command == Some("end") {
                if let Some(name) = context
                    .active_environment
                    .as_ref()
                    .filter(|name| name.starts_with(prefix))
                {
                    add(
                        &format!("close-environment:{name}"),
                        name,
                        replace,
                        &format!("{name}}}"),
                    )?;
                }
            }
        }
    } else if let Some(prefix) = &context.command_prefix {
        for (id, scaffold) in STRUCTURES {
            let name = id.split('-').next().expect("template name");
            if !name.starts_with(&prefix.prefix) {
                continue;
            }
            let bare = format!("\\{name}");
            let insert = if existing_group(source, prefix.replace.end, name == "sqrt") {
                bare.as_str()
            } else {
                scaffold
            };
            add(&format!("builtin:{id}"), insert, prefix.replace, insert)?;
        }
        for name in SYMBOLS
            .iter()
            .filter(|name| name.starts_with(&prefix.prefix))
        {
            let insert = format!("\\{name}");
            add(&format!("builtin:{name}"), &insert, prefix.replace, &insert)?;
        }
        if "begin".starts_with(&prefix.prefix) {
            for name in ENVIRONMENTS {
                let insert = format!("\\begin{{{name}}}\n & \\\\\n & \n\\end{{{name}}}");
                add(
                    &format!("environment:{name}"),
                    name,
                    prefix.replace,
                    &insert,
                )?;
            }
        }
        if "end".starts_with(&prefix.prefix) {
            if let Some(name) = &context.active_environment {
                let insert = format!("\\end{{{name}}}");
                add(
                    &format!("close-environment:{name}"),
                    &insert,
                    prefix.replace,
                    &insert,
                )?;
            }
        }
    } else {
        let at_cursor = FormulaSourceRange {
            start: cursor,
            end: cursor,
        };
        let slot = &context.nodes[*context.owner_path.last().expect("root path")];
        if source[slot.content.start..slot.content.end]
            .trim()
            .is_empty()
        {
            let scaffolds: &[(&str, &str)] = match context.slot {
                FormulaSlot::Superscript => &[
                    ("power-two", "2"),
                    ("power-n", "n"),
                    ("fraction", "\\frac{}{}"),
                ],
                FormulaSlot::Subscript => {
                    &[("index-i", "i"), ("index-n", "n"), ("index-zero", "0")]
                }
                FormulaSlot::RadicalIndex => {
                    &[("root-two", "2"), ("root-three", "3"), ("root-n", "n")]
                }
                FormulaSlot::FractionNumerator
                | FormulaSlot::FractionDenominator
                | FormulaSlot::RadicalBody
                | FormulaSlot::BinomialUpper
                | FormulaSlot::BinomialLower
                | FormulaSlot::MathArgument
                | FormulaSlot::MatrixCell => &[("fraction", "\\frac{}{}"), ("radical", "\\sqrt{}")],
                _ => &[],
            };
            for (id, insert) in scaffolds {
                add(&format!("slot:{id}"), insert, at_cursor, insert)?;
            }
        }
        if context.cell.is_some() {
            add("structure:cell", "&", at_cursor, "&")?;
            add("structure:row", "\\\\", at_cursor, "\\\\")?;
        }
        if cursor == source.len() {
            for owner in context.owner_path.iter().rev() {
                let node = &context.nodes[*owner];
                if node.closed {
                    continue;
                }
                if node.kind == FormulaEditKind::Group {
                    let closing = if source[node.range.start..].starts_with('[') {
                        "]"
                    } else {
                        "}"
                    };
                    add("structure:close-group", closing, at_cursor, closing)?;
                    break;
                }
                if let FormulaEditKind::Environment { name } = &node.kind {
                    let insert = format!("\\end{{{name}}}");
                    add(
                        &format!("close-environment:{name}"),
                        &insert,
                        at_cursor,
                        &insert,
                    )?;
                    break;
                }
            }
        }
    }
    Ok(FormulaCompletionResult {
        context,
        candidates,
    })
}

/// Apply an explicitly selected Core candidate to the same source/revision.
/// Recomputing the edit also prevents a modified response from widening its range.
pub fn apply_formula_completion(
    source: &str,
    cursor: usize,
    revision: u64,
    result: &FormulaCompletionResult,
    candidate_id: &str,
) -> EditResult<AppliedFormulaCompletion> {
    apply_formula_candidate(
        source,
        cursor,
        revision,
        &result.context.snapshot(),
        candidate_id,
    )
}

/// Apply a candidate using a concise snapshot, re-generating the canonical edit.
pub fn apply_formula_candidate(
    source: &str,
    cursor: usize,
    revision: u64,
    snapshot: &FormulaCompletionSnapshot,
    candidate_id: &str,
) -> EditResult<AppliedFormulaCompletion> {
    if source.len() > MAX_BYTES {
        return Err(FormulaCompletionError::LimitExceeded);
    }
    if cursor > source.len() || !source.is_char_boundary(cursor) {
        return Err(FormulaCompletionError::InvalidCursor);
    }
    if snapshot.schema_version != FORMULA_COMPLETION_VERSION
        || snapshot.revision != revision
        || snapshot.cursor != cursor
        || snapshot.source_sha256 != format!("{:x}", Sha256::digest(source.as_bytes()))
    {
        return Err(FormulaCompletionError::StaleContext);
    }
    let current = complete_formula(source, cursor, revision)?;
    let selected = current
        .candidates
        .iter()
        .find(|candidate| candidate.id == candidate_id)
        .ok_or(FormulaCompletionError::UnknownCandidate)?;
    apply_selected(source, selected)
}

pub(super) fn apply_selected(
    source: &str,
    selected: &FormulaCompletionCandidate,
) -> EditResult<AppliedFormulaCompletion> {
    let source = splice(source, selected.replace, &selected.insert)?;
    let tab_stops: Vec<_> = selected
        .tab_stops
        .iter()
        .map(|range| FormulaSourceRange {
            start: selected.replace.start + range.start,
            end: selected.replace.start + range.end,
        })
        .collect();
    let cursor = tab_stops
        .first()
        .map_or(selected.replace.start + selected.insert.len(), |range| {
            range.start
        });
    Ok(AppliedFormulaCompletion {
        source,
        cursor,
        tab_stops,
    })
}
