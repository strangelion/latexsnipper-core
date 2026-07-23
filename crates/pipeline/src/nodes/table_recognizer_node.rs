use async_trait::async_trait;
use latexsnipper_ast::*;
use latexsnipper_foundation::Result;
use latexsnipper_image::operations;
use latexsnipper_runtime::{InferenceContext, ModelInput, ModelOutput, ModelTask, TensorDtype};

use crate::artifacts::RecognizedTable;
use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::text_recognition_service::TextRecognitionService;

fn recognized_text(text: impl Into<String>, confidence: f32) -> TextRun {
    let mut run = TextRun::new(text);
    run.source = Some(SourceInfo::new().with_confidence(confidence.clamp(0.0, 1.0)));
    run
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
            let left_x = left.geometry.as_ref().map_or(0.0, |g| g.x);
            let right_x = right.geometry.as_ref().map_or(0.0, |g| g.x);
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
        self.recognize_tables(ctx).await
    }
}

impl TableRecognizerNode {
    async fn recognize_tables(&self, ctx: &mut PipelineContext) -> Result<()> {
        let tables = ctx.artifacts.table_structures.clone();
        if tables.is_empty() {
            return Ok(());
        }

        let image = match &ctx.image {
            Some(img) => img.clone(),
            None => return Ok(()),
        };

        log::info!("TableRecognizer: processing {} tables", tables.len());

        let mut formula_det = ctx.create_model_executor(ModelTask::FormulaDetection)?;
        let mut formula_rec = ctx.create_model_executor(ModelTask::FormulaRecognition)?;
        let text_rec = ctx.get_or_init_text_rec_service();

        let mut table_blocks = Vec::new();
        for table in &tables {
            if let Some(block) = self
                .recognize_single_table(
                    ctx,
                    &image,
                    table,
                    &mut formula_det,
                    &mut formula_rec,
                    text_rec.as_deref(),
                )
                .await
            {
                table_blocks.push(block);
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
        image: &latexsnipper_image::SnipperImage,
        table: &RecognizedTable,
        formula_det: &mut Option<Box<dyn latexsnipper_runtime::ModelExecutor>>,
        formula_rec: &mut Option<Box<dyn latexsnipper_runtime::ModelExecutor>>,
        text_rec_service: Option<&TextRecognitionService>,
    ) -> Option<Block> {
        let cells = &table.cells;
        if cells.is_empty() {
            return None;
        }
        let mut contents = Vec::with_capacity(cells.len());
        for cell in cells {
            contents.push(
                self.recognize_cell(
                    ctx,
                    image,
                    &cell.rect,
                    formula_det,
                    formula_rec,
                    text_rec_service,
                )
                .await,
            );
        }
        Some(build_table_block(table, contents))
    }

    async fn recognize_cell(
        &self,
        ctx: &mut PipelineContext,
        image: &latexsnipper_image::SnipperImage,
        rect: &Rect,
        formula_det: &mut Option<Box<dyn latexsnipper_runtime::ModelExecutor>>,
        formula_rec: &mut Option<Box<dyn latexsnipper_runtime::ModelExecutor>>,
        text_rec_service: Option<&TextRecognitionService>,
    ) -> Vec<Inline> {
        let w = rect.width as u32;
        let h = rect.height as u32;
        if w < 4 || h < 4 {
            return vec![];
        }
        let cropped = operations::crop(image, *rect);

        // Try formula detection via ModelExecutor
        if let Some(ref mut det) = formula_det {
            let pixels = cropped.pixels().to_vec();
            let shape = vec![cropped.height() as usize, cropped.width() as usize, 3];
            let input = ModelInput {
                name: "image".to_string(),
                data: pixels,
                shape,
                dtype: TensorDtype::UInt8,
            };
            let mut inf_ctx = InferenceContext::new();
            match det.run(input, &mut inf_ctx) {
                Ok(ModelOutput::Detections(raw)) => {
                    let mut boxes: Vec<latexsnipper_inference::DetectionBox> = raw
                        .into_iter()
                        .map(|d| {
                            let r = Rect::new(d.x, d.y, d.width, d.height);
                            match d.quad {
                                Some(q) => latexsnipper_inference::DetectionBox::quad(
                                    Quad::new(
                                        Point::new(q.x1, q.y1),
                                        Point::new(q.x2, q.y2),
                                        Point::new(q.x3, q.y3),
                                        Point::new(q.x4, q.y4),
                                    ),
                                    d.confidence,
                                    d.class_id,
                                    d.class_name,
                                ),
                                None => latexsnipper_inference::DetectionBox::rect(
                                    r,
                                    d.confidence,
                                    d.class_id,
                                    d.class_name,
                                ),
                            }
                        })
                        .collect();

                    latexsnipper_inference::group_formula_detections(&mut boxes);
                    latexsnipper_inference::filter_formula_detections(&mut boxes, 20.0, 0.2);

                    if !boxes.is_empty() {
                        if let Some(ref mut rec) = formula_rec {
                            let mut inlines = Vec::new();
                            let mut has_formula = false;
                            for det in &boxes {
                                let dx = det.rect.x as u32;
                                let dy = det.rect.y as u32;
                                let dw = det.rect.width as u32;
                                let dh = det.rect.height as u32;
                                if dw >= 4 && dh >= 4 {
                                    let fc = operations::crop(
                                        &cropped,
                                        Rect::new(dx as f32, dy as f32, dw as f32, dh as f32),
                                    );
                                    let p = fc.pixels().to_vec();
                                    let s = vec![fc.height() as usize, fc.width() as usize, 3];
                                    let ri = ModelInput {
                                        name: "image".to_string(),
                                        data: p,
                                        shape: s,
                                        dtype: TensorDtype::UInt8,
                                    };
                                    let mut rc = InferenceContext::new();
                                    match rec.run(ri, &mut rc) {
                                        Ok(ModelOutput::Formula(results)) => {
                                            for r in results {
                                                let mut f = Formula::latex(r.latex);
                                                f.confidence = r.confidence;
                                                inlines.push(Inline::Formula(f));
                                                has_formula = true;
                                            }
                                        }
                                        Ok(_other) => {
                                            log::warn!(
                                                "Table cell formula rec: unexpected output type"
                                            );
                                        }
                                        Err(e) => {
                                            ctx.diagnostic_warn(
                                                "recognize_table",
                                                format!("Table cell formula rec failed: {e}"),
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
                Ok(_other) => {
                    ctx.diagnostic_warn(
                        "recognize_table",
                        "Table cell formula detection: unexpected output type",
                    );
                }
                Err(e) => {
                    ctx.diagnostic_warn(
                        "recognize_table",
                        format!("Table cell formula detection failed: {e}"),
                    );
                }
            }
        }

        // Fall back to text recognition
        if let Some(service) = text_rec_service {
            if let Ok(result) = service.recognize_region_result(
                &cropped,
                &Rect::new(0.0, 0.0, w as f32, h as f32),
                None,
            ) {
                if !result.text.is_empty() {
                    return vec![Inline::Text(recognized_text(
                        result.text,
                        result.confidence,
                    ))];
                }
            }
        }

        vec![]
    }
}
