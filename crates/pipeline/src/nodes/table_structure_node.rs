use async_trait::async_trait;
use latexsnipper_ast::Rect;
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;
use latexsnipper_inference::recognize_table_structure;
use latexsnipper_runtime::InferenceSession;

use crate::artifacts::RecognizedTable;
use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::nodes::utils::resolve_model_handle;
use crate::nodes::utils::resolve_variant;

/// Parses table structure using a configurable backend.
///
/// Backend is selected via the `backend` field:
/// - `"tatr"` — loads `models/table-struct/tatr-structure/model.onnx`
/// - `"slanet"` — loads `models/table-struct/slanet-plus/model.onnx`
/// - `"projection"` — rule-based fallback, no model needed
pub struct TableStructureNode {
    name: String,
    backend: String,
}

#[derive(Clone)]
struct BackendChoice {
    backend: String,
    variant: String,
}

impl TableStructureNode {
    pub fn new() -> Self {
        Self {
            name: "table_structure".into(),
            backend: "auto".into(),
        }
    }

    pub fn with_backend(backend: impl Into<String>) -> Self {
        let b: String = backend.into();
        Self {
            name: format!("table_structure_{}", b),
            backend: b,
        }
    }

    fn backend_model_path(
        &self,
        models: &std::path::Path,
        variant: &str,
    ) -> Option<std::path::PathBuf> {
        let variant_dir = models.join("table-struct").join(variant);
        if !variant_dir.is_dir() {
            return None;
        }

        if let Ok(config) = latexsnipper_model::ModelConfig::load(&variant_dir) {
            if let Some(path) = config.pipeline_model_path(&variant_dir) {
                return Some(path);
            }
        }

        let path = variant_dir.join("model.onnx");
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    fn choice(backend: &str, variant: &str) -> BackendChoice {
        BackendChoice {
            backend: backend.to_string(),
            variant: variant.to_string(),
        }
    }

    fn explicit_choice(&self, ctx: &PipelineContext) -> Option<BackendChoice> {
        let variant = ctx.model_variants.get("table-struct")?;
        let backend = match variant.as_str() {
            "slanet-plus" | "slanet" => "slanet",
            "tatr-structure" | "tatr" => "tatr",
            "projection" => "projection",
            _ => return None,
        };

        Some(BackendChoice {
            backend: backend.to_string(),
            variant: variant.clone(),
        })
    }

    fn backend_choices(&self, ctx: &PipelineContext) -> Vec<BackendChoice> {
        if self.backend != "auto" {
            return match self.backend.as_str() {
                "slanet" => vec![Self::choice("slanet", "slanet-plus")],
                "tatr" => vec![Self::choice("tatr", "tatr-structure")],
                "projection" => Vec::new(),
                _ => Vec::new(),
            };
        }

        if let Some(choice) = self.explicit_choice(ctx) {
            return if choice.backend == "projection" {
                Vec::new()
            } else {
                vec![choice]
            };
        }

        match ctx.parse_mode {
            crate::opendoc_hybrid::DocumentParseMode::OpenOcrText
            | crate::opendoc_hybrid::DocumentParseMode::OpenDocHybrid => {
                vec![
                    Self::choice("slanet", "slanet-plus"),
                    Self::choice("tatr", "tatr-structure"),
                ]
            }
            crate::opendoc_hybrid::DocumentParseMode::SpecializedStable => {
                vec![
                    Self::choice("tatr", "tatr-structure"),
                    Self::choice("slanet", "slanet-plus"),
                ]
            }
        }
    }
}

impl Default for TableStructureNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for TableStructureNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let detections = ctx.artifacts.table_detections.clone();
        if detections.is_empty() {
            return Ok(());
        }

        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        let models = match &ctx.models_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        let acc = ctx.acceleration;
        let choices = self.backend_choices(ctx);
        let mut backend_sessions: Vec<(String, Box<dyn InferenceSession>)> = Vec::new();

        for choice in choices {
            let model_path = if ctx
                .model_variants
                .get("table-struct")
                .is_some_and(|variant| variant == &choice.variant)
            {
                match resolve_variant(ctx, &models, "table-struct") {
                    Ok((_, model_path, _)) => model_path,
                    Err(_) => continue,
                }
            } else {
                let Some(model_path) = self.backend_model_path(&models, &choice.variant) else {
                    continue;
                };
                model_path
            };
            let Ok(handle) = resolve_model_handle(ctx, "table-struct", model_path) else {
                continue;
            };
            let Some(backend) = ctx.backend.as_ref() else {
                continue;
            };
            match backend.create_session(&handle, acc) {
                Ok(session) => {
                    backend_sessions.push((choice.backend, session));
                }
                Err(e) => {
                    ctx.diagnostic_warn(
                        "table_structure",
                        format!(
                            "Table structure backend '{}' failed to load: {}",
                            choice.backend, e
                        ),
                    );
                }
            }
        }

        let projection_selected = ctx
            .model_variants
            .get("table-struct")
            .is_some_and(|variant| variant == "projection");
        if backend_sessions.is_empty()
            && self.backend.as_str() != "projection"
            && !projection_selected
        {
            ctx.diagnostic_warn(
                "table_structure",
                "No table structure model available; falling back to projection backend",
            );
        }

        log::info!(
            "TableStructure: parsing {} table regions with {} model backend(s)",
            detections.len(),
            backend_sessions.len()
        );

        let mut all_tables: Vec<RecognizedTable> = Vec::new();

        for det in &detections {
            let x = det.rect.x;
            let y = det.rect.y;
            let w = det.rect.width;
            let h = det.rect.height;

            let table_rect = Rect::new(x, y, w, h);
            let cropped = operations::crop(&image, table_rect);

            let mut grid = None;
            for (backend_name, sess) in &backend_sessions {
                let candidate = recognize_table_structure(&cropped, backend_name, Some(&**sess))?;
                if let Some(cells) = candidate {
                    if cells.is_empty() {
                        continue;
                    }
                    if suspicious_grid(&cells) {
                        ctx.diagnostic_warn(
                            "table_structure",
                            format!(
                                "Table structure backend '{}' produced suspicious grid; trying fallback",
                                backend_name
                            ),
                        );
                        continue;
                    }
                    grid = Some(cells);
                    break;
                }
            }

            if grid.is_none() {
                grid = recognize_table_structure(&cropped, "projection", None)?;
            }

            match grid {
                Some(mut cells) if !cells.is_empty() => {
                    // Convert cell coordinates from child (cropped) space to parent (image) space
                    for cell in &mut cells {
                        cell.rect.x += x;
                        cell.rect.y += y;
                    }
                    let mut table = RecognizedTable::new(table_rect);
                    table.cells = cells;
                    all_tables.push(table);
                }
                Some(_) => {
                    log::warn!("Grid is empty for table at ({}, {})", x, y);
                }
                None => continue,
            }
        }

        ctx.artifacts.table_structures = all_tables;
        log::info!(
            "TableStructure: parsed {} tables",
            ctx.artifacts.table_structures.len()
        );
        Ok(())
    }
}

fn suspicious_grid(cells: &[latexsnipper_inference::GridCell]) -> bool {
    if cells.is_empty() {
        return true;
    }

    let rows = cells.iter().map(|c| c.row).max().unwrap_or(0) + 1;
    let cols = cells.iter().map(|c| c.col).max().unwrap_or(0) + 1;
    let dense_slots = rows.saturating_mul(cols);

    cells.len() > 120
        || rows > 16
        || cols > 12
        || ((rows > 8 || cols > 8) && dense_slots > cells.len().saturating_mul(2))
}
