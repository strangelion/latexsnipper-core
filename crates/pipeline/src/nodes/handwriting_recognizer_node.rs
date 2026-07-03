use async_trait::async_trait;
use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_image::operations;
use latexsnipper_inference::{postprocess_handwriting, recognize_formula, RecognitionParams};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, OnnxRuntimeBackend, RuntimeBackend};

use crate::context::PipelineContext;
use crate::node::PipelineNode;

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

    fn create_backend(models: &std::path::Path) -> Result<OnnxRuntimeBackend> {
        OnnxRuntimeBackend::new(models.to_path_buf())
            .map_err(|e| SnipperError::Runtime(format!("Failed to create ONNX backend: {}", e)))
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
        let crop_key = "handwriting_crops";
        let crops = match ctx.get(crop_key) {
            Some(v) => v.clone(),
            None => return Ok(()),
        };

        let crop_array = match crops.as_array() {
            Some(a) => a.clone(),
            None => return Ok(()),
        };

        if crop_array.is_empty() {
            return Ok(());
        }

        // Load TrOCR model for handwriting recognition
        let rec_dir = models.join("formula-rec/trocr-deit");
        if !rec_dir.exists() {
            log::warn!("TrOCR model not found for handwriting recognition");
            return Ok(());
        }

        let enc_path = rec_dir.join("encoder_model.onnx");
        let dec_path = rec_dir.join("decoder_model.onnx");
        let tok_path = rec_dir.join("tokenizer.json");

        if !enc_path.exists() || !dec_path.exists() || !tok_path.exists() {
            log::warn!("TrOCR model files incomplete for handwriting recognition");
            return Ok(());
        }

        let backend = Self::create_backend(models)?;
        let enc_handle = ModelHandle::with_path("encoder", enc_path);
        let dec_handle = ModelHandle::with_path("decoder", dec_path);

        let enc_session = if let Some(s) = ctx.get_session("handwriting_encoder") {
            s
        } else {
            let s = backend.create_session(&enc_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("handwriting_encoder", s);
            ctx.get_session("handwriting_encoder").ok_or_else(|| {
                SnipperError::Runtime("Failed to cache handwriting encoder session".into())
            })?
        };
        let dec_session = if let Some(s) = ctx.get_session("handwriting_decoder") {
            s
        } else {
            let s = backend.create_session(&dec_handle, AccelerationMode::Cpu)?;
            ctx.cache_session("handwriting_decoder", s);
            ctx.get_session("handwriting_decoder").ok_or_else(|| {
                SnipperError::Runtime("Failed to cache handwriting decoder session".into())
            })?
        };

        // Handwriting-optimized parameters
        let params = RecognitionParams {
            img_size: 384,
            beam_width: 5, // Wider beam for handwriting
            top_k: 5,
            max_tokens: 256,
            ..RecognitionParams::default()
        };

        let mut blocks = Vec::new();

        for crop_val in &crop_array {
            if let Some(rect_val) = crop_val.get("rect") {
                let x = rect_val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let y = rect_val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let w = rect_val.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
                let h = rect_val.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;

                if let Some(ref image) = ctx.image {
                    if w >= 4 && h >= 4 {
                        let cropped = operations::crop(
                            image,
                            Rect::new(x as f32, y as f32, w as f32, h as f32),
                        );

                        match recognize_formula(
                            &cropped,
                            &*enc_session,
                            &*dec_session,
                            &tok_path,
                            &params,
                        ) {
                            Ok(result) => {
                                // Apply handwriting-specific post-processing
                                let processed_text = postprocess_handwriting(&result.text);

                                if !processed_text.is_empty() {
                                    // Determine if the result looks like a formula
                                    if looks_like_formula(&processed_text) {
                                        let mut f = Formula::latex(&processed_text);
                                        f.confidence = result.confidence;
                                        blocks.push(Block::Formula(FormulaBlock {
                                            formula: f,
                                            geometry: Some(Rect::new(
                                                x as f32, y as f32, w as f32, h as f32,
                                            )),
                                            source: Some(
                                                SourceInfo::new().with_page(ctx.current_page),
                                            ),
                                        }));
                                    } else {
                                        blocks.push(Block::Handwriting(HandwritingBlock {
                                            inlines: vec![Inline::Text(TextRun::new(
                                                &processed_text,
                                            ))],
                                            confidence: result.confidence,
                                            geometry: Some(Rect::new(
                                                x as f32, y as f32, w as f32, h as f32,
                                            )),
                                            source: Some(
                                                SourceInfo::new().with_page(ctx.current_page),
                                            ),
                                        }));
                                    }
                                }
                            }
                            Err(e) => log::warn!("Handwriting rec failed: {}", e),
                        }
                    }
                }
            }
        }

        ctx.set(
            "handwriting_blocks",
            serde_json::to_value(&blocks).unwrap_or_default(),
        );
        log::info!("Recognized {} handwriting blocks", blocks.len());
        Ok(())
    }
}

/// Check if text looks like a mathematical formula.
///
/// Heuristic: strong indicator = `\\frac`/`\\sqrt` → always formula;
/// otherwise count structural operators (`\\`, `^`, `_`, `=`).
/// Parentheses and + - alone are NOT formula indicators
/// (common in natural language: "a (b)", "item 1-2").
fn looks_like_formula(text: &str) -> bool {
    // Strongest signal: unambiguous LaTeX commands
    if text.contains("\\frac") || text.contains("\\sqrt") {
        return true;
    }

    // Structural math indicators (rare in natural language)
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

    // Dense notation without spaces: "3x+2=5" or "a+b"
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
        // Parentheses alone should NOT flag as formula
        assert!(!looks_like_formula("This is a test (with explanation)"));
        // Dense math notation
        assert!(looks_like_formula("3x+2=5"));
    }
}
