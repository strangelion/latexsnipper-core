use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use clap::Parser;
use latexsnipper_benchmark::formula::{
    FormulaBenchmarkManifest, FormulaPrediction, FormulaPredictionBundle, FormulaRunMetadata,
    FORMULA_PREDICTION_SCHEMA_VERSION,
};
use latexsnipper_image::color::PixelFormat;
use latexsnipper_image::decode::{decode, ImageSource};
use latexsnipper_image::image::SnipperImage;
use latexsnipper_inference::{
    load_tokenizer_from_str, recognize_formula_with_tokenizer, RecognitionParams,
};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, OnnxRuntimeBackend, RuntimeBackend};
use sha2::{Digest, Sha256};

#[derive(Debug, Parser)]
#[command(name = "latexsnipper-formula-predict")]
struct Args {
    /// Versioned formula benchmark manifest.
    #[arg(long)]
    formula_manifest: PathBuf,
    /// TrOCR directory containing encoder_model.onnx, decoder_model.onnx and tokenizer.json.
    #[arg(long)]
    model_dir: PathBuf,
    /// Prediction bundle destination.
    #[arg(long)]
    output: PathBuf,
    /// UTC timestamp recorded in the bundle. It is explicit to keep reruns auditable.
    #[arg(long)]
    timestamp_utc: String,
    #[arg(long, default_value = "trocr-deit")]
    model_id: String,
    #[arg(long, default_value = "models-v3.1.0")]
    model_version: String,
    /// Only CPU is accepted until an accelerator smoke test has passed on this machine.
    #[arg(long, default_value = "cpu")]
    provider: String,
    #[arg(long, default_value_t = 1)]
    warmup_iterations: usize,
    #[arg(long)]
    max_tokens: Option<usize>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.provider != "cpu" {
        return Err(format!(
            "provider '{}' is not validated by this runner; use cpu",
            args.provider
        )
        .into());
    }

    let manifest: FormulaBenchmarkManifest =
        serde_json::from_slice(&std::fs::read(&args.formula_manifest)?)?;
    let manifest_root = args
        .formula_manifest
        .parent()
        .ok_or("manifest path has no parent")?;
    let encoder_path = args.model_dir.join("encoder_model.onnx");
    let decoder_path = args.model_dir.join("decoder_model.onnx");
    let tokenizer_path = args.model_dir.join("tokenizer.json");
    for path in [&encoder_path, &decoder_path, &tokenizer_path] {
        if !path.is_file() {
            return Err(format!("required model artifact is missing: {}", path.display()).into());
        }
    }

    let model_load_start = Instant::now();
    let backend = OnnxRuntimeBackend::new(args.model_dir.clone())?;
    let encoder = backend.create_session(
        &ModelHandle::with_path("trocr-encoder", encoder_path.clone()),
        AccelerationMode::Cpu,
    )?;
    let decoder = backend.create_session(
        &ModelHandle::with_path("trocr-decoder", decoder_path.clone()),
        AccelerationMode::Cpu,
    )?;
    let tokenizer = load_tokenizer_from_str(&std::fs::read_to_string(&tokenizer_path)?)?;
    let model_load_ms = model_load_start.elapsed().as_secs_f64() * 1000.0;

    let mut params = RecognitionParams::default();
    if let Some(max_tokens) = args.max_tokens {
        params.max_tokens = max_tokens;
    }

    if let Some(sample) = manifest.samples.first() {
        let image = decode_rgb(&manifest_root.join(&sample.image))?;
        for _ in 0..args.warmup_iterations {
            let _ = recognize_formula_with_tokenizer(
                &image, &*encoder, &*decoder, &tokenizer, &params,
            )?;
        }
    }

    let mut predictions = Vec::with_capacity(manifest.samples.len());
    for (index, sample) in manifest.samples.iter().enumerate() {
        let image = decode_rgb(&manifest_root.join(&sample.image))?;
        let started = Instant::now();
        let result =
            recognize_formula_with_tokenizer(&image, &*encoder, &*decoder, &tokenizer, &params)?;
        let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
        let postprocess = result.postprocess.as_ref();
        let diagnostics = postprocess
            .into_iter()
            .flat_map(|evidence| {
                evidence.status_code.iter().cloned().chain(
                    evidence
                        .transformations
                        .iter()
                        .map(|item| format!("correction:{}", item.rule_id)),
                )
            })
            .collect();
        predictions.push(FormulaPrediction {
            sample_id: sample.id.clone(),
            raw_latex: result
                .raw_text
                .clone()
                .unwrap_or_else(|| result.text.clone()),
            normalized_latex: result.normalized_text.clone(),
            corrected_latex: Some(result.text.clone()),
            confidence: Some(f64::from(result.confidence)),
            diagnostics,
            correction_triggered: postprocess.is_some_and(|value| value.trigger.should_run),
            review_required: postprocess.is_some_and(|value| value.review_required),
            top_k: Vec::new(),
            latency_ms,
            premature_eos: postprocess
                .is_some_and(|value| value.corrected_validation.unexpected_eos),
            truncated: postprocess.is_some_and(|value| value.corrected_validation.truncated),
        });
        eprintln!(
            "[{}/{}] {} {:.1} ms",
            index + 1,
            manifest.samples.len(),
            sample.id,
            latency_ms
        );
    }

    let bundle = FormulaPredictionBundle {
        schema_version: FORMULA_PREDICTION_SCHEMA_VERSION,
        dataset_id: manifest.dataset_id,
        dataset_version: manifest.dataset_version,
        metadata: FormulaRunMetadata {
            core_commit: git_commit().unwrap_or_else(|| "unknown".to_owned()),
            model_id: args.model_id,
            model_version: args.model_version,
            model_sha256: model_bundle_sha256([
                encoder_path.as_path(),
                decoder_path.as_path(),
                tokenizer_path.as_path(),
            ])?,
            runtime: "onnxruntime".to_owned(),
            runtime_version: "ort-linked-runtime".to_owned(),
            provider: "cpu".to_owned(),
            os: std::env::consts::OS.to_owned(),
            cpu: std::env::var("PROCESSOR_IDENTIFIER")
                .unwrap_or_else(|_| std::env::consts::ARCH.to_owned()),
            gpu: None,
            thread_count: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
            warmup_iterations: args.warmup_iterations,
            seed: manifest.seed,
            timestamp_utc: args.timestamp_utc,
            model_load_ms,
            peak_rss_bytes: None,
        },
        predictions,
    };
    std::fs::write(args.output, serde_json::to_vec_pretty(&bundle)?)?;
    Ok(())
}

fn decode_rgb(path: &Path) -> Result<SnipperImage, Box<dyn std::error::Error>> {
    let image = decode(ImageSource::File(path))?;
    let mut rgb = Vec::with_capacity((image.width() * image.height() * 3) as usize);
    match image.format() {
        PixelFormat::Rgb => return Ok(image),
        PixelFormat::Rgba => {
            for pixel in image.pixels().as_chunks::<4>().0 {
                rgb.extend_from_slice(&pixel[..3]);
            }
        }
        PixelFormat::Bgr => {
            for pixel in image.pixels().as_chunks::<3>().0 {
                rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
        }
        PixelFormat::Bgra => {
            for pixel in image.pixels().as_chunks::<4>().0 {
                rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
        }
        PixelFormat::Gray => {
            for value in image.pixels() {
                rgb.extend_from_slice(&[*value; 3]);
            }
        }
    }
    Ok(SnipperImage::new(
        image.width(),
        image.height(),
        PixelFormat::Rgb,
        rgb,
    ))
}

fn model_bundle_sha256<const N: usize>(
    paths: [&Path; N],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut digest = Sha256::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("model artifact has no UTF-8 file name")?;
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(std::fs::read(path)?);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
