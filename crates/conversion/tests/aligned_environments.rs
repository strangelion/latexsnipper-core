use latexsnipper_conversion::{
    latex_ast::LatexNode, latex_parser::parse_latex, DocumentConverter, FormulaConversionMode,
    FormulaInputFormat, OutputFormat,
};

#[test]
fn starred_align_and_gather_keep_rows_operands_and_environment_source() {
    for env in ["align*", "gather*"] {
        let source = format!("\\begin{{{env}}}x&=1\\\\y&=2\\end{{{env}}}");
        let ast = parse_latex(&source);
        let LatexNode::Matrix { env: parsed, rows } = &ast else {
            panic!("Expected rows: {ast:?}")
        };
        assert_eq!(parsed, env);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.len() == 2));
        assert_eq!(ast.to_string(), source);
        let xml = DocumentConverter::convert_latex_string(&source, OutputFormat::MathML).unwrap();
        assert_eq!(xml.matches("<mtr>").count(), 2, "{xml}");
        assert_eq!(xml.matches("<mtd>").count(), 4, "{xml}");
        let xml = DocumentConverter::convert_latex_string(&source, OutputFormat::OMML).unwrap();
        assert_eq!(xml.matches("<m:mr>").count(), 2, "{xml}");
        let typst = DocumentConverter::convert_latex_string(&source, OutputFormat::Typst).unwrap();
        for operand in ["x", "y", "1", "2"] {
            assert!(typst.contains(operand), "{typst}");
        }
    }
}

#[test]
fn starred_align_nested_in_a_matrix_keeps_its_rows_and_outer_siblings() {
    let source = r"a+\begin{matrix}\begin{align*}x&=1\\y&=2\end{align*}&3\end{matrix}+z";
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::MathML).unwrap();
    assert_eq!(xml.matches("<mtable>").count(), 2, "{xml}");
    assert_eq!(xml.matches("<mtr>").count(), 3, "{xml}");
    assert!(
        xml.contains("<mi>a</mi>") && xml.contains("<mi>z</mi>"),
        "{xml}"
    );
}

#[test]
fn starred_alignment_remains_outside_unverified_strict_replacement() {
    for env in ["align*", "gather*"] {
        let source = format!("\\begin{{{env}}}x&=1\\\\y&=2\\end{{{env}}}");
        assert!(DocumentConverter::convert_formula_string(
            &source,
            FormulaInputFormat::Latex,
            OutputFormat::OMML,
            FormulaConversionMode::Strict
        )
        .is_err());
        assert!(
            DocumentConverter::convert_latex_string(&source, OutputFormat::Latex)
                .unwrap()
                .contains(&source)
        );
    }
}

#[test]
fn starred_alignment_failure_intake_is_hash_pinned_and_keeps_current_rows() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let record: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("quality/failure-corpus/inbox/starred-alignment-row-loss.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(record["status"], "minimized");
    let fixture = std::fs::read_to_string(root.join(record["sanitizedInputRef"].as_str().unwrap()))
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(
        format!("{:x}", Sha256::digest(fixture.as_bytes())),
        record["inputHash"].as_str().unwrap()
    );
    let fixture: serde_json::Value = serde_json::from_str(&fixture).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let LatexNode::Matrix { rows, .. } = parse_latex(case["latex"].as_str().unwrap()) else {
            panic!("Expected row structure")
        };
        assert_eq!(rows.len() as u64, case["expectedRows"].as_u64().unwrap());
        assert!(rows
            .iter()
            .all(|row| row.len() as u64 == case["expectedColumns"].as_u64().unwrap()));
    }
}
