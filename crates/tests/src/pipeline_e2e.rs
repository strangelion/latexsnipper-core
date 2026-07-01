//! End-to-end pipeline test: Image → Detection → Recognition → AST → Export

use latexsnipper_runtime::{OnnxRuntimeBackend, RuntimeBackend, AccelerationMode, ModelHandle};
use latexsnipper_tensor::Tensor;
use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::operations;
use latexsnipper_image::image::SnipperImage;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_inference::{
    detect_formulas, recognize_formula, DetectionParams, RecognitionParams,
    group_formula_detections, filter_formula_detections,
};
use latexsnipper_ast::*;
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
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

fn crop_region(img: &SnipperImage, x: u32, y: u32, w: u32, h: u32) -> SnipperImage {
    let img_w = img.width();
    let img_h = img.height();
    let x = x.min(img_w.saturating_sub(1));
    let y = y.min(img_h.saturating_sub(1));
    let w = w.min(img_w - x);
    let h = h.min(img_h - y);
    if w == 0 || h == 0 {
        return SnipperImage::new(1, 1, PixelFormat::Rgb, vec![0, 0, 0]);
    }
    let mut pixels = Vec::with_capacity((w * h * 3) as usize);
    for row in 0..h {
        let src_off = ((y + row) * img_w + x) * 3;
        let src_end = src_off + w * 3;
        if src_end as usize <= img.pixels().len() {
            pixels.extend_from_slice(&img.pixels()[src_off as usize..src_end as usize]);
        }
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

/// Full pipeline: detect formulas → recognize → build AST → export
#[test]
fn test_formula_pipeline_e2e() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let image_path = fixtures.join("formula.png");

    let det_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
    let enc_path = models.join("formula-rec/trocr-deit/encoder_model.onnx");
    let dec_path = models.join("formula-rec/trocr-deit/decoder_model.onnx");
    let tok_path = models.join("formula-rec/trocr-deit/tokenizer.json");

    if !det_path.exists() || !enc_path.exists() || !image_path.exists() {
        println!("Skipping: models or fixture not found");
        return;
    }

    println!("=== Formula Pipeline E2E ===\n");

    // 1. Load image
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    println!("1. Loaded image: {}x{}", rgb.width(), rgb.height());

    // 2. Detect formulas
    let det_config = latexsnipper_model::ModelConfig::load(
        &models.join("formula-det/yolov8-mfd"),
    )
    .unwrap();
    let det_params = DetectionParams::from_config(&det_config);

    let det_handle = ModelHandle::with_path("formula-det", det_path);
    let det_session = backend.create_session(&det_handle, AccelerationMode::Cpu).unwrap();

    let mut detections = detect_formulas(&rgb, &*det_session, &det_params).unwrap();
    group_formula_detections(&mut detections);
    filter_formula_detections(&mut detections, 100.0, 0.2);
    println!("2. Detected {} formula regions", detections.len());

    // 3. Recognize formulas
    let enc_handle = ModelHandle::with_path("encoder", enc_path);
    let dec_handle = ModelHandle::with_path("decoder", dec_path);
    let enc_session = backend.create_session(&enc_handle, AccelerationMode::Cpu).unwrap();
    let dec_session = backend.create_session(&dec_handle, AccelerationMode::Cpu).unwrap();

    let rec_params = RecognitionParams::default();
    let mut blocks = Vec::new();

    for (i, det) in detections.iter().enumerate() {
        let x = det.rect.x as u32;
        let y = det.rect.y as u32;
        let w = det.rect.width as u32;
        let h = det.rect.height as u32;

        if w >= 4 && h >= 4 {
            let crop = crop_region(&rgb, x, y, w, h);
            match recognize_formula(&crop, &*enc_session, &*dec_session, &tok_path, &rec_params) {
                Ok(result) => {
                    println!("   Formula {}: \"{}\" (conf={:.3})", i + 1, result.text, result.confidence);
                    let mut f = Formula::latex(result.text);
                    f.confidence = result.confidence;
                    blocks.push(Block::Formula(FormulaBlock {
                        formula: f,
                        geometry: Some(Rect::new(x as f32, y as f32, w as f32, h as f32)),
                        source: Some(SourceInfo::new()),
                    }));
                }
                Err(e) => println!("   Formula {} failed: {}", i + 1, e),
            }
        }
    }

    // 4. Build Document AST
    let doc = Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: rgb.width() as f32,
            height: rgb.height() as f32,
            blocks,
            page_number: Some(1),
        }],
        id_gen: NodeIdGenerator::new(),
    };
    println!("3. Built Document AST with {} blocks", doc.block_count());

    // 5. Export to formats
    println!("\n=== LaTeX Output ===");
    let latex = DocumentConverter::new(OutputFormat::Latex).convert(&doc).unwrap();
    println!("{}\n", latex);

    println!("=== Markdown Output ===");
    let md = DocumentConverter::new(OutputFormat::MarkdownBlock).convert(&doc).unwrap();
    println!("{}\n", md);

    println!("=== Typst Output ===");
    let typst = DocumentConverter::new(OutputFormat::Typst).convert(&doc).unwrap();
    println!("{}\n", typst);

    assert!(!doc.all_blocks().is_empty(), "Document should have blocks");
}

/// Full pipeline for text: detect → recognize → AST → export
#[test]
fn test_text_pipeline_e2e() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let image_path = fixtures.join("text.png");

    // Find models
    let det_candidates = [
        models.join("PP-OCRv6_medium_det_infer/inference.onnx"),
        models.join("PP-OCRv6_small_det_infer/inference.onnx"),
    ];
    let det_path = det_candidates.iter().find(|p| p.exists());

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

    if det_path.is_none() || rec_path.is_none() || !image_path.exists() {
        println!("Skipping: models or fixture not found");
        return;
    }

    println!("=== Text Pipeline E2E ===\n");

    // 1. Load image
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    println!("1. Loaded image: {}x{}", rgb.width(), rgb.height());

    // 2. Detect text regions (simplified: use probability map)
    let handle = ModelHandle::with_path("text-det", det_path.unwrap().to_path_buf());
    let session = backend.create_session(&handle, AccelerationMode::Cpu).unwrap();

    let w = rgb.width();
    let h = rgb.height();
    let scale = if w.max(h) > 960 { 960.0 / w.max(h) as f32 } else { 1.0 };
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
    let det_data = output[0].as_f32_slice().unwrap();
    let det_shape = output[0].shape();
    let det_h = det_shape[2];
    let det_w = det_shape[3];
    let scale_x = w as f32 / det_w as f32;
    let scale_y = h as f32 / det_h as f32;

    // Extract bounding boxes from probability map
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
                queue.push_back((x, y));
                visited[y * det_w + x] = true;
                while let Some((cx, cy)) = queue.pop_front() {
                    min_x = min_x.min(cx); max_x = max_x.max(cx);
                    min_y = min_y.min(cy); max_y = max_y.max(cy);
                    for (dx, dy) in &[(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
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
    boxes.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    println!("2. Detected {} text regions", boxes.len());

    // 3. Recognize text
    let rec_handle = ModelHandle::with_path("text-rec", rec_path.unwrap().to_path_buf());
    let rec_session = backend.create_session(&rec_handle, AccelerationMode::Cpu).unwrap();

    let keys_content = std::fs::read_to_string(keys_path.unwrap()).unwrap();
    let keys = load_paddle_character_dict(&keys_content);

    let mut blocks = Vec::new();
    for (i, &(bx, by, bw, bh)) in boxes.iter().enumerate() {
        let pad_y = ((bh as f32 * 0.2) as u32).max(4);
        let padded_by = by.saturating_sub(pad_y);
        let padded_bh = (bh + pad_y * 2).min(h - padded_by);
        let crop = crop_region(&rgb, bx, padded_by, bw, padded_bh);

        // Prepare input for text recognition
        let img_wh_ratio = crop.width() as f32 / crop.height() as f32;
        let default_wh_ratio: f32 = 320.0 / 48.0;
        let max_wh_ratio = default_wh_ratio.max(img_wh_ratio);
        let target_w = (48.0 * max_wh_ratio).round() as u32;

        let resized = operations::resize(&crop, target_w, 48);
        let norm = operations::normalize(&resized, &[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);

        let input = Tensor::float32("x", vec![1, 3, 48, target_w as usize], norm);
        if let Ok(output) = rec_session.run(&[input]) {
            if let Some(data) = output[0].as_f32_slice() {
                let shape = output[0].shape();
                let text = ctc_decode(data, shape[1], shape[2], &keys);
                if !text.trim().is_empty() {
                    println!("   Region {}: \"{}\"", i + 1, text);
                    blocks.push(Block::Paragraph(ParagraphBlock {
                        inlines: vec![Inline::Text(TextRun::new(text))],
                        geometry: Some(Rect::new(bx as f32, by as f32, bw as f32, bh as f32)),
                        source: Some(SourceInfo::new()),
                    }));
                }
            }
        }
    }

    // 4. Build Document AST
    let doc = Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: rgb.width() as f32,
            height: rgb.height() as f32,
            blocks,
            page_number: Some(1),
        }],
        id_gen: NodeIdGenerator::new(),
    };
    println!("3. Built Document AST with {} blocks", doc.block_count());

    // 5. Export
    println!("\n=== Markdown Output ===");
    let md = DocumentConverter::new(OutputFormat::MarkdownBlock).convert(&doc).unwrap();
    println!("{}\n", md);

    assert!(!doc.all_blocks().is_empty(), "Document should have blocks");
}

/// Mixed content pipeline: detect both formulas and text
#[test]
fn test_mixed_pipeline_e2e() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let image_path = fixtures.join("mixed.png");

    let det_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
    let enc_path = models.join("formula-rec/trocr-deit/encoder_model.onnx");
    let dec_path = models.join("formula-rec/trocr-deit/decoder_model.onnx");
    let tok_path = models.join("formula-rec/trocr-deit/tokenizer.json");

    let text_det_candidates = [
        models.join("PP-OCRv6_medium_det_infer/inference.onnx"),
        models.join("PP-OCRv6_small_det_infer/inference.onnx"),
    ];
    let text_det_path = text_det_candidates.iter().find(|p| p.exists());

    let text_rec_candidates = [
        models.join("PP-OCRv6_medium_rec_infer/inference.onnx"),
        models.join("PP-OCRv6_small_rec_infer/inference.onnx"),
    ];
    let text_rec_path = text_rec_candidates.iter().find(|p| p.exists());

    let keys_candidates = [
        models.join("PP-OCRv6_medium_rec_infer/inference.yml"),
        models.join("PP-OCRv6_small_rec_infer/inference.yml"),
    ];
    let keys_path = keys_candidates.iter().find(|p| p.exists());

    if !det_path.exists() || !enc_path.exists() || !image_path.exists() {
        println!("Skipping: models or fixture not found");
        return;
    }

    println!("=== Mixed Content Pipeline E2E ===\n");

    // 1. Load image
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&image_path)).unwrap();
    let rgb = rgba_to_rgb(&img);
    println!("1. Loaded image: {}x{}", rgb.width(), rgb.height());

    let mut all_blocks: Vec<Block> = Vec::new();

    // 2. Detect and recognize formulas
    if det_path.exists() && enc_path.exists() {
        let det_config = latexsnipper_model::ModelConfig::load(
            &models.join("formula-det/yolov8-mfd"),
        )
        .unwrap();
        let det_params = DetectionParams::from_config(&det_config);

        let det_handle = ModelHandle::with_path("formula-det", det_path);
        let det_session = backend.create_session(&det_handle, AccelerationMode::Cpu).unwrap();

        let mut detections = detect_formulas(&rgb, &*det_session, &det_params).unwrap();
        group_formula_detections(&mut detections);
        filter_formula_detections(&mut detections, 100.0, 0.2);
        println!("2. Detected {} formula regions", detections.len());

        let enc_handle = ModelHandle::with_path("encoder", enc_path);
        let dec_handle = ModelHandle::with_path("decoder", dec_path);
        let enc_session = backend.create_session(&enc_handle, AccelerationMode::Cpu).unwrap();
        let dec_session = backend.create_session(&dec_handle, AccelerationMode::Cpu).unwrap();

        let rec_params = RecognitionParams::default();
        for det in &detections {
            let x = det.rect.x as u32;
            let y = det.rect.y as u32;
            let w = det.rect.width as u32;
            let h = det.rect.height as u32;

            if w >= 4 && h >= 4 {
                let crop = crop_region(&rgb, x, y, w, h);
                if let Ok(result) = recognize_formula(&crop, &*enc_session, &*dec_session, &tok_path, &rec_params) {
                    let mut f = Formula::latex(result.text);
                    f.confidence = result.confidence;
                    all_blocks.push(Block::Formula(FormulaBlock {
                        formula: f,
                        geometry: Some(Rect::new(x as f32, y as f32, w as f32, h as f32)),
                        source: Some(SourceInfo::new()),
                    }));
                }
            }
        }
    }

    // 3. Detect and recognize text (if models available)
    if let Some(text_det) = text_det_path {
        let handle = ModelHandle::with_path("text-det", text_det.to_path_buf());
        if let Ok(session) = backend.create_session(&handle, AccelerationMode::Cpu) {
            let w = rgb.width();
            let h = rgb.height();
            let scale = if w.max(h) > 960 { 960.0 / w.max(h) as f32 } else { 1.0 };
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
            if let Ok(output) = session.run(&[input]) {
                if let Some(det_data) = output[0].as_f32_slice() {
                    let det_shape = output[0].shape();
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
                                queue.push_back((x, y));
                                visited[y * det_w + x] = true;
                                while let Some((cx, cy)) = queue.pop_front() {
                                    min_x = min_x.min(cx); max_x = max_x.max(cx);
                                    min_y = min_y.min(cy); max_y = max_y.max(cy);
                                    for (dx, dy) in &[(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
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
                    boxes.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
                    println!("3. Detected {} text regions", boxes.len());

                    // Recognize text
                    if let Some(text_rec) = text_rec_path {
                        let rec_handle = ModelHandle::with_path("text-rec", text_rec.to_path_buf());
                        if let Ok(rec_session) = backend.create_session(&rec_handle, AccelerationMode::Cpu) {
                            let keys_content = std::fs::read_to_string(keys_path.unwrap()).unwrap();
                            let keys = load_paddle_character_dict(&keys_content);

                            for &(bx, by, bw, bh) in &boxes {
                                let pad_y = ((bh as f32 * 0.2) as u32).max(4);
                                let padded_by = by.saturating_sub(pad_y);
                                let padded_bh = (bh + pad_y * 2).min(h - padded_by);
                                let crop = crop_region(&rgb, bx, padded_by, bw, padded_bh);

                                let img_wh_ratio = crop.width() as f32 / crop.height() as f32;
                                let default_wh_ratio: f32 = 320.0 / 48.0;
                                let max_wh_ratio = default_wh_ratio.max(img_wh_ratio);
                                let target_w = (48.0 * max_wh_ratio).round() as u32;

                                let resized = operations::resize(&crop, target_w, 48);
                                let norm = operations::normalize(&resized, &[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);

                                let input = Tensor::float32("x", vec![1, 3, 48, target_w as usize], norm);
                                if let Ok(output) = rec_session.run(&[input]) {
                                    if let Some(data) = output[0].as_f32_slice() {
                                        let shape = output[0].shape();
                                        let text = ctc_decode(data, shape[1], shape[2], &keys);
                                        if !text.trim().is_empty() {
                                            all_blocks.push(Block::Paragraph(ParagraphBlock {
                                                inlines: vec![Inline::Text(TextRun::new(text))],
                                                geometry: Some(Rect::new(bx as f32, by as f32, bw as f32, bh as f32)),
                                                source: Some(SourceInfo::new()),
                                            }));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 4. Sort blocks by Y position for reading order
    all_blocks.sort_by(|a, b| {
        let ay = a.geometry().map(|g| g.y).unwrap_or(0.0);
        let by = b.geometry().map(|g| g.y).unwrap_or(0.0);
        ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal)
    });

    // 5. Build Document AST
    let doc = Document {
        metadata: Metadata::default(),
        pages: vec![Page {
            width: rgb.width() as f32,
            height: rgb.height() as f32,
            blocks: all_blocks,
            page_number: Some(1),
        }],
        id_gen: NodeIdGenerator::new(),
    };
    println!("4. Built Document AST with {} blocks", doc.block_count());

    // 6. Export
    println!("\n=== LaTeX Output ===");
    let latex = DocumentConverter::new(OutputFormat::Latex).convert(&doc).unwrap();
    println!("{}\n", latex);

    println!("=== Markdown Output ===");
    let md = DocumentConverter::new(OutputFormat::MarkdownBlock).convert(&doc).unwrap();
    println!("{}\n", md);

    assert!(!doc.all_blocks().is_empty(), "Document should have blocks");
}
