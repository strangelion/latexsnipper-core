use latexsnipper_foundation::{Result, SnipperError};
use latexsnipper_runtime::InferenceSession;
use latexsnipper_tensor::{Tensor, TensorData};

use tract_onnx::prelude::*;

/// An `InferenceSession` backed by `tract`.
pub struct TractSession {
    model: std::sync::Arc<TypedRunnableModel<TypedModel>>,
}

impl TractSession {
    pub fn new(model: TypedRunnableModel<TypedModel>) -> Self {
        Self {
            model: std::sync::Arc::new(model),
        }
    }
}

impl Clone for TractSession {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
        }
    }
}

/// Extract a typed slice from a tract Tensor, mapping errors to SnipperError.
fn extract_slice<'a, T: tract_core::prelude::Datum>(
    tensor: &'a tract_core::prelude::Tensor,
    name: &str,
) -> Result<&'a [T]> {
    tensor
        .as_slice::<T>()
        .map_err(|e| SnipperError::Inference(format!("Failed to read {} output: {}", name, e)))
}

impl InferenceSession for TractSession {
    fn run(&self, inputs: &[Tensor]) -> Result<Vec<Tensor>> {
        let model = &*self.model;
        let model_ref = model.model();

        let num_inputs = model_ref.inputs.len();

        if inputs.len() != num_inputs {
            return Err(SnipperError::Inference(format!(
                "Expected {} inputs, got {}",
                num_inputs,
                inputs.len()
            )));
        }

        let mut tract_inputs: TVec<TValue> = tvec![];

        for (i, input) in inputs.iter().enumerate() {
            let fact = model_ref.input_fact(i).map_err(|e| {
                SnipperError::Inference(format!("Failed to get input fact {}: {}", i, e))
            })?;
            let shape: Vec<usize> = fact
                .shape
                .as_concrete()
                .map(|s| s.to_vec())
                .unwrap_or_else(|| input.shape().to_vec());

            let tract_tensor = match input.data() {
                TensorData::Float32(data) => {
                    let arr = ndarray::Array::from_shape_vec(shape.as_slice(), data.clone())
                        .map_err(|e| {
                            SnipperError::Inference(format!(
                                "Failed to create tract tensor from input {}: {}",
                                i, e
                            ))
                        })?;
                    arr.into_tensor()
                }
                TensorData::Float16(data) => {
                    let values: Vec<f16> = data.iter().copied().map(f16::from_bits).collect();
                    let arr =
                        ndarray::Array::from_shape_vec(shape.as_slice(), values).map_err(|e| {
                            SnipperError::Inference(format!(
                                "Failed to create tract tensor from input {}: {}",
                                i, e
                            ))
                        })?;
                    arr.into_tensor()
                }
                TensorData::Int64(data) => {
                    let arr = ndarray::Array::from_shape_vec(shape.as_slice(), data.clone())
                        .map_err(|e| {
                            SnipperError::Inference(format!(
                                "Failed to create tract tensor from input {}: {}",
                                i, e
                            ))
                        })?;
                    arr.into_tensor()
                }
                TensorData::Int32(data) => {
                    let arr = ndarray::Array::from_shape_vec(shape.as_slice(), data.clone())
                        .map_err(|e| {
                            SnipperError::Inference(format!(
                                "Failed to create tract tensor from input {}: {}",
                                i, e
                            ))
                        })?;
                    arr.into_tensor()
                }
                TensorData::UInt8(data) => {
                    let arr = ndarray::Array::from_shape_vec(shape.as_slice(), data.clone())
                        .map_err(|e| {
                            SnipperError::Inference(format!(
                                "Failed to create tract tensor from input {}: {}",
                                i, e
                            ))
                        })?;
                    arr.into_tensor()
                }
                TensorData::Bool(data) => {
                    let arr = ndarray::Array::from_shape_vec(shape.as_slice(), data.clone())
                        .map_err(|e| {
                            SnipperError::Inference(format!(
                                "Failed to create tract tensor from input {}: {}",
                                i, e
                            ))
                        })?;
                    arr.into_tensor()
                }
            };

            tract_inputs.push(tract_tensor.into());
        }

        // Run inference
        let outputs = model
            .run(tract_inputs)
            .map_err(|e| SnipperError::Inference(format!("Tract inference failed: {}", e)))?;

        // Convert outputs back to latexsnipper Tensor
        let mut result = Vec::with_capacity(outputs.len());

        for (i, output) in outputs.iter().enumerate() {
            let name = format!("output_{}", i);
            let tensor = output.clone().into_tensor();

            match tensor.datum_type() {
                DatumType::F32 => {
                    let arr = extract_slice::<f32>(&tensor, &name)?;
                    let shape = tensor.shape().to_vec();
                    result.push(Tensor::float32(name, shape, arr.to_vec()));
                }
                DatumType::F16 => {
                    let arr = extract_slice::<f16>(&tensor, &name)?;
                    let shape = tensor.shape().to_vec();
                    result.push(Tensor::float16_bits(
                        name,
                        shape,
                        arr.iter().map(|value| value.to_bits()).collect(),
                    ));
                }
                DatumType::I64 => {
                    let arr = extract_slice::<i64>(&tensor, &name)?;
                    let shape = tensor.shape().to_vec();
                    result.push(Tensor::int64(name, shape, arr.to_vec()));
                }
                DatumType::I32 => {
                    let arr = extract_slice::<i32>(&tensor, &name)?;
                    let shape = tensor.shape().to_vec();
                    result.push(Tensor::int32(name, shape, arr.to_vec()));
                }
                DatumType::U8 => {
                    let arr = extract_slice::<u8>(&tensor, &name)?;
                    let shape = tensor.shape().to_vec();
                    result.push(Tensor::u8(name, shape, arr.to_vec()));
                }
                DatumType::Bool => {
                    let arr = extract_slice::<bool>(&tensor, &name)?;
                    let shape = tensor.shape().to_vec();
                    result.push(Tensor::boolean(name, shape, arr.to_vec()));
                }
                other => {
                    return Err(SnipperError::Inference(format!(
                        "Unsupported tract output type: {:?}",
                        other
                    )));
                }
            }
        }

        Ok(result)
    }

    fn input_names(&self) -> Vec<String> {
        self.model
            .model()
            .inputs
            .iter()
            .enumerate()
            .map(|(i, _)| format!("input_{}", i))
            .collect()
    }

    fn output_names(&self) -> Vec<String> {
        self.model
            .model()
            .outputs
            .iter()
            .enumerate()
            .map(|(i, _)| format!("output_{}", i))
            .collect()
    }

    fn get_character_list(&self) -> Option<Vec<String>> {
        // Tract doesn't have ORT-style metadata access for character lists.
        // Character lists should be loaded separately from tokenizer files.
        None
    }

    fn release(&mut self) {
        // TractSession uses Arc, so resources are released when all references drop.
    }
}
