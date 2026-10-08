use latexsnipper_conversion::{
    mtef_batch::inspect_mtef_v5_batch,
    mtef_semantic::{read_mtef_v5, ErrorKind},
    mtef_semantic_batch::{
        read_mtef_v5_batch, SemanticBatchLimit, MAX_BATCH_AST_WORK, MAX_BATCH_INPUT_BYTES,
        MAX_BATCH_ITEMS, MAX_BATCH_LOSSES, MAX_BATCH_OUTPUT_BYTES, MAX_BATCH_RECORDS,
    },
};

fn authored(name: &str) -> Vec<u8> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/mtef-semantic-v1.json"
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

fn unique_stream(key: usize, content: &[u8]) -> Vec<u8> {
    let mut bytes = vec![5, 1, 0, 7, 0];
    bytes.extend(format!("pilot{key}").as_bytes());
    bytes.extend([0, 0, 10, 1, 0]);
    bytes.extend(content);
    bytes.extend([0, 0]);
    bytes
}

#[test]
fn duplicates_share_read_ast_text_losses_but_keep_each_source_identity() {
    let first = authored("fraction");
    let duplicate = first.clone();
    let root = authored("square-root");
    let sources = [&first[..], &duplicate[..], &root[..]];
    let batch = read_mtef_v5_batch(&sources).unwrap();
    assert_eq!(batch.len(), 3);
    assert!(!batch.is_empty());
    assert_eq!(batch.stats().unique_reads, 2);
    assert_eq!(batch.stats().reused_reads, 1);
    let a = batch.get(0).unwrap();
    let b = batch.get(1).unwrap();
    assert!(std::ptr::eq(a.report, b.report));
    assert!(std::ptr::eq(
        a.report.ast.as_ref().unwrap(),
        b.report.ast.as_ref().unwrap()
    ));
    assert_eq!(a.report.latex.as_deref(), Some("\\frac{a}{b}"));
    assert_eq!(b.source.as_ptr(), duplicate.as_ptr());
    assert_ne!(b.source.as_ptr(), b.report.inspection.source.as_ptr());
    assert_eq!(b.source, b.report.inspection.source);
    assert!(batch.get(3).is_none());
    assert_eq!(
        batch.stats().input_bytes,
        first.len() + duplicate.len() + root.len()
    );
    assert_eq!(
        batch.stats().output_bytes,
        read_mtef_v5(&first).latex.unwrap().len() + read_mtef_v5(&root).latex.unwrap().len()
    );
}

#[test]
fn matching_shape_or_normalized_output_never_shares_different_raw_sources() {
    let first = authored("fraction");
    let mut different = first.clone();
    let offset = different
        .windows(5)
        .position(|part| part == [2, 0, 131, b'a', 0])
        .unwrap();
    different[offset + 3] = b'x';
    let sources = [&first[..], &different[..]];
    let raw = inspect_mtef_v5_batch(&sources).unwrap();
    assert_eq!(
        raw.get(0).unwrap().structure_key,
        raw.get(1).unwrap().structure_key
    );
    let batch = read_mtef_v5_batch(&sources).unwrap();
    assert_eq!(batch.stats().unique_reads, 2);
    assert_ne!(
        batch.get(0).unwrap().report.latex,
        batch.get(1).unwrap().report.latex
    );
    assert!(!std::ptr::eq(
        batch.get(0).unwrap().report,
        batch.get(1).unwrap().report
    ));
    let a = unique_stream(0, &[2, 0, 131, b'x', 0]);
    let b = unique_stream(1, &[2, 0, 131, b'x', 0]);
    let batch = read_mtef_v5_batch(&[&a[..], &b[..]]).unwrap();
    assert_eq!(
        batch.get(0).unwrap().report.latex,
        batch.get(1).unwrap().report.latex
    );
    assert_eq!(batch.stats().unique_reads, 2);
}

#[test]
fn source_errors_are_per_entry_and_identical_failures_are_reused() {
    let good = authored("variable");
    let bad = vec![5, 1];
    let bad_copy = bad.clone();
    let batch = read_mtef_v5_batch(&[&good[..], &bad[..], &bad_copy[..], &good[..]]).unwrap();
    assert!(batch.get(0).unwrap().report.ast.is_some());
    assert_eq!(
        batch.get(1).unwrap().report.error.as_ref().unwrap().kind,
        ErrorKind::Framing
    );
    assert!(batch.get(1).unwrap().report.ast.is_none());
    assert!(std::ptr::eq(
        batch.get(1).unwrap().report,
        batch.get(2).unwrap().report
    ));
    assert_eq!(batch.get(2).unwrap().source.as_ptr(), bad_copy.as_ptr());
    assert_eq!(batch.stats().unique_reads, 2);
    assert_eq!(batch.stats().reused_reads, 2);
}

#[test]
fn ten_thousand_authored_occurrences_do_one_semantic_read_without_ast_copies() {
    let first = authored("matrix");
    let separate = first.clone();
    let mut inputs = vec![&first[..]; MAX_BATCH_ITEMS];
    inputs[MAX_BATCH_ITEMS - 1] = &separate;
    let batch = read_mtef_v5_batch(&inputs).unwrap();
    assert_eq!(batch.stats().occurrences, 10_000);
    assert_eq!(batch.stats().unique_reads, 1);
    assert_eq!(batch.stats().reused_reads, 9_999);
    assert!(std::ptr::eq(
        batch.get(0).unwrap().report,
        batch.get(9_999).unwrap().report
    ));
    assert_eq!(batch.get(9_999).unwrap().source.as_ptr(), separate.as_ptr());
    assert!(batch.stats().ast_work < 20);
}

#[test]
fn empty_batch_and_item_input_boundaries_are_explicit() {
    let batch = read_mtef_v5_batch(&[]).unwrap();
    assert!(batch.is_empty());
    assert_eq!(batch.stats().occurrences, 0);
    let byte = [0];
    assert!(matches!(
        read_mtef_v5_batch(&vec![&byte[..]; MAX_BATCH_ITEMS + 1]),
        Err(SemanticBatchLimit::Items)
    ));
    let oversized = vec![0; MAX_BATCH_INPUT_BYTES + 1];
    assert!(matches!(
        read_mtef_v5_batch(&[&oversized]),
        Err(SemanticBatchLimit::InputBytes)
    ));
    let source = vec![0; 1024 * 1024];
    let inputs = vec![&source[..]; 16];
    assert_eq!(
        read_mtef_v5_batch(&inputs).unwrap().stats().input_bytes,
        MAX_BATCH_INPUT_BYTES
    );
    assert!(matches!(
        read_mtef_v5_batch(&vec![&source[..]; 17]),
        Err(SemanticBatchLimit::InputBytes)
    ));
}

#[test]
fn record_budget_counts_failed_unique_sources_not_just_returned_asts() {
    let mut content = vec![10; 4092];
    content.extend([100, 0]);
    let sources: Vec<_> = (0..17).map(|key| unique_stream(key, &content)).collect();
    let inputs: Vec<_> = sources.iter().map(Vec::as_slice).collect();
    let batch = read_mtef_v5_batch(&inputs[..16]).unwrap();
    assert!(batch.stats().inspected_records <= MAX_BATCH_RECORDS);
    assert!(batch.get(0).unwrap().report.ast.is_none());
    assert!(matches!(
        read_mtef_v5_batch(&inputs),
        Err(SemanticBatchLimit::Records)
    ));
}

#[test]
fn ast_work_budget_counts_numeric_nodes_merged_out_of_the_final_tree() {
    let content = [2, 0, 136, b'1', 0].repeat(500);
    let sources: Vec<_> = (0..66).map(|key| unique_stream(key, &content)).collect();
    let inputs: Vec<_> = sources.iter().map(Vec::as_slice).collect();
    let batch = read_mtef_v5_batch(&inputs[..65]).unwrap();
    assert_eq!(batch.stats().ast_work, 32_500);
    assert!(batch.stats().ast_work <= MAX_BATCH_AST_WORK);
    assert!(batch.get(0).unwrap().report.ast.is_some());
    assert!(matches!(
        read_mtef_v5_batch(&inputs),
        Err(SemanticBatchLimit::AstWork)
    ));
}

#[test]
fn losses_and_output_budgets_are_independent_and_duplicates_share_storage() {
    let nudged_minus = [2, 0x0c, 129, 130, 131, 0x12, 0x22, 45].repeat(256);
    let sources: Vec<_> = (0..86)
        .map(|key| unique_stream(key, &nudged_minus))
        .collect();
    let inputs: Vec<_> = sources.iter().map(Vec::as_slice).collect();
    let batch = read_mtef_v5_batch(&inputs[..85]).unwrap();
    assert_eq!(batch.stats().losses, 65_450);
    assert!(batch.stats().losses <= MAX_BATCH_LOSSES);
    assert!(matches!(
        read_mtef_v5_batch(&inputs),
        Err(SemanticBatchLimit::Losses)
    ));
    let duplicates = vec![inputs[0]; 100];
    assert_eq!(read_mtef_v5_batch(&duplicates).unwrap().stats().losses, 770);

    let partial = [2, 0, 134, 0x02, 0x22].repeat(1000);
    let sources: Vec<_> = (0..30).map(|key| unique_stream(key, &partial)).collect();
    let inputs: Vec<_> = sources.iter().map(Vec::as_slice).collect();
    let batch = read_mtef_v5_batch(&inputs[..29]).unwrap();
    assert_eq!(batch.stats().output_bytes, 261_000);
    assert!(batch.stats().output_bytes <= MAX_BATCH_OUTPUT_BYTES);
    assert!(matches!(
        read_mtef_v5_batch(&inputs),
        Err(SemanticBatchLimit::OutputBytes)
    ));
}

#[test]
fn batches_do_not_reuse_data_from_a_previous_call() {
    let mut bytes = authored("variable");
    {
        let batch = read_mtef_v5_batch(&[&bytes[..]]).unwrap();
        assert_eq!(batch.get(0).unwrap().report.latex.as_deref(), Some("x"));
    }
    let offset = bytes
        .windows(5)
        .position(|part| part == [2, 0, 131, b'x', 0])
        .unwrap();
    bytes[offset + 3] = b'z';
    let batch = read_mtef_v5_batch(&[&bytes[..]]).unwrap();
    assert_eq!(batch.get(0).unwrap().report.latex.as_deref(), Some("z"));
    assert_eq!(batch.stats().unique_reads, 1);
}

#[test]
fn preference_arrays_cannot_bypass_record_and_ast_budgets() {
    let mut content = Vec::new();
    for _ in 0..32 {
        content.extend([18, 0, 0, 0, 255]);
        content.extend([0; 255]);
    }
    content.extend([2, 0, 131, b'x', 0]);
    let sources: Vec<_> = (0..9).map(|key| unique_stream(key, &content)).collect();
    let inputs: Vec<_> = sources.iter().map(Vec::as_slice).collect();
    let batch = read_mtef_v5_batch(&inputs[..8]).unwrap();
    assert_eq!(batch.stats().structured_array_items, 65_280);
    assert_eq!(batch.get(0).unwrap().report.latex.as_deref(), Some("x"));
    assert!(matches!(
        read_mtef_v5_batch(&inputs),
        Err(SemanticBatchLimit::ArrayItems)
    ));
    let duplicates = read_mtef_v5_batch(&vec![inputs[0]; 100]).unwrap();
    assert_eq!(duplicates.stats().structured_array_items, 8160);
}
