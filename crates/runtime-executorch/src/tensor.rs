//! CPU-copy conversion between common tensors and the ExecuTorch bridge ABI.

use std::ffi::c_void;

use latexsnipper_tensor::{Tensor, TensorData};

use crate::error::{executorch_error, ExecuTorchResult};
use crate::ffi::{
    NativeOutput, NativeTensorView, ET_DATA_BOOL, ET_DATA_FLOAT16, ET_DATA_FLOAT32, ET_DATA_INT32,
    ET_DATA_INT64, ET_DATA_UINT8,
};

pub(crate) struct PreparedInput {
    shape: Vec<i64>,
    dtype: i32,
    bytes: Vec<u8>,
}

impl PreparedInput {
    pub(crate) fn new(tensor: &Tensor) -> ExecuTorchResult<Self> {
        let element_count = checked_element_count(tensor.shape())?;
        let actual_count = data_len(tensor.data());
        if actual_count != element_count {
            return Err(executorch_error(format!(
                "tensor '{}' shape requires {element_count} elements, but data contains {actual_count}",
                tensor.name()
            )));
        }
        let shape = tensor
            .shape()
            .iter()
            .map(|dimension| {
                i64::try_from(*dimension).map_err(|_| {
                    executorch_error(format!(
                        "tensor '{}' dimension exceeds i64::MAX",
                        tensor.name()
                    ))
                })
            })
            .collect::<ExecuTorchResult<Vec<_>>>()?;
        let (dtype, bytes) = match tensor.data() {
            TensorData::Float32(values) => (ET_DATA_FLOAT32, encode(values, f32::to_ne_bytes)),
            TensorData::Float16(values) => (ET_DATA_FLOAT16, encode(values, u16::to_ne_bytes)),
            TensorData::Int64(values) => (ET_DATA_INT64, encode(values, i64::to_ne_bytes)),
            TensorData::Int32(values) => (ET_DATA_INT32, encode(values, i32::to_ne_bytes)),
            TensorData::UInt8(values) => (ET_DATA_UINT8, values.clone()),
            TensorData::Bool(values) => {
                (ET_DATA_BOOL, values.iter().copied().map(u8::from).collect())
            }
        };
        Ok(Self {
            shape,
            dtype,
            bytes,
        })
    }

    pub(crate) fn view(&self) -> NativeTensorView {
        NativeTensorView {
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

pub(crate) fn copy_output(name: &str, output: NativeOutput<'_>) -> ExecuTorchResult<Tensor> {
    let shape = output
        .shape
        .iter()
        .map(|dimension| {
            usize::try_from(*dimension).map_err(|_| {
                executorch_error(format!(
                    "output tensor '{name}' has invalid dimension {dimension}"
                ))
            })
        })
        .collect::<ExecuTorchResult<Vec<_>>>()?;
    let element_count = checked_element_count(&shape)?;

    match output.dtype {
        ET_DATA_FLOAT32 => Ok(Tensor::float32(
            name,
            shape,
            decode::<4, f32>(name, output.bytes, element_count, f32::from_ne_bytes)?,
        )),
        ET_DATA_FLOAT16 => Ok(Tensor::float16_bits(
            name,
            shape,
            decode::<2, u16>(name, output.bytes, element_count, u16::from_ne_bytes)?,
        )),
        ET_DATA_INT64 => Ok(Tensor::int64(
            name,
            shape,
            decode::<8, i64>(name, output.bytes, element_count, i64::from_ne_bytes)?,
        )),
        ET_DATA_INT32 => Ok(Tensor::int32(
            name,
            shape,
            decode::<4, i32>(name, output.bytes, element_count, i32::from_ne_bytes)?,
        )),
        ET_DATA_UINT8 => {
            validate_byte_len(name, output.bytes.len(), element_count)?;
            Ok(Tensor::u8(name, shape, output.bytes.to_vec()))
        }
        ET_DATA_BOOL => {
            validate_byte_len(name, output.bytes.len(), element_count)?;
            Ok(Tensor::boolean(
                name,
                shape,
                output.bytes.iter().map(|value| *value != 0).collect(),
            ))
        }
        dtype => Err(executorch_error(format!(
            "unsupported ExecuTorch output dtype code: {dtype}"
        ))),
    }
}

fn encode<T, const WIDTH: usize>(values: &[T], to_bytes: impl Fn(T) -> [u8; WIDTH]) -> Vec<u8>
where
    T: Copy,
{
    values
        .iter()
        .copied()
        .flat_map(to_bytes)
        .collect::<Vec<_>>()
}

fn decode<const WIDTH: usize, T>(
    name: &str,
    bytes: &[u8],
    element_count: usize,
    from_bytes: impl Fn([u8; WIDTH]) -> T,
) -> ExecuTorchResult<Vec<T>> {
    let expected = element_count
        .checked_mul(WIDTH)
        .ok_or_else(|| executorch_error("tensor byte length overflow"))?;
    validate_byte_len(name, bytes.len(), expected)?;
    Ok(bytes
        .as_chunks::<WIDTH>()
        .0
        .iter()
        .copied()
        .map(from_bytes)
        .collect())
}

fn validate_byte_len(name: &str, actual: usize, expected: usize) -> ExecuTorchResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(executorch_error(format!(
            "output tensor '{name}' requires {expected} bytes, but native output contains {actual}"
        )))
    }
}

fn checked_element_count(shape: &[usize]) -> ExecuTorchResult<usize> {
    shape.iter().try_fold(1usize, |count, dimension| {
        count
            .checked_mul(*dimension)
            .ok_or_else(|| executorch_error("tensor element count overflow"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepares_boolean_tensor_as_one_byte_per_element() {
        let input = Tensor::boolean("mask", vec![3], vec![true, false, true]);
        let prepared = PreparedInput::new(&input).unwrap();
        assert_eq!(prepared.dtype, ET_DATA_BOOL);
        assert_eq!(prepared.bytes, [1, 0, 1]);
    }

    #[test]
    fn rejects_shape_and_data_length_mismatch() {
        let input = Tensor::float32("x", vec![2, 2], vec![1.0, 2.0]);
        assert!(PreparedInput::new(&input).is_err());
    }
}
