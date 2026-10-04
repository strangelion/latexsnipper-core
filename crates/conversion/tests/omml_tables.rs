use latexsnipper_conversion::{
    latex_ast::LatexNode, latex_parser::parse_latex, parse_omml_to_latex, DocumentConverter,
    OutputFormat,
};

#[test]
fn generated_omml_matrix_keeps_its_environment_without_delimiters() {
    let source = r"\begin{matrix}1&2\\3&4\end{matrix}";
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::OMML).unwrap();
    let rebuilt = parse_omml_to_latex(&xml).unwrap();
    assert_eq!(rebuilt, r"\begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix}");
    let LatexNode::Matrix { env, rows } = parse_latex(&rebuilt) else {
        panic!("Expected matrix: {rebuilt}")
    };
    assert_eq!(env, "matrix");
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.len() == 2));
}

#[test]
fn one_cell_and_one_column_fenced_matrices_keep_their_shape() {
    for env in [
        "matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix",
    ] {
        for body in ["1", r"1\\2"] {
            let source = format!("\\begin{{{env}}}{body}\\end{{{env}}}");
            let xml = DocumentConverter::convert_latex_string(&source, OutputFormat::OMML).unwrap();
            let rebuilt = parse_omml_to_latex(&xml).unwrap();
            assert_eq!(rebuilt.replace(' ', ""), source, "{env}: {rebuilt}");
        }
    }
}

#[test]
fn nested_omml_matrix_remains_one_outer_cell() {
    let xml = "<oMath><m><mr><e><m><mr><e><r><t>1</t></r></e><e><r><t>2</t></r></e></mr><mr><e><r><t>3</t></r></e><e><r><t>4</t></r></e></mr></m></e><e><r><t>5</t></r></e></mr></m></oMath>";
    assert_eq!(
        parse_omml_to_latex(xml).unwrap(),
        r"\begin{matrix} \begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix} & 5 \end{matrix}"
    );
}

#[test]
fn empty_cells_and_empty_matrix_rows_are_not_discarded() {
    let xml = "<oMath><m><mr><e/><e><r><t>2</t></r></e><e/></mr><mr/><mr><e/></mr></m></oMath>";
    assert_eq!(
        parse_omml_to_latex(xml).unwrap(),
        r"\begin{matrix}  & 2 &  \\  \\  \end{matrix}"
    );
}

#[test]
fn standalone_matrix_preserves_rows_after_reexport() {
    let source = r"\begin{matrix}1&2\\3&4\end{matrix}";
    let xml = DocumentConverter::convert_latex_string(source, OutputFormat::OMML).unwrap();
    let rebuilt = parse_omml_to_latex(&xml).unwrap();
    let xml = DocumentConverter::convert_latex_string(&rebuilt, OutputFormat::OMML).unwrap();
    assert_eq!(xml.matches("<m:mr>").count(), 2, "{rebuilt}");
    assert_eq!(xml.matches("<m:e>").count(), 4, "{rebuilt}");
}

#[test]
fn nested_fenced_matrix_does_not_gain_an_extra_outer_environment() {
    let xml = "<oMath><d><dPr><begChr val=\"(\"/><endChr val=\")\"/></dPr><e><m><mr><e><m><mr><e><r><t>1</t></r></e><e><r><t>2</t></r></e></mr><mr><e><r><t>3</t></r></e><e><r><t>4</t></r></e></mr></m></e><e><r><t>5</t></r></e></mr></m></e></d></oMath>";
    assert_eq!(
        parse_omml_to_latex(xml).unwrap(),
        r"\begin{pmatrix} \begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix} & 5 \end{pmatrix}"
    );
}

#[test]
fn adjacent_matrices_and_siblings_are_not_unwrapped_as_one_matrix() {
    let matrix = "<m><mr><e><r><t>1</t></r></e></mr></m>";
    for body in [
        format!("{matrix}{matrix}"),
        format!("{matrix}<r><t>+2</t></r>"),
    ] {
        let xml = format!(
            "<oMath><d><dPr><begChr val=\"(\"/><endChr val=\")\"/></dPr><e>{body}</e></d></oMath>"
        );
        let rebuilt = parse_omml_to_latex(&xml).unwrap();
        assert!(!rebuilt.contains(r"\begin{pmatrix}"), "{rebuilt}");
        assert!(
            rebuilt.starts_with('(') && rebuilt.ends_with(')'),
            "{rebuilt}"
        );
        assert_eq!(
            rebuilt.matches(r"\begin{matrix}").count(),
            if body.ends_with(matrix) { 2 } else { 1 }
        );
    }
}

#[test]
fn generated_cases_keep_their_left_only_fence_after_readback() {
    for body in [
        "1",
        "1&x>0",
        r"1&x>0\\0&x<0",
        r"\begin{matrix}1&2\end{matrix}&3",
    ] {
        let source = format!("\\begin{{cases}}{body}\\end{{cases}}");
        let xml = DocumentConverter::convert_latex_string(&source, OutputFormat::OMML).unwrap();
        let rebuilt = parse_omml_to_latex(&xml).unwrap();
        assert_eq!(rebuilt.replace(' ', ""), source, "{xml}");
    }
}
