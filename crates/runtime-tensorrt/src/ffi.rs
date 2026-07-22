use std::ffi::{c_char, c_void, CStr, CString};
use std::fmt;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;

use latexsnipper_runtime::SessionTensorSpec;
use libloading::Library;

use crate::error::{tensorrt_error, TensorRtResult};
use crate::flavor::TensorRtFlavor;
use crate::options::TensorRtOptions;

pub(crate) const TRT_DATA_FLOAT32: i32 = 0;
pub(crate) const TRT_DATA_FLOAT16: i32 = 1;
pub(crate) const TRT_DATA_INT64: i32 = 2;
pub(crate) const TRT_DATA_INT32: i32 = 3;
pub(crate) const TRT_DATA_UINT8: i32 = 4;
pub(crate) const TRT_DATA_BOOL: i32 = 5;

const TENSORRT_BRIDGE_ABI_VERSION: u32 = 1;

#[repr(C)]
struct TensorRtBufferRaw {
    _private: [u8; 0],
}

#[repr(C)]
struct TensorRtSessionRaw {
    _private: [u8; 0],
}

#[repr(C)]
struct TensorRtOutputsRaw {
    _private: [u8; 0],
}

#[repr(C)]
struct NativeShapeProfile {
    input_name: *const c_char,
    min_shape: *const i64,
    opt_shape: *const i64,
    max_shape: *const i64,
    rank: usize,
}

#[repr(C)]
struct NativeBuildOptions {
    device_id: i32,
    precision: i32,
    workspace_bytes: u64,
    profiles: *const NativeShapeProfile,
    profile_count: usize,
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
pub(crate) struct NativeTensorInfo {
    name: *const c_char,
    dtype: i32,
    shape: *const i64,
    rank: usize,
    data: *const c_void,
    byte_len: usize,
}

type AbiVersion = unsafe extern "C" fn() -> u32;
type LastError = unsafe extern "C" fn() -> *const c_char;
type RuntimeVersion = unsafe extern "C" fn() -> *const c_char;
type RuntimeId = unsafe extern "C" fn() -> *const c_char;
type DeviceFingerprint = unsafe extern "C" fn(i32) -> *const c_char;
type DeviceMemory = unsafe extern "C" fn(i32) -> u64;
type BuildEngine =
    unsafe extern "C" fn(*const c_char, *const NativeBuildOptions) -> *mut TensorRtBufferRaw;
type BufferData = unsafe extern "C" fn(*const TensorRtBufferRaw) -> *const u8;
type BufferSize = unsafe extern "C" fn(*const TensorRtBufferRaw) -> usize;
type BufferDestroy = unsafe extern "C" fn(*mut TensorRtBufferRaw);
type SessionLoad = unsafe extern "C" fn(*const u8, usize, i32) -> *mut TensorRtSessionRaw;
type SessionDestroy = unsafe extern "C" fn(*mut TensorRtSessionRaw);
type TensorCount = unsafe extern "C" fn(*const TensorRtSessionRaw, i32) -> usize;
type TensorInfo =
    unsafe extern "C" fn(*const TensorRtSessionRaw, i32, usize, *mut NativeTensorInfo) -> i32;
type SessionRun = unsafe extern "C" fn(
    *mut TensorRtSessionRaw,
    *const NativeTensorView,
    usize,
) -> *mut TensorRtOutputsRaw;
type OutputsDestroy = unsafe extern "C" fn(*mut TensorRtOutputsRaw);
type OutputsCount = unsafe extern "C" fn(*const TensorRtOutputsRaw) -> usize;
type OutputInfo =
    unsafe extern "C" fn(*const TensorRtOutputsRaw, usize, *mut NativeTensorInfo) -> i32;

pub(crate) struct TensorRtApi {
    _library: Library,
    last_error: LastError,
    runtime_version: RuntimeVersion,
    device_fingerprint: DeviceFingerprint,
    device_memory: DeviceMemory,
    build_engine: BuildEngine,
    buffer_data: BufferData,
    buffer_size: BufferSize,
    buffer_destroy: BufferDestroy,
    session_load: SessionLoad,
    session_destroy: SessionDestroy,
    tensor_count: TensorCount,
    tensor_info: TensorInfo,
    session_run: SessionRun,
    outputs_destroy: OutputsDestroy,
    outputs_count: OutputsCount,
    output_info: OutputInfo,
}

impl fmt::Debug for TensorRtApi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TensorRtApi")
            .field("version", &self.version())
            .finish_non_exhaustive()
    }
}

impl TensorRtApi {
    pub(crate) fn load(path: &Path, flavor: TensorRtFlavor) -> TensorRtResult<Self> {
        // SAFETY: The caller only supplies a trusted, separately installed
        // bridge and TensorRtApi owns the library for all symbol lifetimes.
        let library = unsafe { open_library(path) }
            .map_err(|error| tensorrt_error(format!("failed to load native bridge: {error}")))?;
        // SAFETY: Every symbol signature is defined by native/tensorrt_bridge.h.
        unsafe { Self::from_library(library, flavor) }
    }

    unsafe fn from_library(
        library: Library,
        expected_flavor: TensorRtFlavor,
    ) -> TensorRtResult<Self> {
        macro_rules! required {
            ($name:literal, $type:ty) => {
                load_symbol::<$type>(&library, concat!($name, "\0").as_bytes())?
            };
        }
        let abi_version = required!("ls_trt_abi_version", AbiVersion);
        // SAFETY: The symbol has the declared no-argument C signature.
        let actual = unsafe { abi_version() };
        if actual != TENSORRT_BRIDGE_ABI_VERSION {
            return Err(tensorrt_error(format!(
                "unsupported TensorRT bridge ABI {actual}; expected {TENSORRT_BRIDGE_ABI_VERSION}"
            )));
        }
        let runtime_id = required!("ls_trt_runtime_id", RuntimeId);
        // SAFETY: The bridge returns a borrowed, null-terminated static string.
        let actual_runtime = c_string(unsafe { runtime_id() })
            .ok_or_else(|| tensorrt_error("TensorRT bridge returned a null runtime id"))?;
        if actual_runtime != expected_flavor.runtime_id() {
            return Err(tensorrt_error(format!(
                "runtime bridge kind mismatch: expected '{}', got '{actual_runtime}'",
                expected_flavor.runtime_id()
            )));
        }
        Ok(Self {
            last_error: required!("ls_trt_last_error", LastError),
            runtime_version: required!("ls_trt_runtime_version", RuntimeVersion),
            device_fingerprint: required!("ls_trt_device_fingerprint", DeviceFingerprint),
            device_memory: required!("ls_trt_device_memory", DeviceMemory),
            build_engine: required!("ls_trt_build_engine", BuildEngine),
            buffer_data: required!("ls_trt_buffer_data", BufferData),
            buffer_size: required!("ls_trt_buffer_size", BufferSize),
            buffer_destroy: required!("ls_trt_buffer_destroy", BufferDestroy),
            session_load: required!("ls_trt_session_load", SessionLoad),
            session_destroy: required!("ls_trt_session_destroy", SessionDestroy),
            tensor_count: required!("ls_trt_tensor_count", TensorCount),
            tensor_info: required!("ls_trt_tensor_info", TensorInfo),
            session_run: required!("ls_trt_session_run", SessionRun),
            outputs_destroy: required!("ls_trt_outputs_destroy", OutputsDestroy),
            outputs_count: required!("ls_trt_outputs_count", OutputsCount),
            output_info: required!("ls_trt_output_info", OutputInfo),
            _library: library,
        })
    }

    pub(crate) fn version(&self) -> Option<String> {
        // SAFETY: The bridge returns a borrowed, null-terminated static string.
        c_string(unsafe { (self.runtime_version)() })
    }

    pub(crate) fn device_info(&self, device_id: i32) -> TensorRtResult<(String, u64)> {
        // SAFETY: The bridge accepts every i32 and reports invalid devices by
        // returning null and setting its thread-local error.
        let fingerprint = c_string(unsafe { (self.device_fingerprint)(device_id) })
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                tensorrt_error(format!(
                    "CUDA device {device_id} is unavailable: {}",
                    self.last_error_message()
                ))
            })?;
        // SAFETY: The device was validated by the preceding bridge call.
        let memory = unsafe { (self.device_memory)(device_id) };
        Ok((fingerprint, memory))
    }

    pub(crate) fn build_engine(
        &self,
        onnx_path: &Path,
        options: &TensorRtOptions,
    ) -> TensorRtResult<Vec<u8>> {
        let path = CString::new(onnx_path.to_string_lossy().as_bytes()).map_err(|_| {
            tensorrt_error(format!(
                "ONNX path contains an interior NUL byte: {}",
                onnx_path.display()
            ))
        })?;
        let prepared = PreparedProfiles::new(options)?;
        let native_options = NativeBuildOptions {
            device_id: options.device_id,
            precision: options.precision.native_value(),
            workspace_bytes: options.workspace_bytes,
            profiles: prepared.views.as_ptr(),
            profile_count: prepared.views.len(),
        };
        // SAFETY: Path, options, profile names, and shape arrays remain live for
        // the synchronous build call.
        let raw = unsafe { (self.build_engine)(path.as_ptr(), &native_options) };
        let raw = NonNull::new(raw).ok_or_else(|| {
            tensorrt_error(format!(
                "engine build failed: {}",
                self.last_error_message()
            ))
        })?;
        let buffer = NativeEngineBuffer { api: self, raw };
        buffer.copy()
    }

    fn ensure_success(&self, status: i32, operation: &str) -> TensorRtResult<()> {
        if status != 0 {
            return Ok(());
        }
        Err(tensorrt_error(format!(
            "{operation} failed: {}",
            self.last_error_message()
        )))
    }

    fn last_error_message(&self) -> String {
        // SAFETY: The function returns a borrowed thread-local C string.
        c_string(unsafe { (self.last_error)() })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "native bridge returned no error detail".to_owned())
    }
}

struct PreparedProfiles {
    _names: Vec<CString>,
    _min_shapes: Vec<Vec<i64>>,
    _opt_shapes: Vec<Vec<i64>>,
    _max_shapes: Vec<Vec<i64>>,
    views: Vec<NativeShapeProfile>,
}

impl PreparedProfiles {
    fn new(options: &TensorRtOptions) -> TensorRtResult<Self> {
        let names = options
            .profiles
            .keys()
            .map(|name| {
                CString::new(name.as_bytes()).map_err(|_| {
                    tensorrt_error(format!("profile input name contains a NUL byte: {name:?}"))
                })
            })
            .collect::<TensorRtResult<Vec<_>>>()?;
        let min_shapes: Vec<_> = options
            .profiles
            .values()
            .map(|profile| profile.min.clone())
            .collect();
        let opt_shapes: Vec<_> = options
            .profiles
            .values()
            .map(|profile| profile.opt.clone())
            .collect();
        let max_shapes: Vec<_> = options
            .profiles
            .values()
            .map(|profile| profile.max.clone())
            .collect();
        let views = names
            .iter()
            .zip(&min_shapes)
            .zip(&opt_shapes)
            .zip(&max_shapes)
            .map(|(((name, min), opt), max)| NativeShapeProfile {
                input_name: name.as_ptr(),
                min_shape: min.as_ptr(),
                opt_shape: opt.as_ptr(),
                max_shape: max.as_ptr(),
                rank: min.len(),
            })
            .collect();
        Ok(Self {
            _names: names,
            _min_shapes: min_shapes,
            _opt_shapes: opt_shapes,
            _max_shapes: max_shapes,
            views,
        })
    }
}

struct NativeEngineBuffer<'a> {
    api: &'a TensorRtApi,
    raw: NonNull<TensorRtBufferRaw>,
}

impl NativeEngineBuffer<'_> {
    fn copy(&self) -> TensorRtResult<Vec<u8>> {
        // SAFETY: The native buffer remains alive for both metadata calls and copy.
        let size = unsafe { (self.api.buffer_size)(self.raw.as_ptr()) };
        let data = unsafe { (self.api.buffer_data)(self.raw.as_ptr()) };
        if size == 0 || data.is_null() {
            return Err(tensorrt_error(
                "native engine builder returned an empty buffer",
            ));
        }
        // SAFETY: The bridge guarantees `data` addresses at least `size` bytes.
        Ok(unsafe { std::slice::from_raw_parts(data, size) }.to_vec())
    }
}

impl Drop for NativeEngineBuffer<'_> {
    fn drop(&mut self) {
        // SAFETY: This wrapper uniquely owns the bridge buffer.
        unsafe { (self.api.buffer_destroy)(self.raw.as_ptr()) };
    }
}

pub(crate) struct TensorRtProgram {
    api: Arc<TensorRtApi>,
    raw: NonNull<TensorRtSessionRaw>,
}

// SAFETY: All mutable execution-context access is serialized by TensorRtSession.
unsafe impl Send for TensorRtProgram {}

impl TensorRtProgram {
    pub(crate) fn load(
        api: Arc<TensorRtApi>,
        engine: &[u8],
        device_id: i32,
    ) -> TensorRtResult<Self> {
        if engine.is_empty() {
            return Err(tensorrt_error("cannot load an empty TensorRT engine"));
        }
        // SAFETY: Engine bytes remain live for the synchronous deserialization call.
        let raw = unsafe { (api.session_load)(engine.as_ptr(), engine.len(), device_id) };
        let raw = NonNull::new(raw).ok_or_else(|| {
            tensorrt_error(format!(
                "engine deserialization failed: {}",
                api.last_error_message()
            ))
        })?;
        Ok(Self { api, raw })
    }

    pub(crate) fn tensor_specs(&self, direction: i32) -> TensorRtResult<Vec<SessionTensorSpec>> {
        // SAFETY: Session is live and metadata access is read-only.
        let count = unsafe { (self.api.tensor_count)(self.raw.as_ptr(), direction) };
        (0..count)
            .map(|index| {
                let mut info = empty_tensor_info();
                // SAFETY: Output pointer is valid and the bridge initializes it on success.
                let status = unsafe {
                    (self.api.tensor_info)(self.raw.as_ptr(), direction, index, &mut info)
                };
                self.api.ensure_success(status, "query tensor metadata")?;
                tensor_spec(info)
            })
            .collect()
    }

    pub(crate) fn run(&mut self, inputs: &[NativeTensorView]) -> TensorRtResult<NativeOutputs> {
        // SAFETY: Views and all referenced buffers remain live for the synchronous call.
        let raw =
            unsafe { (self.api.session_run)(self.raw.as_ptr(), inputs.as_ptr(), inputs.len()) };
        let raw = NonNull::new(raw).ok_or_else(|| {
            tensorrt_error(format!(
                "inference failed: {}",
                self.api.last_error_message()
            ))
        })?;
        Ok(NativeOutputs {
            api: Arc::clone(&self.api),
            raw,
        })
    }
}

impl Drop for TensorRtProgram {
    fn drop(&mut self) {
        // SAFETY: This wrapper uniquely owns the native session.
        unsafe { (self.api.session_destroy)(self.raw.as_ptr()) };
    }
}

pub(crate) struct NativeOutputs {
    api: Arc<TensorRtApi>,
    raw: NonNull<TensorRtOutputsRaw>,
}

impl NativeOutputs {
    pub(crate) fn count(&self) -> usize {
        // SAFETY: Outputs handle is live.
        unsafe { (self.api.outputs_count)(self.raw.as_ptr()) }
    }

    pub(crate) fn info(&self, index: usize) -> TensorRtResult<NativeTensorInfo> {
        let mut info = empty_tensor_info();
        // SAFETY: Output pointer is valid and bridge initializes it on success.
        let status = unsafe { (self.api.output_info)(self.raw.as_ptr(), index, &mut info) };
        self.api.ensure_success(status, "query inference output")?;
        Ok(info)
    }
}

impl Drop for NativeOutputs {
    fn drop(&mut self) {
        // SAFETY: This wrapper uniquely owns the outputs handle.
        unsafe { (self.api.outputs_destroy)(self.raw.as_ptr()) };
    }
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

fn tensor_spec(info: NativeTensorInfo) -> TensorRtResult<SessionTensorSpec> {
    let name = c_string(info.name).ok_or_else(|| tensorrt_error("tensor name is null"))?;
    let shape = copy_shape(info.shape, info.rank)?
        .into_iter()
        .map(|dimension| (dimension >= 0).then_some(dimension))
        .collect();
    Ok(SessionTensorSpec {
        name,
        shape,
        dtype: dtype_name(info.dtype)?.to_owned(),
    })
}

pub(crate) fn output_parts(
    info: NativeTensorInfo,
) -> TensorRtResult<(i32, Vec<usize>, *const c_void, usize)> {
    let shape = copy_shape(info.shape, info.rank)?
        .into_iter()
        .map(|dimension| {
            usize::try_from(dimension)
                .map_err(|_| tensorrt_error(format!("output has invalid dimension {dimension}")))
        })
        .collect::<TensorRtResult<Vec<_>>>()?;
    if info.byte_len > 0 && info.data.is_null() {
        return Err(tensorrt_error("output data pointer is null"));
    }
    Ok((info.dtype, shape, info.data, info.byte_len))
}

fn copy_shape(pointer: *const i64, rank: usize) -> TensorRtResult<Vec<i64>> {
    if rank == 0 {
        return Ok(Vec::new());
    }
    if pointer.is_null() {
        return Err(tensorrt_error("tensor shape pointer is null"));
    }
    // SAFETY: The bridge guarantees rank elements for a non-null shape pointer.
    Ok(unsafe { std::slice::from_raw_parts(pointer, rank) }.to_vec())
}

fn dtype_name(dtype: i32) -> TensorRtResult<&'static str> {
    match dtype {
        TRT_DATA_FLOAT32 => Ok("f32"),
        TRT_DATA_FLOAT16 => Ok("f16"),
        TRT_DATA_INT64 => Ok("i64"),
        TRT_DATA_INT32 => Ok("i32"),
        TRT_DATA_UINT8 => Ok("u8"),
        TRT_DATA_BOOL => Ok("bool"),
        _ => Err(tensorrt_error(format!(
            "native bridge returned unsupported dtype {dtype}"
        ))),
    }
}

unsafe fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> TensorRtResult<T> {
    // SAFETY: Caller supplies the exact signature from tensorrt_bridge.h.
    unsafe { library.get::<T>(name) }
        .map(|symbol| *symbol)
        .map_err(|error| {
            tensorrt_error(format!(
                "required bridge symbol '{}' is missing: {error}",
                String::from_utf8_lossy(name).trim_end_matches('\0')
            ))
        })
}

#[cfg(target_os = "windows")]
unsafe fn open_library(path: &Path) -> Result<Library, libloading::Error> {
    use libloading::os::windows::{
        Library as WindowsLibrary, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    };
    if path.components().count() > 1 {
        // SAFETY: Same contract as Library::new; packaged dependencies are
        // resolved beside the selected bridge.
        unsafe {
            WindowsLibrary::load_with_flags(
                path,
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        }
        .map(Into::into)
    } else {
        // SAFETY: Same caller contract as this function.
        unsafe { Library::new(path) }
    }
}

#[cfg(not(target_os = "windows"))]
unsafe fn open_library(path: &Path) -> Result<Library, libloading::Error> {
    // SAFETY: Same caller contract as this function.
    unsafe { Library::new(path) }
}

fn c_string(pointer: *const c_char) -> Option<String> {
    (!pointer.is_null()).then(|| {
        // SAFETY: Bridge strings are null-terminated and copied immediately.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    })
}
