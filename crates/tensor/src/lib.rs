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
    /// IEEE-754 binary16 values stored as their exact bit representation.
    /// This keeps serialization stable and avoids lossy promotion to f32.
    Float16(Vec<u16>),
    Int64(Vec<i64>),
    Int32(Vec<i32>),
    UInt8(Vec<u8>),
    Bool(Vec<bool>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TensorDtype {
    Float32,
    Float16,
    Int64,
    Int32,
    UInt8,
    Bool,
}

impl TensorDtype {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Float32 => "f32",
            Self::Float16 => "f16",
            Self::Int64 => "i64",
            Self::Int32 => "i32",
            Self::UInt8 => "u8",
            Self::Bool => "bool",
        }
    }

    pub const fn element_size(self) -> usize {
        match self {
            Self::Float32 | Self::Int32 => 4,
            Self::Float16 => 2,
            Self::Int64 => 8,
            Self::UInt8 | Self::Bool => 1,
        }
    }
}

impl Tensor {
    pub fn float32(name: impl Into<String>, shape: Vec<usize>, data: Vec<f32>) -> Self {
        Self {
            name: name.into(),
            shape,
            data: TensorData::Float32(data),
        }
    }

    pub fn int64(name: impl Into<String>, shape: Vec<usize>, data: Vec<i64>) -> Self {
        Self {
            name: name.into(),
            shape,
            data: TensorData::Int64(data),
        }
    }

    pub fn float16(name: impl Into<String>, shape: Vec<usize>, data: Vec<half::f16>) -> Self {
        Self::float16_bits(
            name,
            shape,
            data.into_iter().map(half::f16::to_bits).collect(),
        )
    }

    pub fn float16_bits(name: impl Into<String>, shape: Vec<usize>, data: Vec<u16>) -> Self {
        Self {
            name: name.into(),
            shape,
            data: TensorData::Float16(data),
        }
    }

    pub fn int32(name: impl Into<String>, shape: Vec<usize>, data: Vec<i32>) -> Self {
        Self {
            name: name.into(),
            shape,
            data: TensorData::Int32(data),
        }
    }

    pub fn u8(name: impl Into<String>, shape: Vec<usize>, data: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            shape,
            data: TensorData::UInt8(data),
        }
    }

    pub fn boolean(name: impl Into<String>, shape: Vec<usize>, data: Vec<bool>) -> Self {
        Self {
            name: name.into(),
            shape,
            data: TensorData::Bool(data),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }
    pub fn data(&self) -> &TensorData {
        &self.data
    }

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

    pub fn as_f16_bits(&self) -> Option<&[u16]> {
        match &self.data {
            TensorData::Float16(d) => Some(d),
            _ => None,
        }
    }

    pub fn to_f16_vec(&self) -> Option<Vec<half::f16>> {
        self.as_f16_bits()
            .map(|data| data.iter().copied().map(half::f16::from_bits).collect())
    }

    pub fn as_i32_slice(&self) -> Option<&[i32]> {
        match &self.data {
            TensorData::Int32(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_u8_slice(&self) -> Option<&[u8]> {
        match &self.data {
            TensorData::UInt8(d) => Some(d),
            _ => None,
        }
    }

    pub fn as_bool_slice(&self) -> Option<&[bool]> {
        match &self.data {
            TensorData::Bool(d) => Some(d),
            _ => None,
        }
    }

    pub fn dtype(&self) -> TensorDtype {
        match &self.data {
            TensorData::Float32(_) => TensorDtype::Float32,
            TensorData::Float16(_) => TensorDtype::Float16,
            TensorData::Int64(_) => TensorDtype::Int64,
            TensorData::Int32(_) => TensorDtype::Int32,
            TensorData::UInt8(_) => TensorDtype::UInt8,
            TensorData::Bool(_) => TensorDtype::Bool,
        }
    }

    pub fn len(&self) -> usize {
        self.shape.iter().product()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float16_bits_round_trip_exactly() {
        let values = vec![half::f16::from_f32(1.5), half::f16::from_f32(-2.25)];
        let tensor = Tensor::float16("x", vec![2], values.clone());
        assert_eq!(tensor.dtype(), TensorDtype::Float16);
        assert_eq!(tensor.to_f16_vec().unwrap(), values);
    }

    #[test]
    fn bool_tensor_has_one_element_per_value() {
        let tensor = Tensor::boolean("mask", vec![3], vec![true, false, true]);
        assert_eq!(tensor.dtype(), TensorDtype::Bool);
        assert_eq!(tensor.as_bool_slice(), Some([true, false, true].as_slice()));
        assert_eq!(tensor.len(), 3);
    }
}
