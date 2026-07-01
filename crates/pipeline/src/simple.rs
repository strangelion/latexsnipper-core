//! Simplified pipeline for demonstrating the Happy Path.
//! Uses mock implementations without requiring real ONNX models.

use latexsnipper_ast::{
    Block, Document, Formula, FormulaBlock, Inline, Page, ParagraphBlock, TextRun,
};
use latexsnipper_foundation::Result;

/// A simplified pipeline stage trait.
pub trait Stage: Send + Sync {
    fn name(&self) -> &str;
    fn process(&self, ctx: &mut PipelineContext) -> Result<()>;
}

/// Simplified pipeline context.
pub struct PipelineContext {
    pub document: Document,
    pub input_image: Option<String>,
    pub detections: Vec<Region>,
    pub crops: Vec<Crop>,
}

#[derive(Debug, Clone)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub class: RegionClass,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RegionClass {
    Formula,
    Text,
}

#[derive(Debug, Clone)]
pub struct Crop {
    pub region: Region,
    pub content: String,
}

impl PipelineContext {
    pub fn new() -> Self {
        Self {
            document: Document::new(),
            input_image: None,
            detections: Vec::new(),
            crops: Vec::new(),
        }
    }

    pub fn with_image(path: impl Into<String>) -> Self {
        let mut ctx = Self::new();
        ctx.input_image = Some(path.into());
        ctx
    }
}

impl Default for PipelineContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock detector that simulates finding formulas and text.
pub struct MockDetector {
    regions: Vec<Region>,
}

impl MockDetector {
    pub fn mock_formula_recognition() -> Self {
        Self {
            regions: vec![
                Region {
                    x: 100.0,
                    y: 50.0,
                    width: 200.0,
                    height: 40.0,
                    class: RegionClass::Formula,
                    confidence: 0.95,
                },
                Region {
                    x: 100.0,
                    y: 150.0,
                    width: 150.0,
                    height: 30.0,
                    class: RegionClass::Text,
                    confidence: 0.92,
                },
                Region {
                    x: 100.0,
                    y: 200.0,
                    width: 180.0,
                    height: 40.0,
                    class: RegionClass::Formula,
                    confidence: 0.88,
                },
            ],
        }
    }
}

impl Stage for MockDetector {
    fn name(&self) -> &str {
        "mock_detector"
    }

    fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        ctx.detections = self.regions.clone();
        log::info!("MockDetector: found {} regions", ctx.detections.len());
        Ok(())
    }
}

/// Mock cropper that extracts content from detected regions.
pub struct MockCropper;

impl Stage for MockCropper {
    fn name(&self) -> &str {
        "mock_cropper"
    }

    fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        ctx.crops = ctx
            .detections
            .iter()
            .enumerate()
            .map(|(i, region)| {
                let content = match region.class {
                    RegionClass::Formula => {
                        if i == 0 {
                            "E = mc^2".to_string()
                        } else {
                            "\\frac{a+b}{c}".to_string()
                        }
                    }
                    RegionClass::Text => "The quick brown fox".to_string(),
                };
                Crop {
                    region: region.clone(),
                    content,
                }
            })
            .collect();
        log::info!("MockCropper: created {} crops", ctx.crops.len());
        Ok(())
    }
}

/// Mock recognizer that produces AST blocks from crops.
pub struct MockRecognizer;

impl Stage for MockRecognizer {
    fn name(&self) -> &str {
        "mock_recognizer"
    }

    fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let mut blocks = Vec::new();

        // Sort by y position for proper document order
        let mut crops = ctx.crops.clone();
        crops.sort_by(|a, b| a.region.y.partial_cmp(&b.region.y).unwrap());

        for crop in &crops {
            match crop.region.class {
                RegionClass::Formula => {
                    let formula = Formula::latex(&crop.content);
                    blocks.push(Block::Formula(FormulaBlock {
                        formula,
                        geometry: Some(latexsnipper_ast::Rect::new(
                            crop.region.x,
                            crop.region.y,
                            crop.region.width,
                            crop.region.height,
                        )),
                        source: None,
                    }));
                }
                RegionClass::Text => {
                    blocks.push(Block::Paragraph(ParagraphBlock {
                        inlines: vec![Inline::Text(TextRun::new(&crop.content))],
                        geometry: Some(latexsnipper_ast::Rect::new(
                            crop.region.x,
                            crop.region.y,
                            crop.region.width,
                            crop.region.height,
                        )),
                        source: None,
                    }));
                }
            }
        }

        ctx.document.pages.push(Page {
            width: 800.0,
            height: 600.0,
            blocks,
            page_number: Some(1),
        });

        log::info!(
            "MockRecognizer: created document with {} blocks",
            ctx.document.block_count()
        );
        Ok(())
    }
}

/// A simple sequential pipeline.
pub struct SimplePipeline {
    name: String,
    stages: Vec<Box<dyn Stage>>,
}

impl SimplePipeline {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            stages: Vec::new(),
        }
    }

    pub fn add_stage(mut self, stage: Box<dyn Stage>) -> Self {
        self.stages.push(stage);
        self
    }

    pub fn run(&self, ctx: &mut PipelineContext) -> Result<()> {
        log::info!(
            "Pipeline '{}' starting with {} stages",
            self.name,
            self.stages.len()
        );

        for stage in &self.stages {
            log::info!("Pipeline '{}' executing stage: {}", self.name, stage.name());
            stage.process(ctx)?;
        }

        log::info!("Pipeline '{}' completed", self.name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_pipeline_produces_document() {
        let pipeline = SimplePipeline::new("test")
            .add_stage(Box::new(MockDetector::mock_formula_recognition()))
            .add_stage(Box::new(MockCropper))
            .add_stage(Box::new(MockRecognizer));

        let mut ctx = PipelineContext::with_image("test.png");
        pipeline.run(&mut ctx).unwrap();

        assert_eq!(ctx.document.block_count(), 3);
        assert_eq!(ctx.document.pages.len(), 1);
    }

    #[test]
    fn mock_pipeline_sorts_by_y_position() {
        let pipeline = SimplePipeline::new("test")
            .add_stage(Box::new(MockDetector::mock_formula_recognition()))
            .add_stage(Box::new(MockCropper))
            .add_stage(Box::new(MockRecognizer));

        let mut ctx = PipelineContext::with_image("test.png");
        pipeline.run(&mut ctx).unwrap();

        let blocks = ctx.document.all_blocks();
        // First block should be at y=50, second at y=150, third at y=200
        assert!(blocks[0].geometry().unwrap().y < blocks[1].geometry().unwrap().y);
        assert!(blocks[1].geometry().unwrap().y < blocks[2].geometry().unwrap().y);
    }
}
