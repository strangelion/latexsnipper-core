use latexsnipper_conversion::{
    latex_ast::LatexNode, latex_parser::parse_latex, parse_omml_to_latex, DocumentConverter,
    FormulaConversionMode, FormulaInputFormat, OutputFormat,
};

#[test]
fn nested_matrix_separators_remain_inside_one_outer_cell() {
    let source = r"\begin{pmatrix}\begin{matrix}1&2\\3&4\end{matrix}&5\end{pmatrix}";
    let LatexNode::Matrix { rows, .. } = parse_latex(source) else {
        panic!("Expected matrix")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].len(), 2);
    let LatexNode::Matrix { rows: inner, .. } = &rows[0][0] else {
        panic!("Expected nested matrix")
    };
    assert_eq!(inner.len(), 2);
    assert!(inner.iter().all(|row| row.len() == 2));
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::OMML).unwrap();
    assert_eq!(xml.matches("<m:m>").count(), 2, "{xml}");
    let rebuilt = parse_omml_to_latex(&xml).unwrap();
    assert_eq!(rebuilt.replace(' ', ""), source, "{rebuilt}");
}

#[test]
fn nested_environment_end_does_not_consume_outer_siblings() {
    let source = r"\begin{matrix}\begin{matrix}1\end{matrix}&2\end{matrix}+3";
    let ast = parse_latex(source);
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::OMML).unwrap();
    assert_eq!(xml.matches("<m:m>").count(), 2, "{ast:?}");
    let rebuilt = parse_omml_to_latex(&xml).unwrap();
    assert_eq!(rebuilt.replace(' ', ""), source, "{rebuilt}");
}

#[test]
fn grouped_separators_are_not_outer_row_or_cell_boundaries() {
    let source = r"\begin{matrix}\text{a&b}&\substack{1\\2}\\3&4\end{matrix}";
    let LatexNode::Matrix { rows, .. } = parse_latex(source) else {
        panic!("Expected matrix")
    };
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.len() == 2));
    assert!(rows[0][0].to_string().contains("a&b"));
    let LatexNode::Command { name, args } = &rows[0][1] else {
        panic!("Expected stack")
    };
    assert_eq!(name, "substack");
    assert_eq!(args.len(), 2);
}

#[test]
fn shared_row_splitter_retains_nested_groups_and_empty_cells() {
    let rows = latexsnipper_conversion::latex_utils::split_matrix_rows(
        r"\begin{matrix}1&2\\3&4\end{matrix}&5\\&6&\\",
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], [r"\begin{matrix}1&2\\3&4\end{matrix}", "5"]);
    assert_eq!(rows[1], ["", "6", ""]);
}

#[test]
fn malformed_nested_environment_stays_out_of_strict_replacement() {
    let source = r"\begin{matrix}\begin{pmatrix}1\end{matrix}\end{pmatrix}";
    assert!(DocumentConverter::convert_formula_string(
        source,
        FormulaInputFormat::Latex,
        OutputFormat::OMML,
        FormulaConversionMode::Strict,
    )
    .is_err());
    assert!(
        DocumentConverter::convert_latex_string(source, OutputFormat::Latex)
            .unwrap()
            .contains(source)
    );
}

#[test]
fn environment_tags_accept_whitespace_and_keep_unicode_cells() {
    let source = "\\begin\n{matrix}\\begin \t{pmatrix}中文\\\\2\\end\n{pmatrix}&3\\end {matrix}+4";
    let LatexNode::Sequence(nodes) = parse_latex(source) else {
        panic!("Expected matrix plus sibling")
    };
    let LatexNode::Matrix { rows, .. } = &nodes[0] else {
        panic!("Expected outer matrix")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].len(), 2);
    let LatexNode::Matrix { rows: inner, .. } = &rows[0][0] else {
        panic!("Expected nested matrix")
    };
    assert_eq!(inner.len(), 2);
    assert!(inner[0][0].to_string().contains("中文"));
    assert!(nodes.last().unwrap().to_string().contains('4'));
}

#[test]
fn blank_interior_rows_and_edge_cells_keep_their_positions() {
    let source = r"\begin{matrix}&1&\\\\2&&\end{matrix}";
    let LatexNode::Matrix { rows, .. } = parse_latex(source) else {
        panic!("Expected matrix")
    };
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.iter().map(Vec::len).collect::<Vec<_>>(), [3, 1, 3]);
    assert!(rows[0][0].is_empty() && rows[0][2].is_empty());
    assert!(rows[1][0].is_empty());
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::OMML).unwrap();
    assert_eq!(xml.matches("<m:mr>").count(), 3);
    assert_eq!(xml.matches("<m:e>").count(), 7);
}
