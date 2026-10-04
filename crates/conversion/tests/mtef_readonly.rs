use latexsnipper_conversion::mtef_readonly::{
    inspect_mtef_v5, DiagnosticKind, Dimension, FieldValue, Inspection, Record, MAX_ARRAY_ITEMS,
    MAX_DEPTH, MAX_INPUT_BYTES, MAX_RECORDS, MAX_STRING_BYTES,
};
use serde_json::Value;

const HEADER: &[u8] = &[5, 1, 0, 7, 0, b'p', b'i', b'l', b'o', b't', 0, 0];

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|item| u8::from_str_radix(item, 16).unwrap())
        .collect()
}

fn stream(body: &[u8]) -> Vec<u8> {
    [HEADER, body].concat()
}

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/mtef-readonly-v1.json"
    ))
    .unwrap()
}

fn field<'a>(record: &'a Record, name: &str) -> &'a FieldValue {
    &record
        .fields
        .iter()
        .find(|field| field.name == name)
        .unwrap()
        .value
}

fn record<'a>(inspection: &'a Inspection<'_>, tag: u8) -> &'a Record {
    inspection
        .records
        .iter()
        .find(|record| record.record_type == tag)
        .unwrap()
}

fn assert_spans(report: &Inspection<'_>) {
    assert!(report.consumed <= report.source.len());
    let mut previous_start = 0;
    for (index, entry) in report.records.iter().enumerate() {
        assert!(entry.span.start >= previous_start);
        assert!(entry.span.start < entry.span.end);
        assert!(entry.span.end <= report.consumed);
        assert_eq!(report.source[entry.span.start], entry.record_type);
        if entry.depth > 0 {
            let parent = report.records[..index]
                .iter()
                .rev()
                .find(|parent| parent.depth < entry.depth)
                .unwrap();
            assert_eq!(parent.depth + 1, entry.depth);
            assert!(parent.span.start < entry.span.start && parent.span.end >= entry.span.end);
        }
        for field in &entry.fields {
            if let FieldValue::Bytes(span) = &field.value {
                assert!(span.start >= entry.span.start && span.end <= entry.span.end);
            }
        }
        previous_start = entry.span.start;
    }
    for diagnostic in &report.diagnostics {
        assert!(diagnostic.offset <= report.source.len());
    }
}

#[test]
fn authored_versioned_fixtures_preserve_raw_bytes_and_known_record_boundaries() {
    let fixtures = fixture();
    assert_eq!(fixtures["schemaVersion"], 1);
    assert!(fixtures["provenance"]
        .as_str()
        .unwrap()
        .contains("synthetic"));
    assert_eq!(hex(fixtures["header_hex"].as_str().unwrap()), HEADER);
    for row in fixtures["accepted"].as_array().unwrap() {
        let bytes = stream(&hex(row["body_hex"].as_str().unwrap()));
        let before = bytes.clone();
        let report = inspect_mtef_v5(&bytes);
        assert!(report.complete, "{}: {:?}", row["name"], report.diagnostics);
        assert!(report.diagnostics.is_empty());
        assert!(report.records.iter().all(|entry| entry.complete));
        let tags: Vec<_> = report
            .records
            .iter()
            .map(|entry| u64::from(entry.record_type))
            .collect();
        let expected: Vec<_> = row["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tag| tag.as_u64().unwrap())
            .collect();
        assert_eq!(tags, expected, "{}", row["name"]);
        assert_eq!(report.source, before);
        assert_eq!(report.source.as_ptr(), bytes.as_ptr());
        assert_eq!(report.consumed, bytes.len());
        let header = report.header.as_ref().unwrap();
        assert_eq!(&report.source[header.application_key.clone()], b"pilot");
        assert!(!header.inline);
        assert_spans(&report);
    }
}

#[test]
fn malformed_fixtures_stop_with_diagnostic_and_keep_entire_source() {
    for row in fixture()["rejected"].as_array().unwrap() {
        let bytes = stream(&hex(row["body_hex"].as_str().unwrap()));
        let report = inspect_mtef_v5(&bytes);
        assert!(!report.complete, "{}", row["name"]);
        assert_eq!(
            format!("{:?}", report.diagnostics.last().unwrap().kind),
            row["kind"].as_str().unwrap()
        );
        assert_eq!(report.source, bytes);
        assert_spans(&report);
    }
}

#[test]
fn fraction_root_and_matrix_keep_slots_without_claiming_semantic_conversion() {
    let fixtures = fixture();
    let make = |name: &str| {
        let row = fixtures["accepted"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap();
        stream(&hex(row["body_hex"].as_str().unwrap()))
    };
    let bytes = make("fraction-slots");
    let report = inspect_mtef_v5(&bytes);
    assert_eq!(
        field(record(&report, 3), "selector"),
        &FieldValue::Unsigned(11)
    );
    let slots: Vec<_> = report
        .records
        .iter()
        .filter(|entry| entry.record_type == 1 && entry.depth == 2)
        .collect();
    assert_eq!(slots.len(), 2);
    let operands: Vec<_> = report
        .records
        .iter()
        .filter(|entry| entry.record_type == 2)
        .map(|entry| field(entry, "mtcode"))
        .collect();
    assert_eq!(
        operands,
        [&FieldValue::Unsigned(97), &FieldValue::Unsigned(98)]
    );
    let bytes = make("root-and-null-slot");
    let report = inspect_mtef_v5(&bytes);
    assert_eq!(
        field(record(&report, 3), "selector"),
        &FieldValue::Unsigned(10)
    );
    let null_slot = report
        .records
        .iter()
        .find(|entry| entry.record_type == 1 && entry.span.len() == 2)
        .unwrap();
    assert_eq!(field(null_slot, "options"), &FieldValue::Unsigned(1));
    let bytes = make("matrix-four-slots");
    let report = inspect_mtef_v5(&bytes);
    let matrix = record(&report, 5);
    assert_eq!(field(matrix, "rows"), &FieldValue::Unsigned(2));
    assert_eq!(field(matrix, "columns"), &FieldValue::Unsigned(2));
    assert_eq!(
        report
            .records
            .iter()
            .filter(|entry| entry.record_type == 1 && entry.depth == 2)
            .count(),
        4
    );
}

#[test]
fn preferences_keep_decimal_spelling_and_opaque_names() {
    let fixtures = fixture();
    let row = fixtures["accepted"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "definitions-and-preferences")
        .unwrap();
    let bytes = stream(&hex(row["body_hex"].as_str().unwrap()));
    let report = inspect_mtef_v5(&bytes);
    assert!(report.complete);
    let prefs = record(&report, 18);
    assert_eq!(
        field(prefs, "sizes"),
        &FieldValue::Dimensions(vec![
            Dimension {
                units: 2,
                value: "12.5".into()
            },
            Dimension {
                units: 4,
                value: "60".into()
            },
        ])
    );
    assert_eq!(
        field(prefs, "spacing"),
        &FieldValue::Dimensions(vec![Dimension {
            units: 2,
            value: "-0.5".into()
        }])
    );
    if let FieldValue::Bytes(span) = field(record(&report, 17), "font_name") {
        assert_eq!(&report.source[span.clone()], &[b'f', 255]);
    } else {
        panic!("font name must remain bytes");
    }
    if let FieldValue::Styles(styles) = field(prefs, "styles") {
        assert_eq!(styles[0].font_index, 0);
        assert_eq!(styles[0].character_style, None);
        assert_eq!(styles[1].font_index, 1);
        assert_eq!(styles[1].character_style, Some(3));
    } else {
        panic!("missing styles");
    }
}

#[test]
fn variable_width_signed_unsigned_and_template_variation_are_distinct() {
    let bytes = stream(&hex(
        "08 ff 00 01 03 02 20 ff 7f 80 03 00 0b 85 02 00 00 00",
    ));
    let report = inspect_mtef_v5(&bytes);
    assert!(report.complete, "{:?}", report.diagnostics);
    assert_eq!(
        field(record(&report, 8), "font_index"),
        &FieldValue::Unsigned(256)
    );
    assert_eq!(
        field(record(&report, 2), "typeface"),
        &FieldValue::Signed(127)
    );
    assert_eq!(
        field(record(&report, 3), "variation"),
        &FieldValue::Unsigned(0x205)
    );
    assert!(!record(&report, 2)
        .fields
        .iter()
        .any(|field| field.name == "mtcode"));
    let bytes = stream(&hex("02 20 7f 02 20 ff 00 00 00"));
    let report = inspect_mtef_v5(&bytes);
    assert_eq!(
        field(&report.records[0], "typeface"),
        &FieldValue::Signed(-1)
    );
    assert_eq!(
        field(&report.records[1], "typeface"),
        &FieldValue::Signed(-32768)
    );
}

#[test]
fn nudge_embellishment_and_encoded_char_fields_follow_flags_not_generic_options() {
    let bytes = stream(&hex(
        "02 0d 81 7e 83 78 00 41 06 08 80 80 2c 01 70 fe 09 00 02 30 83 34 12 00",
    ));
    let report = inspect_mtef_v5(&bytes);
    assert!(report.complete, "{:?}", report.diagnostics);
    assert_eq!(field(&report.records[0], "nudge_x"), &FieldValue::Signed(1));
    assert_eq!(
        field(&report.records[0], "nudge_y"),
        &FieldValue::Signed(-2)
    );
    assert_eq!(
        field(&report.records[0], "font_position"),
        &FieldValue::Unsigned(65)
    );
    let embellishment = record(&report, 6);
    assert_eq!(embellishment.depth, 1);
    assert_eq!(field(embellishment, "nudge_x"), &FieldValue::Signed(300));
    assert_eq!(field(embellishment, "nudge_y"), &FieldValue::Signed(-400));
    assert_eq!(
        field(embellishment, "embellishment"),
        &FieldValue::Unsigned(9)
    );
    assert_eq!(
        field(&report.records[3], "font_position"),
        &FieldValue::Unsigned(0x1234)
    );
    assert_spans(&report);
}

#[test]
fn size_records_and_rgb_cmyk_keep_integer_units() {
    let bytes = stream(&hex("09 65 80 fe 09 64 02 70 fe 09 01 81 0a 0b 0c 0d 0e 10 00 00 00 e8 03 f4 01 10 07 00 00 00 00 e8 03 00 00 69 6e 6b 00 0f ff 00 01 00"));
    let report = inspect_mtef_v5(&bytes);
    assert!(report.complete, "{:?}", report.diagnostics);
    assert_eq!(
        field(&report.records[0], "negative_point_size"),
        &FieldValue::Signed(-384)
    );
    assert_eq!(
        field(&report.records[1], "size_delta"),
        &FieldValue::Signed(-400)
    );
    assert_eq!(
        field(&report.records[2], "size_delta"),
        &FieldValue::Signed(1)
    );
    for (index, entry) in report.records[3..8].iter().enumerate() {
        assert_eq!(entry.span.len(), 1);
        assert_eq!(
            field(entry, "logical_size"),
            &FieldValue::Unsigned(index as u16)
        );
    }
    assert_eq!(
        field(&report.records[8], "green"),
        &FieldValue::Unsigned(1000)
    );
    assert_eq!(
        field(&report.records[9], "yellow"),
        &FieldValue::Unsigned(1000)
    );
    assert_eq!(
        field(&report.records[10], "color_index"),
        &FieldValue::Unsigned(256)
    );
}

#[test]
fn future_records_are_length_framed_and_warn_even_when_complete() {
    for tag in [100, 101, 255] {
        let bytes = stream(&[tag, 3, 0, 20, 255, 10, 0]);
        let report = inspect_mtef_v5(&bytes);
        assert!(report.complete);
        assert_eq!(report.records.len(), 3);
        assert_eq!(report.diagnostics[0].kind, DiagnosticKind::FutureRecord);
        if let FieldValue::Bytes(span) = field(&report.records[0], "opaque_payload") {
            assert_eq!(&bytes[span.clone()], &[0, 20, 255]);
        } else {
            panic!("missing opaque payload");
        }
    }
    let mut body = vec![100, 255, 0, 1];
    body.extend([20; 256]);
    body.push(0);
    let bytes = stream(&body);
    assert!(inspect_mtef_v5(&bytes).complete);
}

#[test]
fn unknown_unframed_record_never_scans_for_fake_end_or_later_known_tag() {
    let bytes = stream(&[10, 1, 0, 2, 0, 131, b'x', 0, 20, 0, 10, 0]);
    let report = inspect_mtef_v5(&bytes);
    assert!(!report.complete);
    assert_eq!(report.consumed, HEADER.len() + 9);
    assert_eq!(report.diagnostics.last().unwrap().offset, HEADER.len() + 8);
    assert_eq!(report.records.last().unwrap().record_type, 20);
    assert!(!report.records[1].complete);
    assert!(report.records[2].complete);
    assert_eq!(report.source, bytes);
    assert_spans(&report);
}

#[test]
fn every_truncated_prefix_of_authored_complete_fixtures_is_incomplete() {
    for row in fixture()["accepted"].as_array().unwrap() {
        let bytes = stream(&hex(row["body_hex"].as_str().unwrap()));
        for end in 0..bytes.len() {
            let report = inspect_mtef_v5(&bytes[..end]);
            assert!(!report.complete, "{} prefix {}", row["name"], end);
            assert!(!report.diagnostics.is_empty());
            assert_eq!(report.source, &bytes[..end]);
            assert_spans(&report);
        }
    }
}

#[test]
fn versions_containers_reserved_bits_and_missing_ruler_are_not_guessed() {
    for version in [0, 1, 2, 3, 4, 6, 255] {
        let bytes = [version, 0, 0, 0];
        let report = inspect_mtef_v5(&bytes);
        assert!(!report.complete);
        assert!(report.header.is_none());
        assert_eq!(
            report.diagnostics[0].kind,
            DiagnosticKind::UnsupportedVersion
        );
    }
    let ole_container = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
    assert_eq!(
        inspect_mtef_v5(&ole_container).diagnostics[0].kind,
        DiagnosticKind::UnsupportedVersion
    );
    let mut bytes = stream(&[0]);
    bytes[HEADER.len() - 1] = 2;
    assert_eq!(
        inspect_mtef_v5(&bytes).diagnostics[0].kind,
        DiagnosticKind::UnsupportedOptions
    );
    for body in [vec![1, 2, 0], vec![4, 2, 2, 1, 0]] {
        assert_eq!(
            inspect_mtef_v5(&stream(&body)).diagnostics[0].kind,
            DiagnosticKind::Malformed
        );
    }
    let mut bytes = stream(&[0]);
    bytes[HEADER.len() - 1] = 1;
    assert!(inspect_mtef_v5(&bytes).header.unwrap().inline);
}

#[test]
fn input_string_record_depth_and_aggregate_array_limits_are_enforced() {
    let bytes = vec![5; MAX_INPUT_BYTES + 1];
    assert_eq!(
        inspect_mtef_v5(&bytes).diagnostics[0].kind,
        DiagnosticKind::LimitExceeded
    );
    let mut body = vec![19];
    body.extend(vec![b'a'; MAX_STRING_BYTES + 1]);
    body.extend([0, 0]);
    assert_eq!(
        inspect_mtef_v5(&stream(&body)).diagnostics[0].kind,
        DiagnosticKind::LimitExceeded
    );
    let mut body = vec![10; MAX_RECORDS];
    body.push(0);
    assert_eq!(
        inspect_mtef_v5(&stream(&body)).diagnostics[0].kind,
        DiagnosticKind::LimitExceeded
    );
    let mut body = Vec::new();
    for _ in 0..MAX_DEPTH {
        body.extend([1, 0]);
    }
    body.extend(vec![0; MAX_DEPTH + 1]);
    assert_eq!(
        inspect_mtef_v5(&stream(&body)).diagnostics[0].kind,
        DiagnosticKind::LimitExceeded
    );
    let mut body = Vec::new();
    for _ in 0..MAX_ARRAY_ITEMS / 255 + 1 {
        body.extend([7, 255]);
        body.extend(vec![0; 255 * 3]);
    }
    body.push(0);
    assert_eq!(
        inspect_mtef_v5(&stream(&body)).diagnostics[0].kind,
        DiagnosticKind::LimitExceeded
    );
}

#[test]
fn deterministic_byte_mutations_do_not_panic_and_always_preserve_input() {
    for row in fixture()["accepted"].as_array().unwrap() {
        let original = stream(&hex(row["body_hex"].as_str().unwrap()));
        for index in 0..original.len() {
            for value in [0, 1, 5, 20, 100, 128, 255] {
                let mut bytes = original.clone();
                bytes[index] = value;
                let report = inspect_mtef_v5(&bytes);
                assert_eq!(report.source, bytes);
                assert_spans(&report);
                // Complete framing is not a promise that a mutation is a valid equation.
                if !report.complete {
                    assert!(!report.diagnostics.is_empty());
                }
            }
        }
    }
}

#[test]
fn readonly_inspection_does_not_enable_registered_mtef_conversions() {
    use latexsnipper_conversion::{
        CapabilityRegistry, CapabilityTarget, DocumentConverter, FormulaConversionMode,
        FormulaInputFormat, OutputFormat,
    };
    for target in [
        CapabilityTarget::Native,
        CapabilityTarget::Wasm32UnknownUnknown,
    ] {
        for mode in [
            FormulaConversionMode::Strict,
            FormulaConversionMode::BestEffort,
        ] {
            for output in [
                OutputFormat::Latex,
                OutputFormat::MathML,
                OutputFormat::OMML,
            ] {
                let capability = CapabilityRegistry::formula_conversion(
                    FormulaInputFormat::Mtef,
                    output,
                    mode,
                    target,
                );
                assert!(!capability.available);
                assert_eq!(capability.path, "unsupported");
                assert!(capability
                    .unavailable_reason
                    .unwrap()
                    .contains("read-only inspector"));
                assert!(DocumentConverter::convert_formula_string(
                    "not binary MTEF",
                    FormulaInputFormat::Mtef,
                    output,
                    mode
                )
                .is_err());
            }
        }
    }
}

#[test]
fn exact_limits_and_partition_lengths_do_not_reject_smaller_valid_frames() {
    let mut body = vec![10; MAX_RECORDS - 1];
    body.push(0);
    assert!(inspect_mtef_v5(&stream(&body)).complete);
    let mut body = Vec::new();
    for _ in 0..MAX_DEPTH - 1 {
        body.extend([1, 0]);
    }
    body.extend(vec![0; MAX_DEPTH]);
    assert!(inspect_mtef_v5(&stream(&body)).complete);
    let mut body = vec![19];
    body.extend(vec![b'a'; MAX_STRING_BYTES]);
    body.extend([0, 0]);
    assert!(inspect_mtef_v5(&stream(&body)).complete);
    // Four rows have five partition boundaries and therefore two bytes.
    // Slot count/alignment/partition-bit meanings are not semantic validation.
    let bytes = stream(&hex("05 00 04 02 01 04 00 00 00 00 00 00"));
    let report = inspect_mtef_v5(&bytes);
    assert!(report.complete, "{:?}", report.diagnostics);
    if let FieldValue::Bytes(span) = field(record(&report, 5), "row_partitions") {
        assert_eq!(span.len(), 2);
    } else {
        panic!("missing partitions");
    }
    let bytes = stream(&hex("12 00 00 00 00 00"));
    assert!(inspect_mtef_v5(&bytes).complete);
    let bytes = stream(&hex("10 00 e9 03 00 00 00 00 00"));
    assert_eq!(
        inspect_mtef_v5(&bytes).diagnostics[0].kind,
        DiagnosticKind::Malformed
    );
}
