use latexsnipper_model::ModelConfig;
use latexsnipper_runtime::{MemoryModelResolver, ModelId, ModelResolver};
use serde::Serialize;

use crate::error::{WasmError, WasmErrorCode};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileValidation {
    pub profile: String,
    pub ready: bool,
    pub missing: Vec<String>,
    pub variants: Vec<String>,
}

pub fn validate_profile(
    resolver: &MemoryModelResolver,
    profile: &str,
) -> Result<ProfileValidation, WasmError> {
    match profile {
        "formula" | "formula_layout" => {
            validate_components(resolver, profile, &["formula-det", "formula-rec"])
        }
        "text" => validate_components(resolver, profile, &["text-det", "text-rec"]),
        "mixed" => validate_components(
            resolver,
            profile,
            &["formula-det", "formula-rec", "text-det", "text-rec"],
        ),
        "table" => validate_table_profile(resolver, profile),
        "handwriting" => {
            validate_specialized_profile(resolver, profile, &["formula-rec"], &["handwriting-det"])
        }
        _ => Err(WasmError::new(
            WasmErrorCode::UnsupportedMode,
            format!("Unknown recognition mode: {profile}"),
        )),
    }
}

fn validate_table_profile(
    resolver: &MemoryModelResolver,
    profile: &str,
) -> Result<ProfileValidation, WasmError> {
    let mut validation = validate_components(resolver, profile, &["text-rec"])?;
    if let Some(variant) = find_variant(resolver, "table-struct") {
        validation.variants.push(format!("table-struct/{variant}"));
        validate_component(resolver, "table-struct", &variant, &mut validation.missing);
        validate_specialized_metadata(
            resolver,
            profile,
            "table-struct",
            &variant,
            &mut validation.missing,
        );
    } else {
        validation
            .variants
            .push("table-struct/projection".to_string());
    }

    if let Some(variant) = find_variant(resolver, "table-det") {
        validation.variants.push(format!("table-det/{variant}"));
        validate_component(resolver, "table-det", &variant, &mut validation.missing);
    }
    validation.ready = validation.missing.is_empty();
    Ok(validation)
}

fn validate_specialized_profile(
    resolver: &MemoryModelResolver,
    profile: &str,
    required: &[&str],
    optional: &[&str],
) -> Result<ProfileValidation, WasmError> {
    let mut validation = validate_components(resolver, profile, required)?;
    for category in optional {
        let Some(variant) = find_variant(resolver, category) else {
            continue;
        };
        validation.variants.push(format!("{category}/{variant}"));
        validate_component(resolver, category, &variant, &mut validation.missing);
    }

    for selected in validation.variants.clone() {
        let Some((category, variant)) = selected.split_once('/') else {
            continue;
        };
        validate_specialized_metadata(
            resolver,
            profile,
            category,
            variant,
            &mut validation.missing,
        );
    }
    validation.ready = validation.missing.is_empty();
    Ok(validation)
}

fn validate_specialized_metadata(
    resolver: &MemoryModelResolver,
    profile: &str,
    category: &str,
    variant: &str,
    missing: &mut Vec<String>,
) {
    let id = ModelId::new(category, variant);
    let prefix = id.composite_key();
    let Ok(config_text) = resolver.read_text_artifact(&id, "config.json") else {
        return;
    };
    let Ok(config) = ModelConfig::from_json_str(&config_text) else {
        return;
    };

    if config.preprocessing.is_none() {
        missing.push(format!(
            "{prefix}/config.json (preprocessing metadata missing)"
        ));
    }
    if profile == "table" && category == "table-struct" {
        if !matches!(config.model_type.as_str(), "slanet" | "tatr") {
            missing.push(format!("{prefix}/config.json (unsupported table runtime)"));
        }
        if config.input.is_none() || config.output.is_none() {
            missing.push(format!("{prefix}/config.json (table I/O schema missing)"));
        }
    }
    if profile == "handwriting" && category == "formula-rec" {
        if config.encoder.is_none() || config.decoder.is_none() {
            missing.push(format!(
                "{prefix}/config.json (handwriting I/O schema missing)"
            ));
        }
        if config.decoding.is_none() {
            missing.push(format!(
                "{prefix}/config.json (handwriting decoding metadata missing)"
            ));
        }
    }
}

fn validate_components(
    resolver: &MemoryModelResolver,
    profile: &str,
    categories: &[&str],
) -> Result<ProfileValidation, WasmError> {
    let mut missing = Vec::new();
    let mut variants = Vec::new();

    for category in categories {
        let Some(variant) = find_variant(resolver, category) else {
            missing.push(format!("{category}/<variant>/config.json"));
            continue;
        };
        variants.push(format!("{category}/{variant}"));
        validate_component(resolver, category, &variant, &mut missing);
    }

    Ok(ProfileValidation {
        profile: profile.to_string(),
        ready: missing.is_empty(),
        missing,
        variants,
    })
}

fn validate_component(
    resolver: &MemoryModelResolver,
    category: &str,
    variant: &str,
    missing: &mut Vec<String>,
) {
    let id = ModelId::new(category, variant);
    let prefix = id.composite_key();
    let config_text = match resolver.read_text_artifact(&id, "config.json") {
        Ok(text) => text,
        Err(_) => {
            missing.push(format!("{prefix}/config.json"));
            return;
        }
    };
    let config = match ModelConfig::from_json_str(&config_text) {
        Ok(config) => config,
        Err(_) => {
            missing.push(format!("{prefix}/config.json (invalid)"));
            return;
        }
    };

    let files = config
        .pipeline
        .as_ref()
        .and_then(|pipeline| pipeline.model_files.as_ref());
    let primary = files
        .and_then(|value| value.primary.as_deref())
        .unwrap_or("model.onnx");

    if config.encoder.is_some() || config.decoder.is_some() {
        require_artifact(
            resolver,
            &prefix,
            files
                .and_then(|value| value.encoder.as_deref())
                .unwrap_or("encoder_model.onnx"),
            missing,
        );
        require_artifact(
            resolver,
            &prefix,
            files
                .and_then(|value| value.decoder.as_deref())
                .unwrap_or("decoder_model.onnx"),
            missing,
        );
    } else {
        require_artifact(resolver, &prefix, primary, missing);
    }

    if category.ends_with("-rec") {
        let decoding = config.decoding.as_ref();
        let vocabulary = decoding
            .and_then(|value| value.keys_file.as_deref())
            .or_else(|| decoding.and_then(|value| value.tokenizer_file.as_deref()))
            .unwrap_or_else(|| {
                if category == "text-rec" {
                    "inference.yml"
                } else {
                    "tokenizer.json"
                }
            });
        require_artifact(resolver, &prefix, vocabulary, missing);
    }
}

fn require_artifact(
    resolver: &MemoryModelResolver,
    prefix: &str,
    artifact: &str,
    missing: &mut Vec<String>,
) {
    let key = format!("{prefix}/{artifact}");
    if !resolver.has(&key) {
        missing.push(key);
    }
}

fn find_variant(resolver: &MemoryModelResolver, category: &str) -> Option<String> {
    let prefix = format!("{category}/");
    let mut variants: Vec<_> = resolver
        .list()
        .into_iter()
        .filter_map(|key| {
            key.strip_prefix(&prefix)
                .and_then(|rest| rest.split('/').next())
                .filter(|variant| !variant.is_empty())
                .map(str::to_string)
        })
        .collect();
    variants.sort();
    variants.dedup();
    variants.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_file_config(primary: &str, vocabulary: Option<(&str, &str)>) -> Vec<u8> {
        let mut config = serde_json::json!({
            "model_type": "test",
            "pipeline": { "model_files": { "primary": primary } }
        });
        if let Some((key, value)) = vocabulary {
            config["decoding"] = serde_json::json!({ key: value });
        }
        serde_json::to_vec(&config).unwrap()
    }

    #[test]
    fn readiness_follows_loaded_artifacts() {
        let resolver = MemoryModelResolver::new();
        resolver.store(
            "text-det/tiny/config.json",
            single_file_config("det.onnx", None),
        );
        resolver.store("text-det/tiny/det.onnx", vec![1]);
        resolver.store(
            "text-rec/tiny/config.json",
            single_file_config("rec.onnx", Some(("keys_file", "keys.txt"))),
        );
        resolver.store("text-rec/tiny/rec.onnx", vec![2]);

        let incomplete = validate_profile(&resolver, "text").unwrap();
        assert!(!incomplete.ready);
        assert_eq!(incomplete.missing, vec!["text-rec/tiny/keys.txt"]);

        resolver.store("text-rec/tiny/keys.txt", b"a\nb".to_vec());
        assert!(validate_profile(&resolver, "text").unwrap().ready);
    }

    #[test]
    fn specialized_pipelines_require_complete_artifacts() {
        let resolver = MemoryModelResolver::new();
        assert!(!validate_profile(&resolver, "table").unwrap().ready);
        assert!(!validate_profile(&resolver, "handwriting").unwrap().ready);

        resolver.store(
            "table-struct/browser/config.json",
            serde_json::to_vec(&serde_json::json!({
                "model_type": "slanet",
                "input": { "name": "x", "shape": [1, 3, 488, 488], "dtype": "float32" },
                "output": { "name": "structure_probs", "shape": [1, -1, 50] },
                "preprocessing": { "resize": { "width": 488, "height": 488 } },
                "pipeline": { "model_files": { "primary": "model.onnx" } }
            }))
            .unwrap(),
        );
        resolver.store("table-struct/browser/model.onnx", vec![1]);
        resolver.store(
            "text-rec/browser/config.json",
            serde_json::to_vec(&serde_json::json!({
                "model_type": "crnn_ctc",
                "input": { "name": "x", "shape": [1, 3, 48, 320], "dtype": "float32" },
                "output": { "name": "softmax", "shape": [1, -1, 10] },
                "preprocessing": { "resize": { "height": 48 } },
                "decoding": { "keys_file": "keys.txt" },
                "pipeline": { "model_files": { "primary": "model.onnx" } }
            }))
            .unwrap(),
        );
        resolver.store("text-rec/browser/model.onnx", vec![2]);
        resolver.store("text-rec/browser/keys.txt", b"a\nb".to_vec());
        assert!(validate_profile(&resolver, "table").unwrap().ready);

        resolver.store(
            "formula-rec/browser/config.json",
            serde_json::to_vec(&serde_json::json!({
                "model_type": "trocr",
                "encoder": {
                    "input": { "name": "pixel_values", "shape": [1, 3, 384, 384], "dtype": "float32" },
                    "output": { "name": "last_hidden_state", "shape": [1, 577, 384] }
                },
                "decoder": {
                    "input_ids": { "name": "input_ids" },
                    "encoder_hidden": { "name": "encoder_hidden_states" },
                    "output": { "name": "logits", "shape": [1, -1, 10] }
                },
                "preprocessing": { "resize": { "width": 384, "height": 384 } },
                "decoding": { "tokenizer_file": "tokenizer.json" },
                "pipeline": { "model_files": {
                    "encoder": "encoder.onnx",
                    "decoder": "decoder.onnx",
                    "tokenizer": "tokenizer.json"
                } }
            }))
            .unwrap(),
        );
        resolver.store("formula-rec/browser/encoder.onnx", vec![3]);
        resolver.store("formula-rec/browser/decoder.onnx", vec![4]);
        resolver.store("formula-rec/browser/tokenizer.json", b"{}".to_vec());
        assert!(validate_profile(&resolver, "handwriting").unwrap().ready);
    }

    #[test]
    fn table_profile_uses_builtin_projection_without_a_structure_model() {
        let resolver = MemoryModelResolver::new();
        resolver.store(
            "text-rec/browser/config.json",
            serde_json::to_vec(&serde_json::json!({
                "model_type": "crnn_ctc",
                "input": { "name": "x", "shape": [1, 3, 48, 320], "dtype": "float32" },
                "output": { "name": "softmax", "shape": [1, -1, 10] },
                "preprocessing": { "resize": { "height": 48 } },
                "decoding": { "keys_file": "keys.txt" },
                "pipeline": { "model_files": { "primary": "model.onnx" } }
            }))
            .unwrap(),
        );
        resolver.store("text-rec/browser/model.onnx", vec![2]);
        resolver.store("text-rec/browser/keys.txt", b"a\nb".to_vec());

        let validation = validate_profile(&resolver, "table").unwrap();
        assert!(validation.ready);
        assert!(validation
            .variants
            .contains(&"table-struct/projection".to_string()));
    }
}
