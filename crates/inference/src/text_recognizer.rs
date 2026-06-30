use std::path::Path;

use latexsnipper_foundation::{SnipperError, Result};
use latexsnipper_image::SnipperImage;
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::Tensor;

use crate::types::RecognitionResult;

/// Text recognition parameters loaded from config.json.
#[derive(Debug, Clone)]
pub struct TextRecParams {
    pub target_h: u32,
    pub max_w: u32,
    pub blank_id: usize,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

impl TextRecParams {
    pub fn from_config(config: &latexsnipper_model::ModelConfig) -> Self {
        let mut params = Self::default();
        let (width, height) = config.resize_dimensions();
        if let Some(h) = height { params.target_h = h; }
        if let Some(w) = width { params.max_w = w; }
        if let Some(decoding) = &config.decoding {
            if let Some(blank_id) = decoding.blank_id {
                params.blank_id = blank_id;
            }
        }
        let mean = config.normalization_mean();
        if mean.len() == 3 {
            params.mean = [mean[0], mean[1], mean[2]];
        }
        let std = config.normalization_std();
        if std.len() == 3 {
            params.std = [std[0], std[1], std[2]];
        }
        params
    }
}

impl Default for TextRecParams {
    fn default() -> Self {
        Self {
            target_h: 48,
            max_w: 320,
            blank_id: 0,
            mean: [0.5, 0.5, 0.5],
            std: [0.5, 0.5, 0.5],
        }
    }
}

/// Recognize text using CRNN + CTC decode.
/// Keys are loaded from a file (legacy support).
pub fn recognize_text(
    image: &SnipperImage,
    session: &dyn InferenceSession,
    keys_path: &Path,
    params: &TextRecParams,
) -> Result<RecognitionResult> {
    let (keys, first_char_id) = load_keys(keys_path)?;
    recognize_text_with_keys(image, session, &keys, first_char_id, params)
}

/// Recognize text using CRNN + CTC decode with pre-loaded keys.
/// Keys can be extracted from ONNX model metadata or loaded from file.
pub fn recognize_text_with_keys(
    image: &SnipperImage,
    session: &dyn InferenceSession,
    keys: &[String],
    first_char_id: usize,
    params: &TextRecParams,
) -> Result<RecognitionResult> {
    let (processed, _orig_w) = preprocess(image, params);

    let input = Tensor::float32(
        "x",
        vec![1, 3, params.target_h as usize, params.max_w as usize],
        latexsnipper_image::operations::normalize(&processed, &params.mean, &params.std),
    );
    let outputs = session.run(&[input])?;

    let output = outputs.first().ok_or_else(|| SnipperError::Inference("No output".into()))?;
    let logits = output.as_f32_slice()
        .ok_or_else(|| SnipperError::Inference("Output not float32".into()))?;
    let shape = output.shape().to_vec();

    let (text, confidence) = ctc_decode(logits, &shape, keys, first_char_id);

    Ok(RecognitionResult { text, confidence })
}

fn preprocess(image: &SnipperImage, params: &TextRecParams) -> (SnipperImage, u32) {
    let w = image.width();
    let h = image.height();
    let orig_w = w;

    // RapidOCR: dynamic width based on aspect ratio
    // img_width = target_h * max(320/48, image_wh_ratio)
    let img_wh_ratio = w as f32 / h as f32;
    let default_wh_ratio: f32 = 320.0 / 48.0;
    let max_wh_ratio = default_wh_ratio.max(img_wh_ratio);
    let target_w = (params.target_h as f32 * max_wh_ratio).round() as u32;

    // Calculate resized width maintaining aspect ratio
    let ratio = w as f32 / h as f32;
    let new_w = if (params.target_h as f32 * ratio).ceil() as u32 > target_w {
        target_w
    } else {
        (params.target_h as f32 * ratio).ceil() as u32
    };

    let resized = latexsnipper_image::operations::resize(image, new_w, params.target_h);

    let padded = if new_w < target_w {
        let bpp = resized.bytes_per_pixel();
        let mut pixels = resized.pixels().to_vec();
        let pad_bytes = ((target_w - new_w) * params.target_h * bpp as u32) as usize;
        pixels.extend(vec![0u8; pad_bytes]);
        SnipperImage::new(target_w, params.target_h, resized.format(), pixels)
    } else {
        resized
    };

    (padded, orig_w)
}

pub fn load_keys(path: &Path) -> Result<(Vec<String>, usize)> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| SnipperError::Model(format!("Failed to read keys: {}", e)))?;

    if path.file_name().and_then(|n| n.to_str()) == Some("inference.yml") {
        return Ok((load_paddle_character_dict(&content), 1));
    }

    // For .txt keys files, strip only newlines (not whitespace)
    // to preserve fullwidth space \u3000 as first character
    let keys: Vec<String> = content.lines()
        .map(|l| l.trim_end_matches('\n').trim_end_matches('\r').to_string())
        .collect();
    Ok((keys, 2))
}

fn load_paddle_character_dict(content: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut in_dict = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "character_dict:" {
            in_dict = true;
            continue;
        }
        if in_dict && !trimmed.starts_with("-") {
            break;
        }
        if in_dict {
            let value = trimmed.trim_start_matches('-').trim().trim_matches('\'').trim_matches('"');
            keys.push(value.to_string());
        }
    }

    // RapidOCR convention: blank at 0, space at end
    // The model's vocab_size = keys.len() + 2 (blank + space)
    // ID 0 = blank, ID 1..N = chars, ID N+1 = space
    keys.push(" ".to_string()); // space token at end

    keys
}

fn ctc_decode(logits: &[f32], shape: &[usize], keys: &[String], first_char_id: usize) -> (String, f32) {
    if shape.len() < 3 {
        return (String::new(), 0.0);
    }

    let seq_len = shape[1];
    let vocab_size = shape[2];

    let mut prev_id = 0usize;
    let mut result = String::new();
    let mut confidences = Vec::new();

    for t in 0..seq_len {
        let start = t * vocab_size;
        let end = start + vocab_size;
        if end > logits.len() { break; }

        let slice = &logits[start..end];

        // Find best token (argmax)
        let mut best_id = 0;
        let mut best_val = f32::NEG_INFINITY;
        for (i, &val) in slice.iter().enumerate() {
            if val > best_val {
                best_val = val;
                best_id = i;
            }
        }

        if best_id != 0 && best_id != prev_id {
            // Map model output ID to character
            // Model metadata: blank at 0, chars at 1..N, space at N+1 (first_char_id=0)
            // File keys: blank at 0, space at 1, chars at 2+ (first_char_id=1 or 2)
            let char_idx = if first_char_id == 0 {
                best_id  // Direct mapping for model metadata
            } else {
                best_id.wrapping_sub(first_char_id)  // Offset for file keys
            };
            if let Some(ch) = keys.get(char_idx) {
                result.push_str(ch);
                confidences.push(1.0);
            }
        }
        prev_id = best_id;
    }

    let avg_confidence = if confidences.is_empty() { 0.0 }
    else { confidences.iter().sum::<f32>() / confidences.len() as f32 };

    (result, avg_confidence)
}

/// Post-process text to insert spaces between words.
/// Handles Latin/CJK transitions and punctuation boundaries.
pub fn insert_spaces(text: &str) -> String {
    if text.is_empty() {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    let mut result = String::new();

    for (i, &ch) in chars.iter().enumerate() {
        result.push(ch);

        if i + 1 < chars.len() {
            let next = chars[i + 1];
            let should_space = match (char_type(ch), char_type(next)) {
                // Latin to CJK: add space
                (CharType::Latin, CharType::CJK) => true,
                // CJK to Latin: add space
                (CharType::CJK, CharType::Latin) => true,
                // After punctuation (but not before certain chars)
                (CharType::Punct, CharType::Latin) if next != ',' && next != '.' => true,
                (CharType::Punct, CharType::CJK) => true,
                // Before opening bracket
                (CharType::Latin, CharType::Bracket) => true,
                _ => false,
            };

            if should_space {
                result.push(' ');
            }
        }
    }

    result
}

#[derive(Clone, Copy)]
enum CharType {
    Latin,
    CJK,
    Digit,
    Punct,
    Bracket,
    Other,
}

fn char_type(ch: char) -> CharType {
    match ch {
        'a'..='z' | 'A'..='Z' => CharType::Latin,
        '0'..='9' => CharType::Digit,
        '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' => CharType::CJK,
        '(' | ')' | '[' | ']' | '{' | '}' | '\u{3008}' | '\u{3009}' | '\u{FF08}' | '\u{FF09}' => CharType::Bracket,
        '.' | ',' | ';' | ':' | '!' | '?' | '\u{3002}' | '\u{FF0C}' | '\u{FF1B}' | '\u{FF01}' | '\u{FF1F}' => CharType::Punct,
        _ => CharType::Other,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_spaces_latin_cjk() {
        assert_eq!(insert_spaces("Hello你好"), "Hello 你好");
        assert_eq!(insert_spaces("你好World"), "你好 World");
    }

    #[test]
    fn test_insert_spaces_punctuation() {
        assert_eq!(insert_spaces("Hello.World"), "Hello. World");
        // Chinese comma after CJK should not add space
        assert_eq!(insert_spaces("你好World"), "你好 World");
    }

    #[test]
    fn test_insert_spaces_empty() {
        assert_eq!(insert_spaces(""), "");
    }

    #[test]
    fn test_insert_spaces_no_change() {
        assert_eq!(insert_spaces("Hello"), "Hello");
        assert_eq!(insert_spaces("123"), "123");
    }

    #[test]
    fn paddle_character_dict_is_loaded_from_inference_yml() {
        let content = r#"
PostProcess:
  name: CTCLabelDecode
  character_dict:
  - A
  - B
  - '!'
Other:
  value: ignored
"#;
        let keys = load_paddle_character_dict(content);
        // Space token is appended at end (RapidOCR convention)
        assert_eq!(keys, vec!["A", "B", "!", " "]);
    }

    #[test]
    fn ctc_decode_rejects_invalid_shape_without_panic() {
        let (text, confidence) = ctc_decode(&[0.0, 1.0], &[2], &[], 1);
        assert!(text.is_empty());
        assert_eq!(confidence, 0.0);
    }
}
