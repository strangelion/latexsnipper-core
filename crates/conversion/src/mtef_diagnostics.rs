//! Experimental reference and slot diagnostics, not MTEF semantic conversion.
//!
//! Raw framing remains a separate, unchanged interface. This pass checks only
//! documented definition order and a finite set of slot shapes. An empty issue
//! list is never proof of mathematical validity, layout fidelity or conversion.

use crate::mtef_readonly::{inspect_mtef_v5, FieldValue, Inspection, Record};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    IncompleteFraming,
    FutureSemantics,
    UnresolvedReference,
    UnsupportedTypeface,
    UnsupportedTemplate,
    MissingCharacterIdentity,
    UnmappedMtCode,
    SlotCountMismatch,
    UnsupportedSlotObject,
    EmptyEquation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub kind: IssueKind,
    /// Index in `inspection.records`, absent for a whole-stream issue.
    pub record_index: Option<usize>,
    pub offset: usize,
    pub field: &'static str,
    pub value: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosedInspection<'a> {
    pub inspection: Inspection<'a>,
    pub issues: Vec<Issue>,
}

/// Inspect unchanged raw v5 bytes, then diagnose finite reference/slot rules.
/// No font loading, MTCode casting, default-style guessing, OLE activation or
/// registered import is performed. Framing failures keep their original report.
pub fn diagnose_mtef_v5(source: &[u8]) -> DiagnosedInspection<'_> {
    let inspection = inspect_mtef_v5(source);
    let issues = diagnose_inspection(&inspection);
    DiagnosedInspection { inspection, issues }
}

// The batch layer already owns a bounded parser-produced report. Do not parse
// or copy identical raw streams again merely to obtain their diagnostics.
pub(crate) fn diagnose_inspection(inspection: &Inspection<'_>) -> Vec<Issue> {
    let mut issues = Vec::new();
    if !inspection.complete {
        issues.push(Issue {
            kind: IssueKind::IncompleteFraming,
            record_index: None,
            offset: inspection.consumed,
            field: "framing",
            value: None,
        });
        return issues;
    }

    // Definition indices are stream-global, not reset at an END. Four encoding
    // indices are predefined; custom ENCODING_DEF indices begin at five.
    let mut encodings = 4;
    let mut fonts = 0;
    let mut colors = 0;
    let mut children = vec![Vec::new(); inspection.records.len()];
    let mut parents: Vec<usize> = Vec::new();
    let mut has_structure = false;
    for (index, record) in inspection.records.iter().enumerate() {
        parents.truncate(record.depth);
        if let Some(&parent) = parents.last() {
            children[parent].push(index);
        }
        parents.push(index);
        has_structure |= matches!(record.record_type, 1..=5);
        match record.record_type {
            3 if !matches!(unsigned(record, "selector"), Some(10 | 11)) => issue(
                &mut issues,
                index,
                record,
                IssueKind::UnsupportedTemplate,
                "selector",
                unsigned(record, "selector").map(i32::from),
            ),
            17 => {
                reference(&mut issues, index, record, "encoding_index", encodings);
                fonts += 1;
            }
            19 => encodings += 1,
            16 => colors += 1,
            8 => reference(&mut issues, index, record, "font_index", fonts),
            15 => reference(&mut issues, index, record, "color_index", colors),
            18 => {
                if let Some(FieldValue::Styles(styles)) = field(record, "styles") {
                    for style in styles {
                        // Zero is explicitly unused, not an invalid font reference.
                        if usize::from(style.font_index) > fonts {
                            issue(
                                &mut issues,
                                index,
                                record,
                                IssueKind::UnresolvedReference,
                                "styles.font_index",
                                Some(i32::from(style.font_index)),
                            );
                        }
                    }
                }
            }
            2 => {
                if let Some(FieldValue::Signed(typeface)) = field(record, "typeface") {
                    // Positive built-in styles may use omitted/default prefs.
                    // Explicit negative fonts require a separately reviewed map.
                    if !matches!(typeface, 1..=12 | 22..=24) {
                        issue(
                            &mut issues,
                            index,
                            record,
                            IssueKind::UnsupportedTypeface,
                            "typeface",
                            Some(*typeface),
                        );
                    }
                }
                if let Some(FieldValue::Unsigned(code)) = field(record, "mtcode") {
                    issue(
                        &mut issues,
                        index,
                        record,
                        IssueKind::UnmappedMtCode,
                        "mtcode",
                        Some(i32::from(*code)),
                    );
                } else {
                    issue(
                        &mut issues,
                        index,
                        record,
                        IssueKind::MissingCharacterIdentity,
                        "mtcode",
                        None,
                    );
                }
            }
            100..=255 => issue(
                &mut issues,
                index,
                record,
                IssueKind::FutureSemantics,
                "opaque_payload",
                None,
            ),
            _ => {}
        }
    }
    for (index, record) in inspection.records.iter().enumerate() {
        let expected = match record.record_type {
            5 => unsigned(record, "rows")
                .zip(unsigned(record, "columns"))
                .map(|(rows, columns)| usize::from(rows) * usize::from(columns)),
            // Fraction and root slots include NULL lines. Other templates need
            // variation-specific review and are not guessed from child count.
            3 if matches!(unsigned(record, "selector"), Some(10 | 11)) => Some(2),
            _ => None,
        };
        if let Some(expected) = expected {
            let direct = &children[index];
            let actual = direct
                .iter()
                .filter(|&&child| inspection.records[child].record_type == 1)
                .count();
            if actual != expected {
                issue(
                    &mut issues,
                    index,
                    record,
                    IssueKind::SlotCountMismatch,
                    "line_slots",
                    Some(actual as i32),
                );
            }
            for &child in direct {
                let slot = &inspection.records[child];
                if matches!(slot.record_type, 2..=6) {
                    issue(
                        &mut issues,
                        child,
                        slot,
                        IssueKind::UnsupportedSlotObject,
                        "line_slots",
                        Some(i32::from(slot.record_type)),
                    );
                }
            }
        }
    }
    if !has_structure {
        issues.push(Issue {
            kind: IssueKind::EmptyEquation,
            record_index: None,
            offset: inspection
                .header
                .as_ref()
                .map_or(0, |header| header.span.end),
            field: "equation",
            value: None,
        });
    }
    issues
}

fn field<'a>(record: &'a Record, name: &str) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| &field.value)
}

fn unsigned(record: &Record, name: &str) -> Option<u16> {
    match field(record, name) {
        Some(FieldValue::Unsigned(value)) => Some(*value),
        _ => None,
    }
}

fn reference(
    issues: &mut Vec<Issue>,
    index: usize,
    record: &Record,
    name: &'static str,
    count: usize,
) {
    if let Some(value) = unsigned(record, name) {
        if value == 0 || usize::from(value) > count {
            issue(
                issues,
                index,
                record,
                IssueKind::UnresolvedReference,
                name,
                Some(i32::from(value)),
            );
        }
    }
}

fn issue(
    issues: &mut Vec<Issue>,
    index: usize,
    record: &Record,
    kind: IssueKind,
    field: &'static str,
    value: Option<i32>,
) {
    issues.push(Issue {
        kind,
        record_index: Some(index),
        offset: record.span.start,
        field,
        value,
    });
}
