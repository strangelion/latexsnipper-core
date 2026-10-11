//! Shared finite min/max script and limit handling without changing the public AST.
use crate::latex_ast::LatexNode;

pub(crate) struct Extremum<'a> {
    pub name: &'a str,
    pub lower: Option<&'a LatexNode>,
    pub upper: Option<&'a LatexNode>,
    pub limits: Option<bool>,
}

pub(crate) fn head(node: &LatexNode) -> Option<Extremum<'_>> {
    match node {
        LatexNode::Operator(name) if matches!(name.as_str(), "min" | "max") => Some(Extremum {
            name,
            lower: None,
            upper: None,
            limits: None,
        }),
        LatexNode::Subscript { base, sub } => {
            let mut value = head(base)?;
            if value.lower.is_some() {
                return None;
            }
            value.lower = Some(sub);
            Some(value)
        }
        LatexNode::Superscript { base, exp } => {
            let mut value = head(base)?;
            if value.upper.is_some() {
                return None;
            }
            value.upper = Some(exp);
            Some(value)
        }
        LatexNode::Command { name, args }
            if matches!(name.as_str(), "limits" | "nolimits" | "displaylimits")
                && args.len() == 1 =>
        {
            let mut value = head(&args[0])?;
            value.limits = match name.as_str() {
                "limits" => Some(true),
                "nolimits" => Some(false),
                _ => None,
            };
            Some(value)
        }
        _ => None,
    }
}

// Resolve inherited math style once, before target-specific recursive rendering.
pub(crate) fn resolve(node: &mut LatexNode, display: bool) {
    use LatexNode::*;
    match node {
        Operator(name) if matches!(name.as_str(), "min" | "max") => {
            let operator = Operator(name.clone());
            *node = Command {
                name: if display { "limits" } else { "nolimits" }.into(),
                args: vec![operator],
            };
        }
        Command { name, args } if head_command(name, args) => {
            if name == "displaylimits" {
                *name = if display { "limits" } else { "nolimits" }.into();
            }
            // Explicit limit placement wins over the inherited style.
            let vertical = name == "limits";
            resolve(&mut args[0], vertical);
        }
        Command { name, args } => {
            let child_display = match name.as_str() {
                "displaystyle" => true,
                "textstyle" | "scriptstyle" | "scriptscriptstyle" => false,
                _ => display,
            };
            for child in args {
                resolve(child, child_display);
            }
        }
        Superscript { base, exp } => {
            resolve(base, display);
            resolve(exp, false);
        }
        Subscript { base, sub } => {
            resolve(base, display);
            resolve(sub, false);
        }
        Fraction { num, den } => {
            resolve(num, false);
            resolve(den, false);
        }
        Math { content, display } => {
            for child in content {
                resolve(child, *display);
            }
        }
        Sequence(nodes) | Group(nodes) | Description(nodes) => {
            for child in nodes {
                resolve(child, display);
            }
        }
        Delimited { content, .. } | DescriptionItem { content, .. } => {
            for child in content {
                resolve(child, display);
            }
        }
        SquareRoot { index, content } => {
            if let Some(index) = index {
                resolve(index, false);
            }
            resolve(content, display);
        }
        FontModifier { content, .. }
        | Accent { content, .. }
        | Footnote { content }
        | Theorem { content, .. }
        | Proof { content }
        | Minipage { content, .. }
        | Float { content, .. } => resolve(content, display),
        Matrix { rows, .. } | Array { rows, .. } | Cases(rows) => {
            for child in rows.iter_mut().flatten() {
                resolve(child, false);
            }
        }
        Overbrace { content, label } | Underbrace { content, label } => {
            resolve(content, display);
            if let Some(label) = label {
                resolve(label, false);
            }
        }
        Overset { top, base } => {
            resolve(base, display);
            resolve(top, false);
        }
        Underset { bottom, base } => {
            resolve(base, display);
            resolve(bottom, false);
        }
        XArrow { above, below, .. } => {
            for child in above.iter_mut().chain(below.iter_mut()) {
                resolve(child, false);
            }
        }
        _ => {}
    }
}

fn head_command(name: &str, args: &[LatexNode]) -> bool {
    matches!(name, "limits" | "nolimits" | "displaylimits")
        && args.len() == 1
        && head(&args[0]).is_some()
}

// Reuse the bounded shape gate; never strip delimiters from mixed documents.
pub(crate) fn standalone_math(source: &str) -> Option<(String, bool)> {
    let source = source.trim();
    for (open, close, display) in [
        (r"\[", r"\]", true),
        (r"\(", r"\)", false),
        ("$$", "$$", true),
        ("$", "$", false),
    ] {
        if let Some(inner) = source
            .strip_prefix(open)
            .and_then(|rest| rest.strip_suffix(close))
        {
            let fragment =
                crate::formula_fragment::latex_display_to_fragment(&format!("\\[\n{inner}\n\\]"));
            return fragment.ok().map(|content| (content, display));
        }
    }
    None
}
