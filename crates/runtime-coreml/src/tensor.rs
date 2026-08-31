//! CPU-copy conversion between shared tensors and `MLMultiArray` bridge views.

use std::ffi::{c_void, CString};

use latexsnipper_tensor::{Tensor, TensorData};

use crate::error::{coreml_error, CoreMlResult};
use crate::ffi::{NativeOutput, NativeTensorView, COREML_FLOAT16, COREML_FLOAT32, COREML_INT32};

pub(crate) struct PreparedInput {
    name: CString,
    shape: Vec<i64>,
    dtype: i32,
    bytes: Vec<u8>,
}

impl PreparedInput {
    pub(crate) fn new(name: &str, tensor: &Tensor) -> CoreMlResult<Self> {
        let element_count = checked_element_count(tensor.shape())?;
        let actual_count = data_len(tensor.data());
        if element_count != actual_count {
            return Err(coreml_error(format!(
                "tensor '{name}' shape requires {element_count} elements, but data contains {actual_count}"
            )));
        }
        let name = CString::new(name)
            .map_err(|_| coreml_error("Core ML tensor name contains an interior NUL byte"))?;
        let shape = tensor
            .shape()
            .iter()
            .map(|dimension| {
                i64::try_from(*dimension).map_err(|_| {
                    coreml_error("Core ML tensor dimension exceeds the native i64 range")
                })
            })
            .collect::<CoreMlResult<Vec<_>>>()?;
        let (dtype, bytes) = match tensor.data() {
            TensorData::Float32(values) => (COREML_FLOAT32, encode(values, f32::to_ne_bytes)),
            TensorData::Float16(values) => (COREML_FLOAT16, encode(values, u16::to_ne_bytes)),
            TensorData::Int32(values) => (COREML_INT32, encode(values, i32::to_ne_bytes)),
            other => {
                return Err(coreml_error(format!(
                    "Core ML MLMultiArray does not support shared tensor dtype {}",
                    data_type_name(other)
                )))
            }
        };
        Ok(Self {
            name,
            shape,
            dtype,
            bytes,
        })
    }

    pub(crate) fn view(&self) -> NativeTensorView {
        NativeTensorView {
            name: self.name.as_ptr(),
            dtype: self.dtype,
            shape: self.shape.as_ptr(),
            rank: self.shape.len(),
            data: if self.bytes.is_empty() {
                std::ptr::null()
            } else {
                self.bytes.as_ptr().cast::<c_void>()
            },
            byte_len: self.bytes.len(),
        }
    }
}

pub(crate) fn copy_output(output: NativeOutput<'_>) -> CoreMlResult<Tensor> {
    let name = output.name;
    let shape = output
        .shape
        .iter()
        .map(|dimension| {
            usize::try_from(*dimension).map_err(|_| {
                coreml_error(format!(
                    "Core ML output '{}' has invalid dimension {dimension}",
                    name
                ))
            })
        })
        .collect::<CoreMlResult<Vec<_>>>()?;
    let count = checked_element_count(&shape)?;
    match output.dtype {
        COREML_FLOAT32 => {
            let values = decode::<4, f32>(&name, output.bytes, count, f32::from_ne_bytes)?;
            Ok(Tensor::float32(name, shape, values))
        }
        COREML_FLOAT16 => {
            let values = decode::<2, u16>(&name, output.bytes, count, u16::from_ne_bytes)?;
            Ok(Tensor::float16_bits(name, shape, values))
        }
        COREML_INT32 => {
            let values = decode::<4, i32>(&name, output.bytes, count, i32::from_ne_bytes)?;
            Ok(Tensor::int32(name, shape, values))
        }
        dtype => Err(coreml_error(format!(
            "unsupported Core ML output dtype code {dtype}"
        ))),
    }
}

fn encode<T, const WIDTH: usize>(values: &[T], to_bytes: impl Fn(T) -> [u8; WIDTH]) -> Vec<u8>
where
    T: Copy,
{
    values.iter().copied().flat_map(to_bytes).collect()
}

fn decode<const WIDTH: usize, T>(
    name: &str,
    bytes: &[u8],
    count: usize,
    from_bytes: impl Fn([u8; WIDTH]) -> T,
) -> CoreMlResult<Vec<T>> {
    let expected = count
        .checked_mul(WIDTH)
        .ok_or_else(|| coreml_error("Core ML tensor byte length overflow"))?;
    if bytes.len() != expected {
        return Err(coreml_error(format!(
            "Core ML output '{name}' requires {expected} bytes, but native output contains {}",
            bytes.len()
        )));
    }
    Ok(bytes
        .as_chunks::<WIDTH>()
        .0
        .iter()
        .copied()
        .map(from_bytes)
        .collect())
}

fn checked_element_count(shape: &[usize]) -> CoreMlResult<usize> {
    shape.iter().try_fold(1usize, |count, dimension| {
        count
            .checked_mul(*dimension)
            .ok_or_else(|| coreml_error("Core ML tensor element count overflow"))
    })
}

fn data_len(data: &TensorData) -> usize {
    match data {
        TensorData::Float32(values) => values.len(),
        TensorData::Float16(values) => values.len(),
        TensorData::Int64(values) => values.len(),
        TensorData::Int32(values) => values.len(),
        TensorData::UInt8(values) => values.len(),
        TensorData::Bool(values) => values.len(),
    }
}

fn data_type_name(data: &TensorData) -> &'static str {
    match data {
        TensorData::Float32(_) => "f32",
        TensorData::Float16(_) => "f16",
        TensorData::Int64(_) => "i64",
        TensorData::Int32(_) => "i32",
        TensorData::UInt8(_) => "u8",
        TensorData::Bool(_) => "bool",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dtype_without_a_coreml_multiarray_representation() {
        let tensor = Tensor::int64("tokens", vec![1], vec![1]);
        assert!(PreparedInput::new("tokens", &tensor).is_err());
    }

    #[test]
    fn prepares_exact_float16_bits() {
        let tensor = Tensor::float16_bits("x", vec![2], vec![0x3c00, 0xc000]);
        let prepared = PreparedInput::new("x", &tensor).unwrap();
        assert_eq!(prepared.dtype, COREML_FLOAT16);
        assert_eq!(prepared.bytes, [0x00, 0x3c, 0x00, 0xc0]);
    }
}
