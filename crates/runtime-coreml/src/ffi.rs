//! Safe ownership wrappers over the Apple-only Core ML C bridge.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;
use std::ptr::NonNull;

use latexsnipper_runtime::SessionTensorSpec;

use crate::error::{coreml_error, CoreMlResult};

pub(crate) const COREML_FLOAT32: i32 = 0;
pub(crate) const COREML_FLOAT16: i32 = 1;
pub(crate) const COREML_INT32: i32 = 2;

const COREML_BRIDGE_ABI_VERSION: u32 = 1;

#[repr(C)]
struct CoreMlSessionRaw {
    _private: [u8; 0],
}

#[repr(C)]
struct CoreMlOutputsRaw {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeTensorView {
    pub name: *const c_char,
    pub dtype: i32,
    pub shape: *const i64,
    pub rank: usize,
    pub data: *const c_void,
    pub byte_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeTensorInfo {
    name: *const c_char,
    dtype: i32,
    shape: *const i64,
    rank: usize,
    data: *const c_void,
    byte_len: usize,
}

unsafe extern "C" {
    fn ls_coreml_bridge_abi_version() -> u32;
    fn ls_coreml_last_error() -> *const c_char;
    fn ls_coreml_runtime_version() -> *const c_char;
    fn ls_coreml_compile_model(source: *const c_char, destination: *const c_char) -> i32;
    fn ls_coreml_session_create(
        compiled_model: *const c_char,
        compute_units: i32,
    ) -> *mut CoreMlSessionRaw;
    fn ls_coreml_session_destroy(session: *mut CoreMlSessionRaw);
    fn ls_coreml_tensor_count(session: *const CoreMlSessionRaw, input: i32) -> usize;
    fn ls_coreml_tensor_info(
        session: *const CoreMlSessionRaw,
        input: i32,
        index: usize,
        info: *mut NativeTensorInfo,
    ) -> i32;
    fn ls_coreml_session_run(
        session: *mut CoreMlSessionRaw,
        inputs: *const NativeTensorView,
        input_count: usize,
    ) -> *mut CoreMlOutputsRaw;
    fn ls_coreml_outputs_destroy(outputs: *mut CoreMlOutputsRaw);
    fn ls_coreml_outputs_count(outputs: *const CoreMlOutputsRaw) -> usize;
    fn ls_coreml_output_info(
        outputs: *const CoreMlOutputsRaw,
        index: usize,
        info: *mut NativeTensorInfo,
    ) -> i32;
}

pub(crate) fn runtime_version() -> String {
    // SAFETY: The bridge returns a process-lifetime null-terminated string.
    c_string(unsafe { ls_coreml_runtime_version() })
        .unwrap_or_else(|| "Core ML / unknown Apple OS".to_owned())
}

pub(crate) fn compile_model(source: &Path, destination: &Path) -> CoreMlResult<()> {
    ensure_abi()?;
    let source = path_c_string(source)?;
    let destination = path_c_string(destination)?;
    // SAFETY: Both paths remain live and null-terminated for this synchronous call.
    let status = unsafe { ls_coreml_compile_model(source.as_ptr(), destination.as_ptr()) };
    ensure_success(status, "compile Core ML model")
}

pub(crate) struct CoreMlProgram {
    raw: NonNull<CoreMlSessionRaw>,
}

// SAFETY: The native model is accessed only while its owning CoreMlSession
// mutex is held; the bridge additionally dispatches predictions serially.
unsafe impl Send for CoreMlProgram {}

impl CoreMlProgram {
    pub(crate) fn load(path: &Path, compute_units: i32) -> CoreMlResult<Self> {
        ensure_abi()?;
        let path = path_c_string(path)?;
        // SAFETY: Path remains live for this synchronous create call.
        let raw = unsafe { ls_coreml_session_create(path.as_ptr(), compute_units) };
        let raw = NonNull::new(raw).ok_or_else(|| {
            coreml_error(format!(
                "load compiled Core ML model failed: {}",
                last_error()
            ))
        })?;
        Ok(Self { raw })
    }

    pub(crate) fn inputs(&self) -> CoreMlResult<Vec<SessionTensorSpec>> {
        self.tensor_specs(true)
    }

    pub(crate) fn outputs(&self) -> CoreMlResult<Vec<SessionTensorSpec>> {
        self.tensor_specs(false)
    }

    fn tensor_specs(&self, input: bool) -> CoreMlResult<Vec<SessionTensorSpec>> {
        // SAFETY: Native session is live and metadata access is read-only.
        let count = unsafe { ls_coreml_tensor_count(self.raw.as_ptr(), i32::from(input)) };
        (0..count)
            .map(|index| {
                let mut info = empty_tensor_info();
                // SAFETY: Output pointer is valid and the bridge validates index.
                let status = unsafe {
                    ls_coreml_tensor_info(self.raw.as_ptr(), i32::from(input), index, &mut info)
                };
                ensure_success(status, "query Core ML tensor metadata")?;
                tensor_spec(info, input, index)
            })
            .collect()
    }

    pub(crate) fn run(&mut self, inputs: &[NativeTensorView]) -> CoreMlResult<CoreMlOutputs> {
        // SAFETY: Views and their backing storage remain live through the
        // synchronous call. Rust and native layers both serialize access.
        let raw =
            unsafe { ls_coreml_session_run(self.raw.as_ptr(), inputs.as_ptr(), inputs.len()) };
        let raw = NonNull::new(raw)
            .ok_or_else(|| coreml_error(format!("Core ML prediction failed: {}", last_error())))?;
        Ok(CoreMlOutputs { raw })
    }
}

impl Drop for CoreMlProgram {
    fn drop(&mut self) {
        // SAFETY: The native session is owned and destroyed exactly once.
        unsafe { ls_coreml_session_destroy(self.raw.as_ptr()) };
    }
}

pub(crate) struct CoreMlOutputs {
    raw: NonNull<CoreMlOutputsRaw>,
}

impl CoreMlOutputs {
    pub(crate) fn count(&self) -> usize {
        // SAFETY: The native output container remains live.
        unsafe { ls_coreml_outputs_count(self.raw.as_ptr()) }
    }

    pub(crate) fn info(&self, index: usize) -> CoreMlResult<NativeOutput<'_>> {
        let mut info = empty_tensor_info();
        // SAFETY: Output pointer is live and bridge validates the index.
        let status = unsafe { ls_coreml_output_info(self.raw.as_ptr(), index, &mut info) };
        ensure_success(status, "query Core ML output")?;
        let name = c_string(info.name)
            .ok_or_else(|| coreml_error(format!("Core ML output {index} has no name")))?;
        let shape = copy_shape(info.shape, info.rank)?;
        let bytes = if info.byte_len == 0 {
            &[]
        } else {
            if info.data.is_null() {
                return Err(coreml_error(format!(
                    "Core ML output '{name}' has null data with byte length {}",
                    info.byte_len
                )));
            }
            // SAFETY: The output container owns this allocation through the borrow.
            unsafe { std::slice::from_raw_parts(info.data.cast::<u8>(), info.byte_len) }
        };
        Ok(NativeOutput {
            name,
            dtype: info.dtype,
            shape,
            bytes,
        })
    }
}

impl Drop for CoreMlOutputs {
    fn drop(&mut self) {
        // SAFETY: The native output container is owned and destroyed once.
        unsafe { ls_coreml_outputs_destroy(self.raw.as_ptr()) };
    }
}

pub(crate) struct NativeOutput<'outputs> {
    pub name: String,
    pub dtype: i32,
    pub shape: Vec<i64>,
    pub bytes: &'outputs [u8],
}

fn ensure_abi() -> CoreMlResult<()> {
    // SAFETY: Constant query has no preconditions.
    let actual = unsafe { ls_coreml_bridge_abi_version() };
    if actual == COREML_BRIDGE_ABI_VERSION {
        Ok(())
    } else {
        Err(coreml_error(format!(
            "Core ML bridge ABI mismatch: expected {COREML_BRIDGE_ABI_VERSION}, got {actual}"
        )))
    }
}

fn ensure_success(status: i32, operation: &str) -> CoreMlResult<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(coreml_error(format!("{operation}: {}", last_error())))
    }
}

fn last_error() -> String {
    // SAFETY: The bridge returns a thread-local null-terminated string.
    c_string(unsafe { ls_coreml_last_error() })
        .unwrap_or_else(|| "unknown native Core ML error".to_owned())
}

fn c_string(pointer: *const c_char) -> Option<String> {
    (!pointer.is_null()).then(|| {
        // SAFETY: Non-null bridge strings are null-terminated and immediately copied.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    })
}

fn path_c_string(path: &Path) -> CoreMlResult<CString> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
        coreml_error(format!(
            "Core ML path contains an interior NUL byte: {}",
            path.display()
        ))
    })
}

fn tensor_spec(
    info: NativeTensorInfo,
    input: bool,
    index: usize,
) -> CoreMlResult<SessionTensorSpec> {
    let direction = if input { "input" } else { "output" };
    let name = c_string(info.name)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| coreml_error(format!("Core ML {direction} {index} has no name")))?;
    Ok(SessionTensorSpec {
        name,
        shape: copy_shape(info.shape, info.rank)?
            .into_iter()
            .map(|dimension| (dimension >= 0).then_some(dimension))
            .collect(),
        dtype: dtype_name(info.dtype)?.to_owned(),
    })
}

fn copy_shape(pointer: *const i64, rank: usize) -> CoreMlResult<Vec<i64>> {
    if rank == 0 {
        return Ok(Vec::new());
    }
    if pointer.is_null() {
        return Err(coreml_error("native Core ML tensor shape is null"));
    }
    // SAFETY: Bridge provides `rank` live dimensions for a non-null pointer.
    Ok(unsafe { std::slice::from_raw_parts(pointer, rank) }.to_vec())
}

fn empty_tensor_info() -> NativeTensorInfo {
    NativeTensorInfo {
        name: std::ptr::null(),
        dtype: -1,
        shape: std::ptr::null(),
        rank: 0,
        data: std::ptr::null(),
        byte_len: 0,
    }
}

pub(crate) fn dtype_name(dtype: i32) -> CoreMlResult<&'static str> {
    match dtype {
        COREML_FLOAT32 => Ok("f32"),
        COREML_FLOAT16 => Ok("f16"),
        COREML_INT32 => Ok("i32"),
        other => Err(coreml_error(format!(
            "unsupported Core ML tensor dtype code {other}"
        ))),
    }
}
