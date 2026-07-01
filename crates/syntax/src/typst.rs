use latexsnipper_ast::{Block, Document, Inline};
use latexsnipper_foundation::Result;

use crate::renderer::Renderer;

/// Typst renderer — converts Document AST to Typst syntax.
pub struct TypstRenderer;

impl Renderer for TypstRenderer {
    fn render(&self, doc: &Document) -> Result<String> {
        let mut parts = Vec::new();
        for page in &doc.pages {
            for block in &page.blocks {
                match block {
                    Block::Formula(f) => {
                        let latex = f.formula.as_latex();
                        let typst = latex_to_typst(latex);
                        if f.formula.display_mode {
                            parts.push(format!("$ {} $", typst));
                        } else {
                            parts.push(typst);
                        }
                    }
                    Block::Paragraph(p) => {
                        let text: String = p
                            .inlines
                            .iter()
                            .map(|i| match i {
                                Inline::Text(t) => t.text.clone(),
                                Inline::Formula(f) => {
                                    let typst = latex_to_typst(f.as_latex());
                                    if f.display_mode {
                                        format!("$ {} $", typst)
                                    } else {
                                        typst
                                    }
                                }
                                _ => String::new(),
                            })
                            .collect();
                        if !text.is_empty() {
                            parts.push(text);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(parts.join("\n\n"))
    }

    fn name(&self) -> &str {
        "typst"
    }
}

/// Convert LaTeX formula to Typst syntax.
pub fn latex_to_typst(latex: &str) -> String {
    let mut result = latex.to_string();

    // LaTeX → Typst symbol mappings
    let mappings = [
        ("\\frac{", "("),
        ("}{", ")/("),
        ("\\sqrt{", "sqrt("),
        ("\\int", "integral"),
        ("\\sum", "sum"),
        ("\\prod", "product"),
        ("\\infty", "infinity"),
        ("\\pi", "pi"),
        ("\\alpha", "alpha"),
        ("\\beta", "beta"),
        ("\\gamma", "gamma"),
        ("\\delta", "delta"),
        ("\\theta", "theta"),
        ("\\lambda", "lambda"),
        ("\\sigma", "sigma"),
        ("\\omega", "omega"),
        ("\\pm", "plus.minus"),
        ("\\times", "times"),
        ("\\div", "div"),
        ("\\cdot", "dot"),
        ("\\leq", "lt.eq"),
        ("\\geq", "gt.eq"),
        ("\\neq", "neq"),
        ("\\approx", "approx"),
        ("\\rightarrow", "rightarrow"),
        ("\\leftarrow", "leftarrow"),
        ("\\in", "in"),
        ("\\notin", "notin"),
        ("\\subset", "subset"),
        ("\\cup", "union"),
        ("\\cap", "intersect"),
    ];

    for (from, to) in &mappings {
        result = result.replace(from, to);
    }

    // Remove remaining backslashes
    result = result.replace("\\", "");

    result
}
