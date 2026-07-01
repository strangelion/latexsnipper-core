use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::operations;
use latexsnipper_runtime::{OnnxRuntimeBackend, RuntimeBackend, AccelerationMode, ModelHandle};
use latexsnipper_tensor::Tensor;
use std::time::{Duration, Instant};

fn models_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap()
        .join("models")
}

fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .parent().unwrap()
        .join("fixtures")
}

fn report(name: &str, iterations: usize, elapsed: Duration) {
    let per_iter = elapsed.as_nanos() / iterations as u128;
    println!("bench {name}: {iterations} iters, {elapsed:.2?}, {per_iter} ns/iter ({:.1} ms/iter)", elapsed.as_millis() as f64 / iterations as f64);
}

fn bench_text_recognition() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&fixtures.join("text.png"))).unwrap();
    let rgb = operations::bgr_to_rgb(&img);
    let crop = operations::crop(&rgb, latexsnipper_ast::Rect::new(20.0, 40.0, 400.0, 40.0));

    let rec_path = models.join("text-rec/v6-small/inference.onnx");
    let rec_handle = ModelHandle::with_path("text-rec", rec_path);
    let rec_session = backend.create_session(&rec_handle, AccelerationMode::Cpu).unwrap();

    for _ in 0..3 {
        let resized = operations::resize(&crop, 320, 48);
        let pixels = operations::normalize(&resized, &[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);
        let input = Tensor::float32("x", vec![1, 3, 48, 320], pixels);
        let _ = rec_session.run(&[input]);
    }

    let iterations = 50;
    let start = Instant::now();
    for _ in 0..iterations {
        let resized = operations::resize(&crop, 320, 48);
        let pixels = operations::normalize(&resized, &[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);
        let input = Tensor::float32("x", vec![1, 3, 48, 320], pixels);
        let _ = rec_session.run(&[input]);
    }
    report("text_recognition_v6_small", iterations, start.elapsed());
}

fn bench_formula_detection() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&fixtures.join("formula.png"))).unwrap();
    let rgb = operations::bgr_to_rgb(&img);
    let (letterboxed, _, _, _) = operations::letterbox(&rgb, 768);
    let pixels = operations::normalize(&letterboxed, &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]);

    let det_path = models.join("formula-det/yolov8-mfd/mathcraft-mfd.onnx");
    let det_handle = ModelHandle::with_path("formula-det", det_path);
    let det_session = backend.create_session(&det_handle, AccelerationMode::Cpu).unwrap();

    for _ in 0..3 {
        let input = Tensor::float32("images", vec![1, 3, 768, 768], pixels.clone());
        let _ = det_session.run(&[input]);
    }

    let iterations = 50;
    let start = Instant::now();
    for _ in 0..iterations {
        let input = Tensor::float32("images", vec![1, 3, 768, 768], pixels.clone());
        let _ = det_session.run(&[input]);
    }
    report("formula_detection_yolov8", iterations, start.elapsed());
}

fn bench_formula_recognition() {
    let models = models_dir();
    let fixtures = fixtures_dir();
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();
    let img = decode(ImageSource::File(&fixtures.join("formula.png"))).unwrap();
    let rgb = operations::bgr_to_rgb(&img);
    let crop = operations::crop(&rgb, latexsnipper_ast::Rect::new(417.0, 44.0, 276.0, 83.0));

    let enc_path = models.join("formula-rec/trocr-deit/encoder_model.onnx");
    let dec_path = models.join("formula-rec/trocr-deit/decoder_model.onnx");
    let enc_handle = ModelHandle::with_path("encoder", enc_path);
    let dec_handle = ModelHandle::with_path("decoder", dec_path);
    let enc_session = backend.create_session(&enc_handle, AccelerationMode::Cpu).unwrap();
    let dec_session = backend.create_session(&dec_handle, AccelerationMode::Cpu).unwrap();

    let resized = operations::resize(&crop, 384, 384);
    let pixels = operations::normalize(&resized, &[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);

    for _ in 0..3 {
        let input = Tensor::float32("pixel_values", vec![1, 3, 384, 384], pixels.clone());
        let enc_out = enc_session.run(&[input]).unwrap();
        let hidden = enc_out[0].as_f32_slice().unwrap().to_vec();
        let hidden_shape = enc_out[0].shape().to_vec();
        let mut token_ids: Vec<i64> = vec![2];
        for _ in 0..10 {
            let input_ids = Tensor::int64("input_ids", vec![1, token_ids.len()], token_ids.clone());
            let hidden_tensor = Tensor::float32("encoder_hidden_states", hidden_shape.clone(), hidden.clone());
            let dec_out = dec_session.run(&[input_ids, hidden_tensor]).unwrap();
            let logits = dec_out[0].as_f32_slice().unwrap();
            let vocab_size = dec_out[0].shape().last().unwrap_or(&0);
            let last_start = (token_ids.len() - 1) * vocab_size;
            let last_end = last_start + vocab_size;
            if last_end > logits.len() { break; }
            let step = &logits[last_start..last_end];
            let best = step.iter().enumerate().max_by(|a,b| a.1.partial_cmp(b.1).unwrap()).map(|(i,_)| i).unwrap_or(0);
            if best == 2 { break; }
            token_ids.push(best as i64);
        }
    }

    let iterations = 10;
    let start = Instant::now();
    for _ in 0..iterations {
        let input = Tensor::float32("pixel_values", vec![1, 3, 384, 384], pixels.clone());
        let enc_out = enc_session.run(&[input]).unwrap();
        let hidden = enc_out[0].as_f32_slice().unwrap().to_vec();
        let hidden_shape = enc_out[0].shape().to_vec();
        let mut token_ids: Vec<i64> = vec![2];
        for _ in 0..50 {
            let input_ids = Tensor::int64("input_ids", vec![1, token_ids.len()], token_ids.clone());
            let hidden_tensor = Tensor::float32("encoder_hidden_states", hidden_shape.clone(), hidden.clone());
            let dec_out = dec_session.run(&[input_ids, hidden_tensor]).unwrap();
            let logits = dec_out[0].as_f32_slice().unwrap();
            let vocab_size = dec_out[0].shape().last().unwrap_or(&0);
            let last_start = (token_ids.len() - 1) * vocab_size;
            let last_end = last_start + vocab_size;
            if last_end > logits.len() { break; }
            let step = &logits[last_start..last_end];
            let best = step.iter().enumerate().max_by(|a,b| a.1.partial_cmp(b.1).unwrap()).map(|(i,_)| i).unwrap_or(0);
            if best == 2 { break; }
            token_ids.push(best as i64);
        }
    }
    report("formula_recognition_trocr", iterations, start.elapsed());
}

fn main() {
    println!("=== Core Recognition Benchmarks ===\n");
    println!("--- Text Recognition (v6 small) ---");
    bench_text_recognition();
    println!("\n--- Formula Detection (YOLOv8-MFD) ---");
    bench_formula_detection();
    println!("\n--- Formula Recognition (TrOCR) ---");
    bench_formula_recognition();
    println!("\n=== Benchmarks Complete ===");
}
