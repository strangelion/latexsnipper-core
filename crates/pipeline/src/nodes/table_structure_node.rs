use async_trait::async_trait;
use latexsnipper_ast::Rect;
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;
use latexsnipper_inference::recognize_table_structure;
use latexsnipper_runtime::{AccelerationMode, InferenceSession, ModelHandle};

use crate::context::PipelineContext;
use crate::node::PipelineNode;

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
        let path = match self.backend.as_str() {
            "tatr" => models.join("table-struct/tatr-structure/model.onnx"),
            "slanet" => models.join("table-struct/slanet-plus/model.onnx"),
            _ => return None,
        };
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
        let detections = match ctx.get("table_detections") {
            Some(v) => v.clone(),
            None => return Ok(()),
        };

        let det_array = match detections.as_array() {
            Some(a) => a.clone(),
            None => return Ok(()),
        };

        if det_array.is_empty() {
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
                let backend = ctx.backend.as_ref()?;
                let handle = ModelHandle::with_path(&self.backend, model_path);
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
            det_array.len(),
            self.backend
        );

        let mut all_structures = Vec::new();

        for det_val in &det_array {
            if let Some(rect_val) = det_val.get("rect") {
                let x = rect_val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let y = rect_val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let w = rect_val.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let h = rect_val.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

                let table_rect = Rect::new(x, y, w, h);
                let cropped = operations::crop(&image, table_rect);

                let grid = if let Some(ref sess) = backend_session {
                    recognize_table_structure(&cropped, &self.backend, Some(&**sess))?
                } else {
                    recognize_table_structure(&cropped, "projection", None)?
                };

                let grid = match grid {
                    Some(g) => g,
                    None => continue,
                };

                if grid.is_empty() {
                    log::warn!("Grid is empty for table at ({}, {})", x, y);
                    continue;
                }

                let cells_json: Vec<serde_json::Value> = grid
                    .iter()
                    .map(|cell| {
                        serde_json::json!({
                            "row": cell.row,
                            "col": cell.col,
                            "rowspan": cell.rowspan,
                            "colspan": cell.colspan,
                            "rect": {
                                "x": cell.rect.x + x,
                                "y": cell.rect.y + y,
                                "w": cell.rect.width,
                                "h": cell.rect.height,
                            }
                        })
                    })
                    .collect();

                all_structures.push(serde_json::json!({
                    "rect": {"x": x, "y": y, "w": w, "h": h},
                    "cells": cells_json,
                }));
            }
        }

        ctx.set("table_structures", serde_json::json!(all_structures));
        log::info!(
            "TableStructure: parsed {} tables via '{}'",
            all_structures.len(),
            self.backend
        );
        Ok(())
    }
}
