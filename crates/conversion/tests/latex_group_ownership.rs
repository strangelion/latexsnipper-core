use latexsnipper_conversion::{
    latex_ast::LatexNode,
    latex_parser::parse_latex,
    omml::{latex_to_omml, validate_omml_latex},
};

#[test]
fn nested_braces_keep_function_arguments_and_script_bases_owned() {
    let LatexNode::Group(call) = parse_latex(r"{\sin{x+y}}") else {
        panic!("outer group")
    };
    assert_eq!(call.len(), 2);
    assert!(matches!(&call[0], LatexNode::Operator(name) if name == "sin"));
    assert!(matches!(&call[1], LatexNode::Group(nodes) if nodes.len() == 3));
    let LatexNode::Group(nodes) = parse_latex(r"{{x+y}_2+z}") else {
        panic!("outer group")
    };
    assert!(matches!(&nodes[0], LatexNode::Subscript { base, sub }
        if matches!(base.as_ref(), LatexNode::Group(nodes) if nodes.len() == 3)
            && matches!(sub.as_ref(), LatexNode::Text(text) if text == "2")));
    for name in ["sin", "exp", "dim"] {
        let omml = latex_to_omml(&format!(r"{{\{name}{{x+y}}}}+z"));
        let end = omml.find("</m:func>").expect("owned function");
        assert!(omml[..end].contains("<m:t>x</m:t>"));
        assert!(omml[..end].contains("<m:t>y</m:t>"));
        assert!(!omml[..end].contains("<m:t>z</m:t>"));
        assert!(omml[end..].contains("<m:t>z</m:t>"));
    }
}

#[test]
fn escaped_braces_and_comments_do_not_change_nested_group_ownership() {
    let source = "{\\sin{x% } ignored\n+\\{y\\}}}+z";
    let LatexNode::Sequence(nodes) = parse_latex(source) else {
        panic!("sequence")
    };
    let LatexNode::Group(call) = &nodes[0] else {
        panic!("outer")
    };
    let LatexNode::Group(argument) = &call[1] else {
        panic!("argument")
    };
    assert!(argument.iter().any(|node| matches!(node,
        LatexNode::Command { name, .. } if name == "{")));
    assert!(argument.iter().any(|node| matches!(node,
        LatexNode::Command { name, .. } if name == "}")));
    assert!(!nodes[0].to_string().contains("ignored"));
    assert!(matches!(nodes.last(), Some(LatexNode::Text(text)) if text == "z"));
}

#[test]
fn excessive_plain_grouping_is_bounded_and_strictly_refused_then_recovers() {
    let source = format!("{}x{}", "{".repeat(5000), "}".repeat(5000));
    let mut ast = parse_latex(&source);
    let mut groups = 0;
    while let LatexNode::Group(mut children) = ast {
        assert_eq!(children.len(), 1);
        groups += 1;
        assert!(groups <= 129);
        ast = children.pop().unwrap();
    }
    assert!(matches!(ast, LatexNode::Text(text) if text.contains('x') && text.starts_with('{')));
    assert!(validate_omml_latex(&source).is_err());
    let commented = format!("{}% }} ignored\nx{}+z", "{".repeat(5000), "}".repeat(5000));
    let LatexNode::Sequence(nodes) = parse_latex(&commented) else {
        panic!("outer sequence")
    };
    assert!(matches!(nodes.last(), Some(LatexNode::Text(text)) if text == "z"));
    assert!(validate_omml_latex(r"{\sin{x+y}}").is_ok());
}
