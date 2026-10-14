//! Private structural comparison shared by bounded format pilots, not grammars.
use crate::latex_ast::LatexNode;

// Ignore invisible wrappers, retain visible delimiters and operand ownership.
// Callers must validate supported nodes and resource budgets before comparison.
#[derive(PartialEq, Eq)]
pub(super) struct Shape {
    name: String,
    children: Vec<Vec<Shape>>,
}
pub(super) fn shape(node: &LatexNode) -> Vec<Shape> {
    shape_impl(node, false)
}

pub(super) fn unicode_shape(node: &LatexNode) -> Vec<Shape> {
    shape_impl(node, true)
}

fn shape_impl(node: &LatexNode, ignore_spaces: bool) -> Vec<Shape> {
    let child = |node: &LatexNode| shape_impl(node, ignore_spaces);
    if ignore_spaces {
        if matches!(
            node,
            LatexNode::Operator(_) | LatexNode::OperatorName { .. }
        ) {
            if let Some(name) = crate::unicodemath_pilot::function_name(node) {
                return vec![Shape {
                    name: format!("function:{name}"),
                    children: vec![],
                }];
            }
        }
        if let Some(head) = crate::unicodemath_pilot::nary_limits(node) {
            return vec![Shape {
                name: format!("nary:{}:{:?}:{:?}", head.name, head.limits, head.grow),
                children: vec![
                    head.lower.map_or_else(Vec::new, child),
                    head.upper.map_or_else(Vec::new, child),
                ],
            }];
        }
    }
    let combined = |nodes: &[LatexNode]| nodes.iter().flat_map(child).collect::<Vec<_>>();
    let (name, children) = match node {
        LatexNode::Sequence(nodes) | LatexNode::Group(nodes) => return combined(nodes),
        LatexNode::Text(text) => {
            return text
                .chars()
                .filter(|ch| !ignore_spaces || *ch != ' ')
                .map(|ch| Shape {
                    name: format!("text:{ch}"),
                    children: vec![],
                })
                .collect()
        }
        LatexNode::Fraction { num, den } => ("frac".into(), vec![child(num), child(den)]),
        LatexNode::SquareRoot { index, content } => (
            if index.is_some() {
                "indexed-root"
            } else {
                "sqrt"
            }
            .into(),
            vec![
                index.as_ref().map_or_else(Vec::new, |index| child(index)),
                child(content),
            ],
        ),
        LatexNode::Subscript { base, sub } => ("sub".into(), vec![child(base), child(sub)]),
        LatexNode::Superscript { base, exp } => ("sup".into(), vec![child(base), child(exp)]),
        LatexNode::Greek(name) => (format!("greek:{name}"), vec![]),
        LatexNode::Operator(name) => (format!("operator:{name}"), vec![]),
        LatexNode::FontModifier { font, content } => (format!("font:{font}"), vec![child(content)]),
        LatexNode::Command { name, args } => (
            format!("command:{name}"),
            args.iter()
                .map(|node| shape_impl(node, ignore_spaces && name != "text"))
                .collect(),
        ),
        LatexNode::Symbol(name) | LatexNode::Relation(name) => {
            (format!("symbol:{}", name.trim_start_matches('\\')), vec![])
        }
        LatexNode::Delimited {
            left,
            content,
            right,
        } => (format!("delimited:{left}:{right}"), vec![combined(content)]),
        LatexNode::Matrix { env, rows } => (
            format!("matrix:{env}"),
            rows.iter()
                .map(|row| {
                    vec![Shape {
                        name: "row".into(),
                        children: row.iter().map(child).collect(),
                    }]
                })
                .collect(),
        ),
        _ => ("unsupported".into(), vec![]),
    };
    vec![Shape { name, children }]
}
