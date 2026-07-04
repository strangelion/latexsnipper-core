//! Example: Loading a custom CTC OCR model with ModelRegistry.
//!
//! This example demonstrates how to:
//! 1. Create a ModelRegistry from a directory
//! 2. Find models by task
//! 3. Load model manifests
//! 4. Create model executors

use latexsnipper_runtime::{ModelRegistry, ModelTask};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example 1: Create registry from a directory
    let models_dir = std::env::var("MODELS_DIR").unwrap_or_else(|_| "models".into());

    println!("Loading models from: {}", models_dir);

    let registry = match ModelRegistry::from_dir(&models_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Warning: Could not load models directory: {}", e);
            eprintln!("Using empty registry for demonstration.");
            ModelRegistry::new()
        }
    };

    // List all available models
    let ids = registry.list_ids();
    println!("Available models: {:?}", ids);

    // Example 2: Find models by task
    let text_rec_models = registry.find_by_task(ModelTask::TextRecognition);
    println!("\nText recognition models:");
    for (manifest, dir) in &text_rec_models {
        println!(
            "  - {} v{} ({})",
            manifest.id,
            manifest.version,
            dir.display()
        );
        println!("    Adapter: {}", manifest.adapter);
        println!("    Input: {:?}", manifest.input.shape);
    }

    // Example 3: Get a specific model
    if let Some(manifest) = registry.get("custom/ctc-ocr") {
        println!("\nFound custom CTC model:");
        println!("  Task: {:?}", manifest.task);
        println!("  Version: {}", manifest.version);
        println!("  Adapter: {}", manifest.adapter);
        println!("  Input shape: {:?}", manifest.input.shape);
        println!("  Decoding: {:?}", manifest.decoding);
    }

    // Example 4: Check model availability
    let has_formula_det = registry.has("formula-det/yolov8-mfd");
    println!("\nFormula detection model available: {}", has_formula_det);

    Ok(())
}
