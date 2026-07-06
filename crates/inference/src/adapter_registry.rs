//! Built-in adapter registration.
//!
//! This module provides functions to register all built-in model adapters
//! with a `ModelRegistry`. This enables automatic package creation from
//! manifests that declare `adapter = "adapter-name-v1"`.

use latexsnipper_runtime::{ModelId, ModelManifest, ModelRegistry};

use crate::adapters::{
    CrnnTextRecognizerPackage, DbNetTextDetectorPackage, TrOcrFormulaPackage, YoloV8DetectorPackage,
};

/// Convert a ModelManifest to a ModelConfig.
///
/// This allows adapters to use manifest parameters directly when
/// config.json doesn't exist, enabling "config-only" model packages.
fn manifest_to_config(manifest: &ModelManifest) -> latexsnipper_model::ModelConfig {
    let input = Some(latexsnipper_model::InputConfig {
        name: manifest.input.name.clone(),
        shape: manifest.input.shape.clone(),
        dtype: manifest.input.dtype.clone(),
        range: None,
    });

    let output = manifest
        .output
        .first()
        .map(|o| latexsnipper_model::OutputConfig {
            name: o.name.clone(),
            shape: o.shape.clone(),
            description: None,
        });

    let preprocessing =
        manifest
            .preprocessing
            .as_ref()
            .map(|p| latexsnipper_model::PreprocessConfig {
                resize: p.resize.as_ref().map(|r| latexsnipper_model::ResizeConfig {
                    width: r.width,
                    height: r.height,
                    keep_ratio: r.keep_ratio,
                    pad_value: None,
                }),
                normalization: Some(latexsnipper_model::NormalizationConfig {
                    mean: p.mean.clone(),
                    std: p.std.clone(),
                }),
                color_format: p.color_format.clone(),
                divisible_by: None,
                pad_value: None,
            });

    let decoding = manifest
        .decoding
        .as_ref()
        .map(|d| latexsnipper_model::DecodingConfig {
            decoding_type: Some(d.decoding_type.clone()),
            beam_width: d.beam_width,
            blank_id: d.blank_id,
            output_layout: d.output_layout.as_ref().and_then(|s| {
                match s.to_lowercase().as_str() {
                    "ntc" => Some(latexsnipper_model::CtcOutputLayout::Ntc),
                    "tnc" => Some(latexsnipper_model::CtcOutputLayout::Tnc),
                    _ => None,
                }
            }),
            logits_kind: d.logits_kind.as_ref().and_then(|s| {
                match s.to_lowercase().as_str() {
                    "logits" => Some(latexsnipper_model::LogitsKind::Logits),
                    "probabilities" => Some(latexsnipper_model::LogitsKind::Probabilities),
                    "log_probabilities" => Some(latexsnipper_model::LogitsKind::LogProbabilities),
                    _ => None,
                }
            }),
            temperature: d.temperature,
            top_k: None,
            tokenizer_file: None,
            keys_file: None,
            top_p: None,
        });

    let model_type = match manifest.task {
        latexsnipper_runtime::ModelTask::FormulaDetection => "yolov8",
        latexsnipper_runtime::ModelTask::FormulaRecognition => "trocr",
        latexsnipper_runtime::ModelTask::TextDetection => "dbnet",
        latexsnipper_runtime::ModelTask::TextRecognition => "crnn_ctc",
        latexsnipper_runtime::ModelTask::TableDetection => "picodet_layout",
        latexsnipper_runtime::ModelTask::TableStructure => "slanet",
        latexsnipper_runtime::ModelTask::LayoutAnalysis => "picodet_layout",
        latexsnipper_runtime::ModelTask::HandwritingRecognition => "trocr",
    };

    latexsnipper_model::ModelConfig {
        model_type: model_type.to_string(),
        model_family: Some(format!("{} v{}", manifest.adapter, manifest.version)),
        license: None,
        task_type: None,
        num_classes: None,
        dynamic_shapes: None,
        input,
        output,
        encoder: None,
        decoder: None,
        preprocessing,
        postprocessing: None,
        decoding,
        quantization: None,
        outputs: None,
        extra: None,
        pipeline: None,
    }
}

/// Register all built-in adapters with the registry.
///
/// After calling this, the registry can create packages from manifests
/// that declare:
/// - `adapter = "yolov8-detection-v1"` → `YoloV8DetectorPackage`
/// - `adapter = "dbnet-detection-v1"` → `DbNetTextDetectorPackage`
/// - `adapter = "trocr-recognition-v1"` → `TrOcrFormulaPackage`
/// - `adapter = "ctc-recognition-v1"` → `CrnnTextRecognizerPackage`
///
/// Adapters will first try to load config.json from the model directory.
/// If config.json doesn't exist, they fall back to manifest parameters.
pub fn register_builtin_adapters(registry: &mut ModelRegistry) {
    // YOLOv8 Formula Detection
    registry.register_adapter("yolov8-detection-v1", |manifest, model_dir| {
        let model_id = ModelId::from_composite_key(&manifest.id);
        let config_exists = model_dir.join("config.json").exists();

        // Try config.json first, fall back to manifest
        let config = if config_exists {
            latexsnipper_model::ModelConfig::load(model_dir).ok()
        } else {
            Some(manifest_to_config(manifest))
        };

        let package = if let Some(config) = config {
            YoloV8DetectorPackage::from_config(&config, model_id)
        } else {
            let params = crate::formula_detector::DetectionParams::default();
            YoloV8DetectorPackage::with_params(params, model_id)
        };

        // Set model path from manifest
        let package = if let Some(primary) = &manifest.files.primary {
            package.with_model_path(model_dir.join(primary))
        } else {
            package
        };

        Ok(Box::new(package))
    });

    // TrOCR Formula Recognition
    registry.register_adapter("trocr-recognition-v1", |manifest, model_dir| {
        let model_id = ModelId::from_composite_key(&manifest.id);
        let config_exists = model_dir.join("config.json").exists();

        // Try config.json first, fall back to manifest
        let config = if config_exists {
            latexsnipper_model::ModelConfig::load(model_dir).ok()
        } else {
            Some(manifest_to_config(manifest))
        };

        let package = if let Some(config) = config {
            TrOcrFormulaPackage::from_config(&config, model_id)
        } else {
            TrOcrFormulaPackage::from_config(&latexsnipper_model::ModelConfig::minimal(), model_id)
        };

        // Set paths from manifest
        let package = if let (Some(enc), Some(dec), Some(tok)) = (
            &manifest.files.encoder,
            &manifest.files.decoder,
            &manifest.files.tokenizer,
        ) {
            package.with_paths(
                model_dir.join(enc),
                model_dir.join(dec),
                model_dir.join(tok),
            )
        } else {
            package
        };

        Ok(Box::new(package))
    });

    // DBNet CTC Text Detection
    registry.register_adapter("dbnet-detection-v1", |manifest, model_dir| {
        let model_id = ModelId::from_composite_key(&manifest.id);
        let config_exists = model_dir.join("config.json").exists();

        // Try config.json first, fall back to manifest
        let config = if config_exists {
            latexsnipper_model::ModelConfig::load(model_dir).ok()
        } else {
            Some(manifest_to_config(manifest))
        };

        let package = if let Some(config) = config {
            DbNetTextDetectorPackage::from_config(&config, model_id)
        } else {
            DbNetTextDetectorPackage::from_config(
                &latexsnipper_model::ModelConfig::minimal(),
                model_id,
            )
        };

        // Set model path from manifest
        let package = if let Some(primary) = &manifest.files.primary {
            package.with_model_path(model_dir.join(primary))
        } else {
            package
        };

        Ok(Box::new(package))
    });

    // CRNN CTC Text Recognition
    registry.register_adapter("ctc-recognition-v1", |manifest, model_dir| {
        let model_id = ModelId::from_composite_key(&manifest.id);
        let config_exists = model_dir.join("config.json").exists();

        // Try config.json first, fall back to manifest
        let config = if config_exists {
            latexsnipper_model::ModelConfig::load(model_dir).ok()
        } else {
            Some(manifest_to_config(manifest))
        };

        let package = if let Some(config) = config {
            CrnnTextRecognizerPackage::from_config(&config, model_id)
        } else {
            CrnnTextRecognizerPackage::from_config(
                &latexsnipper_model::ModelConfig::minimal(),
                model_id,
            )
        };

        // Set paths from manifest
        let package = if let (Some(model), Some(keys)) =
            (&manifest.files.primary, &manifest.files.tokenizer)
        {
            package.with_paths(model_dir.join(model), model_dir.join(keys))
        } else {
            package
        };

        Ok(Box::new(package))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use latexsnipper_runtime::{ManifestTensorSpec, ModelTask};

    #[test]
    fn test_register_builtin_adapters() {
        let mut registry = ModelRegistry::new();
        register_builtin_adapters(&mut registry);

        let adapters = registry.registered_adapters();
        assert!(adapters.contains(&"yolov8-detection-v1"));
        assert!(adapters.contains(&"dbnet-detection-v1"));
        assert!(adapters.contains(&"trocr-recognition-v1"));
        assert!(adapters.contains(&"ctc-recognition-v1"));
    }

    #[test]
    fn test_manifest_to_config() {
        let manifest = ModelManifest {
            id: "formula-det/yolov8".to_string(),
            task: ModelTask::FormulaDetection,
            version: "1.0".to_string(),
            adapter: "yolov8-detection-v1".to_string(),
            input: ManifestTensorSpec {
                name: "images".to_string(),
                shape: vec![1, 3, 640, 640],
                dtype: "float32".to_string(),
            },
            output: vec![ManifestTensorSpec {
                name: "output".to_string(),
                shape: vec![1, 6, 8400],
                dtype: "float32".to_string(),
            }],
            files: Default::default(),
            preprocessing: Some(latexsnipper_runtime::ManifestPreprocessing {
                resize: Some(latexsnipper_runtime::ManifestResize {
                    width: Some(640),
                    height: Some(640),
                    keep_ratio: Some(true),
                }),
                mean: Some(vec![0.0, 0.0, 0.0]),
                std: Some(vec![1.0, 1.0, 1.0]),
                color_format: Some("RGB".to_string()),
            }),
            decoding: None,
            checksums: Default::default(),
        };

        let config = manifest_to_config(&manifest);
        assert_eq!(config.model_type, "yolov8");
        assert!(config.input.is_some());
        assert!(config.preprocessing.is_some());

        let preprocessing = config.preprocessing.unwrap();
        assert!(preprocessing.resize.is_some());
        assert!(preprocessing.normalization.is_some());
    }
}
