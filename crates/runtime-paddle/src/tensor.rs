//! CPU-copy conversion between common tensors and Paddle tensor handles.

use crate::error::{paddle_error, PaddleResult};
use crate::ffi::{
    PaddleTensor, PADDLE_DATA_BOOL, PADDLE_DATA_FLOAT16, PADDLE_DATA_FLOAT32, PADDLE_DATA_INT32,
    PADDLE_DATA_INT64, PADDLE_DATA_UINT8,
};
use latexsnipper_runtime::SessionTensorSpec;
use latexsnipper_tensor::{Tensor, TensorData};

pub(crate) fn copy_input(handle: &mut PaddleTensor<'_>, tensor: &Tensor) -> PaddleResult<()> {
    let element_count = checked_element_count(tensor.shape())?;
    let actual_count = data_len(tensor.data());
    if actual_count != element_count {
        return Err(paddle_error(format!(
            "tensor '{}' shape requires {element_count} elements, but data contains {actual_count}",
            tensor.name()
        )));
    }

    let shape = tensor
        .shape()
        .iter()
        .map(|dimension| {
            i64::try_from(*dimension).map_err(|_| {
                paddle_error(format!(
                    "tensor '{}' dimension exceeds i64::MAX",
                    tensor.name()
                ))
            })
        })
        .collect::<PaddleResult<Vec<_>>>()?;
    handle.reshape(&shape)?;

    match tensor.data() {
        TensorData::Float32(values) => handle.copy_from_f32(values),
        TensorData::Float16(values) => handle.copy_from_f16(values),
        TensorData::Int64(values) => handle.copy_from_i64(values),
        TensorData::Int32(values) => handle.copy_from_i32(values),
        TensorData::UInt8(values) => handle.copy_from_u8(values),
        TensorData::Bool(values) => {
            let bytes: Vec<_> = values.iter().copied().map(u8::from).collect();
            handle.copy_from_bool(&bytes)
        }
    }
}

pub(crate) fn copy_output(name: &str, handle: &PaddleTensor<'_>) -> PaddleResult<Tensor> {
    let raw_shape = handle.shape()?;
    let shape = concrete_output_shape(name, &raw_shape)?;
    let element_count = checked_element_count(&shape)?;

    match handle.dtype()? {
        PADDLE_DATA_FLOAT32 => {
            let mut values = vec![0.0; element_count];
            handle.copy_to_f32(&mut values)?;
            Ok(Tensor::float32(name, shape, values))
        }
        PADDLE_DATA_FLOAT16 => {
            let mut values = vec![0; element_count];
            handle.copy_to_f16(&mut values)?;
            Ok(Tensor::float16_bits(name, shape, values))
        }
        PADDLE_DATA_INT64 => {
            let mut values = vec![0; element_count];
            handle.copy_to_i64(&mut values)?;
            Ok(Tensor::int64(name, shape, values))
        }
        PADDLE_DATA_INT32 => {
            let mut values = vec![0; element_count];
            handle.copy_to_i32(&mut values)?;
            Ok(Tensor::int32(name, shape, values))
        }
        PADDLE_DATA_UINT8 => {
            let mut values = vec![0; element_count];
            handle.copy_to_u8(&mut values)?;
            Ok(Tensor::u8(name, shape, values))
        }
        PADDLE_DATA_BOOL => {
            let mut values = vec![0; element_count];
            handle.copy_to_bool(&mut values)?;
            Ok(Tensor::boolean(
                name,
                shape,
                values.into_iter().map(|value| value != 0).collect(),
            ))
        }
        dtype => Err(paddle_error(format!(
            "unsupported Paddle output dtype code: {dtype}"
        ))),
    }
}

pub(crate) fn tensor_spec(
    name: &str,
    handle: &PaddleTensor<'_>,
) -> PaddleResult<SessionTensorSpec> {
    Ok(SessionTensorSpec {
        name: name.to_owned(),
        shape: handle
            .shape()?
            .into_iter()
            .map(|dimension| (dimension >= 0).then_some(dimension))
            .collect(),
        dtype: dtype_name(handle.dtype()?).to_owned(),
    })
}

fn dtype_name(dtype: i32) -> &'static str {
    match dtype {
        PADDLE_DATA_FLOAT32 => "f32",
        PADDLE_DATA_FLOAT16 => "f16",
        PADDLE_DATA_INT32 => "i32",
        PADDLE_DATA_INT64 => "i64",
        PADDLE_DATA_UINT8 => "u8",
        PADDLE_DATA_BOOL => "bool",
        _ => "unknown",
    }
}

fn concrete_output_shape(name: &str, shape: &[i64]) -> PaddleResult<Vec<usize>> {
    shape
        .iter()
        .map(|dimension| {
            usize::try_from(*dimension).map_err(|_| {
                paddle_error(format!(
                    "output tensor '{name}' still has a dynamic dimension {dimension} after inference"
                ))
            })
        })
        .collect()
}

fn checked_element_count(shape: &[usize]) -> PaddleResult<usize> {
    shape.iter().try_fold(1usize, |count, dimension| {
        count
            .checked_mul(*dimension)
            .ok_or_else(|| paddle_error("tensor element count overflow"))
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
    fn rejects_dynamic_output_dimensions() {
        let error = concrete_output_shape("tokens", &[1, -1]).unwrap_err();
        assert!(error.to_string().contains("dynamic dimension -1"));
    }

    #[test]
    fn detects_shape_overflow() {
        assert!(checked_element_count(&[usize::MAX, 2]).is_err());
    }
}
