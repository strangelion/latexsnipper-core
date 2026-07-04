use async_trait::async_trait;
use latexsnipper_ast::*;
use latexsnipper_foundation::Result;
use latexsnipper_inference::parse_formula_latex;

use crate::context::PipelineContext;
use crate::node::PipelineNode;

/// Parses formula layout from recognized formulas.
///
/// This node reads formula blocks from artifacts,
/// parses each formula's LaTeX string into a structured FormulaLayout,
/// and writes the layout back into each FormulaBlock's formula.layout field.
pub struct FormulaLayoutNode {
    name: String,
}

impl FormulaLayoutNode {
    pub fn new() -> Self {
        Self {
            name: "formula_layout".into(),
        }
    }
}

impl Default for FormulaLayoutNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for FormulaLayoutNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let mut blocks = std::mem::take(&mut ctx.artifacts.formula_blocks);

        if blocks.is_empty() {
            return Ok(());
        }

        log::info!(
            "FormulaLayout: parsing layout for {} formulas",
            blocks.len()
        );

        for block in &mut blocks {
            if let Block::Formula(formula_block) = block {
                let latex = formula_block.formula.as_latex().to_string();
                match parse_formula_latex(&latex) {
                    Ok(layout) => {
                        log::debug!("Parsed formula layout: {} symbols", layout.symbol_count);
                        formula_block.formula.layout = Some(layout);
                    }
                    Err(e) => {
                        log::warn!("Failed to parse formula layout: {}", e);
                    }
                }
            }
        }

        ctx.artifacts.formula_blocks = blocks;

        log::info!(
            "FormulaLayout: parsed layout for {} formulas",
            ctx.artifacts.formula_blocks.len()
        );
        Ok(())
    }
}