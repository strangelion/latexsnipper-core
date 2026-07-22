use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use latexsnipper_image::ImageSource;
use latexsnipper_inference::{FormulaBackend, PPFormulaNetAdapter};
use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{RuntimeRegistry, RuntimeResolver};
use latexsnipper_runtime_paddle::PaddleInferenceFactory;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImageCase {
    id: String,
    image: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageResult {
    id: String,
    decoded_latex: String,
    elapsed_ms: f64,
    variant_id: String,
    official_full_graph: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().collect();
    if arguments.len() != 5 {
        return Err(format!(
            "usage: {} <runtime-home> <model-home> <cases.json> <output.json>",
            arguments.first().map_or("ppfn_images", String::as_str)
        )
        .into());
    }

    let runtime_home = Path::new(&arguments[1]);
    let model_home = Path::new(&arguments[2]);
    let cases: Vec<ImageCase> = serde_json::from_slice(&fs::read(&arguments[3])?)?;
    let config = ModelConfig::load(model_home)?;

    let registry = RuntimeRegistry::with_factory(PaddleInferenceFactory::with_library_path(
        runtime_home.to_path_buf(),
    ));
    let resolved = RuntimeResolver::new(&registry).resolve(
        "formula-rec/pp-formulanet-s",
        &config.runtime_variants,
        model_home,
        Some("paddle-native"),
    )?;
    let adapter =
        PPFormulaNetAdapter::from_resolved_variant(&registry, &resolved, model_home, &config)?;

    let mut results = Vec::with_capacity(cases.len());
    for case in cases {
        let image = latexsnipper_image::decode::decode(ImageSource::File(&case.image))?;
        let started = Instant::now();
        let recognition = adapter.recognize(&image)?;
        results.push(ImageResult {
            id: case.id,
            decoded_latex: recognition.text,
            elapsed_ms: started.elapsed().as_secs_f64() * 1_000.0,
            variant_id: adapter.variant_id().to_owned(),
            official_full_graph: adapter.uses_official_full_graph(),
        });
    }

    fs::write(&arguments[4], serde_json::to_vec_pretty(&results)?)?;
    Ok(())
}
