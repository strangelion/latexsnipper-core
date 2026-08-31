//! Development runner used by the official Python/Rust parity gate.

use std::path::{Path, PathBuf};
use std::time::Instant;

use latexsnipper_runtime::{
    RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind, RuntimeOptions, TensorMap,
};
use latexsnipper_runtime_paddle::PaddleInferenceFactory;
use latexsnipper_tensor::{Tensor, TensorData};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CaseInput {
    id: String,
    tensor: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseOutput {
    id: String,
    token_ids: Vec<i64>,
    eos_position: Option<usize>,
    decoded_latex: String,
    elapsed_ms: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() != 5 {
        return Err(
            "usage: ppfn_tokens <runtime-home> <model-home> <case-manifest.json> <output.json>"
                .into(),
        );
    }
    let runtime_home = PathBuf::from(&arguments[1]);
    let model_home = PathBuf::from(&arguments[2]);
    let case_manifest = PathBuf::from(&arguments[3]);
    let output_path = PathBuf::from(&arguments[4]);

    let cases: Vec<CaseInput> = serde_json::from_slice(&std::fs::read(case_manifest)?)?;
    let tokenizer = tokenizers::Tokenizer::from_file(model_home.join("tokenizer.json"))
        .map_err(|error| format!("failed to load tokenizer: {error}"))?;
    let factory = PaddleInferenceFactory::with_library_path(runtime_home);
    let artifacts = RuntimeArtifacts::new(RuntimeKind::PaddleInference)
        .with_file("model", model_home.join("inference.json"))
        .with_file("params", model_home.join("inference.pdiparams"));
    let session = factory.create_session(&artifacts, &RuntimeOptions::default())?;
    let input_name = session
        .metadata()
        .inputs
        .first()
        .ok_or("Paddle graph has no input")?
        .name
        .clone();

    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        let values = read_f32_tensor(&case.tensor)?;
        if values.len() != 384 * 384 {
            return Err(format!(
                "{} contains {} f32 values; expected {}",
                case.tensor.display(),
                values.len(),
                384 * 384
            )
            .into());
        }
        let mut inputs = TensorMap::new();
        inputs.insert(
            input_name.clone(),
            Tensor::float32(&input_name, vec![1, 1, 384, 384], values),
        );
        let started = Instant::now();
        let response = session.run(RunRequest::new(inputs))?;
        let output = response
            .first_output()
            .ok_or("Paddle graph has no output")?;
        let token_ids = match output.data() {
            TensorData::Int64(values) => values.clone(),
            TensorData::Int32(values) => values.iter().copied().map(i64::from).collect(),
            other => return Err(format!("Paddle output is not integer tokens: {other:?}").into()),
        };
        let eos_position = token_ids.iter().position(|token| *token == 2);
        let decode_end = eos_position.map_or(token_ids.len(), |position| position + 1);
        let decode_ids = token_ids[..decode_end]
            .iter()
            .map(|token| u32::try_from(*token))
            .collect::<Result<Vec<_>, _>>()?;
        let decoded_latex = tokenizer
            .decode(&decode_ids, true)
            .map_err(|error| format!("tokenizer decode failed: {error}"))?;
        results.push(CaseOutput {
            id: case.id,
            token_ids,
            eos_position,
            decoded_latex,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        });
    }

    std::fs::write(output_path, serde_json::to_vec_pretty(&results)?)?;
    Ok(())
}

fn read_f32_tensor(path: &Path) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() % size_of::<f32>() != 0 {
        return Err(format!("{} has a partial f32 value", path.display()).into());
    }
    Ok(bytes
        .as_chunks::<{ size_of::<f32>() }>()
        .0
        .iter()
        .copied()
        .map(f32::from_le_bytes)
        .collect())
}
