use latexsnipper_conversion::{
    latex_ast::LatexNode,
    mtef_readonly::{inspect_mtef_v5, MAX_INPUT_BYTES},
    mtef_semantic::{read_mtef_v5, ErrorKind, LossKind, MAX_MATRIX_CELLS, PROFILE_VERSION},
    DocumentConverter, OutputFormat,
};

const HEADER: &[u8] = &[5, 1, 0, 7, 0, b'p', b'i', b'l', b'o', b't', 0, 0];
fn stream(body: &[u8]) -> Vec<u8> {
    [HEADER, body].concat()
}
fn character(code: u16, typeface: u8) -> Vec<u8> {
    let mut bytes = vec![2, 0, typeface + 128];
    bytes.extend(code.to_le_bytes());
    bytes
}
fn variable(ch: char) -> Vec<u8> {
    character(ch as u16, 3)
}
fn number(ch: char) -> Vec<u8> {
    character(ch as u16, 8)
}
fn line(content: &[u8]) -> Vec<u8> {
    [&[1, 0][..], content, &[0]].concat()
}
fn null() -> Vec<u8> {
    vec![1, 1]
}
fn template(selector: u8, variation: u8, slots: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![3, 0, selector, variation, 0];
    for slot in slots {
        bytes.extend(slot);
    }
    bytes.push(0);
    bytes
}
fn equation(content: &[u8]) -> Vec<u8> {
    stream(&[vec![10], line(content), vec![0]].concat())
}
fn fraction(num: &[u8], den: &[u8]) -> Vec<u8> {
    template(11, 0, &[line(num), line(den)])
}
fn latex(bytes: &[u8]) -> String {
    let report = read_mtef_v5(bytes);
    assert!(report.error.is_none(), "{:?}", report.error);
    report.latex.unwrap()
}
fn reject(bytes: &[u8], kind: ErrorKind) {
    let before = bytes.to_vec();
    let report = read_mtef_v5(bytes);
    assert_eq!(
        report.error.as_ref().map(|error| error.kind),
        Some(kind),
        "{:?}",
        report.error
    );
    assert!(report.ast.is_none() && report.latex.is_none());
    assert_eq!(report.inspection.source, before);
    assert_eq!(report.inspection.source.as_ptr(), bytes.as_ptr());
    assert!(report.error.unwrap().offset <= bytes.len());
}

#[test]
fn authored_versioned_fixtures_match_fixed_outputs_and_keep_original_bytes() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/mtef-semantic-v1.json"
    ))
    .unwrap();
    assert_eq!(fixture["schemaVersion"], 1);
    assert_eq!(fixture["profileVersion"], PROFILE_VERSION);
    assert!(fixture["provenance"]
        .as_str()
        .unwrap()
        .contains("synthetic"));
    for row in fixture["accepted"].as_array().unwrap() {
        let bytes: Vec<_> = format!(
            "{} {}",
            fixture["header_hex"].as_str().unwrap(),
            row["body_hex"].as_str().unwrap()
        )
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect();
        let before = bytes.clone();
        let report = read_mtef_v5(&bytes);
        assert!(
            report.error.is_none(),
            "{}: {:?}",
            row["name"],
            report.error
        );
        assert_eq!(report.inspection, inspect_mtef_v5(&bytes));
        assert_eq!(report.inspection.source.as_ptr(), bytes.as_ptr());
        assert_eq!(bytes, before);
        assert_eq!(report.latex.unwrap(), row["latex"].as_str().unwrap());
        let root = match report.ast.unwrap() {
            LatexNode::Text(_) => "text",
            LatexNode::Fraction { .. } => "fraction",
            LatexNode::SquareRoot { .. } => "root",
            LatexNode::Matrix { .. } => "matrix",
            LatexNode::Superscript { .. } => "superscript",
            _ => panic!("fixture root"),
        };
        assert_eq!(root, row["root"].as_str().unwrap());
        assert!(report
            .losses
            .iter()
            .any(|loss| loss.kind == LossKind::Typography));
    }
}

#[test]
fn nested_fraction_root_matrix_operands_survive_mathml_and_omml_handoff() {
    let nth = template(10, 1, &[line(&variable('y')), line(&number('3'))]);
    let matrix = [
        vec![5, 0, 4, 2, 1, 1, 2, 0, 0],
        line(&variable('a')),
        line(&nth),
        vec![0],
    ]
    .concat();
    let bytes = equation(
        &[
            variable('p'),
            fraction(&matrix, &fraction(&number('1'), &number('2'))),
            variable('q'),
        ]
        .concat(),
    );
    let source = latex(&bytes);
    let mathml = DocumentConverter::convert_latex_string(&source, OutputFormat::MathML).unwrap();
    let omml = DocumentConverter::convert_latex_string(&source, OutputFormat::OMML).unwrap();
    assert_eq!(mathml.matches("<mfrac>").count(), 2, "{mathml}");
    assert_eq!(mathml.matches("<mroot>").count(), 1);
    assert_eq!(mathml.matches("<mtd>").count(), 2);
    assert_eq!(omml.matches("<m:f>").count(), 2, "{omml}");
    assert_eq!(omml.matches("<m:rad>").count(), 1);
    assert!(mathml.contains("p") && mathml.contains("q") && mathml.contains("y"));
    assert!(omml.contains(">p<") && omml.contains(">q<") && omml.contains(">y<"));
}

#[test]
fn roots_require_correct_null_degree_and_nth_degree_slots() {
    let good = equation(&template(
        10,
        1,
        &[line(&variable('x')), line(&number('3'))],
    ));
    let report = read_mtef_v5(&good);
    let Some(LatexNode::SquareRoot {
        index: Some(index),
        content,
    }) = report.ast
    else {
        panic!("nth root")
    };
    assert_eq!(index.to_string(), "3");
    assert_eq!(content.to_string(), "x");
    reject(
        &equation(&template(10, 1, &[line(&variable('x')), null()])),
        ErrorKind::InvalidSlots,
    );
    reject(
        &equation(&template(
            10,
            0,
            &[line(&variable('x')), line(&number('3'))],
        )),
        ErrorKind::InvalidSlots,
    );
    reject(
        &equation(&template(10, 0, &[null(), null()])),
        ErrorKind::InvalidSlots,
    );
    reject(
        &equation(&template(10, 2, &[line(&variable('x')), null()])),
        ErrorKind::UnsupportedTemplate,
    );
}

#[test]
fn scripts_attach_to_last_atom_and_numeric_run_not_the_whole_line() {
    let sup = template(28, 0, &[null(), line(&number('2'))]);
    let bytes = equation(&[variable('a'), variable('b'), sup.clone()].concat());
    let report = read_mtef_v5(&bytes);
    let Some(LatexNode::Sequence(nodes)) = report.ast else {
        panic!("sequence")
    };
    assert_eq!(nodes[0].to_string(), "a");
    let LatexNode::Superscript { base, exp } = &nodes[1] else {
        panic!("script")
    };
    assert_eq!(base.to_string(), "b");
    assert_eq!(exp.to_string(), "2");
    assert_eq!(
        latex(&equation(&[number('1'), number('2'), sup.clone()].concat())),
        "{12}^{2}"
    );
    let both = template(29, 0, &[line(&variable('i')), line(&number('2'))]);
    assert_eq!(
        latex(&equation(&[variable('x'), both].concat())),
        "{{x}_{i}}^{2}"
    );
    reject(&equation(&sup), ErrorKind::InvalidSlots);
    reject(
        &equation(&[variable('x'), sup.clone(), sup].concat()),
        ErrorKind::InvalidSlots,
    );
    reject(
        &equation(
            &[
                variable('x'),
                template(28, 1, &[null(), line(&number('2'))]),
            ]
            .concat(),
        ),
        ErrorKind::UnsupportedTemplate,
    );
    reject(
        &equation(
            &[
                variable('x'),
                template(28, 0, &[line(&number('2')), null()]),
            ]
            .concat(),
        ),
        ErrorKind::InvalidSlots,
    );
}

#[test]
fn glyph_inventory_has_command_boundaries_and_no_private_use_or_tex_injection() {
    let source = latex(&equation(
        &[
            character(0x03b1, 4),
            variable('x'),
            character(0x00b1, 6),
            variable('y'),
        ]
        .concat(),
    ));
    assert_eq!(source, "\\alpha x\\pm y");
    let xml = DocumentConverter::convert_latex_string(&source, OutputFormat::MathML).unwrap();
    assert!(
        xml.contains('α') && xml.contains('±') && xml.contains(">x<") && xml.contains(">y<"),
        "{xml}: {:?}",
        latexsnipper_conversion::latex_parser::parse_latex(&source)
    );
    for code in [
        0xe000,
        0xf700,
        0xd800,
        0,
        b'\\' as u16,
        b'{' as u16,
        b'_' as u16,
        b'^' as u16,
    ] {
        reject(
            &equation(&character(code, 3)),
            ErrorKind::UnsupportedCharacter,
        );
    }
    let encoded_only = equation(&[2, 0x24, 131, b'x']);
    reject(&encoded_only, ErrorKind::UnsupportedCharacter);
    let explicit_font = equation(&[2, 0, 127, b'x', 0]);
    reject(&explicit_font, ErrorKind::UnsupportedCharacter);
}

#[test]
fn declared_layout_losses_do_not_turn_into_visual_fidelity_claims() {
    let mut ch = vec![2, 0x0c, 129, 130, 131];
    ch.extend((0x2212u16).to_le_bytes());
    ch.push(45);
    let colored = [vec![16, 0, 0, 0, 0, 0, 0, 0, 15, 1], line(&ch), vec![0]].concat();
    let bytes = stream(&colored);
    let report = read_mtef_v5(&bytes);
    assert_eq!(report.latex.as_deref(), Some("-"));
    for kind in [
        LossKind::Typography,
        LossKind::Color,
        LossKind::Geometry,
        LossKind::EncodedPosition,
        LossKind::CharacterNormalization,
    ] {
        assert!(report.losses.iter().any(|loss| loss.kind == kind));
    }
    for variation in 1..=7 {
        let bytes = equation(&template(
            11,
            variation,
            &[line(&variable('a')), line(&variable('b'))],
        ));
        let report = read_mtef_v5(&bytes);
        assert_eq!(report.latex.as_deref(), Some("\\frac{a}{b}"));
        assert!(report
            .losses
            .iter()
            .any(|loss| loss.kind == LossKind::TemplateLayout));
    }
}

#[test]
fn unknown_records_unsupported_embellishments_and_bad_references_have_no_partial_ast() {
    for suffix in [vec![100, 1, 0], vec![6, 0, 9], template(99, 0, &[])] {
        let bytes = equation(&[variable('x'), suffix].concat());
        assert!(read_mtef_v5(&bytes).ast.is_none());
        assert!(read_mtef_v5(&bytes).latex.is_none());
    }
    reject(
        &equation(&[2, 1, 131, b'x', 0, 6, 0, 9, 0]),
        ErrorKind::UnsupportedCharacter,
    );
    reject(
        &stream(&[17, 5, b'f', 0, 1, 0, 2, 0, 131, b'x', 0, 0, 0]),
        ErrorKind::Reference,
    );
    reject(&stream(&[0]), ErrorKind::InvalidRoot);
    reject(&stream(&[2, 0, 131, b'x', 0, 0]), ErrorKind::InvalidRoot);
}

#[test]
fn matrix_and_pile_keep_row_major_cells_and_empty_placeholders() {
    let bytes = equation(
        &[
            vec![5, 0, 4, 2, 1, 2, 2, 0, 0],
            line(&variable('a')),
            null(),
            line(&variable('c')),
            line(&variable('d')),
            vec![0],
        ]
        .concat(),
    );
    let report = read_mtef_v5(&bytes);
    let Some(LatexNode::Matrix { rows, env }) = report.ast else {
        panic!("matrix")
    };
    assert_eq!(env, "matrix");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].len(), 2);
    assert_eq!(rows[0][0].to_string(), "a");
    assert!(rows[0][1].is_empty());
    assert_eq!(rows[1][0].to_string(), "c");
    let pile = [
        vec![4, 0, 2, 1],
        line(&variable('x')),
        line(&fraction(&number('1'), &number('2'))),
        vec![0],
    ]
    .concat();
    let bytes = stream(&[pile, vec![0]].concat());
    let report = read_mtef_v5(&bytes);
    let Some(LatexNode::Matrix { rows, env }) = report.ast else {
        panic!("pile")
    };
    assert_eq!(env, "aligned");
    assert_eq!(rows.len(), 2);
    let bad = equation(
        &[
            vec![5, 0, 4, 2, 1, 2, 2, 0, 0],
            line(&variable('a')),
            vec![0],
        ]
        .concat(),
    );
    reject(&bad, ErrorKind::InvalidSlots);
}

#[test]
fn input_matrix_and_nesting_budgets_reject_before_unbounded_ast_or_output() {
    reject(&vec![0; MAX_INPUT_BYTES + 1], ErrorKind::Framing);
    for (size, accepted) in [(32u8, true), (33u8, false)] {
        let partition = (usize::from(size) + 1).div_ceil(4);
        let mut matrix = vec![5, 0, 4, 2, 1, size, size];
        matrix.extend(vec![0; partition * 2]);
        for _ in 0..usize::from(size).pow(2) {
            matrix.extend(null());
        }
        matrix.push(0);
        let bytes = equation(&matrix);
        assert!(inspect_mtef_v5(&bytes).complete);
        if accepted {
            assert!(read_mtef_v5(&bytes).error.is_none());
            assert_eq!(usize::from(size).pow(2), MAX_MATRIX_CELLS);
        } else {
            reject(&bytes, ErrorKind::LimitExceeded);
        }
    }
    let mut nested = variable('x');
    for _ in 0..65 {
        nested = fraction(&nested, &number('1'));
    }
    reject(&equation(&nested), ErrorKind::Framing);
}

#[test]
fn every_prefix_and_deterministic_byte_mutation_keeps_source_and_never_partial_success() {
    let bytes = equation(
        &[
            variable('p'),
            fraction(&variable('a'), &number('2')),
            variable('q'),
        ]
        .concat(),
    );
    for end in 0..bytes.len() {
        reject(&bytes[..end], ErrorKind::Framing);
    }
    for offset in 0..bytes.len() {
        for byte in [0, 1, 128, 255] {
            let mut changed = bytes.clone();
            changed[offset] = byte;
            let report = read_mtef_v5(&changed);
            assert_eq!(report.inspection.source, changed);
            assert_eq!(report.ast.is_some(), report.latex.is_some());
            assert_eq!(report.ast.is_none(), report.error.is_some());
            assert!(report
                .losses
                .iter()
                .all(|loss| loss.offset <= changed.len()));
        }
    }
}

#[test]
fn unsupported_headers_and_whitespace_only_required_slots_do_not_normalize_to_empty_math() {
    let good = equation(&variable('x'));
    for (offset, value) in [(1, 2), (2, 2), (3, 3)] {
        let mut bytes = good.clone();
        bytes[offset] = value;
        assert!(inspect_mtef_v5(&bytes).complete);
        reject(&bytes, ErrorKind::UnsupportedHeader);
    }
    reject(&equation(&variable(' ')), ErrorKind::InvalidRoot);
    reject(
        &equation(&fraction(&variable(' '), &variable('x'))),
        ErrorKind::InvalidSlots,
    );
}
