//! Versioned provider tensor-smoke fixtures.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_tensor::Tensor;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{RunRequest, RuntimeSession, TensorMap};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderSmokeFixture {
    pub schema_version: u32,
    pub model: SmokeModel,
    pub inputs: Vec<SmokeInput>,
    pub input_sha256: String,
    pub expected_outputs: Vec<SmokeOutput>,
    #[serde(default)]
    pub expected_output_sha256: Option<String>,
    #[serde(default)]
    pub tolerance: Option<f32>,
    #[serde(skip)]
    fixture_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SmokeModel {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SmokeInput {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<usize>,
    pub generator: SmokeInputGenerator,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum SmokeInputGenerator {
    ModuloRamp { modulus: u32, divisor: f32 },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SmokeOutput {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<usize>,
}

#[derive(Debug)]
pub struct ProviderSmokeOutcome {
    pub output_sha256: String,
    pub inference_duration: Duration,
}

impl ProviderSmokeFixture {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).map_err(|error| {
            SnipperError::Runtime(format!(
                "failed to read provider smoke fixture '{}': {error}",
                path.display()
            ))
        })?;
        let mut fixture: Self = serde_json::from_slice(&bytes).map_err(|error| {
            SnipperError::Runtime(format!(
                "invalid provider smoke fixture '{}': {error}",
                path.display()
            ))
        })?;
        if fixture.schema_version != 1 {
            return Err(SnipperError::Runtime(format!(
                "unsupported provider smoke fixture schema {}",
                fixture.schema_version
            )));
        }
        fixture.fixture_dir = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        fixture.validate()?;
        Ok(fixture)
    }

    pub fn model_path(&self) -> PathBuf {
        self.fixture_dir.join(&self.model.path)
    }

    pub fn model_sha256(&self) -> &str {
        &self.model.sha256
    }

    pub fn input_tensors(&self) -> Result<TensorMap> {
        let mut tensors = TensorMap::new();
        for input in &self.inputs {
            if input.dtype != "f32" {
                return Err(SnipperError::Runtime(format!(
                    "provider smoke input '{}' uses unsupported dtype '{}'",
                    input.name, input.dtype
                )));
            }
            let length = input.shape.iter().try_fold(1usize, |size, dimension| {
                size.checked_mul(*dimension).ok_or_else(|| {
                    SnipperError::Runtime(format!(
                        "provider smoke input '{}' shape overflows",
                        input.name
                    ))
                })
            })?;
            let values = match input.generator {
                SmokeInputGenerator::ModuloRamp { modulus, divisor } => {
                    if modulus == 0 || !divisor.is_finite() || divisor == 0.0 {
                        return Err(SnipperError::Runtime(format!(
                            "provider smoke input '{}' has an invalid modulo-ramp generator",
                            input.name
                        )));
                    }
                    (0..length)
                        .map(|index| (index as u32 % modulus) as f32 / divisor)
                        .collect()
                }
            };
            if tensors
                .insert(
                    input.name.clone(),
                    Tensor::float32(&input.name, input.shape.clone(), values),
                )
                .is_some()
            {
                return Err(SnipperError::Runtime(format!(
                    "duplicate provider smoke input '{}'",
                    input.name
                )));
            }
        }
        Ok(tensors)
    }

    pub fn execute(&self, session: &dyn RuntimeSession) -> Result<ProviderSmokeOutcome> {
        let started = Instant::now();
        let response = session.run(RunRequest::new(self.input_tensors()?))?;
        let inference_duration = started.elapsed();
        self.validate_outputs(&response.outputs)?;
        Ok(ProviderSmokeOutcome {
            output_sha256: tensor_map_sha256(&response.outputs)?,
            inference_duration,
        })
    }

    fn validate(&self) -> Result<()> {
        validate_sha256("model SHA", &self.model.sha256)?;
        let model_path = self.model_path();
        let observed_model_sha = file_sha256(&model_path)?;
        if !observed_model_sha.eq_ignore_ascii_case(&self.model.sha256) {
            return Err(SnipperError::Runtime(format!(
                "provider smoke model hash mismatch for '{}'",
                model_path.display()
            )));
        }
        let inputs = self.input_tensors()?;
        if self.inputs.is_empty() || self.expected_outputs.is_empty() {
            return Err(SnipperError::Runtime(
                "provider smoke fixture requires inputs and expected outputs".to_owned(),
            ));
        }
        validate_sha256("input SHA", &self.input_sha256)?;
        let observed_input_sha = tensor_map_sha256(&inputs)?;
        if !observed_input_sha.eq_ignore_ascii_case(&self.input_sha256) {
            return Err(SnipperError::Runtime(
                "provider smoke generated input hash mismatch".to_owned(),
            ));
        }
        if let Some(sha) = &self.expected_output_sha256 {
            validate_sha256("output SHA", sha)?;
        }
        if self
            .tolerance
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(SnipperError::Runtime(
                "provider smoke tolerance must be finite and non-negative".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn validate_outputs(&self, outputs: &TensorMap) -> Result<()> {
        if outputs.len() != self.expected_outputs.len() {
            return Err(SnipperError::Runtime(format!(
                "provider smoke output count mismatch: expected {}, observed {}",
                self.expected_outputs.len(),
                outputs.len()
            )));
        }
        for expected in &self.expected_outputs {
            let observed = outputs.get(&expected.name).ok_or_else(|| {
                SnipperError::Runtime(format!(
                    "provider smoke output '{}' is missing",
                    expected.name
                ))
            })?;
            if observed.dtype().as_str() != expected.dtype || observed.shape() != expected.shape {
                return Err(SnipperError::Runtime(format!(
                    "provider smoke output '{}' metadata mismatch",
                    expected.name
                )));
            }
        }
        let output_sha = tensor_map_sha256(outputs)?;
        if let Some(expected_sha) = &self.expected_output_sha256 {
            if !output_sha.eq_ignore_ascii_case(expected_sha) {
                return Err(SnipperError::Runtime(
                    "provider smoke output hash mismatch".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

pub fn tensor_map_sha256(tensors: &TensorMap) -> Result<String> {
    let bytes = serde_json::to_vec(tensors).map_err(|error| {
        SnipperError::Runtime(format!("failed to hash provider smoke tensors: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path).map_err(|error| {
        SnipperError::Runtime(format!(
            "failed to read provider smoke model '{}': {error}",
            path.display()
        ))
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_sha256(label: &str, value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SnipperError::Runtime(format!(
            "provider smoke {label} must be a 64-character SHA-256"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versioned_fixture_verifies_model_and_input_hashes() {
        let fixture = ProviderSmokeFixture::load(Path::new(
            "../../contracts/fixtures/provider-smoke-v1.json",
        ))
        .unwrap();
        assert_eq!(
            fixture.model_sha256(),
            "ec6ecac6a32e663f67bd3967a6579171783c7185042cc61bb7ca84a92fdc5daa"
        );
        assert_eq!(
            tensor_map_sha256(&fixture.input_tensors().unwrap()).unwrap(),
            "f4e0ec8e493d64ecab6aaa12e8407e758d871fb5f34cc372690f3a7bac3ac120"
        );
    }
}
