//! Integration tests using real ONNX models from models/

use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::image::SnipperImage;
use latexsnipper_image::operations;
use latexsnipper_inference::{
    detect_formulas, filter_formula_detections, group_formula_detections, recognize_formula,
    DetectionParams, RecognitionParams,
};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, OnnxRuntimeBackend, RuntimeBackend};
use latexsnipper_tensor::Tensor;
use std::path::PathBuf;

fn models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("models")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
}

fn rgba_to_rgb(img: &SnipperImage) -> SnipperImage {
    let mut rgb = Vec::with_capacity((img.width() * img.height() * 3) as usize);
    for chunk in img.pixels().chunks_exact(4) {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }
    SnipperImage::new(img.width(), img.height(), PixelFormat::Rgb, rgb)
}

#[test]
fn test_formula_detection_with_real_model() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let model_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
    let image_path = fixtures.join("formula.png");

    if !model_path.exists() || !image_path.exists() {
        println!("Skipping: model or fixture not found");
        return;
    }

    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);

    let det_config =
        latexsnipper_model::ModelConfig::load(&models.join("formula-det/yolov8-mfd")).unwrap();
    let params = DetectionParams::from_config(&det_config);

    let handle = ModelHandle::with_path("formula-det", model_path);
    let session = backend
        .create_session(&handle, AccelerationMode::Cpu)
        .unwrap();

    let mut detections = detect_formulas(&rgb, &*session, &params).unwrap();
    group_formula_detections(&mut detections);
    filter_formula_detections(&mut detections, 100.0, 0.2);

    println!("Formula detections: {}", detections.len());
    for d in &detections {
        println!(
            "  - [{:.0},{:.0} {:.0}x{:.0}] conf={:.3} class={}",
            d.rect.x, d.rect.y, d.rect.width, d.rect.height, d.confidence, d.class_name
        );
    }

    assert!(!detections.is_empty(), "Should detect at least one formula");
    assert!(
        detections.iter().any(|d| d.confidence > 0.5),
        "Should have high-confidence detection"
    );
}

#[test]
fn test_formula_recognition_with_real_model() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let enc_path = models.join("formula-rec/trocr-deit/encoder_model.onnx");
    let dec_path = models.join("formula-rec/trocr-deit/decoder_model.onnx");
    let tok_path = models.join("formula-rec/trocr-deit/tokenizer.json");
    let image_path = fixtures.join("formula.png");

    if !enc_path.exists() || !dec_path.exists() || !image_path.exists() {
        println!("Skipping: models or fixture not found");
        return;
    }

    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);

    // Crop first formula region (from test results)
    let crop = crop_region(&rgb, 417, 44, 276, 83);

    let enc_handle = ModelHandle::with_path("encoder", enc_path);
    let dec_handle = ModelHandle::with_path("decoder", dec_path);
    let enc_session = backend
        .create_session(&enc_handle, AccelerationMode::Cpu)
        .unwrap();
    let dec_session = backend
        .create_session(&dec_handle, AccelerationMode::Cpu)
        .unwrap();

    let params = RecognitionParams::default();
    let result =
        recognize_formula(&crop, &*enc_session, &*dec_session, &tok_path, &params).unwrap();

    println!("Formula recognition: {}", result.text);
    assert!(!result.text.is_empty(), "Should recognize formula");
    assert!(
        result.text.contains("mc") || result.text.contains("="),
        "Should recognize E=mc^2, got: {}",
        result.text
    );
}

#[test]
fn test_text_detection_with_real_model() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let model_path = models.join("PP-OCRv6_medium_det_infer/inference.onnx");
    let image_path = fixtures.join("text.png");

    if !model_path.exists() || !image_path.exists() {
        println!("Skipping: model or fixture not found");
        return;
    }

    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);

    let handle = ModelHandle::with_path("text-det", model_path);
    let session = backend
        .create_session(&handle, AccelerationMode::Cpu)
        .unwrap();

    // Preprocess: resize, pad, normalize
    let w = rgb.width();
    let h = rgb.height();
    let scale = if w.max(h) > 960 {
        960.0 / w.max(h) as f32
    } else {
        1.0
    };
    let nw = ((w as f32 * scale).ceil() as u32 + 31) / 32 * 32;
    let nh = ((h as f32 * scale).ceil() as u32 + 31) / 32 * 32;
    let resized = operations::resize(&rgb, nw, nh);
    let padded = operations::pad_to_stride(&resized, 32);
    let pixels = operations::normalize(&padded, &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]);

    let input = Tensor::float32(
        "x",
        vec![1, 3, padded.height() as usize, padded.width() as usize],
        pixels,
    );
    let output = session.run(&[input]).unwrap();

    if let Some(data) = output[0].as_f32_slice() {
        let non_zero = data.iter().filter(|&&v| v > 0.3).count();
        println!(
            "Text detection: shape={:?}, pixels>0.3={}",
            output[0].shape(),
            non_zero
        );
        assert!(non_zero > 0, "Should detect text regions");
    }
}

#[test]
fn test_text_recognition_with_real_model() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let image_path = fixtures.join("text.png");

    // Find text rec model
    let rec_candidates = [
        models.join("PP-OCRv6_medium_rec_infer/inference.onnx"),
        models.join("PP-OCRv6_small_rec_infer/inference.onnx"),
    ];
    let rec_path = rec_candidates.iter().find(|p| p.exists());

    let keys_candidates = [
        models.join("PP-OCRv6_medium_rec_infer/inference.yml"),
        models.join("PP-OCRv6_small_rec_infer/inference.yml"),
    ];
    let keys_path = keys_candidates.iter().find(|p| p.exists());

    if rec_path.is_none() || keys_path.is_none() || !image_path.exists() {
        println!("Skipping: model or fixture not found");
        return;
    }

    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);

    // Crop a known text line
    let crop = crop_region(&rgb, 35, 30, 700, 45);

    let handle = ModelHandle::with_path("text-rec", rec_path.unwrap().to_path_buf());
    let session = backend
        .create_session(&handle, AccelerationMode::Cpu)
        .unwrap();

    // Prepare input
    let img_wh_ratio = crop.width() as f32 / crop.height() as f32;
    let default_wh_ratio: f32 = 320.0 / 48.0;
    let max_wh_ratio = default_wh_ratio.max(img_wh_ratio);
    let target_w = (48.0 * max_wh_ratio).round() as u32;

    let resized = operations::resize(&crop, target_w, 48);
    let norm = operations::normalize(&resized, &[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);

    let input = Tensor::float32("x", vec![1, 3, 48, target_w as usize], norm);
    let output = session.run(&[input]).unwrap();

    if let Some(data) = output[0].as_f32_slice() {
        let shape = output[0].shape();
        println!("Text recognition: output shape={:?}", shape);

        // Load keys and decode
        let keys_content = std::fs::read_to_string(keys_path.unwrap()).unwrap();
        let keys = load_paddle_character_dict(&keys_content);
        let text = ctc_decode(data, shape[1], shape[2], &keys);

        println!("Recognized text: {:?}", text);
        assert!(!text.trim().is_empty(), "Should recognize text");
    }
}

fn crop_region(img: &SnipperImage, x: u32, y: u32, w: u32, h: u32) -> SnipperImage {
    let mut pixels = Vec::with_capacity((w * h * 3) as usize);
    for row in 0..h {
        let src_off = ((y + row) * img.width() + x) * 3;
        let src_end = src_off + w * 3;
        pixels.extend_from_slice(&img.pixels()[src_off as usize..src_end as usize]);
    }
    SnipperImage::new(w, h, PixelFormat::Rgb, pixels)
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
        if in_dict && !trimmed.starts_with('-') {
            break;
        }
        if in_dict {
            let value = trimmed
                .trim_start_matches('-')
                .trim()
                .trim_matches('\'')
                .trim_matches('"');
            keys.push(value.to_string());
        }
    }
    keys.insert(0, "blank".to_string());
    keys.push(" ".to_string());
    keys
}

fn ctc_decode(logits: &[f32], seq_len: usize, vocab: usize, keys: &[String]) -> String {
    let mut prev_id = 0usize;
    let mut text = String::new();
    for t in 0..seq_len {
        let start = t * vocab;
        let end = start + vocab;
        if end > logits.len() {
            break;
        }
        let slice = &logits[start..end];
        let max_id = slice
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        if max_id != 0 && max_id != prev_id {
            if let Some(ch) = keys.get(max_id) {
                text.push_str(ch);
            }
        }
        prev_id = max_id;
    }
    text
}
