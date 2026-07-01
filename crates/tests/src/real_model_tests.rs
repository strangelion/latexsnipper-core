use latexsnipper_runtime::{OnnxRuntimeBackend, RuntimeBackend, AccelerationMode, ModelHandle};
use latexsnipper_tensor::Tensor;
use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::operations;
use latexsnipper_image::image::SnipperImage;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_inference::{recognize_formula, RecognitionParams};
use std::path::PathBuf;

fn models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join("models")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap().join("fixtures")
}

fn rgba_to_rgb(img: &SnipperImage) -> SnipperImage {
    let mut rgb = Vec::with_capacity((img.width() * img.height() * 3) as usize);
    for chunk in img.pixels().chunks_exact(4) {
        rgb.push(chunk[0]); rgb.push(chunk[1]); rgb.push(chunk[2]);
    }
    SnipperImage::new(img.width(), img.height(), PixelFormat::Rgb, rgb)
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

fn prepare_rec_input(img: &SnipperImage, _max_w: u32) -> (Vec<f32>, u32) {
    // RapidOCR: dynamic width based on aspect ratio
    let img_wh_ratio = img.width() as f32 / img.height() as f32;
    let default_wh_ratio: f32 = 320.0 / 48.0;
    let max_wh_ratio = default_wh_ratio.max(img_wh_ratio);
    let target_w = (48.0 * max_wh_ratio).round() as u32;

    let ratio = img.width() as f32 / img.height() as f32;
    let rec_w = if (48.0 * ratio).ceil() as u32 > target_w {
        target_w
    } else {
        (48.0 * ratio).ceil() as u32
    };

    let resized = operations::resize(img, rec_w, 48);
    let norm = operations::normalize(&resized, &[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);
    let mut data = vec![0.0f32; 3 * 48 * target_w as usize];
    let cw = rec_w as usize;
    for c in 0..3 {
        for y in 0..48 {
            let src = c * 48 * cw + y * cw;
            let dst = c * 48 * target_w as usize + y * target_w as usize;
            data[dst..dst + cw].copy_from_slice(&norm[src..src + cw]);
        }
    }
    (data, target_w)
}

fn text_rec_model(models: &PathBuf) -> Option<(PathBuf, PathBuf, u32)> {
    let candidates = [
        (
            models.join("text-rec/v6-small/inference.onnx"),
            models.join("text-rec/v6-small/inference.yml"),
            3200,
        ),
        (
            models.join("PP-OCRv6_small_rec_infer/inference.onnx"),
            models.join("PP-OCRv6_small_rec_infer/inference.yml"),
            3200,
        ),
        (
            models.join("PP-OCRv6_medium_rec_infer/inference.onnx"),
            models.join("PP-OCRv6_medium_rec_infer/inference.yml"),
            3200,
        ),
    ];

    candidates.into_iter().find(|(model, keys, _)| model.exists() && keys.exists())
}

fn ctc_decode(logits: &[f32], seq_len: usize, vocab: usize, keys: &[String]) -> String {
    let mut prev_id = 0usize;
    let mut text = String::new();
    for t in 0..seq_len {
        let start = t * vocab;
        let end = start + vocab;
        if end > logits.len() { break; }
        let slice = &logits[start..end];
        let max_id = slice.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i).unwrap_or(0);
        if max_id != 0 && max_id != prev_id {
            // Model metadata has blank at position 0, so use direct mapping
            if let Some(ch) = keys.get(max_id) { text.push_str(ch); }
        }
        prev_id = max_id;
    }
    text
}

fn load_keys(keys_path: &std::path::Path) -> Vec<String> {
    let content = std::fs::read_to_string(keys_path).unwrap();
    if keys_path.file_name().and_then(|n| n.to_str()) == Some("inference.yml") {
        load_paddle_character_dict(&content)
    } else {
        content.lines().map(|s| s.to_string()).collect()
    }
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
    // Add blank at position 0 (RapidOCR convention)
    keys.insert(0, "blank".to_string());
    // Add space at end
    keys.push(" ".to_string());
    keys
}

fn run_text_rec(backend: &OnnxRuntimeBackend, models: &PathBuf, crop: &SnipperImage) -> Option<String> {
    let (rec_path, keys_path, max_w) = text_rec_model(models)?;
    let (input_data, input_w) = prepare_rec_input(crop, max_w);
    let handle = ModelHandle::with_path("text-rec", rec_path);
    let session = backend.create_session(&handle, AccelerationMode::Cpu).ok()?;
    let input = Tensor::float32("x", vec![1, 3, 48, input_w as usize], input_data);
    let output = session.run(&[input]).ok()?;
    let data = output[0].as_f32_slice()?;
    let shape = output[0].shape();
    if shape.len() < 3 { return None; }
    let keys = load_keys(&keys_path);
    Some(ctc_decode(data, shape[1], shape[2], &keys))
}

fn assert_usable_text(text: &str) {
    let trimmed = text.trim();
    assert!(!trimmed.is_empty(), "OCR text should not be empty");
    assert!(!trimmed.contains('\u{fffd}'), "OCR text contains replacement characters: {trimmed:?}");
    let alnum = trimmed.chars().filter(|c| c.is_alphanumeric()).count();
    assert!(alnum >= 3, "OCR text has too few readable characters: {trimmed:?}");
}

#[test]
fn test_doc_ori() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let model_path = models.join("pplcnet_doc_ori/default/pplcnet_doc_ori.onnx");
    let image_path = fixtures.join("text.png");
    if !model_path.exists() || !image_path.exists() { println!("Skipping"); return; }

    let backend = OnnxRuntimeBackend::new(models).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    let resized = operations::resize(&rgb, 224, 224);
    let pixels = operations::normalize(&resized, &[0.485, 0.456, 0.406], &[0.229, 0.224, 0.225]);
    let handle = ModelHandle::with_path("doc-ori", model_path);
    let session = backend.create_session(&handle, AccelerationMode::Cpu).unwrap();
    let output = session.run(&[Tensor::float32("input", vec![1, 3, 224, 224], pixels)]).unwrap();
    if let Some(data) = output[0].as_f32_slice() {
        let max_idx = data.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).map(|(i, _)| i).unwrap_or(0);
        println!("Doc-ori: {} degrees (conf {:.4})", [0, 90, 180, 270][max_idx.min(3)], data[max_idx.min(3)]);
    }
    println!("doc-ori: PASSED");
}

#[test]
fn test_text_det() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let model_path = models.join("text-det/ppocrv5-mobile/ppocrv5_mobile_det.onnx");
    let image_path = fixtures.join("text.png");
    if !model_path.exists() || !image_path.exists() { println!("Skipping"); return; }

    let backend = OnnxRuntimeBackend::new(models).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    let w = rgb.width(); let h = rgb.height();
    let scale = if w.max(h) > 960 { 960.0 / w.max(h) as f32 } else { 1.0 };
    let nw = ((w as f32 * scale).ceil() as u32 + 31) / 32 * 32;
    let nh = ((h as f32 * scale).ceil() as u32 + 31) / 32 * 32;
    let resized = operations::resize(&rgb, nw, nh);
    let padded = operations::pad_to_stride(&resized, 32);
    let handle = ModelHandle::with_path("text-det", model_path);
    let session = backend.create_session(&handle, AccelerationMode::Cpu).unwrap();
    let output = session.run(&[Tensor::float32("x", vec![1, 3, padded.height() as usize, padded.width() as usize],
        operations::normalize(&padded, &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]))]).unwrap();
    if let Some(data) = output[0].as_f32_slice() {
        let non_zero = data.iter().filter(|&&v| v > 0.3).count();
        println!("Text-det: shape={:?}, pixels>0.3={}", output[0].shape(), non_zero);
        assert!(non_zero > 0);
    }
    println!("text-det: PASSED");
}

#[test]
fn test_text_rec_known_line() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let image_path = fixtures.join("text.png");
    if text_rec_model(&models).is_none() || !image_path.exists() {
        println!("Skipping");
        return;
    }

    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    let crop = crop_region(&rgb, 35, 30, 700, 45);
    let text = run_text_rec(&backend, &models, &crop).expect("text recognition should run");
    println!("Text-rec known line: {text:?}");

    assert_usable_text(&text);
    let normalized = text.to_ascii_lowercase();
    assert!(
        normalized.contains("quick") || normalized.contains("brown") || normalized.contains("lazy"),
        "OCR should recognize expected words from text fixture, got {text:?}"
    );
    println!("text-rec: PASSED");
}

#[test]
fn test_formula_det() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let model_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
    let image_path = fixtures.join("formula.png");
    if !model_path.exists() || !image_path.exists() { println!("Skipping"); return; }

    let backend = OnnxRuntimeBackend::new(models).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    let (letterboxed, _, _, _) = operations::letterbox(&rgb, 768);
    let pixels = operations::normalize(&letterboxed, &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]);
    let handle = ModelHandle::with_path("formula-det", model_path);
    let session = backend.create_session(&handle, AccelerationMode::Cpu).unwrap();
    let output = session.run(&[Tensor::float32("images", vec![1, 3, 768, 768], pixels)]).unwrap();
    if let Some(data) = output[0].as_f32_slice() {
        let max_val = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        println!("Formula-det: shape={:?}, max={:.4}", output[0].shape(), max_val);
        assert!(max_val > 0.0);
    }
    println!("formula-det: PASSED");
}

#[test]
fn test_formula_rec_e2e() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let image_path = fixtures.join("formula.png");

    let enc_path = models.join("formula-rec/trocr-deit/encoder_model.onnx");
    let dec_path = models.join("formula-rec/trocr-deit/decoder_model.onnx");
    let tok_path = models.join("formula-rec/trocr-deit/tokenizer.json");

    if !enc_path.exists() || !dec_path.exists() || !image_path.exists() {
        println!("Skipping"); return;
    }

    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);

    // Crop first formula region
    let crop = crop_region(&rgb, 417, 44, 276, 83);

    // Use recognize_formula (includes repair_latex + wrap_latex_delimiters)
    let enc_handle = ModelHandle::with_path("encoder", enc_path);
    let dec_handle = ModelHandle::with_path("decoder", dec_path);
    let enc_session = backend.create_session(&enc_handle, AccelerationMode::Cpu).unwrap();
    let dec_session = backend.create_session(&dec_handle, AccelerationMode::Cpu).unwrap();

    let params = RecognitionParams::default();
    let result = recognize_formula(&crop, &*enc_session, &*dec_session, &tok_path, &params).unwrap();

    println!("TrOCR result: {}", result.text);
    assert!(!result.text.is_empty(), "TrOCR should produce output");
    assert!(result.text.contains("mc") || result.text.contains("="), "Should recognize formula: {}", result.text);
    println!("formula-rec-e2e: PASSED");
}

#[test]
fn test_multi_model() {
    let models = models_dir();
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let all = vec![
        ("formula-det", "formula-det/yolov8-mfd/mathcraft-mfd.onnx"),
        ("text-det", "text-det/ppocrv5-mobile/ppocrv5_mobile_det.onnx"),
        ("text-rec-v6", "v6_models/PP-OCRv6_medium_rec_infer/inference.onnx"),
        ("text-rec", "text-rec/ppocrv5-mobile/ppocrv5_mobile_rec.onnx"),
        ("doc-ori", "pplcnet_doc_ori/default/pplcnet_doc_ori.onnx"),
    ];
    let mut loaded = 0;
    for (name, rel) in all {
        let path = models.join(rel);
        if !path.exists() { println!("Skipped: {}", name); continue; }
        let handle = ModelHandle::with_path(name, path);
        match backend.create_session(&handle, AccelerationMode::Cpu) {
            Ok(_) => { loaded += 1; println!("Loaded: {}", name); }
            Err(e) => println!("Failed: {} ({})", name, e),
        }
    }
    println!("Loaded {}/4", loaded);
    assert!(loaded >= 1);
    println!("multi-model: PASSED");
}

#[test]
fn test_text_e2e() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let det_path = models.join("text-det/v6-small/inference.onnx");
    let image_path = fixtures.join("text.png");
    if !det_path.exists() || text_rec_model(&models).is_none() || !image_path.exists() { println!("Skipping"); return; }

    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    println!("1. Image: {}x{}", rgb.width(), rgb.height());

    let w = rgb.width(); let h = rgb.height();
    let scale = if w.max(h) > 960 { 960.0 / w.max(h) as f32 } else { 1.0 };
    let nw = ((w as f32 * scale).ceil() as u32 + 31) / 32 * 32;
    let nh = ((h as f32 * scale).ceil() as u32 + 31) / 32 * 32;
    let resized = operations::resize(&rgb, nw, nh);
    let padded = operations::pad_to_stride(&resized, 32);

    let det_handle = ModelHandle::with_path("text-det", det_path);
    let det_session = backend.create_session(&det_handle, AccelerationMode::Cpu).unwrap();
    let det_out = det_session.run(&[Tensor::float32("x", vec![1, 3, padded.height() as usize, padded.width() as usize],
        operations::normalize(&padded, &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]))]).unwrap();
    println!("2. Det output: {:?}", det_out[0].shape());

    let det_data = det_out[0].as_f32_slice().unwrap();
    let det_shape = det_out[0].shape();
    let det_h = det_shape[2];
    let det_w = det_shape[3];
    let scale_x = w as f32 / det_w as f32;
    let scale_y = h as f32 / det_h as f32;

    let mut boxes: Vec<(u32, u32, u32, u32)> = Vec::new();
    let thresh = 0.5f32;
    let mut visited = vec![false; det_h * det_w];
    for y in 0..det_h {
        for x in 0..det_w {
            if visited[y * det_w + x] { continue; }
            if det_data[y * det_w + x] > thresh {
                let mut min_x = x; let mut max_x = x;
                let mut min_y = y; let mut max_y = y;
                let mut queue: std::collections::VecDeque<(usize, usize)> = std::collections::VecDeque::new();
                queue.push_back((x as usize, y as usize));
                visited[y * det_w + x] = true;
                while let Some((cx, cy)) = queue.pop_front() {
                    min_x = min_x.min(cx); max_x = max_x.max(cx);
                    min_y = min_y.min(cy); max_y = max_y.max(cy);
                    for (dx, dy) in &[(1i32,0i32),(-1,0),(0,1),(0,-1)] {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx >= 0 && nx < det_w as i32 && ny >= 0 && ny < det_h as i32 {
                            let ni = ny as usize * det_w + nx as usize;
                            if !visited[ni] && det_data[ni] > thresh {
                                visited[ni] = true;
                                queue.push_back((nx as usize, ny as usize));
                            }
                        }
                    }
                }
                let bx = (min_x as f32 * scale_x).max(0.0) as u32;
                let by = (min_y as f32 * scale_y).max(0.0) as u32;
                let bw = ((max_x - min_x + 1) as f32 * scale_x).min(w as f32 - bx as f32) as u32;
                let bh = ((max_y - min_y + 1) as f32 * scale_y).min(h as f32 - by as f32) as u32;
                if bw > 4 && bh > 4 {
                    boxes.push((bx, by, bw, bh));
                }
            }
        }
    }
    println!("3. Found {} text regions", boxes.len());

    // Sort boxes by reading order: top-to-bottom, then left-to-right
    boxes.sort_by(|a, b| {
        let ay = a.1;
        let by = b.1;
        ay.cmp(&by).then_with(|| a.0.cmp(&b.0))
    });

    let mut recognized = Vec::new();
    for (i, &(bx, by, bw, bh)) in boxes.iter().enumerate() {
        // Add vertical padding (20% of height, min 4px) for better recognition
        let pad_y = ((bh as f32 * 0.2) as u32).max(4);
        let padded_by = by.saturating_sub(pad_y);
        let padded_bh = (bh + pad_y * 2).min(h - padded_by);
        let crop = crop_region(&rgb, bx, padded_by, bw, padded_bh);
        if let Some(text) = run_text_rec(&backend, &models, &crop) {
            println!("   Region {}: \"{}\" ({}x{})", i, text, bw, padded_bh);
            if !text.trim().is_empty() {
                recognized.push(text);
            }
        }
    }

    assert!(!boxes.is_empty(), "Should detect at least one text region");
    assert!(!recognized.is_empty(), "Should recognize text from at least one detected region");
    assert!(recognized.iter().any(|text| {
        let lower = text.to_ascii_lowercase();
        lower.contains("quick") || lower.contains("lorem") || lower.contains("ipsum")
    }), "Recognized text should contain expected fixture words: {:?}", recognized);
    println!("4. Text e2e: PASSED");
}

#[test]
fn test_formula_e2e() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let det_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
    let image_path = fixtures.join("formula.png");
    if !det_path.exists() || !image_path.exists() { println!("Skipping"); return; }

    let backend = OnnxRuntimeBackend::new(models).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    println!("1. Image: {}x{}", rgb.width(), rgb.height());

    let (letterboxed, _scale, _pad_x, _pad_y) = operations::letterbox(&rgb, 768);
    let pixels = operations::normalize(&letterboxed, &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]);
    let handle = ModelHandle::with_path("formula-det", det_path);
    let session = backend.create_session(&handle, AccelerationMode::Cpu).unwrap();
    let output = session.run(&[Tensor::float32("images", vec![1, 3, 768, 768], pixels)]).unwrap();
    println!("2. Output: {:?}", output[0].shape());

    if let Some(data) = output[0].as_f32_slice() {
        let shape = output[0].shape();
        // ort reports [1, 6, N] but data is [N, 6] row-major
        // Each 6 values: [cx, cy, w, h, emb_score, iso_score]
        let num_preds = shape[1].max(shape[2]);
        println!("   YOLO output: shape={:?}, {} anchors", shape, num_preds);

        let mut detections = 0;
        for p in 0..num_preds.min(20) {
            let base = p * 6;
            if base + 5 >= data.len() { break; }
            let cx = data[base];
            let cy = data[base + 1];
            let bw = data[base + 2];
            let bh = data[base + 3];
            let conf0 = data[base + 4];
            let conf1 = data[base + 5];
            let conf = conf0.max(conf1);
            println!("   Anchor {}: cx={:.1}, cy={:.1}, w={:.1}, h={:.1}, emb={:.4}, iso={:.4}, max={:.4}",
                p, cx, cy, bw, bh, conf0, conf1, conf);
            if conf > 0.25 { detections += 1; }
        }
        println!("3. First 20 anchors, {} above 0.25", detections);
    }
    println!("4. Formula e2e: PASSED");
}
