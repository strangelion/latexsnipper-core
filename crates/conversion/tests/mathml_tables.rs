use latexsnipper_conversion::{parse_mathml_to_latex, DocumentConverter, OutputFormat};

#[test]
fn mathml_matrix_rows_and_columns_survive_readback() {
    let xml = "<math><mtable><mtr><mtd><mn>1</mn></mtd><mtd><mn>2</mn></mtd></mtr><mtr><mtd><mn>3</mn></mtd><mtd><mn>4</mn></mtd></mtr></mtable></math>";
    assert_eq!(
        parse_mathml_to_latex(xml).unwrap(),
        r"\begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix}"
    );
}

#[test]
fn single_column_table_does_not_become_a_single_row() {
    let xml = "<math><mtable><mtr><mtd><mi>a</mi></mtd></mtr><mtr><mtd><mi>b</mi></mtd></mtr></mtable></math>";
    assert_eq!(
        parse_mathml_to_latex(xml).unwrap(),
        r"\begin{matrix} a \\ b \end{matrix}"
    );
}

#[test]
fn table_preserves_empty_cells_and_rows() {
    let xml = "<math><mtable><mtr><mtd/><mtd><mn>2</mn></mtd><mtd/></mtr><mtr/><mtr><mtd/></mtr></mtable></math>";
    assert_eq!(
        parse_mathml_to_latex(xml).unwrap(),
        r"\begin{matrix}  & 2 &  \\  \\  \end{matrix}"
    );
}

#[test]
fn nested_tables_do_not_split_outer_cells() {
    let xml = "<math><mtable><mtr><mtd><mtable><mtr><mtd><mn>1</mn></mtd><mtd><mn>2</mn></mtd></mtr><mtr><mtd><mn>3</mn></mtd><mtd><mn>4</mn></mtd></mtr></mtable></mtd><mtd><mn>5</mn></mtd></mtr></mtable></math>";
    assert_eq!(
        parse_mathml_to_latex(xml).unwrap(),
        r"\begin{matrix} \begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix} & 5 \end{matrix}"
    );
}

#[test]
fn namespace_table_rows_are_structural_not_text_heuristics() {
    let xml = "<m:math xmlns:m=\"http://www.w3.org/1998/Math/MathML\"><m:mtable><m:mtr><m:mtd><m:mn>1</m:mn></m:mtd></m:mtr><m:mtr><m:mtd><m:mn>2</m:mn></m:mtd></m:mtr></m:mtable></m:math>";
    assert_eq!(
        parse_mathml_to_latex(xml).unwrap(),
        r"\begin{matrix} 1 \\ 2 \end{matrix}"
    );
}

#[test]
fn generated_matrix_and_stack_keep_row_counts_after_readback() {
    for (source, cells) in [
        (r"\begin{matrix}1&2\\3&4\end{matrix}", 4),
        (r"\substack{1\\2}", 2),
    ] {
        let xml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
        let rebuilt = parse_mathml_to_latex(&xml).unwrap();
        let roundtrip =
            DocumentConverter::convert_latex_string(&rebuilt, OutputFormat::MathML).unwrap();
        assert_eq!(roundtrip.matches("<mtr>").count(), 2, "{rebuilt}");
        assert_eq!(roundtrip.matches("<mtd>").count(), cells, "{rebuilt}");
        for operand in ["1", "2"] {
            assert!(rebuilt.contains(operand), "{rebuilt}");
        }
    }
}

#[test]
fn table_loss_intake_is_hash_pinned_and_its_expected_structure_is_now_reproduced() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let record: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("quality/failure-corpus/inbox/mathml-table-row-loss.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(record["status"], "minimized");
    let input = std::fs::read_to_string(root.join(record["sanitizedInputRef"].as_str().unwrap()))
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(
        format!("{:x}", Sha256::digest(input.as_bytes())),
        record["inputHash"].as_str().unwrap()
    );
    let fixture: serde_json::Value = serde_json::from_str(&input).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let rebuilt = parse_mathml_to_latex(case["mathml"].as_str().unwrap()).unwrap();
        assert_eq!(rebuilt, case["expectedLatex"].as_str().unwrap());
        assert_ne!(rebuilt, case["actualLatex"].as_str().unwrap());
    }
}
