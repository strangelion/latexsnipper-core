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
    let combined = |nodes: &[LatexNode]| nodes.iter().flat_map(shape).collect::<Vec<_>>();
    let (name, children) = match node {
        LatexNode::Sequence(nodes) | LatexNode::Group(nodes) => return combined(nodes),
        LatexNode::Text(text) => {
            return text
                .chars()
                .map(|ch| Shape {
                    name: format!("text:{ch}"),
                    children: vec![],
                })
                .collect()
        }
        LatexNode::Fraction { num, den } => ("frac".into(), vec![shape(num), shape(den)]),
        LatexNode::SquareRoot { index, content } => (
            if index.is_some() {
                "indexed-root"
            } else {
                "sqrt"
            }
            .into(),
            vec![
                index.as_ref().map_or_else(Vec::new, |index| shape(index)),
                shape(content),
            ],
        ),
        LatexNode::Subscript { base, sub } => ("sub".into(), vec![shape(base), shape(sub)]),
        LatexNode::Superscript { base, exp } => ("sup".into(), vec![shape(base), shape(exp)]),
        LatexNode::Greek(name) => (format!("greek:{name}"), vec![]),
        LatexNode::Operator(name) => (format!("operator:{name}"), vec![]),
        LatexNode::FontModifier { font, content } => (format!("font:{font}"), vec![shape(content)]),
        LatexNode::Command { name, args } => {
            (format!("command:{name}"), args.iter().map(shape).collect())
        }
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
                        children: row.iter().map(shape).collect(),
                    }]
                })
                .collect(),
        ),
        _ => ("unsupported".into(), vec![]),
    };
    vec![Shape { name, children }]
}
