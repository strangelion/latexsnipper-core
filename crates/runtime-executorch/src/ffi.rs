//! Safe ownership wrappers over the versioned ExecuTorch C bridge.

use std::ffi::{c_char, c_void, CStr, CString};
use std::fmt;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;

use latexsnipper_runtime::SessionTensorSpec;
use libloading::Library;

use crate::error::{executorch_error, ExecuTorchResult};

pub(crate) const ET_DATA_FLOAT32: i32 = 0;
pub(crate) const ET_DATA_FLOAT16: i32 = 1;
pub(crate) const ET_DATA_INT64: i32 = 2;
pub(crate) const ET_DATA_INT32: i32 = 3;
pub(crate) const ET_DATA_UINT8: i32 = 4;
pub(crate) const ET_DATA_BOOL: i32 = 5;

const EXECUTORCH_BRIDGE_ABI_VERSION: u32 = 1;

#[repr(C)]
struct ExecuTorchSessionRaw {
    _private: [u8; 0],
}

#[repr(C)]
struct ExecuTorchOutputsRaw {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeTensorView {
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

type AbiVersion = unsafe extern "C" fn() -> u32;
type LastError = unsafe extern "C" fn() -> *const c_char;
type RuntimeVersion = unsafe extern "C" fn() -> *const c_char;
type SessionLoad = unsafe extern "C" fn(*const c_char) -> *mut ExecuTorchSessionRaw;
type SessionDestroy = unsafe extern "C" fn(*mut ExecuTorchSessionRaw);
type MethodCount = unsafe extern "C" fn(*const ExecuTorchSessionRaw) -> usize;
type MethodName = unsafe extern "C" fn(*const ExecuTorchSessionRaw, usize) -> *const c_char;
type TensorCount = unsafe extern "C" fn(*const ExecuTorchSessionRaw, *const c_char, i32) -> usize;
type TensorInfo = unsafe extern "C" fn(
    *const ExecuTorchSessionRaw,
    *const c_char,
    i32,
    usize,
    *mut NativeTensorInfo,
) -> i32;
type SessionRun = unsafe extern "C" fn(
    *mut ExecuTorchSessionRaw,
    *const c_char,
    *const NativeTensorView,
    usize,
) -> *mut ExecuTorchOutputsRaw;
type OutputsDestroy = unsafe extern "C" fn(*mut ExecuTorchOutputsRaw);
type OutputsCount = unsafe extern "C" fn(*const ExecuTorchOutputsRaw) -> usize;
type OutputInfo =
    unsafe extern "C" fn(*const ExecuTorchOutputsRaw, usize, *mut NativeTensorInfo) -> i32;

pub(crate) struct ExecuTorchApi {
    _library: Library,
    last_error: LastError,
    runtime_version: RuntimeVersion,
    session_load: SessionLoad,
    session_destroy: SessionDestroy,
    method_count: MethodCount,
    method_name: MethodName,
    tensor_count: TensorCount,
    tensor_info: TensorInfo,
    session_run: SessionRun,
    outputs_destroy: OutputsDestroy,
    outputs_count: OutputsCount,
    output_info: OutputInfo,
}

impl fmt::Debug for ExecuTorchApi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecuTorchApi")
            .field("version", &self.version())
            .finish_non_exhaustive()
    }
}

impl ExecuTorchApi {
    pub(crate) fn load(path: &Path) -> ExecuTorchResult<Self> {
        // SAFETY: Loading is limited to a trusted runtime installation and the
        // returned Library remains owned by ExecuTorchApi.
        let library = unsafe { open_library(path) }
            .map_err(|error| executorch_error(format!("failed to load native library: {error}")))?;
        // SAFETY: Symbol signatures exactly match native/executorch_bridge.h.
        unsafe { Self::from_library(library) }
    }

    unsafe fn from_library(library: Library) -> ExecuTorchResult<Self> {
        macro_rules! required {
            ($name:literal, $type:ty) => {
                load_symbol::<$type>(&library, concat!($name, "\0").as_bytes())?
            };
        }

        let abi_version = required!("ls_et_abi_version", AbiVersion);
        // SAFETY: Symbol was loaded with its exact no-argument signature.
        let actual_version = unsafe { abi_version() };
        if actual_version != EXECUTORCH_BRIDGE_ABI_VERSION {
            return Err(executorch_error(format!(
                "unsupported ExecuTorch bridge ABI {actual_version}; expected {EXECUTORCH_BRIDGE_ABI_VERSION}"
            )));
        }

        Ok(Self {
            last_error: required!("ls_et_last_error", LastError),
            runtime_version: required!("ls_et_runtime_version", RuntimeVersion),
            session_load: required!("ls_et_session_load", SessionLoad),
            session_destroy: required!("ls_et_session_destroy", SessionDestroy),
            method_count: required!("ls_et_method_count", MethodCount),
            method_name: required!("ls_et_method_name", MethodName),
            tensor_count: required!("ls_et_tensor_count", TensorCount),
            tensor_info: required!("ls_et_tensor_info", TensorInfo),
            session_run: required!("ls_et_session_run", SessionRun),
            outputs_destroy: required!("ls_et_outputs_destroy", OutputsDestroy),
            outputs_count: required!("ls_et_outputs_count", OutputsCount),
            output_info: required!("ls_et_output_info", OutputInfo),
            _library: library,
        })
    }

    pub(crate) fn version(&self) -> Option<String> {
        // SAFETY: Bridge returns a borrowed, null-terminated static string.
        let pointer = unsafe { (self.runtime_version)() };
        c_string(pointer)
    }

    fn ensure_success(&self, status: i32, operation: &str) -> ExecuTorchResult<()> {
        if status != 0 {
            return Ok(());
        }
        Err(executorch_error(format!(
            "{operation} failed: {}",
            self.last_error_message()
        )))
    }

    fn last_error_message(&self) -> String {
        // SAFETY: Function has no arguments and returns a borrowed C string.
        c_string(unsafe { (self.last_error)() })
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "native bridge returned no error detail".to_owned())
    }
}

unsafe fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> ExecuTorchResult<T> {
    // SAFETY: Caller supplies the exact signature from executorch_bridge.h.
    unsafe { library.get::<T>(name) }
        .map(|symbol| *symbol)
        .map_err(|error| {
            executorch_error(format!(
                "required ExecuTorch bridge symbol '{}' is missing: {error}",
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
        // SAFETY: Same contract as Library::new; DLL_LOAD_DIR resolves any
        // packaged native dependencies beside the selected bridge.
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
        // SAFETY: Non-null bridge strings are guaranteed to be null-terminated
        // and remain valid through this immediate copy.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    })
}

#[derive(Debug, Clone)]
pub(crate) struct NativeMethodMetadata {
    pub name: String,
    pub inputs: Vec<SessionTensorSpec>,
    pub outputs: Vec<SessionTensorSpec>,
}

pub(crate) struct ExecuTorchProgram {
    api: Arc<ExecuTorchApi>,
    raw: NonNull<ExecuTorchSessionRaw>,
}

// SAFETY: The Module behind this handle is only accessed while the owning
// ExecuTorchSession mutex is held.
unsafe impl Send for ExecuTorchProgram {}

impl ExecuTorchProgram {
    pub(crate) fn load(api: Arc<ExecuTorchApi>, path: &Path) -> ExecuTorchResult<Self> {
        let path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
            executorch_error(format!(
                "program path contains an interior NUL byte: {}",
                path.display()
            ))
        })?;
        // SAFETY: Path is a live null-terminated string for the synchronous call.
        let pointer = unsafe { (api.session_load)(path.as_ptr()) };
        let raw = NonNull::new(pointer).ok_or_else(|| {
            executorch_error(format!(
                "load ExecuTorch program failed: {}",
                api.last_error_message()
            ))
        })?;
        Ok(Self { api, raw })
    }

    pub(crate) fn methods(&self) -> ExecuTorchResult<Vec<NativeMethodMetadata>> {
        // SAFETY: Session is live and metadata access is read-only.
        let count = unsafe { (self.api.method_count)(self.raw.as_ptr()) };
        (0..count)
            .map(|index| {
                // SAFETY: Index is within the count returned above.
                let pointer = unsafe { (self.api.method_name)(self.raw.as_ptr(), index) };
                let name = c_string(pointer).ok_or_else(|| {
                    executorch_error(format!(
                        "get ExecuTorch method name failed: {}",
                        self.api.last_error_message()
                    ))
                })?;
                Ok(NativeMethodMetadata {
                    inputs: self.tensor_specs(&name, true)?,
                    outputs: self.tensor_specs(&name, false)?,
                    name,
                })
            })
            .collect()
    }

    fn tensor_specs(&self, method: &str, input: bool) -> ExecuTorchResult<Vec<SessionTensorSpec>> {
        let method = CString::new(method)
            .map_err(|_| executorch_error("method name contains an interior NUL byte"))?;
        let direction = i32::from(input);
        // SAFETY: Session and method string are live for this metadata call.
        let count =
            unsafe { (self.api.tensor_count)(self.raw.as_ptr(), method.as_ptr(), direction) };
        (0..count)
            .map(|index| {
                let mut info = empty_tensor_info();
                // SAFETY: Output pointer is valid and index is within count.
                let status = unsafe {
                    (self.api.tensor_info)(
                        self.raw.as_ptr(),
                        method.as_ptr(),
                        direction,
                        index,
                        &mut info,
                    )
                };
                self.api
                    .ensure_success(status, "query ExecuTorch tensor metadata")?;
                tensor_spec_from_info(info, input, index)
            })
            .collect()
    }

    pub(crate) fn run(
        &mut self,
        method: &str,
        inputs: &[NativeTensorView],
    ) -> ExecuTorchResult<ExecuTorchOutputs> {
        let method = CString::new(method)
            .map_err(|_| executorch_error("method name contains an interior NUL byte"))?;
        // SAFETY: All views and their backing buffers remain live for the
        // synchronous native call. Access is serialized by the session mutex.
        let pointer = unsafe {
            (self.api.session_run)(
                self.raw.as_ptr(),
                method.as_ptr(),
                inputs.as_ptr(),
                inputs.len(),
            )
        };
        let raw = NonNull::new(pointer).ok_or_else(|| {
            executorch_error(format!(
                "execute method failed: {}",
                self.api.last_error_message()
            ))
        })?;
        Ok(ExecuTorchOutputs {
            api: Arc::clone(&self.api),
            raw,
        })
    }
}

impl Drop for ExecuTorchProgram {
    fn drop(&mut self) {
        // SAFETY: Native session is owned and destroyed exactly once.
        unsafe { (self.api.session_destroy)(self.raw.as_ptr()) };
    }
}

pub(crate) struct ExecuTorchOutputs {
    api: Arc<ExecuTorchApi>,
    raw: NonNull<ExecuTorchOutputsRaw>,
}

impl ExecuTorchOutputs {
    pub(crate) fn count(&self) -> usize {
        // SAFETY: Output list is live and access is read-only.
        unsafe { (self.api.outputs_count)(self.raw.as_ptr()) }
    }

    pub(crate) fn info(&self, index: usize) -> ExecuTorchResult<NativeOutput<'_>> {
        let mut info = empty_tensor_info();
        // SAFETY: Output pointer is live; bridge validates the index.
        let status = unsafe { (self.api.output_info)(self.raw.as_ptr(), index, &mut info) };
        self.api.ensure_success(status, "query ExecuTorch output")?;
        let shape = copy_shape(info.shape, info.rank)?;
        let bytes = if info.byte_len == 0 {
            &[]
        } else {
            if info.data.is_null() {
                return Err(executorch_error(format!(
                    "ExecuTorch output at index {index} has null data with byte length {}",
                    info.byte_len
                )));
            }
            // SAFETY: Output list owns this buffer through the returned borrow.
            unsafe { std::slice::from_raw_parts(info.data.cast::<u8>(), info.byte_len) }
        };
        Ok(NativeOutput {
            dtype: info.dtype,
            shape,
            bytes,
        })
    }
}

impl Drop for ExecuTorchOutputs {
    fn drop(&mut self) {
        // SAFETY: Output list is owned and destroyed exactly once.
        unsafe { (self.api.outputs_destroy)(self.raw.as_ptr()) };
    }
}

pub(crate) struct NativeOutput<'outputs> {
    pub dtype: i32,
    pub shape: Vec<i64>,
    pub bytes: &'outputs [u8],
}

fn tensor_spec_from_info(
    info: NativeTensorInfo,
    input: bool,
    index: usize,
) -> ExecuTorchResult<SessionTensorSpec> {
    let prefix = if input { "input" } else { "output" };
    let name = c_string(info.name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("{prefix}_{index}"));
    Ok(SessionTensorSpec {
        name,
        shape: copy_shape(info.shape, info.rank)?
            .into_iter()
            .map(|dimension| (dimension >= 0).then_some(dimension))
            .collect(),
        dtype: dtype_name(info.dtype).to_owned(),
    })
}

fn copy_shape(pointer: *const i64, rank: usize) -> ExecuTorchResult<Vec<i64>> {
    if rank == 0 {
        return Ok(Vec::new());
    }
    if pointer.is_null() {
        return Err(executorch_error("native tensor shape pointer is null"));
    }
    // SAFETY: Bridge guarantees `rank` live dimensions for a non-null pointer.
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

pub(crate) const fn dtype_name(dtype: i32) -> &'static str {
    match dtype {
        ET_DATA_FLOAT32 => "f32",
        ET_DATA_FLOAT16 => "f16",
        ET_DATA_INT64 => "i64",
        ET_DATA_INT32 => "i32",
        ET_DATA_UINT8 => "u8",
        ET_DATA_BOOL => "bool",
        _ => "unknown",
    }
}
