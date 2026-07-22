use std::env;
use std::path::PathBuf;
use std::time::Instant;

use latexsnipper_runtime::providers::onnx_factory::OnnxRuntimeFactory;
use latexsnipper_runtime::{
    ExecutionProviderSpec, RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind,
    RuntimeOptions, TensorMap,
};
use latexsnipper_tensor::Tensor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 || args.len() > 7 {
        return Err(format!(
            "usage: {} MODEL PROVIDERS_JSON INPUT_NAME SHAPE [WARMUP] [ITERATIONS]",
            args.first()
                .map(String::as_str)
                .unwrap_or("onnx_provider_benchmark")
        )
        .into());
    }

    let model = PathBuf::from(&args[1]);
    let providers: Vec<ExecutionProviderSpec> = serde_json::from_str(&args[2])?;
    let input_name = args[3].clone();
    let shape = parse_shape(&args[4])?;
    let warmup = parse_count(args.get(5), 5, "warmup")?;
    let iterations = parse_count(args.get(6), 50, "iterations")?;
    let element_count = shape.iter().try_fold(1usize, |count, dimension| {
        count
            .checked_mul(*dimension)
            .ok_or("input shape element count overflow")
    })?;
    let input = Tensor::float32(input_name.clone(), shape, vec![0.25; element_count]);

    let factory = OnnxRuntimeFactory::new(
        model
            .parent()
            .ok_or("model path must have a parent directory")?
            .to_path_buf(),
    );
    let artifacts =
        RuntimeArtifacts::new(RuntimeKind::OnnxRuntime).with_file("model", model.clone());
    let options = RuntimeOptions {
        providers: providers.clone(),
        ..RuntimeOptions::default()
    };
    let session = factory.create_session(&artifacts, &options)?;

    for _ in 0..warmup {
        run_once(session.as_ref(), &input_name, &input)?;
    }
    let mut samples = Vec::with_capacity(iterations);
    let mut output_names = Vec::new();
    for _ in 0..iterations {
        let started = Instant::now();
        let response = run_once(session.as_ref(), &input_name, &input)?;
        samples.push(started.elapsed().as_secs_f64() * 1_000.0);
        output_names = response.outputs.keys().cloned().collect();
    }
    samples.sort_by(f64::total_cmp);
    let mean_ms = samples.iter().sum::<f64>() / samples.len() as f64;
    let p50_ms = percentile(&samples, 0.50);
    let p95_ms = percentile(&samples, 0.95);
    let provider_names: Vec<_> = providers
        .into_iter()
        .map(|provider| provider.name)
        .collect();

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "model": model,
            "providers": provider_names,
            "warmup": warmup,
            "iterations": iterations,
            "meanMs": mean_ms,
            "p50Ms": p50_ms,
            "p95Ms": p95_ms,
            "outputs": output_names,
        }))?
    );
    Ok(())
}

fn run_once(
    session: &dyn latexsnipper_runtime::RuntimeSession,
    input_name: &str,
    input: &Tensor,
) -> latexsnipper_foundation::Result<latexsnipper_runtime::RunResponse> {
    session.run(RunRequest::new(TensorMap::from([(
        input_name.to_owned(),
        input.clone(),
    )])))
}

fn parse_shape(value: &str) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
    let shape: Vec<usize> = value
        .split(['x', ','])
        .map(str::trim)
        .filter(|dimension| !dimension.is_empty())
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    if shape.is_empty() || shape.contains(&0) {
        return Err("shape must contain only positive dimensions".into());
    }
    Ok(shape)
}

fn parse_count(
    value: Option<&String>,
    default: usize,
    name: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let value = value.map_or(Ok(default), |value| value.parse())?;
    if value == 0 {
        return Err(format!("{name} must be greater than zero").into());
    }
    Ok(value)
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * fraction).round() as usize;
    sorted[index]
}
