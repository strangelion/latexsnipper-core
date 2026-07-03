use std::sync::Arc;

use async_trait::async_trait;
use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::operations;
use latexsnipper_inference::{
    detect_formulas, filter_formula_detections, group_formula_detections, load_keys,
    recognize_formula, recognize_text_with_keys, DetectionParams, RecognitionParams, TextRecParams,
};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, RuntimeBackend};

use crate::context::PipelineContext;
use crate::node::PipelineNode;

type InferenceArc = Arc<Box<dyn latexsnipper_runtime::InferenceSession>>;
type FormulaRecSession = (InferenceArc, InferenceArc, std::path::PathBuf);
type TextRecSession = (InferenceArc, std::path::PathBuf);

/// Recognizes content in table cells.
///
/// For each cell in the parsed table structure, this node:
/// 1. Crops the cell region
/// 2. Determines if the cell contains text or formula
/// 3. Recognizes the content
/// 4. Builds the TableBlock with recognized content
///
/// Session caching: ONNX sessions are cached in PipelineContext so they are
/// created only once and reused across all tables in a single pipeline run.
pub struct TableRecognizerNode {
    name: String,
}

impl TableRecognizerNode {
    pub fn new() -> Self {
        Self {
            name: "recognize_table".into(),
        }
    }

    fn get_backend(ctx: &PipelineContext) -> Result<Arc<dyn RuntimeBackend>> {
        ctx.backend
            .clone()
            .ok_or_else(|| SnipperError::Runtime("No backend configured".into()))
    }
}

impl Default for TableRecognizerNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for TableRecognizerNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let models = match &ctx.models_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        self.recognize_tables(ctx, &models).await
    }
}

impl TableRecognizerNode {
    async fn recognize_tables(
        &self,
        ctx: &mut PipelineContext,
        models: &std::path::Path,
    ) -> Result<()> {
        let structures = match ctx.get("table_structures") {
            Some(v) => v.clone(),
            None => return Ok(()),
        };

        let structures_array = match structures.as_array() {
            Some(a) => a.clone(),
            None => return Ok(()),
        };

        if structures_array.is_empty() {
            return Ok(());
        }

        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        log::info!(
            "TableRecognizer: processing {} tables",
            structures_array.len()
        );

        let mut table_blocks = Vec::new();

        // Load models ONCE (with ctx session caching) for all tables
        let backend = Self::get_backend(ctx)?;
        let formula_det_session = self.load_formula_det_session(ctx, &*backend, models)?;
        let formula_rec_session = self.load_formula_rec_session(ctx, &*backend, models)?;
        let text_rec_session = self.load_text_rec_session(ctx, &*backend, models)?;

        for structure_val in &structures_array {
            if let Some(table_block) = self
                .recognize_single_table(
                    image.clone(),
                    structure_val,
                    &formula_det_session,
                    &formula_rec_session,
                    &text_rec_session,
                )
                .await?
            {
                table_blocks.push(table_block);
            }
        }

        ctx.set(
            "table_blocks",
            serde_json::to_value(&table_blocks).unwrap_or_default(),
        );

        log::info!("Recognized {} table blocks", table_blocks.len());
        Ok(())
    }

    async fn recognize_single_table(
        &self,
        image: latexsnipper_image::SnipperImage,
        structure_val: &serde_json::Value,
        formula_det_session: &Option<InferenceArc>,
        formula_rec_session: &Option<FormulaRecSession>,
        text_rec_session: &Option<TextRecSession>,
    ) -> Result<Option<Block>> {
        let table_rect = structure_val
            .get("rect")
            .ok_or_else(|| SnipperError::Inference("Table structure missing rect".into()))?;

        let x = table_rect.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let y = table_rect.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let w = table_rect.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let h = table_rect.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;

        let cells = structure_val
            .get("cells")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        if cells.is_empty() {
            return Ok(None);
        }

        // Determine max row and col
        let max_row = cells
            .iter()
            .filter_map(|c| c.get("row").and_then(|r| r.as_u64()))
            .max()
            .unwrap_or(0) as usize;
        let _max_col = cells
            .iter()
            .filter_map(|c| c.get("col").and_then(|r| r.as_u64()))
            .max()
            .unwrap_or(0) as usize;

        // Create 2D grid for rows and columns
        let mut rows: Vec<Vec<TableCell>> = vec![vec![]; max_row + 1];

        for cell_val in &cells {
            let row = cell_val.get("row").and_then(|r| r.as_u64()).unwrap_or(0) as usize;
            let _col = cell_val.get("col").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
            let rowspan = cell_val
                .get("rowspan")
                .and_then(|r| r.as_u64())
                .unwrap_or(1) as u32;
            let colspan = cell_val
                .get("colspan")
                .and_then(|c| c.as_u64())
                .unwrap_or(1) as u32;

            let cell_rect = cell_val.get("rect").map(|r| {
                let x = r.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let y = r.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let w = r.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                let h = r.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                Rect::new(x, y, w, h)
            });

            let geometry = cell_rect;
            let source = Some(SourceInfo::new());

            // Recognize cell content
            let inlines = if let Some(ref rect) = cell_rect {
                self.recognize_cell_content(
                    &image,
                    rect,
                    formula_det_session,
                    formula_rec_session,
                    text_rec_session,
                )
                .await
            } else {
                vec![]
            };

            let cell = TableCell {
                inlines,
                colspan,
                rowspan,
                geometry,
                source,
            };

            if row < rows.len() {
                rows[row].push(cell);
            }
        }

        // Sort cells in each row by column position
        for row in &mut rows {
            row.sort_by(|a, b| {
                let ax = a.geometry.as_ref().map_or(0.0, |g| g.x);
                let bx = b.geometry.as_ref().map_or(0.0, |g| g.x);
                ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        let table_block = Block::Table(TableBlock {
            rows,
            geometry: Some(Rect::new(x, y, w, h)),
            source: Some(SourceInfo::new()),
        });

        Ok(Some(table_block))
    }

    /// Load formula detection session, using ctx session cache.
    fn load_formula_det_session(
        &self,
        ctx: &mut PipelineContext,
        backend: &dyn RuntimeBackend,
        models: &std::path::Path,
    ) -> Result<Option<Arc<Box<dyn latexsnipper_runtime::InferenceSession>>>> {
        // Check cache first
        if let Some(s) = ctx.get_session("formula_det") {
            return Ok(Some(s));
        }

        let det_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
        if !det_path.exists() {
            return Ok(None);
        }

        let handle = ModelHandle::with_path("formula-det", det_path);
        let session = backend.create_session(&handle, AccelerationMode::Cpu)?;
        ctx.cache_session("formula_det", session);
        Ok(ctx.get_session("formula_det"))
    }

    /// Load formula recognition sessions (encoder + decoder), using ctx session cache.
    fn load_formula_rec_session(
        &self,
        ctx: &mut PipelineContext,
        backend: &dyn RuntimeBackend,
        models: &std::path::Path,
    ) -> Result<Option<FormulaRecSession>> {
        let enc_path = models.join("formula-rec/trocr-deit/encoder_model.onnx");
        let dec_path = models.join("formula-rec/trocr-deit/decoder_model.onnx");
        let tok_path = models.join("formula-rec/trocr-deit/tokenizer.json");

        if !enc_path.exists() || !dec_path.exists() || !tok_path.exists() {
            return Ok(None);
        }

        let enc_session = match ctx.get_session("formula_encoder") {
            Some(s) => s,
            None => {
                let enc_handle = ModelHandle::with_path("encoder", enc_path);
                let s = backend.create_session(&enc_handle, AccelerationMode::Cpu)?;
                ctx.cache_session("formula_encoder", s);
                ctx.get_session("formula_encoder").unwrap()
            }
        };

        let dec_session = match ctx.get_session("formula_decoder") {
            Some(s) => s,
            None => {
                let dec_handle = ModelHandle::with_path("decoder", dec_path);
                let s = backend.create_session(&dec_handle, AccelerationMode::Cpu)?;
                ctx.cache_session("formula_decoder", s);
                ctx.get_session("formula_decoder").unwrap()
            }
        };

        Ok(Some((enc_session, dec_session, tok_path)))
    }

    /// Load text recognition session, using ctx session cache.
    fn load_text_rec_session(
        &self,
        ctx: &mut PipelineContext,
        backend: &dyn RuntimeBackend,
        models: &std::path::Path,
    ) -> Result<Option<TextRecSession>> {
        // Check cache
        if let Some(s) = ctx.get_session("text_rec") {
            let keys_path = self.find_text_rec_keys(models);
            return Ok(Some((s, keys_path)));
        }

        let rec_path = self.find_text_rec_model(models);
        let keys_path = self.find_text_rec_keys(models);

        if rec_path.is_none() {
            return Ok(None);
        }

        let handle = ModelHandle::with_path("text-rec", rec_path.unwrap());
        let session = backend.create_session(&handle, AccelerationMode::Cpu)?;
        ctx.cache_session("text_rec", session);
        Ok(ctx.get_session("text_rec").map(|s| (s, keys_path)))
    }

    fn find_text_rec_model(&self, models: &std::path::Path) -> Option<std::path::PathBuf> {
        let candidates = [
            models.join("v6_models/PP-OCRv6_small_rec_infer/inference.onnx"),
            models.join("v6_models/PP-OCRv6_small_rec_infer/model.onnx"),
            models.join("text-rec/v6-small/inference.onnx"),
            models.join("text-rec/v6-small/text-rec.onnx"),
        ];
        candidates.iter().find(|p| p.exists()).cloned()
    }

    fn find_text_rec_keys(&self, models: &std::path::Path) -> std::path::PathBuf {
        let candidates = [
            models.join("v6_models/PP-OCRv6_small_rec_infer/ppocr_keys.txt"),
            models.join("v6_models/PP-OCRv6_small_rec_infer/inference.yml"),
            models.join("text-rec/v6-small/ppocr_keys.txt"),
            models.join("text-rec/v6-small/inference.yml"),
        ];
        candidates
            .iter()
            .find(|p| p.exists())
            .cloned()
            .unwrap_or_else(|| models.join("text-rec/v6-small/inference.yml"))
    }

    async fn recognize_cell_content(
        &self,
        image: &latexsnipper_image::SnipperImage,
        rect: &Rect,
        formula_det: &Option<InferenceArc>,
        formula_rec: &Option<FormulaRecSession>,
        text_rec: &Option<TextRecSession>,
    ) -> Vec<Inline> {
        let w = rect.width as u32;
        let h = rect.height as u32;

        if w < 4 || h < 4 {
            return vec![];
        }

        let cropped = operations::crop(image, *rect);

        // Try formula detection first
        if let Some(ref det_session) = formula_det {
            let det_params = DetectionParams::default();
            if let Ok(mut detections) = detect_formulas(&cropped, &**det_session, &det_params) {
                group_formula_detections(&mut detections);
                filter_formula_detections(&mut detections, 20.0, 0.2);

                if !detections.is_empty() {
                    if let Some((ref enc, ref dec, ref tok)) = formula_rec {
                        let rec_params = RecognitionParams::default();
                        let mut inlines: Vec<Inline> = Vec::new();
                        let mut has_formula = false;
                        for det in &detections {
                            let dx = det.rect.x as u32;
                            let dy = det.rect.y as u32;
                            let dw = det.rect.width as u32;
                            let dh = det.rect.height as u32;

                            if dw >= 4 && dh >= 4 {
                                let formula_crop = operations::crop(
                                    &cropped,
                                    Rect::new(dx as f32, dy as f32, dw as f32, dh as f32),
                                );
                                if let Ok(result) = recognize_formula(
                                    &formula_crop,
                                    &**enc,
                                    &**dec,
                                    tok,
                                    &rec_params,
                                ) {
                                    let formula = Formula::latex(result.text);
                                    inlines.push(Inline::Formula(formula));
                                    has_formula = true;
                                }
                            }
                        }
                        if has_formula {
                            return inlines;
                        }
                    }
                }
            }
        }

        // No formula detected — try text recognition on the whole cell
        if let Some((ref rec_session, ref keys_path)) = text_rec {
            let (keys, first_char_id) = if let Some(chars) = rec_session.get_character_list() {
                (chars, 1)
            } else {
                load_keys(keys_path).unwrap_or_default()
            };
            let rec_params = TextRecParams::default();
            if let Ok(result) = recognize_text_with_keys(
                &cropped,
                &**rec_session,
                &keys,
                first_char_id,
                &rec_params,
            ) {
                if !result.text.trim().is_empty() {
                    return vec![Inline::Text(TextRun::new(result.text))];
                }
            }
        }

        vec![]
    }
}
