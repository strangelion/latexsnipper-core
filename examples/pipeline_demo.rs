//! Pipeline Demo — Image → Detection → Recognition → AST → Export
//!
//! Run: cargo run --example pipeline_demo

use latexsnipper_ast::*;
use latexsnipper_conversion::{DocumentConverter, OutputFormat};
use latexsnipper_runtime::{OnnxRuntimeBackend, RuntimeBackend, AccelerationMode, ModelHandle};
use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::image::SnipperImage;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_inference::{detect_formulas, recognize_formula, DetectionParams, RecognitionParams};
use std::path::PathBuf;

fn models_dir() -> PathBuf {
    PathBuf::from("models")
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from("fixtures")
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

fn main() {
    let models = models_dir();
    let fixtures = fixtures_dir();

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║     LaTeXSnipper Pipeline Demo                         ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    // 1. Load image
    let image_path = fixtures.join("formula.png");
    if !image_path.exists() {
        println!("Error: fixture not found at {:?}", image_path);
        return;
    }

    let backend = OnnxRuntimeBackend::new(models.clone()).expect("Failed to create ONNX backend");
    let img = decode(ImageSource::File(&image_path)).expect("Failed to decode image");
    let rgb = rgba_to_rgb(&img);
    println!("1. Input: {}x{} image\n", rgb.width(), rgb.height());

    // 2. Detect formulas
    let det_config = latexsnipper_model::ModelConfig::load(
        &models.join("formula-det/yolov8-mfd")
    ).expect("Failed to load detection config");

    let det_params = DetectionParams::from_config(&det_config);
    let det_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
    let det_handle = ModelHandle::with_path("formula-det", det_path);
    let det_session = backend.create_session(&det_handle, AccelerationMode::Cpu).unwrap();

    let mut detections = detect_formulas(&rgb, &*det_session, &det_params).unwrap();
    detections.sort_by(|a, b| {
        a.rect.y.partial_cmp(&b.rect.y).unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.rect.x.partial_cmp(&b.rect.x).unwrap_or(std::cmp::Ordering::Equal))
    });
    println!("2. Detected {} formula regions\n", detections.len());

    // 3. Recognize formulas
    let enc_path = models.join("formula-rec/trocr-deit/encoder_model.onnx");
    let dec_path = models.join("formula-rec/trocr-deit/decoder_model.onnx");
    let tok_path = models.join("formula-rec/trocr-deit/tokenizer.json");

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
                    println!("   [{i}] \"{}\"", result.text.chars().take(50).collect::<String>());
                    let mut f = Formula::latex(result.text);
                    f.confidence = result.confidence;
                    blocks.push(Block::Formula(FormulaBlock {
                        formula: f,
                        geometry: Some(Rect::new(x as f32, y as f32, w as f32, h as f32)),
                        source: Some(SourceInfo::new()),
                    }));
                }
                Err(e) => println!("   [{i}] Error: {e}"),
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
    println!("\n3. Document AST: {} blocks\n", doc.block_count());

    // 5. Export to all formats
    println!("4. Export Results:\n");

    let latex = DocumentConverter::new(OutputFormat::Latex).convert(&doc).unwrap();
    println!("   ── LaTeX ({} chars) ──", latex.len());
    for line in latex.lines().take(3) {
        println!("   {}", line);
    }

    let md = DocumentConverter::new(OutputFormat::MarkdownBlock).convert(&doc).unwrap();
    println!("\n   ── Markdown ({} chars) ──", md.len());
    for line in md.lines().take(3) {
        println!("   {}", line);
    }

    let typst = DocumentConverter::new(OutputFormat::Typst).convert(&doc).unwrap();
    println!("\n   ── Typst ({} chars) ──", typst.len());
    for line in typst.lines().take(3) {
        println!("   {}", line);
    }

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║  Pipeline: Image → Detect → Recognize → AST → Export   ║");
    println!("╚══════════════════════════════════════════════════════════╝");
}
