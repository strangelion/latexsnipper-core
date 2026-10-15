//! Finite presentation-fence ownership, separate from TeX argument grouping.
use super::*;

const PAIRS: &[(&str, &str)] = &[
    ("(", ")"),
    ("[", "]"),
    ("\\{", "\\}"),
    ("\\langle", "\\rangle"),
    ("\\lceil", "\\rceil"),
    ("\\lfloor", "\\rfloor"),
    ("\\lvert", "\\rvert"),
    ("\\lVert", "\\rVert"),
    ("⟨", "⟩"),
    ("⌈", "⌉"),
    ("⌊", "⌋"),
];
const SYMMETRIC: &[&str] = &["|", "\\vert", "\\Vert", "\\|", ".", "/", "\\backslash"];

fn control(source: &str, name: &str) -> bool {
    source
        .strip_prefix('\\')
        .and_then(|source| source.strip_prefix(name))
        .is_some_and(|rest| !rest.starts_with(|ch: char| ch.is_ascii_alphabetic()))
}

fn token(source: &str) -> Option<&str> {
    let first = source.chars().next()?;
    let size = if first == '\\' {
        let rest = source.get(1..)?;
        if rest.starts_with(|ch: char| ch.is_ascii_alphabetic()) {
            1 + rest.bytes().take_while(u8::is_ascii_alphabetic).count()
        } else {
            1 + rest.chars().next()?.len_utf8()
        }
    } else {
        first.len_utf8()
    };
    let value = &source[..size];
    (PAIRS
        .iter()
        .any(|(left, right)| value == *left || value == *right)
        || SYMMETRIC.contains(&value))
    .then_some(value)
}

fn matching(opening: &str) -> Option<&'static str> {
    PAIRS
        .iter()
        .find_map(|(left, right)| (*left == opening).then_some(*right))
        .or_else(|| SYMMETRIC.iter().find(|value| **value == opening).copied())
}

pub(super) fn builtin_command(name: &str) -> bool {
    PAIRS
        .iter()
        .flat_map(|(left, right)| [*left, *right])
        .chain(SYMMETRIC.iter().copied())
        .any(|value| value.strip_prefix('\\') == Some(name))
}

pub(super) fn opening_command(name: &str) -> bool {
    PAIRS
        .iter()
        .any(|(left, _)| left.strip_prefix('\\') == Some(name))
}

impl EditingParser<'_> {
    fn fence_marker(&mut self, parent: usize, depth: usize) -> EditResult<()> {
        if self.ch() == Some('\\') {
            self.command(parent, depth, false, false)
        } else {
            self.leaf(parent)
        }
    }

    fn fence_boundary(&self, mut parent: usize, expected: &str) -> bool {
        if self.ch() == Some('}')
            || self.ch() == Some('&')
            || self.source[self.pos..].starts_with("\\\\")
            || self.end_environment().is_some()
        {
            return true;
        }
        // An optional argument's ] belongs to that lexical owner, unless a
        // nested square fence expects it. Stop at the nearest lexical group.
        if self.ch() == Some(']') && expected != "]" {
            loop {
                let node = &self.nodes[parent];
                if node.kind == FormulaEditKind::Group {
                    let source = &self.source[node.range.start..];
                    if source.starts_with('{') {
                        return false;
                    }
                    if source.starts_with('[') && node.slot != Some(FormulaSlot::Group) {
                        return true;
                    }
                }
                let Some(next) = node.parent else { break };
                parent = next;
            }
        }
        false
    }

    pub(super) fn fence(&mut self, parent: usize, depth: usize) -> EditResult<bool> {
        let start = self.pos;
        let rest = &self.source[start..];
        let scalable = control(rest, "left");
        let opening = token(rest);
        let fixed = opening.and_then(|opening| {
            PAIRS
                .iter()
                .find(|(left, _)| *left == opening)
                .map(|(_, right)| *right)
        });
        if !scalable && fixed.is_none() {
            if control(rest, "right")
                || opening.is_some_and(|value| PAIRS.iter().any(|(_, right)| *right == value))
            {
                self.fence_marker(parent, depth)?;
                if control(rest, "right") {
                    self.trivia(parent)?;
                    if token(&self.source[self.pos..]).is_some() {
                        self.fence_marker(parent, depth)?;
                    }
                }
                self.diagnostic(
                    "UNMATCHED_DELIMITER",
                    FormulaSourceRange {
                        start,
                        end: self.pos,
                    },
                );
                return Ok(true);
            }
            return Ok(false);
        }
        let id = self.add(
            Some(parent),
            FormulaEditKind::Group,
            Some(FormulaSlot::Group),
            start,
        )?;
        self.fence_marker(id, depth)?;
        let expected = if scalable {
            self.trivia(id)?;
            let Some(opening) = token(&self.source[self.pos..]) else {
                self.nodes[id].content.start = self.pos;
                self.finish(id, self.pos, false);
                self.diagnostic("MISSING_DELIMITER", self.nodes[id].range);
                return Ok(true);
            };
            let expected = matching(opening).unwrap_or(".");
            self.fence_marker(id, depth)?;
            expected
        } else {
            fixed.expect("fixed fence")
        };
        self.nodes[id].content.start = self.pos;
        let mut body_end;
        let closed = loop {
            body_end = self.pos;
            if self.ch().is_none() || self.fence_boundary(id, expected) {
                break false;
            }
            let rest = &self.source[self.pos..];
            if control(rest, "right") {
                if !scalable {
                    break false;
                }
                self.fence_marker(id, depth)?;
                self.trivia(id)?;
                if token(&self.source[self.pos..]).is_some() {
                    self.fence_marker(id, depth)?;
                    break true;
                }
                self.diagnostic(
                    "MISSING_DELIMITER",
                    FormulaSourceRange {
                        start: body_end,
                        end: self.pos,
                    },
                );
                break false;
            }
            if scalable && control(rest, "middle") {
                self.fence_marker(id, depth)?;
                self.trivia(id)?;
                if token(&self.source[self.pos..]).is_some() {
                    self.fence_marker(id, depth)?;
                } else {
                    self.diagnostic(
                        "MISSING_DELIMITER",
                        FormulaSourceRange {
                            start: body_end,
                            end: self.pos,
                        },
                    );
                }
                continue;
            }
            if !scalable {
                if let Some(value) = token(rest) {
                    // Ordinary round/square intervals may have mixed endpoints.
                    let interval = matches!(expected, ")" | "]") && matches!(value, ")" | "]");
                    if value == expected || interval {
                        self.fence_marker(id, depth)?;
                        break true;
                    }
                    if PAIRS.iter().any(|(_, right)| *right == value) {
                        break false;
                    }
                }
            }
            self.atom(id, depth + 1, false)?;
        };
        self.finish(id, body_end, closed);
        if !closed {
            self.diagnostic("UNCLOSED_DELIMITER", self.nodes[id].range);
        }
        Ok(true)
    }
}

fn closing_hint(source: &str, node: &FormulaEditNode) -> Option<String> {
    if node.slot != Some(FormulaSlot::Group) {
        return None;
    }
    let rest = &source[node.range.start..];
    if control(rest, "left") {
        // Recover the opening marker without guessing private macro expansion.
        let mut tail = &rest[5..];
        loop {
            tail = tail.trim_start_matches(char::is_whitespace);
            if !tail.starts_with('%') {
                break;
            }
            tail = tail.find(['\r', '\n']).map_or("", |end| &tail[end..]);
        }
        let opening = token(tail)?;
        let expected = matching(opening).unwrap_or(".");
        let pending = &source[node.content.end..node.range.end];
        Some(if control(pending, "right") {
            expected.into()
        } else {
            format!("\\right{expected}")
        })
    } else {
        let opening = token(rest)?;
        PAIRS
            .iter()
            .find_map(|(left, right)| (*left == opening).then(|| (*right).to_string()))
    }
}

pub(super) fn is_fence(source: &str, node: &FormulaEditNode) -> bool {
    node.kind == FormulaEditKind::Group
        && node.slot == Some(FormulaSlot::Group)
        && (control(&source[node.range.start..], "left")
            || token(&source[node.range.start..])
                .is_some_and(|value| PAIRS.iter().any(|(left, _)| *left == value)))
}

pub(super) fn owns_closing_header(source: &str, node: &FormulaEditNode, cursor: usize) -> bool {
    !node.closed
        && node.kind == FormulaEditKind::Group
        && node.content.end <= cursor
        && cursor <= node.range.end
        && control(&source[node.range.start..], "left")
        && control(&source[node.content.end..node.range.end], "right")
}

pub(super) fn prefix_closure(
    source: &str,
    context: &FormulaCompletionContext,
) -> Option<(FormulaSourceRange, String)> {
    if context.cursor != source.len() {
        return None;
    }
    let prefix = context.command_prefix.as_ref()?;
    for owner in context.owner_path.iter().rev() {
        let node = &context.nodes[*owner];
        if node.kind == FormulaEditKind::Group && !node.closed {
            let hint = closing_hint(source, node)?;
            return hint
                .strip_prefix('\\')
                .is_some_and(|hint| hint.starts_with(&prefix.prefix))
                .then_some((prefix.replace, hint));
        }
        if node.kind == FormulaEditKind::Argument
            || matches!(node.kind, FormulaEditKind::Script { .. })
        {
            return None;
        }
    }
    None
}

pub(super) fn closure(
    source: &str,
    context: &FormulaCompletionContext,
) -> Option<(String, String)> {
    if context.cursor != source.len() {
        return None;
    }
    for owner in context.owner_path.iter().rev() {
        let node = &context.nodes[*owner];
        if node.closed {
            continue;
        }
        if node.kind == FormulaEditKind::Argument
            || matches!(node.kind, FormulaEditKind::Script { .. })
        {
            return None;
        }
        if node.kind == FormulaEditKind::Group {
            let presentation = closing_hint(source, node);
            if let Some(prefix) = &context.command_prefix {
                if presentation.is_none()
                    || (prefix.replace.end > node.content.start
                        && prefix.replace.start < node.content.end)
                {
                    return None;
                }
            }
            return presentation
                .map(|insert| ("structure:close-delimiter".into(), insert))
                .or_else(|| {
                    source[node.range.start..]
                        .starts_with('{')
                        .then(|| ("structure:close-group".into(), "}".into()))
                        .or_else(|| {
                            source[node.range.start..]
                                .starts_with('[')
                                .then(|| ("structure:close-group".into(), "]".into()))
                        })
                });
        }
        if let FormulaEditKind::Environment { name } = &node.kind {
            if context.command_prefix.is_none() {
                return Some((
                    format!("close-environment:{name}"),
                    format!("\\end{{{name}}}"),
                ));
            }
        }
    }
    None
}
