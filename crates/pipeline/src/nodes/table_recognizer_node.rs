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
        if let Some(store) = ctx.crop_artifact_store().cloned() {
            match latexsnipper_image::decode::encode_png(&cropped)
                .map_err(|error| error.to_string())
                .and_then(|png| {
                    store
                        .save_png(&png, *rect, image.pixels())
                        .map_err(|error| error.to_string())
                }) {
                Ok(reference) => {
                    ctx.artifacts
                        .artifact_graph
                        .insert(latexsnipper_artifact::ArtifactRecord {
                            id: latexsnipper_artifact::ArtifactId::from(
                                reference.artifact_ref.clone(),
                            ),
                            kind: latexsnipper_artifact::ArtifactKind::CroppedRegion,
                            stable_id: None,
                            content_ref: Some(reference.content_ref.clone()),
                            checksum: Some(reference.crop_hash.clone()),
                            provenance: Vec::new(),
                        });
                    if let Ok(evidence) = serde_json::to_string(&reference) {
                        ctx.diagnostic_info(
                            "recognize_table",
                            format!("TABLE_CELL_CROP_REFERENCE {evidence}"),
                        );
                    }
                }
                Err(error) => ctx.diagnostic_warn(
                    "recognize_table",
                    format!("debug crop artifact was not saved: {error}"),
                ),
            }
        }
        let mut route = latexsnipper_inference::CellRecognitionRoute::TextOnly;
        let mut detector_overlap = 0.0f32;
        let mut formula_candidate: Option<(Vec<Inline>, String, f32)> = None;

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
                        let detector_confidence = boxes
                            .iter()
                            .map(|item| item.confidence)
                            .fold(0.0f32, f32::max);
                        detector_overlap = (boxes
                            .iter()
                            .map(|item| item.rect.width * item.rect.height)
                            .sum::<f32>()
                            / (rect.width * rect.height).max(1.0))
                        .clamp(0.0, 1.0);
                        route = latexsnipper_inference::cell_recognition_route(detector_confidence);
                        if route != latexsnipper_inference::CellRecognitionRoute::TextOnly {
                            if let Some(ref mut rec) = formula_rec {
                                let mut inlines = Vec::new();
                                let mut latex = Vec::new();
                                let mut confidences = Vec::new();
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
                                                    latex.push(r.latex.clone());
                                                    confidences.push(r.confidence);
                                                    let mut f = Formula::latex(r.latex);
                                                    f.confidence = r.confidence;
                                                    f.recognition_provenance =
                                                        r.provenance.map(Box::new);
                                                    f.recognition_evidence =
                                                        r.evidence.map(Box::new);
                                                    inlines.push(Inline::Formula(f));
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
                                if !inlines.is_empty() {
                                    let confidence = confidences.iter().sum::<f32>()
                                        / confidences.len().max(1) as f32;
                                    formula_candidate =
                                        Some((inlines, latex.join(" "), confidence));
                                }
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

        if route == latexsnipper_inference::CellRecognitionRoute::FormulaOnly {
            return formula_candidate
                .map(|candidate| candidate.0)
                .unwrap_or_default();
        }

        let text_candidate = text_rec_service.and_then(|service| {
            service
                .recognize_region_result(&cropped, &Rect::new(0.0, 0.0, w as f32, h as f32), None)
                .ok()
                .filter(|result| !result.text.is_empty())
                .map(|result| {
                    let inline =
                        Inline::Text(recognized_text(result.text.clone(), result.confidence));
                    (vec![inline], result.text, result.confidence)
                })
        });

        if route == latexsnipper_inference::CellRecognitionRoute::DualCandidate {
            match (formula_candidate, text_candidate) {
                (Some(formula), Some(text)) => {
                    let decision = latexsnipper_inference::select_ambiguous_cell_candidate(
                        latexsnipper_inference::CellCandidate {
                            kind: latexsnipper_inference::CellCandidateKind::Formula,
                            content: &formula.1,
                            confidence: formula.2,
                        },
                        latexsnipper_inference::CellCandidate {
                            kind: latexsnipper_inference::CellCandidateKind::Text,
                            content: &text.1,
                            confidence: text.2,
                        },
                        latexsnipper_inference::CellGeometryEvidence {
                            aspect_ratio: rect.width / rect.height.max(1.0),
                            detector_overlap,
                        },
                    );
                    if let Ok(evidence) = serde_json::to_string(&decision) {
                        ctx.diagnostic_info(
                            "recognize_table",
                            format!("TABLE_CELL_CANDIDATE_DECISION {evidence}"),
                        );
                    }
                    return match decision.selected_candidate {
                        latexsnipper_inference::CellCandidateKind::Formula => formula.0,
                        latexsnipper_inference::CellCandidateKind::Text => text.0,
                    };
                }
                (Some(formula), None) => return formula.0,
                (None, Some(text)) => return text.0,
                (None, None) => return Vec::new(),
            }
        }

        text_candidate
            .map(|candidate| candidate.0)
            .unwrap_or_default()
    }
}
