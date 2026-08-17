use latexsnipper_model::RuntimeVariant;
use latexsnipper_runtime::{
    create_lsmodel_archive_with_manifest, inspect_lsmodel_archive, ManifestDecoding,
    ManifestPreprocessing, ManifestResize, ManifestTensorSpec, ModelFiles, ModelManifest,
    ModelTask,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

pub struct LegacyPackageRequest<'a> {
    pub source: &'a Path,
    pub output: &'a Path,
    pub catalog: &'a Path,
    pub category: &'a str,
    pub variant: &'a str,
    pub version: &'a str,
}

pub fn package_legacy_model(request: LegacyPackageRequest<'_>) -> Result<ModelManifest, String> {
    let catalog: Value = read_json(request.catalog)?;
    let variant = catalog
        .pointer(&format!("/categories/{}/variants", request.category))
        .and_then(Value::as_array)
        .and_then(|variants| {
            variants
                .iter()
                .find(|value| value.get("id").and_then(Value::as_str) == Some(request.variant))
        })
        .ok_or_else(|| {
            format!(
                "model variant {}/{} is not present in {}",
                request.category,
                request.variant,
                request.catalog.display()
            )
        })?;
    let config_path = request.source.join("config.json");
    let config: Value = read_json(&config_path)?;
    let runtime_variants: Vec<RuntimeVariant> = serde_json::from_value(
        variant
            .get("runtimeVariants")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    )
    .map_err(|error| format!("invalid runtimeVariants: {error}"))?;
    let declared_files = variant
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| "catalog variant has no files array".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| "catalog files must contain strings".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let input = find_tensor(&config, &["/input", "/encoder/input"])
        .ok_or_else(|| "config.json has no usable input or encoder.input tensor".to_owned())?;
    let output = find_outputs(&config)?;
    let files = manifest_files(&runtime_variants, &declared_files);
    let checksums = checksums(request.source, &declared_files)?;
    let manifest = ModelManifest {
        id: format!("{}/{}", request.category, request.variant),
        task: task_for_category(request.category)?,
        version: request.version.to_owned(),
        adapter: required_string(variant, "adapter")?,
        input,
        output,
        files,
        preprocessing: preprocessing(&config),
        decoding: decoding(&config),
        checksums,
        runtime_variants,
    };
    create_lsmodel_archive_with_manifest(request.source, request.output, &manifest)
        .map_err(|error| error.to_string())?;
    let inspection = inspect_lsmodel_archive(
        std::fs::File::open(request.output)
            .map_err(|error| format!("failed to reopen {}: {error}", request.output.display()))?,
    )
    .map_err(|error| error.to_string())?;
    if inspection.manifest.id != manifest.id {
        return Err("created archive manifest id does not match the requested model".to_owned());
    }
    Ok(manifest)
}

pub fn inspect_archive(
    path: &Path,
) -> Result<latexsnipper_runtime::LsModelArchiveInspection, String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    inspect_lsmodel_archive(file).map_err(|error| error.to_string())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&source).map_err(|error| format!("invalid {}: {error}", path.display()))
}

fn task_for_category(category: &str) -> Result<ModelTask, String> {
    match category {
        "formula-det" => Ok(ModelTask::FormulaDetection),
        "formula-rec" => Ok(ModelTask::FormulaRecognition),
        "text-det" => Ok(ModelTask::TextDetection),
        "text-rec" => Ok(ModelTask::TextRecognition),
        "table-det" => Ok(ModelTask::TableDetection),
        "table-struct" => Ok(ModelTask::TableStructure),
        "layout" => Ok(ModelTask::LayoutAnalysis),
        other => Err(format!("unsupported model category: {other}")),
    }
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("catalog variant has no {key}"))
}

fn tensor(value: &Value) -> Option<ManifestTensorSpec> {
    let name = value.get("name")?.as_str()?.to_owned();
    let shape = value
        .get("shape")?
        .as_array()?
        .iter()
        .map(Value::as_i64)
        .collect::<Option<Vec<_>>>()?;
    let dtype = value
        .get("dtype")
        .and_then(Value::as_str)
        .unwrap_or("float32")
        .to_owned();
    Some(ManifestTensorSpec { name, shape, dtype })
}

fn find_tensor(config: &Value, pointers: &[&str]) -> Option<ManifestTensorSpec> {
    pointers
        .iter()
        .find_map(|pointer| config.pointer(pointer).and_then(tensor))
}

fn find_outputs(config: &Value) -> Result<Vec<ManifestTensorSpec>, String> {
    for pointer in ["/outputs", "/output"] {
        if let Some(value) = config.pointer(pointer) {
            if let Some(values) = value.as_array() {
                let tensors = values.iter().filter_map(tensor).collect::<Vec<_>>();
                if !tensors.is_empty() {
                    return Ok(tensors);
                }
            } else if let Some(tensor) = tensor(value) {
                return Ok(vec![tensor]);
            }
        }
    }
    find_tensor(config, &["/decoder/output", "/encoder/output"])
        .map(|tensor| vec![tensor])
        .ok_or_else(|| "config.json has no usable output tensor".to_owned())
}

fn manifest_files(runtime_variants: &[RuntimeVariant], declared: &[String]) -> ModelFiles {
    let artifacts = runtime_variants.first().map(|variant| &variant.artifacts);
    let artifact = |names: &[&str]| {
        artifacts.and_then(|values| {
            names
                .iter()
                .find_map(|name| values.get(*name).map(String::to_owned))
        })
    };
    let declared_named = |names: &[&str]| {
        declared
            .iter()
            .find(|path| names.contains(&path.as_str()))
            .cloned()
    };
    ModelFiles {
        primary: artifact(&["model", "primary"]),
        encoder: artifact(&["encoder"]),
        decoder: artifact(&["decoder"]),
        tokenizer: artifact(&["tokenizer"])
            .or_else(|| declared_named(&["tokenizer.json", "inference.yml"])),
        config: artifact(&["config"]).or_else(|| declared_named(&["config.json"])),
    }
}

fn checksums(source: &Path, declared: &[String]) -> Result<HashMap<String, String>, String> {
    let mut paths = BTreeSet::new();
    paths.extend(declared.iter().cloned());
    let mut result = HashMap::new();
    for relative in paths {
        let path = source.join(&relative);
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("failed to read declared file {}: {error}", path.display()))?;
        result.insert(relative, format!("{:x}", Sha256::digest(bytes)));
    }
    Ok(result)
}

fn preprocessing(config: &Value) -> Option<ManifestPreprocessing> {
    let value = config.get("preprocessing")?;
    let resize = value.get("resize").map(|resize| ManifestResize {
        width: resize
            .get("width")
            .and_then(Value::as_u64)
            .map(|v| v as u32),
        height: resize
            .get("height")
            .and_then(Value::as_u64)
            .map(|v| v as u32),
        keep_ratio: resize.get("keep_ratio").and_then(Value::as_bool),
    });
    let floats = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_f64)
                    .map(|number| number as f32)
                    .collect::<Vec<_>>()
            })
    };
    Some(ManifestPreprocessing {
        resize,
        mean: floats("/normalization/mean"),
        std: floats("/normalization/std"),
        color_format: value
            .get("color_format")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn decoding(config: &Value) -> Option<ManifestDecoding> {
    let value = config.get("decoding")?;
    Some(ManifestDecoding {
        decoding_type: value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("model-default")
            .to_owned(),
        beam_width: value
            .get("beam_width")
            .and_then(Value::as_u64)
            .map(|v| v as usize),
        blank_id: value
            .get("blank_id")
            .and_then(Value::as_u64)
            .map(|v| v as usize),
        output_layout: value
            .get("output_layout")
            .and_then(Value::as_str)
            .map(str::to_owned),
        logits_kind: value
            .get("logits_kind")
            .and_then(Value::as_str)
            .map(str::to_owned),
        temperature: value
            .get("temperature")
            .and_then(Value::as_f64)
            .map(|v| v as f32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_arrays_and_encoder_decoder_configs_are_supported() {
        let array = serde_json::json!({
            "output": [
                {"name": "scores", "shape": [1, 10]},
                {"name": "boxes", "shape": [1, 4]}
            ]
        });
        assert_eq!(find_outputs(&array).unwrap().len(), 2);

        let encoder_decoder = serde_json::json!({
            "encoder": {"input": {"name": "pixels", "shape": [1, 3, 32, 32], "dtype": "float32"}},
            "decoder": {"output": {"name": "logits", "shape": [1, -1, 20]}}
        });
        assert_eq!(
            find_tensor(&encoder_decoder, &["/input", "/encoder/input"])
                .unwrap()
                .name,
            "pixels"
        );
        assert_eq!(find_outputs(&encoder_decoder).unwrap()[0].name, "logits");
    }
}
