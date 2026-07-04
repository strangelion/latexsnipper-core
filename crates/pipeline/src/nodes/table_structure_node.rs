use async_trait::async_trait;
use latexsnipper_ast::Rect;
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;
use latexsnipper_inference::recognize_table_structure;
use latexsnipper_runtime::{AccelerationMode, InferenceSession};

use crate::artifacts::RecognizedTable;
use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::nodes::utils::resolve_model_handle;

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

impl TableStructureNode {
    pub fn new() -> Self {
        Self {
            name: "table_structure".into(),
            backend: "tatr".into(),
        }
    }

    pub fn with_backend(backend: impl Into<String>) -> Self {
        let b: String = backend.into();
        Self {
            name: format!("table_structure_{}", b),
            backend: b,
        }
    }

    fn backend_model_path(&self, models: &std::path::Path) -> Option<std::path::PathBuf> {
        let category = match self.backend.as_str() {
            "tatr" => "table-struct/tatr-structure",
            "slanet" => "table-struct/slanet-plus",
            _ => return None,
        };

        if let Some((_config, path, _dir)) =
            latexsnipper_model::ModelConfig::find_best(models, category)
        {
            return Some(path);
        }

        let path = models.join(category).join("model.onnx");
        if path.exists() {
            Some(path)
        } else {
            None
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

        // Load backend model if needed
        let backend_session: Option<Box<dyn InferenceSession>> =
            (|| -> Option<Box<dyn InferenceSession>> {
                let model_path = self.backend_model_path(&models)?;
                let handle = resolve_model_handle(ctx, &self.backend, model_path).ok()?;
                let backend = ctx.backend.as_ref()?;
                backend.create_session(&handle, AccelerationMode::Cpu).ok()
            })();

        if self.backend.as_str() != "projection" && backend_session.is_none() {
            log::warn!(
                "Table structure model for '{}' not found, skipping",
                self.backend
            );
            return Ok(());
        }

        log::info!(
            "TableStructure: parsing {} table regions with backend '{}'",
            detections.len(),
            self.backend
        );

        let mut all_tables: Vec<RecognizedTable> = Vec::new();

        for det in &detections {
            let x = det.rect.x;
            let y = det.rect.y;
            let w = det.rect.width;
            let h = det.rect.height;

            let table_rect = Rect::new(x, y, w, h);
            let cropped = operations::crop(&image, table_rect);

            let grid = if let Some(ref sess) = backend_session {
                recognize_table_structure(&cropped, &self.backend, Some(&**sess))?
            } else {
                recognize_table_structure(&cropped, "projection", None)?
            };

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
            "TableStructure: parsed {} tables via '{}'",
            ctx.artifacts.table_structures.len(),
            self.backend
        );
        Ok(())
    }
}
