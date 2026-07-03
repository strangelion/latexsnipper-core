use async_trait::async_trait;
use latexsnipper_foundation::Result;
use latexsnipper_inference::parse_formula_latex;

use crate::context::PipelineContext;
use crate::node::PipelineNode;

/// Parses formula layout from recognized formulas.
///
/// This node reads formula blocks from context metadata,
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
        let formula_blocks = match ctx.get("formula_blocks") {
            Some(v) => v.clone(),
            None => return Ok(()),
        };

        let blocks_array = match formula_blocks.as_array() {
            Some(a) => a.clone(),
            None => return Ok(()),
        };

        if blocks_array.is_empty() {
            return Ok(());
        }

        log::info!(
            "FormulaLayout: parsing layout for {} formulas",
            blocks_array.len()
        );

        let mut updated_blocks: Vec<serde_json::Value> = Vec::new();

        for block_val in &blocks_array {
            let mut updated = block_val.clone();
            if let Some(formula_val) = updated.get_mut("formula") {
                if let Some(latex) = formula_val
                    .get("source")
                    .and_then(|s| s.get("content"))
                    .and_then(|c| c.as_str())
                {
                    match parse_formula_latex(latex) {
                        Ok(layout) => {
                            log::debug!(
                                "Parsed formula layout: {} symbols",
                                layout.symbol_count
                            );
                            // Write the FormulaLayout back into the formula's layout field
                            if let Ok(layout_val) = serde_json::to_value(&layout) {
                                formula_val.as_object_mut()
                                    .map(|obj| obj.insert("layout".to_string(), layout_val));
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to parse formula layout: {}", e);
                        }
                    }
                }
            }
            updated_blocks.push(updated);
        }

        // Write the updated blocks back to context metadata
        ctx.set(
            "formula_blocks",
            serde_json::json!(updated_blocks),
        );

        log::info!(
            "FormulaLayout: parsed and wrote back layout for {} formulas",
            updated_blocks.len()
        );
        Ok(())
    }
}
