use std::sync::Arc;

use async_trait::async_trait;
use latexsnipper_ast::*;
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;
use latexsnipper_inference::{
    detect_formulas, filter_formula_detections, group_formula_detections, recognize_formula,
    DetectionParams, RecognitionParams,
};
use latexsnipper_runtime::RuntimeBackend;

use crate::artifacts::RecognizedTable;
use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::nodes::utils::{get_backend, resolve_model_handle};
use crate::text_recognition_service::TextRecognitionService;

type InferenceArc = Arc<Box<dyn latexsnipper_runtime::InferenceSession>>;
type FormulaRecSession = (InferenceArc, InferenceArc, std::path::PathBuf);

fn recognized_text(text: impl Into<String>, confidence: f32) -> TextRun {
    let mut run = TextRun::new(text);
    run.source = Some(SourceInfo::new().with_confidence(confidence.clamp(0.0, 1.0)));
    run
}

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
}

fn build_table_block(table: &RecognizedTable, recognized_contents: Vec<Vec<Inline>>) -> Block {
    let max_row = table.cells.iter().map(|cell| cell.row).max().unwrap_or(0);
    let mut rows: Vec<Vec<TableCell>> = vec![Vec::new(); max_row + 1];

    for (cell, inlines) in table.cells.iter().zip(recognized_contents) {
        let content = if inlines.is_empty() {
            Vec::new()
        } else {
            vec![Block::Paragraph(ParagraphBlock {
                inlines,
                geometry: None,
                source: None,
                style: None,
            })]
        };
        rows[cell.row].push(TableCell {
            content,
            colspan: cell.colspan,
            rowspan: cell.rowspan,
            data_type: None,
            formula: None,
            style: None,
            border_style: None,
            border_width: None,
            border_color: None,
            background: None,
            alignment: None,
            geometry: Some(cell.rect),
            source: Some(SourceInfo::new()),
        });
    }

    for cells in &mut rows {
        cells.sort_by(|left, right| {
            let left_x = left.geometry.as_ref().map_or(0.0, |geometry| geometry.x);
            let right_x = right.geometry.as_ref().map_or(0.0, |geometry| geometry.x);
            left_x.total_cmp(&right_x)
        });
    }

    Block::Table(TableBlock {
        rows: rows
            .into_iter()
            .map(|cells| TableRow {
                cells,
                height: None,
                is_header: false,
            })
            .collect(),
        columns: Vec::new(),
        caption: None,
        style: None,
        geometry: Some(table.table_rect),
        source: Some(SourceInfo::new()),
    })
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
        let tables = ctx.artifacts.table_structures.clone();
        if tables.is_empty() {
            return Ok(());
        }

        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        log::info!("TableRecognizer: processing {} tables", tables.len());

        // Load models ONCE (with ctx session caching) for all tables
        let backend = get_backend(ctx)?;
        let formula_det_session = self.load_formula_det_session(ctx, &*backend, models)?;
        let formula_rec_session = self.load_formula_rec_session(ctx, &*backend, models)?;
        let text_rec_service = ctx.get_or_init_text_rec_service();

        let mut table_blocks = Vec::new();

        for table in &tables {
            if let Some(table_block) = self
                .recognize_single_table(
                    ctx,
                    image.clone(),
                    table,
                    &formula_det_session,
                    &formula_rec_session,
                    text_rec_service.as_deref(),
                )
                .await?
            {
                table_blocks.push(table_block);
            }
        }

        ctx.artifacts.table_blocks = table_blocks;

        log::info!(
            "Recognized {} table blocks",
            ctx.artifacts.table_blocks.len()
        );
        Ok(())
    }

    async fn recognize_single_table(
        &self,
        ctx: &mut PipelineContext,
        image: latexsnipper_image::SnipperImage,
        table: &RecognizedTable,
        formula_det_session: &Option<InferenceArc>,
        formula_rec_session: &Option<FormulaRecSession>,
        text_rec_service: Option<&TextRecognitionService>,
    ) -> Result<Option<Block>> {
        let cells = &table.cells;
        if cells.is_empty() {
            return Ok(None);
        }

        let mut recognized_contents = Vec::with_capacity(cells.len());
        for cell in cells {
            let inlines = self
                .recognize_cell_content(
                    ctx,
                    &image,
                    &cell.rect,
                    formula_det_session,
                    formula_rec_session,
                    text_rec_service,
                )
                .await;
            recognized_contents.push(inlines);
        }

        Ok(Some(build_table_block(table, recognized_contents)))
    }

    /// Load formula detection session, using ctx session cache.
    ///
    /// Respects ctx.model_variants for variant selection.
    fn load_formula_det_session(
        &self,
        ctx: &mut PipelineContext,
        backend: &dyn RuntimeBackend,
        models: &std::path::Path,
    ) -> Result<Option<Arc<Box<dyn latexsnipper_runtime::InferenceSession>>>> {
        if let Some(s) = ctx.get_session("formula_det") {
            return Ok(Some(s));
        }

        let variant = ctx
            .model_variants
            .get("formula-det")
            .cloned()
            .unwrap_or_else(|| "yolov8-mfd".into());

        let det_path = models.join(format!("formula-det/{}/mathcraft-mfd.onnx", variant));
        if !det_path.exists() {
            return Ok(None);
        }

        let handle = resolve_model_handle(ctx, "formula-det", det_path)?;
        let session = backend.create_session(&handle, ctx.acceleration)?;
        ctx.cache_session("formula_det", session);
        Ok(ctx.get_session("formula_det"))
    }

    /// Load formula recognition sessions (encoder + decoder), using ctx session cache.
    ///
    /// Respects ctx.model_variants for variant selection.
    fn load_formula_rec_session(
        &self,
        ctx: &mut PipelineContext,
        backend: &dyn RuntimeBackend,
        models: &std::path::Path,
    ) -> Result<Option<FormulaRecSession>> {
        let variant = ctx
            .model_variants
            .get("formula-rec")
            .cloned()
            .unwrap_or_else(|| "trocr-deit".into());

        let variant_dir = models.join(format!("formula-rec/{}", variant));
        let enc_path = variant_dir.join("encoder_model.onnx");
        let dec_path = variant_dir.join("decoder_model.onnx");
        let tok_path = variant_dir.join("tokenizer.json");

        if !enc_path.exists() || !dec_path.exists() || !tok_path.exists() {
            return Ok(None);
        }

        let enc_session = match ctx.get_session("formula_encoder") {
            Some(s) => s,
            None => {
                let enc_handle = resolve_model_handle(
                    ctx,
                    &format!("formula-rec/{}/encoder", variant),
                    enc_path,
                )?;
                let s = backend.create_session(&enc_handle, ctx.acceleration)?;
                ctx.cache_session("formula_encoder", s);
                ctx.get_session("formula_encoder").unwrap()
            }
        };

        let dec_session = match ctx.get_session("formula_decoder") {
            Some(s) => s,
            None => {
                let dec_handle = resolve_model_handle(
                    ctx,
                    &format!("formula-rec/{}/decoder", variant),
                    dec_path,
                )?;
                let s = backend.create_session(&dec_handle, ctx.acceleration)?;
                ctx.cache_session("formula_decoder", s);
                ctx.get_session("formula_decoder").unwrap()
            }
        };

        Ok(Some((enc_session, dec_session, tok_path)))
    }

    async fn recognize_cell_content(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        rect: &Rect,
        formula_det: &Option<InferenceArc>,
        formula_rec: &Option<FormulaRecSession>,
        text_rec_service: Option<&TextRecognitionService>,
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
            match detect_formulas(&cropped, &**det_session, &det_params) {
                Ok(mut detections) => {
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
                                    match recognize_formula(
                                        &formula_crop,
                                        &**enc,
                                        &**dec,
                                        tok,
                                        &rec_params,
                                    ) {
                                        Ok(result) => {
                                            let mut formula = Formula::latex(result.text);
                                            formula.confidence = result.confidence;
                                            inlines.push(Inline::Formula(formula));
                                            has_formula = true;
                                        }
                                        Err(e) => {
                                            ctx.diagnostic_error(
                                                "recognize_table",
                                                format!("Formula recognition failed in cell at ({:.0},{:.0}): {}", dx, dy, e),
                                            );
                                        }
                                    }
                                }
                            }
                            if has_formula {
                                return inlines;
                            }
                        }
                    }
                }
                Err(e) => {
                    ctx.diagnostic_error(
                        "recognize_table",
                        format!("Formula detection failed in table: {}", e),
                    );
                }
            }
        }

        // No formula detected — try shared text recognition service
        if let Some(service) = text_rec_service {
            match service.recognize_region_result(image, rect, None) {
                Ok(result) if !result.text.trim().is_empty() => {
                    return vec![Inline::Text(recognized_text(
                        result.text,
                        result.confidence,
                    ))];
                }
                Ok(_) => {}
                Err(e) => {
                    ctx.diagnostic_error(
                        "recognize_table",
                        format!("Text recognition failed in cell: {}", e),
                    );
                }
            }
        }

        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_inference::GridCell;

    #[test]
    fn table_block_preserves_geometry_merges_empty_and_multilingual_cells() {
        let table = RecognizedTable {
            table_rect: Rect::new(0.0, 0.0, 200.0, 100.0),
            cells: vec![
                GridCell {
                    row: 0,
                    col: 0,
                    rowspan: 1,
                    colspan: 2,
                    rect: Rect::new(0.0, 0.0, 200.0, 50.0),
                },
                GridCell {
                    row: 1,
                    col: 0,
                    rowspan: 1,
                    colspan: 1,
                    rect: Rect::new(0.0, 50.0, 100.0, 50.0),
                },
                GridCell {
                    row: 1,
                    col: 1,
                    rowspan: 1,
                    colspan: 1,
                    rect: Rect::new(100.0, 50.0, 100.0, 50.0),
                },
            ],
        };
        let header = "\u{59d3}\u{540d} Name";
        let multilingual = "Alice \u{793a}\u{4f8b}";
        let block = build_table_block(
            &table,
            vec![
                vec![Inline::Text(TextRun::new(header))],
                Vec::new(),
                vec![Inline::Text(recognized_text(multilingual, 0.91))],
            ],
        );
        let Block::Table(table) = block else {
            panic!("expected table block");
        };
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0].cells[0].colspan, 2);
        assert_eq!(table.rows[0].cells[0].geometry.unwrap().width, 200.0);
        assert!(table.rows[1].cells[0].content.is_empty());
        let inlines = table.rows[1].cells[1].collect_inlines();
        let Inline::Text(text) = &inlines[0] else {
            panic!("expected multilingual text cell");
        };
        assert_eq!(text.text, multilingual);
        assert_eq!(
            text.source.as_ref().and_then(|source| source.confidence),
            Some(0.91)
        );
    }
}
