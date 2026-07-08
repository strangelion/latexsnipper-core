#![cfg(target_os = "windows")]

use latexsnipper_ast::Rect;
use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::image::SnipperImage;
use latexsnipper_image::operations;
use latexsnipper_inference::{
    detect_text, load_keys, recognize_text_with_keys, TextDetParams, TextRecParams,
};
use latexsnipper_pipeline::text_recognition_service::TextRecognitionService;
use latexsnipper_runtime::{AccelerationMode, ModelHandle, OnnxRuntimeBackend, RuntimeBackend};

fn root_dir() -> std::path::PathBuf {
    std::env::current_dir().unwrap()
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
fn openocr_mobile_text_models_recognize_fixture() {
    let root = root_dir();
    let models = root.join("models");
    let det_dir = models.join("text-det/openocr-mobile");
    let rec_dir = models.join("text-rec/openocr-mobile");
    let image_path = root.join("tests/fixtures/openocr/text-en.png");

    let det_path = det_dir.join("model.onnx");
    let rec_path = rec_dir.join("model.onnx");
    let keys_path = rec_dir.join("inference.yml");
    if !det_path.exists() || !rec_path.exists() || !keys_path.exists() || !image_path.exists() {
        eprintln!("SKIP: OpenOCR mobile models or fixture not found");
        return;
    }

    let image = rgba_to_rgb(&decode(ImageSource::File(&image_path)).unwrap());
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();

    let det_config = latexsnipper_model::ModelConfig::load(&det_dir).unwrap();
    let det_params = TextDetParams::from_config(&det_config);
    let det_handle = ModelHandle::with_path("openocr-text-det", det_path);
    let det_session = backend
        .create_session(&det_handle, AccelerationMode::Cpu)
        .unwrap();
    let detections = detect_text(&image, &*det_session, &det_params).unwrap();
    eprintln!("OpenOCR detections: {:?}", detections);
    assert!(
        !detections.is_empty(),
        "OpenOCR text detector found no text"
    );

    let rec_config = latexsnipper_model::ModelConfig::load(&rec_dir).unwrap();
    let rec_params = TextRecParams::from_config(&rec_config);
    let rec_handle = ModelHandle::with_path("openocr-text-rec", rec_path);
    let rec_session = backend
        .create_session(&rec_handle, AccelerationMode::Cpu)
        .unwrap();
    let (keys, first_char_id) = load_keys(&keys_path).unwrap();

    let full =
        recognize_text_with_keys(&image, &*rec_session, &keys, first_char_id, &rec_params).unwrap();
    eprintln!("OpenOCR full-image text: {:?}", full.text);
    assert!(
        full.text.contains("Hello") && full.text.contains("OCR"),
        "OpenOCR text recognizer returned unexpected text: {:?}",
        full.text
    );

    let det = detections
        .iter()
        .max_by(|a, b| {
            let aa = a.rect.width * a.rect.height;
            let bb = b.rect.width * b.rect.height;
            aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap();
    let pad = 2.0;
    let crop_x = (det.rect.x - pad).max(0.0);
    let crop_y = (det.rect.y - pad).max(0.0);
    let crop_w = (det.rect.width + pad * 2.0).min(image.width() as f32 - crop_x);
    let crop_h = (det.rect.height + pad * 2.0).min(image.height() as f32 - crop_y);
    let crop = operations::crop(&image, Rect::new(crop_x, crop_y, crop_w, crop_h));
    let cropped =
        recognize_text_with_keys(&crop, &*rec_session, &keys, first_char_id, &rec_params).unwrap();
    eprintln!("OpenOCR cropped text: {:?}", cropped.text);
    assert!(
        cropped.text.contains("Hello") && cropped.text.contains("OCR"),
        "OpenOCR crop recognition returned unexpected text: {:?}",
        cropped.text
    );

    if let Some(quad) = det.quad.as_ref() {
        let (tw, th) = quad.warp_target_size();
        let padding = (th as f32 * 0.1).max(2.0);
        let warped = operations::warp_quad_to_rect(&image, quad, tw.max(4), th.max(4), padding);
        let warped_text =
            recognize_text_with_keys(&warped, &*rec_session, &keys, first_char_id, &rec_params)
                .unwrap();
        eprintln!("OpenOCR service-style warped text: {:?}", warped_text.text);
    }

    let x = det.rect.x as u32;
    let y = det.rect.y as u32;
    let w = det.rect.width as u32;
    let h = det.rect.height as u32;
    let pad_x = (w as f32 * 0.02).max(2.0) as u32;
    let pad_y = (h as f32 * 0.2).max(4.0) as u32;
    let crop_x = x.saturating_sub(pad_x);
    let crop_y = y.saturating_sub(pad_y);
    let crop_w = (w + pad_x * 2).min(image.width().saturating_sub(crop_x));
    let crop_h = (h + pad_y * 2).min(image.height().saturating_sub(crop_y));
    let service_crop = operations::crop(
        &image,
        Rect::new(crop_x as f32, crop_y as f32, crop_w as f32, crop_h as f32),
    );
    let service_crop_text = recognize_text_with_keys(
        &service_crop,
        &*rec_session,
        &keys,
        first_char_id,
        &rec_params,
    )
    .unwrap();
    eprintln!(
        "OpenOCR service-style crop text: {:?}",
        service_crop_text.text
    );

    let service = TextRecognitionService::try_load(
        &models,
        Some("openocr-mobile"),
        Some(std::sync::Arc::new(backend)),
        AccelerationMode::Cpu,
    )
    .expect("OpenOCR text recognition service should load");
    let service_text = service
        .recognize_region(&image, &det.rect, det.quad.as_ref())
        .unwrap();
    eprintln!("OpenOCR service text: {:?}", service_text);
    assert!(
        service_text.contains("Hello") && service_text.contains("OCR"),
        "OpenOCR service recognition returned unexpected text: {:?}",
        service_text
    );
}
