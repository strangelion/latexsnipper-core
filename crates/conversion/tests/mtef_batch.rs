use latexsnipper_conversion::mtef_batch::{
    inspect_mtef_v5_batch, BatchLimit, MAX_BATCH_INPUT_BYTES, MAX_BATCH_ITEMS, MAX_BATCH_RECORDS,
};
use latexsnipper_conversion::mtef_readonly::{inspect_mtef_v5, MAX_RECORDS};

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
    format!(
        "{} {}",
        fixture["header_hex"].as_str().unwrap(),
        row["body_hex"].as_str().unwrap()
    )
    .split_whitespace()
    .map(|byte| u8::from_str_radix(byte, 16).unwrap())
    .collect()
}

#[test]
fn duplicates_share_reports_but_keep_each_original_source_pointer() {
    let first = authored("fraction-slots");
    let second = first.clone();
    let root = authored("root-and-null-slot");
    let sources = [&first[..], &second[..], &root[..]];
    let batch = inspect_mtef_v5_batch(&sources).unwrap();
    assert_eq!(batch.len(), 3);
    assert_eq!(batch.inspections(), 2);
    assert_eq!(batch.reused_inspections(), 1);
    assert_eq!(batch.get(1).unwrap().source.as_ptr(), second.as_ptr());
    assert!(std::ptr::eq(
        batch.get(0).unwrap().inspection,
        batch.get(1).unwrap().inspection
    ));
    assert_eq!(*batch.get(0).unwrap().inspection, inspect_mtef_v5(&first));
    assert!(batch.get(3).is_none());
    assert!(!batch.is_empty());
}

#[test]
fn similar_fraction_operands_group_without_reusing_their_decoded_values() {
    let first = authored("fraction-slots");
    let mut different = first.clone();
    let offset = different
        .windows(5)
        .position(|value| value == [2, 0, 0x83, b'a', 0])
        .unwrap();
    different[offset + 3] = b'x';
    let root = authored("root-and-null-slot");
    let sources = [&first[..], &different[..], &root[..]];
    let batch = inspect_mtef_v5_batch(&sources).unwrap();
    assert_eq!(batch.inspections(), 3);
    assert_eq!(batch.reused_inspections(), 0);
    assert_eq!(
        batch.get(0).unwrap().structure_key,
        batch.get(1).unwrap().structure_key
    );
    assert_ne!(
        batch.get(0).unwrap().structure_key,
        batch.get(2).unwrap().structure_key
    );
    assert_ne!(
        batch.get(0).unwrap().inspection.records,
        batch.get(1).unwrap().inspection.records
    );
}

#[test]
fn matrix_dimensions_and_null_slots_are_distinct_shapes() {
    let square = authored("matrix-four-slots");
    let report = inspect_mtef_v5(&square);
    let start = report
        .records
        .iter()
        .find(|record| record.record_type == 5)
        .unwrap()
        .span
        .start;
    let mut row = square.clone();
    row[start + 5] = 1;
    row[start + 6] = 4;
    // Four columns require two partition bytes instead of the square's one.
    row.insert(start + 9, 0);
    let sources = [&square[..], &row[..]];
    let batch = inspect_mtef_v5_batch(&sources).unwrap();
    assert!(batch.get(1).unwrap().inspection.complete);
    assert_ne!(
        batch.get(0).unwrap().structure_key,
        batch.get(1).unwrap().structure_key
    );

    let null_root = authored("root-and-null-slot");
    let report = inspect_mtef_v5(&null_root);
    let slot_start = report
        .records
        .iter()
        .find(|record| record.record_type == 1 && null_root[record.span.start + 1] == 1)
        .unwrap()
        .span
        .start;
    let mut empty_root = null_root.clone();
    empty_root[slot_start + 1] = 0;
    empty_root.insert(slot_start + 2, 0);
    let batch = inspect_mtef_v5_batch(&[&null_root, &empty_root]).unwrap();
    assert!(batch.get(1).unwrap().inspection.complete);
    assert_ne!(
        batch.get(0).unwrap().structure_key,
        batch.get(1).unwrap().structure_key
    );
}

#[test]
fn stopped_and_future_streams_keep_diagnostics_and_have_no_shape_key() {
    let complete = authored("variable");
    let truncated = &complete[..complete.len() - 1];
    let mut future = complete[..12].to_vec();
    future.extend([100, 1, 42, 0]);
    let sources = [truncated, truncated, &future[..]];
    let batch = inspect_mtef_v5_batch(&sources).unwrap();
    assert_eq!(batch.inspections(), 2);
    for (index, source) in sources.iter().enumerate() {
        let entry = batch.get(index).unwrap();
        assert_eq!(entry.source, *source);
        assert!(entry.structure_key.is_none());
        assert!(!entry.inspection.diagnostics.is_empty());
    }
}

#[test]
fn batch_budgets_bound_occurrences_input_work_and_inspection_allocations() {
    let empty = inspect_mtef_v5_batch(&[]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.inspected_records(), 0);
    let too_many: Vec<&[u8]> = vec![&[]; MAX_BATCH_ITEMS + 1];
    assert_eq!(
        inspect_mtef_v5_batch(&too_many).unwrap_err(),
        BatchLimit::Items
    );
    let large = vec![0; MAX_BATCH_INPUT_BYTES / 2 + 1];
    assert_eq!(
        inspect_mtef_v5_batch(&[&large, &large]).unwrap_err(),
        BatchLimit::InputBytes
    );
    let mut sources = Vec::new();
    for variant in 0..=MAX_BATCH_RECORDS / MAX_RECORDS {
        let mut bytes = authored("variable")[..12].to_vec();
        bytes[4] = variant as u8;
        bytes.extend(std::iter::repeat_n(10, MAX_RECORDS - 1));
        bytes.push(0);
        sources.push(bytes);
    }
    let borrowed: Vec<_> = sources.iter().map(Vec::as_slice).collect();
    assert_eq!(
        inspect_mtef_v5_batch(&borrowed).unwrap_err(),
        BatchLimit::Records
    );
}

#[test]
fn ten_thousand_authored_duplicate_occurrences_are_inspected_once() {
    let bytes = authored("fraction-slots");
    let sources = vec![bytes.as_slice(); MAX_BATCH_ITEMS];
    let batch = inspect_mtef_v5_batch(&sources).unwrap();
    assert_eq!(batch.inspections(), 1);
    assert_eq!(batch.reused_inspections(), MAX_BATCH_ITEMS - 1);
    assert_eq!(
        batch.inspected_records(),
        inspect_mtef_v5(&bytes).records.len()
    );
}
