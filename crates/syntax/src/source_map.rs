use std::collections::HashMap;

use latexsnipper_ast::{Document, Span};

/// Maps caller-managed `SourceInfo.stable_id` values to UTF-8 byte ranges.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceMap {
    by_stable_id: HashMap<String, Span>,
    by_span: Vec<(Span, String)>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, stable_id: impl Into<String>, span: Span) {
        let stable_id = stable_id.into();
        self.by_stable_id.insert(stable_id.clone(), span);
        self.by_span.push((span, stable_id));
    }

    /// Rebuild a map from the source-bearing block identities in a document.
    pub fn from_document(document: &Document) -> Self {
        let mut map = Self::new();
        for block in document.all_blocks() {
            if let Some(source) = block.source() {
                if let (Some(stable_id), Some(span)) = (&source.stable_id, source.span) {
                    map.insert(stable_id.clone(), span);
                }
            }
        }
        map
    }

    pub fn span_for(&self, stable_id: &str) -> Option<Span> {
        self.by_stable_id.get(stable_id).copied()
    }

    /// Returns identities whose source ranges intersect `span`.
    pub fn candidates_for(&self, span: Span) -> Vec<&str> {
        self.by_span
            .iter()
            .filter(|(candidate, _)| candidate.start < span.end && span.start < candidate.end)
            .map(|(_, stable_id)| stable_id.as_str())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.by_stable_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_stable_id.is_empty()
    }

    /// Update mapped spans after replacing `replaced` with text of a different
    /// byte length. Callers must ensure the replacement is structurally safe.
    pub fn apply_text_replacement(&mut self, replaced: Span, replacement_len: usize) {
        let delta = replacement_len as isize - replaced.len() as isize;
        for span in self.by_stable_id.values_mut() {
            adjust_span(span, replaced, delta);
        }
        for (span, _) in &mut self.by_span {
            adjust_span(span, replaced, delta);
        }
    }
}

fn adjust_span(span: &mut Span, replaced: Span, delta: isize) {
    if span.end <= replaced.start {
        return;
    }
    if span.start >= replaced.end {
        span.start = offset(span.start, delta);
        span.end = offset(span.end, delta);
        return;
    }
    span.end = offset(span.end, delta);
}

fn offset(value: usize, delta: isize) -> usize {
    if delta >= 0 {
        value.saturating_add(delta as usize)
    } else {
        value.saturating_sub(delta.unsigned_abs())
    }
}

/// Output of a source-aware parser.
#[derive(Debug, Clone)]
pub struct ParsedDocument {
    pub document: Document,
    pub source_map: SourceMap,
}
