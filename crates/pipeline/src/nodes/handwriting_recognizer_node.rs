use async_trait::async_trait;
use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::operations;
use latexsnipper_inference::{postprocess_handwriting, recognize_formula, RecognitionParams};

use crate::context::PipelineContext;
use crate::node::PipelineNode;
use crate::nodes::utils::{get_backend, get_or_create_session, load_config, resolve_model_handle};

/// Recognizes content in handwriting-detected regions.
///
/// Uses TrOCR (optimized for handwriting) to recognize text and formulas
/// in regions detected as handwriting.
pub struct HandwritingRecognizerNode {
    name: String,
}

impl HandwritingRecognizerNode {
    pub fn new() -> Self {
        Self {
            name: "recognize_handwriting".into(),
        }
    }
}

impl Default for HandwritingRecognizerNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PipelineNode for HandwritingRecognizerNode {
    fn name(&self) -> &str {
        &self.name
    }

    async fn process(&self, ctx: &mut PipelineContext) -> Result<()> {
        let models = match &ctx.models_dir {
            Some(d) => d.clone(),
            None => return Ok(()),
        };

        self.recognize_handwriting(ctx, &models).await
    }
}

impl HandwritingRecognizerNode {
    async fn recognize_handwriting(
        &self,
        ctx: &mut PipelineContext,
        models: &std::path::Path,
    ) -> Result<()> {
        let detections = ctx.artifacts.handwriting_detections.clone();
        if detections.is_empty() {
            return Ok(());
        }

        let rec_config = match load_config(ctx, models, "formula-rec") {
            Ok(c) => c,
            Err(_) => {
                ctx.diagnostic_warn(
                    "recognize_handwriting",
                    "TrOCR model config not found for handwriting recognition",
                );
                return Ok(());
            }
        };

        let (_variant_config, rec_dir, _variant_dir) =
            latexsnipper_model::ModelConfig::find_best(models, "formula-rec").ok_or_else(|| {
                SnipperError::Model("TrOCR model not found for handwriting recognition".into())
            })?;

        let enc_path = rec_config
            .pipeline_encoder_path(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Encoder not found".into()))?;
        let dec_path = rec_config
            .pipeline_decoder_path(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Decoder not found".into()))?;
        let tok_path = rec_config
            .pipeline_tokenizer_path(&rec_dir)
            .ok_or_else(|| SnipperError::Model("Tokenizer not found".into()))?;

        let backend = get_backend(ctx)?;
        let enc_handle = resolve_model_handle(ctx, "formula-rec/encoder", enc_path)?;
        let dec_handle = resolve_model_handle(ctx, "formula-rec/decoder", dec_path)?;

        let enc_session = get_or_create_session(ctx, "handwriting_encoder", &backend, &enc_handle)?;
        let dec_session = get_or_create_session(ctx, "handwriting_decoder", &backend, &dec_handle)?;

        // Handwriting-optimized parameters
        let params = RecognitionParams {
            img_size: 384,
            beam_width: 5,
            top_k: 5,
            max_tokens: 256,
            ..RecognitionParams::default()
        };

        let mut blocks = Vec::new();

        for det in &detections {
            let x = det.rect.x as u32;
            let y = det.rect.y as u32;
            let w = det.rect.width as u32;
            let h = det.rect.height as u32;

            if let Some(ref image) = ctx.image {
                if w >= 4 && h >= 4 {
                    let cropped =
                        operations::crop(image, Rect::new(x as f32, y as f32, w as f32, h as f32));

                    match recognize_formula(
                        &cropped,
                        &*enc_session,
                        &*dec_session,
                        &tok_path,
                        &params,
                    ) {
                        Ok(result) => {
                            let processed_text = postprocess_handwriting(&result.text);

                            if !processed_text.is_empty() {
                                if looks_like_formula(&processed_text) {
                                    let mut f = Formula::latex(&processed_text);
                                    f.confidence = result.confidence;
                                    blocks.push(Block::Formula(FormulaBlock {
                                        formula: f,
                                        geometry: Some(Rect::new(
                                            x as f32, y as f32, w as f32, h as f32,
                                        )),
                                        source: Some(SourceInfo::new().with_page(ctx.current_page)),
                                    }));
                                } else {
                                    blocks.push(Block::Handwriting(HandwritingBlock {
                                        inlines: vec![Inline::Text(TextRun::new(&processed_text))],
                                        confidence: result.confidence,
                                        geometry: Some(Rect::new(
                                            x as f32, y as f32, w as f32, h as f32,
                                        )),
                                        source: Some(SourceInfo::new().with_page(ctx.current_page)),
                                    }));
                                }
                            }
                        }
                        Err(e) => log::warn!("Handwriting rec failed: {}", e),
                    }
                }
            }
        }

        ctx.artifacts.handwriting_blocks = blocks;
        log::info!(
            "Recognized {} handwriting blocks",
            ctx.artifacts.handwriting_blocks.len()
        );
        Ok(())
    }
}

/// Check if text looks like a mathematical formula.
fn looks_like_formula(text: &str) -> bool {
    if text.contains("\\frac") || text.contains("\\sqrt") {
        return true;
    }

    let strong_indicators = [
        "\\", // Backslash commands
        "^",  // Superscript
        "_",  // Subscript
        "=",  // Equations
    ];

    let strong_count = strong_indicators
        .iter()
        .filter(|&&ind| text.contains(ind))
        .count();

    if strong_count >= 2 {
        return true;
    }

    text.len() >= 3 && !text.contains(' ') && (text.contains('+') || text.contains('='))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_formula() {
        assert!(looks_like_formula("E = mc^2"));
        assert!(looks_like_formula("\\frac{a}{b}"));
        assert!(looks_like_formula("x^2 + y^2 = z^2"));
        assert!(!looks_like_formula("Hello World"));
        assert!(!looks_like_formula("This is text"));
        assert!(!looks_like_formula("This is a test (with explanation)"));
        assert!(looks_like_formula("3x+2=5"));
    }
}
