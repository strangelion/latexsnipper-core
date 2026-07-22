use std::ffi::{c_void, CString};

use latexsnipper_tensor::{Tensor, TensorData};

use crate::error::{tensorrt_error, TensorRtResult};
use crate::ffi::{
    output_parts, NativeTensorInfo, NativeTensorView, TRT_DATA_BOOL, TRT_DATA_FLOAT16,
    TRT_DATA_FLOAT32, TRT_DATA_INT32, TRT_DATA_INT64, TRT_DATA_UINT8,
};

enum InputData {
    Float32(Vec<f32>),
    Float16(Vec<u16>),
    Int64(Vec<i64>),
    Int32(Vec<i32>),
    UInt8(Vec<u8>),
    Bool(Vec<u8>),
}

impl InputData {
    fn pointer(&self) -> *const c_void {
        match self {
            Self::Float32(data) => data.as_ptr().cast(),
            Self::Float16(data) => data.as_ptr().cast(),
            Self::Int64(data) => data.as_ptr().cast(),
            Self::Int32(data) => data.as_ptr().cast(),
            Self::UInt8(data) | Self::Bool(data) => data.as_ptr().cast(),
        }
    }

    fn byte_len(&self) -> usize {
        match self {
            Self::Float32(data) => std::mem::size_of_val(data.as_slice()),
            Self::Float16(data) => std::mem::size_of_val(data.as_slice()),
            Self::Int64(data) => std::mem::size_of_val(data.as_slice()),
            Self::Int32(data) => std::mem::size_of_val(data.as_slice()),
            Self::UInt8(data) | Self::Bool(data) => data.len(),
        }
    }
}

pub(crate) struct PreparedInput {
    name: CString,
    shape: Vec<i64>,
    dtype: i32,
    data: InputData,
}

impl PreparedInput {
    pub(crate) fn new(name: &str, tensor: &Tensor) -> TensorRtResult<Self> {
        let name = CString::new(name.as_bytes())
            .map_err(|_| tensorrt_error(format!("input name contains a NUL byte: {name:?}")))?;
        let shape = tensor
            .shape()
            .iter()
            .map(|dimension| {
                i64::try_from(*dimension)
                    .map_err(|_| tensorrt_error(format!("input dimension {dimension} exceeds i64")))
            })
            .collect::<TensorRtResult<Vec<_>>>()?;
        let (dtype, data) = match tensor.data() {
            TensorData::Float32(data) => (TRT_DATA_FLOAT32, InputData::Float32(data.clone())),
            TensorData::Float16(data) => (TRT_DATA_FLOAT16, InputData::Float16(data.clone())),
            TensorData::Int64(data) => (TRT_DATA_INT64, InputData::Int64(data.clone())),
            TensorData::Int32(data) => (TRT_DATA_INT32, InputData::Int32(data.clone())),
            TensorData::UInt8(data) => (TRT_DATA_UINT8, InputData::UInt8(data.clone())),
            TensorData::Bool(data) => (
                TRT_DATA_BOOL,
                InputData::Bool(data.iter().map(|value| u8::from(*value)).collect()),
            ),
        };
        Ok(Self {
            name,
            shape,
            dtype,
            data,
        })
    }

    pub(crate) fn view(&self) -> NativeTensorView {
        NativeTensorView {
            name: self.name.as_ptr(),
            dtype: self.dtype,
            shape: self.shape.as_ptr(),
            rank: self.shape.len(),
            data: self.data.pointer(),
            byte_len: self.data.byte_len(),
        }
    }
}

pub(crate) fn copy_output(name: &str, info: NativeTensorInfo) -> TensorRtResult<Tensor> {
    let (dtype, shape, pointer, byte_len) = output_parts(info)?;
    let elements = shape
        .iter()
        .try_fold(1usize, |count, dimension| count.checked_mul(*dimension))
        .ok_or_else(|| tensorrt_error("output element count overflow"))?;
    macro_rules! copy_typed {
        ($type:ty) => {{
            let expected = elements
                .checked_mul(std::mem::size_of::<$type>())
                .ok_or_else(|| tensorrt_error("output byte length overflow"))?;
            if byte_len != expected {
                return Err(tensorrt_error(format!(
                    "output '{name}' has {byte_len} bytes, expected {expected}"
                )));
            }
            if elements == 0 {
                Vec::new()
            } else {
                // SAFETY: FFI metadata was validated and contains `elements` values.
                unsafe { std::slice::from_raw_parts(pointer.cast::<$type>(), elements) }.to_vec()
            }
        }};
    }
    match dtype {
        TRT_DATA_FLOAT32 => Ok(Tensor::float32(name, shape, copy_typed!(f32))),
        TRT_DATA_FLOAT16 => Ok(Tensor::float16_bits(name, shape, copy_typed!(u16))),
        TRT_DATA_INT64 => Ok(Tensor::int64(name, shape, copy_typed!(i64))),
        TRT_DATA_INT32 => Ok(Tensor::int32(name, shape, copy_typed!(i32))),
        TRT_DATA_UINT8 => Ok(Tensor::u8(name, shape, copy_typed!(u8))),
        TRT_DATA_BOOL => Ok(Tensor::boolean(
            name,
            shape,
            copy_typed!(u8)
                .into_iter()
                .map(|value| value != 0)
                .collect(),
        )),
        _ => Err(tensorrt_error(format!(
            "output '{name}' has unsupported dtype {dtype}"
        ))),
    }
}
