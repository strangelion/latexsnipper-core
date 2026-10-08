use latexsnipper_conversion::mtef_diagnostics::{diagnose_mtef_v5, IssueKind};
use latexsnipper_conversion::mtef_readonly::{inspect_mtef_v5, MAX_INPUT_BYTES, MAX_RECORDS};

const HEADER: &[u8] = &[5, 1, 0, 7, 0, b'p', b'i', b'l', b'o', b't', 0, 0];

fn stream(body: &[u8]) -> Vec<u8> {
    [HEADER, body].concat()
}

fn authored(name: &str) -> Vec<u8> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/mtef-readonly-v1.json"
    ))
    .unwrap();
    let row = fixture["accepted"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .unwrap();
    let body: Vec<_> = row["body_hex"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect();
    stream(&body)
}

fn kinds(bytes: &[u8]) -> Vec<IssueKind> {
    diagnose_mtef_v5(bytes)
        .issues
        .into_iter()
        .map(|issue| issue.kind)
        .collect()
}

#[test]
fn raw_framing_and_original_source_are_unchanged() {
    for name in [
        "variable",
        "fraction-slots",
        "root-and-null-slot",
        "matrix-four-slots",
        "definitions-and-preferences",
        "ruler-and-pile",
    ] {
        let bytes = authored(name);
        let before = bytes.clone();
        let report = diagnose_mtef_v5(&bytes);
        assert_eq!(report.inspection, inspect_mtef_v5(&bytes));
        assert_eq!(report.inspection.source.as_ptr(), bytes.as_ptr());
        assert_eq!(bytes, before);
        assert!(report
            .issues
            .iter()
            .all(|issue| issue.offset <= bytes.len()));
    }
}

#[test]
fn predefined_encodings_and_first_custom_encoding_are_one_based() {
    for encoding in 1..=4 {
        let bytes = stream(&[17, encoding, b'f', 0, 1, 1, 0]);
        assert!(!kinds(&bytes).contains(&IssueKind::UnresolvedReference));
    }
    let custom = stream(&[19, b'e', 0, 17, 5, b'f', 0, 1, 1, 0]);
    assert!(!kinds(&custom).contains(&IssueKind::UnresolvedReference));
    for encoding in [0, 5] {
        let bytes = stream(&[17, encoding, b'f', 0, 1, 1, 0]);
        let report = diagnose_mtef_v5(&bytes);
        let issue = report
            .issues
            .iter()
            .find(|issue| issue.kind == IssueKind::UnresolvedReference)
            .unwrap();
        assert_eq!(issue.field, "encoding_index");
        assert_eq!(issue.value, Some(i32::from(encoding)));
    }
}

#[test]
fn later_definitions_do_not_retroactively_resolve_references() {
    let body = [
        17, 5, b'f', 0, 19, b'e', 0, 8, 2, 0, 17, 1, b'g', 0, 1, 1, 0,
    ];
    let bytes = stream(&body);
    let report = diagnose_mtef_v5(&bytes);
    let missing: Vec<_> = report
        .issues
        .iter()
        .filter(|issue| issue.kind == IssueKind::UnresolvedReference)
        .map(|issue| issue.field)
        .collect();
    assert_eq!(missing, ["encoding_index", "font_index"]);
    // A nested definition remains stream-global after that line's END.
    let bytes = stream(&[1, 0, 17, 1, b'f', 0, 0, 8, 1, 0, 0]);
    assert!(!kinds(&bytes).contains(&IssueKind::UnresolvedReference));
}

#[test]
fn equation_preferences_allow_unused_zero_and_omitted_defaults() {
    let unused = stream(&[18, 0, 0, 0, 1, 0, 1, 0, 2, 0, 131, b'x', 0, 0, 0]);
    assert!(!kinds(&unused).contains(&IssueKind::UnresolvedReference));
    assert!(!kinds(&unused).contains(&IssueKind::UnsupportedTypeface));
    let missing = stream(&[18, 0, 0, 0, 1, 1, 0, 1, 1, 0]);
    let report = diagnose_mtef_v5(&missing);
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.field == "styles.font_index" && issue.value == Some(1)));
    let defined = stream(&[17, 1, b'f', 0, 18, 0, 0, 0, 1, 1, 0, 1, 1, 0]);
    assert!(!kinds(&defined).contains(&IssueKind::UnresolvedReference));
}

#[test]
fn colors_require_prior_matching_definitions() {
    let valid = stream(&[16, 0, 0, 0, 0, 0, 0, 0, 15, 1, 1, 1, 0]);
    assert!(!kinds(&valid).contains(&IssueKind::UnresolvedReference));
    for index in [0, 1] {
        let missing = stream(&[15, index, 16, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0]);
        assert!(kinds(&missing).contains(&IssueKind::UnresolvedReference));
    }
}

#[test]
fn mtcode_and_explicit_font_positions_never_become_guessed_characters() {
    for typeface in [0, 13, -1, -128] {
        let bytes = stream(&[2, 0, (typeface + 128) as u8, 0x00, 0xe0, 0]);
        let issue_kinds = kinds(&bytes);
        assert!(issue_kinds.contains(&IssueKind::UnsupportedTypeface));
        assert!(issue_kinds.contains(&IssueKind::UnmappedMtCode));
    }
    let encoded_only = stream(&[2, 0x24, 131, b'x', 0]);
    assert!(kinds(&encoded_only).contains(&IssueKind::MissingCharacterIdentity));
    let no_identity = stream(&[2, 0x20, 131, 0]);
    assert!(kinds(&no_identity).contains(&IssueKind::MissingCharacterIdentity));
}

#[test]
fn direct_line_slots_include_null_but_not_nested_lines() {
    for name in ["fraction-slots", "root-and-null-slot", "matrix-four-slots"] {
        assert!(!kinds(&authored(name)).contains(&IssueKind::SlotCountMismatch));
    }
    let nested = stream(&[3, 0, 11, 0, 0, 1, 0, 1, 1, 0, 0, 0]);
    assert!(kinds(&nested).contains(&IssueKind::SlotCountMismatch));
    let nulls = stream(&[3, 0, 11, 0, 0, 1, 1, 1, 1, 0, 0]);
    assert!(!kinds(&nulls).contains(&IssueKind::SlotCountMismatch));
    let wrong_kind = stream(&[3, 0, 11, 0, 0, 2, 0, 131, b'x', 0, 1, 1, 1, 1, 0, 0]);
    assert!(kinds(&wrong_kind).contains(&IssueKind::UnsupportedSlotObject));
}

#[test]
fn matrix_dimensions_do_not_accept_fewer_or_extra_slots() {
    for actual in [3, 5] {
        let mut body = vec![5, 0, 4, 2, 1, 2, 2, 0, 0];
        for _ in 0..actual {
            body.extend([1, 1]);
        }
        body.extend([0, 0]);
        assert!(kinds(&stream(&body)).contains(&IssueKind::SlotCountMismatch));
    }
}

#[test]
fn unknown_templates_and_future_records_are_not_semantically_approved() {
    let future = stream(&[100, 1, 0, 0]);
    assert!(kinds(&future).contains(&IssueKind::FutureSemantics));
    assert!(kinds(&future).contains(&IssueKind::EmptyEquation));
    assert!(kinds(&stream(&[0])).contains(&IssueKind::EmptyEquation));
    // Other template selectors are not assigned a guessed operand count.
    let unknown = stream(&[3, 0, 99, 0, 0, 0, 0]);
    assert!(!kinds(&unknown).contains(&IssueKind::SlotCountMismatch));
    assert!(kinds(&unknown).contains(&IssueKind::UnsupportedTemplate));
}

#[test]
fn truncation_mutation_and_limits_preserve_bounded_reports() {
    let bytes = authored("definitions-and-preferences");
    for end in 0..bytes.len() {
        let report = diagnose_mtef_v5(&bytes[..end]);
        assert!(!report.inspection.complete);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].kind, IssueKind::IncompleteFraming);
        assert_eq!(report.inspection.source, &bytes[..end]);
    }
    for offset in 0..bytes.len() {
        for replacement in [0, 1, 128, 255] {
            let mut changed = bytes.clone();
            changed[offset] = replacement;
            let report = diagnose_mtef_v5(&changed);
            assert_eq!(report.inspection.source, changed);
            assert!(report
                .issues
                .iter()
                .all(|issue| issue.offset <= changed.len()));
            assert!(report.inspection.records.len() <= MAX_RECORDS);
        }
    }
    let oversized = vec![0; MAX_INPUT_BYTES + 1];
    assert_eq!(kinds(&oversized), [IssueKind::IncompleteFraming]);
}
