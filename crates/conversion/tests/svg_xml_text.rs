use latexsnipper_ast::Inline;
use latexsnipper_conversion::parse_svg;

fn text(svg: &str) -> String {
    let result = parse_svg(svg);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    result
        .shapes
        .iter()
        .flat_map(|shape| &shape.text)
        .filter_map(|inline| match inline {
            Inline::Text(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn svg_text_preserves_xml_references_and_spaces() {
    assert_eq!(
        text("<svg><text> a &amp; b &#20013;&#x1F600; <![CDATA[&amp;]]> </text></svg>"),
        " a & b 中😀 &amp; "
    );
}

#[test]
fn svg_nested_text_spans_and_empty_spans_do_not_drop_the_tail() {
    assert_eq!(
        text("<svg><text>a<tspan>b<tspan>中</tspan>c</tspan><tspan/>d</text></svg>"),
        "ab中cd"
    );
}

#[test]
fn svg_invalid_text_has_diagnostics_not_a_partial_shape() {
    for content in ["a &missing; b", "a &#0; b", "a<tspan>b</wrong>c"] {
        let result = parse_svg(&format!("<svg><text>{content}</text></svg>"));
        assert!(!result.diagnostics.is_empty());
        assert!(result.shapes.is_empty());
    }
}

#[test]
fn positioned_text_span_reports_layout_loss_instead_of_claiming_fidelity() {
    let result = parse_svg("<svg><text>a<tspan x=\"100\" fill=\"red\">b</tspan>c</text></svg>");
    assert_eq!(result.shapes.len(), 1);
    assert!(result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("flattened")));
}
