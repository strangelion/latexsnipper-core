use serde::{Deserialize, Serialize};

/// A multi-dimensional array for inference I/O.
/// Shared across Image, Inference, and Runtime crates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tensor {
    name: String,
    shape: Vec<usize>,
    data: TensorData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TensorData {
    Float32(Vec<f32>),
    Int64(Vec<i64>),
    Int32(Vec<i32>),
    UInt8(Vec<u8>),
}

impl Tensor {
    pub fn float32(name: impl Into<String>, shape: Vec<usize>, data: Vec<f32>) -> Self {
        Self { name: name.into(), shape, data: TensorData::Float32(data) }
    }

    pub fn int64(name: impl Into<String>, shape: Vec<usize>, data: Vec<i64>) -> Self {
        Self { name: name.into(), shape, data: TensorData::Int64(data) }
    }

    pub fn name(&self) -> &str { &self.name }
    pub fn shape(&self) -> &[usize] { &self.shape }
    pub fn data(&self) -> &TensorData { &self.data }

    pub fn as_f32_slice(&self) -> Option<&[f32]> {
        match &self.data {
            TensorData::Float32(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_i64_slice(&self) -> Option<&[i64]> {
        match &self.data {
            TensorData::Int64(d) => Some(d),
            _ => None,
        }
    }

    pub fn len(&self) -> usize {
        self.shape.iter().product()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
