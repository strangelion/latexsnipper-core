//! Safe ownership wrappers over the versioned LaTeXSnipper Paddle C bridge.

use std::ffi::{c_char, c_void, CStr, CString};
use std::fmt;
use std::marker::PhantomData;
use std::mem::size_of_val;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;

use libloading::Library;

use crate::error::{paddle_error, PaddleResult};
use crate::options::PaddleOptions;

pub(crate) const PADDLE_DATA_FLOAT32: i32 = 0;
pub(crate) const PADDLE_DATA_FLOAT16: i32 = 1;
pub(crate) const PADDLE_DATA_INT64: i32 = 2;
pub(crate) const PADDLE_DATA_INT32: i32 = 3;
pub(crate) const PADDLE_DATA_UINT8: i32 = 4;
pub(crate) const PADDLE_DATA_BOOL: i32 = 5;

const PADDLE_BRIDGE_ABI_VERSION: u32 = 1;
const INVALID_DIMENSION: i64 = i64::MIN;

#[repr(C)]
struct PaddleConfigRaw {
    _private: [u8; 0],
}

#[repr(C)]
struct PaddlePredictorRaw {
    _private: [u8; 0],
}

#[repr(C)]
struct PaddleTensorRaw {
    _private: [u8; 0],
}

type AbiVersion = unsafe extern "C" fn() -> u32;
type LastError = unsafe extern "C" fn() -> *const c_char;
type RuntimeVersion = unsafe extern "C" fn() -> *const c_char;
type ConfigCreate = unsafe extern "C" fn() -> *mut PaddleConfigRaw;
type ConfigDestroy = unsafe extern "C" fn(*mut PaddleConfigRaw);
type ConfigSetModel =
    unsafe extern "C" fn(*mut PaddleConfigRaw, *const c_char, *const c_char) -> i32;
type ConfigSetThreads = unsafe extern "C" fn(*mut PaddleConfigRaw, i32) -> i32;
type ConfigSetBool = unsafe extern "C" fn(*mut PaddleConfigRaw, i32) -> i32;
type ConfigEnableGpu = unsafe extern "C" fn(*mut PaddleConfigRaw, u64, i32) -> i32;
type PredictorCreate = unsafe extern "C" fn(*const PaddleConfigRaw) -> *mut PaddlePredictorRaw;
type PredictorDestroy = unsafe extern "C" fn(*mut PaddlePredictorRaw);
type PredictorCount = unsafe extern "C" fn(*const PaddlePredictorRaw) -> usize;
type PredictorName = unsafe extern "C" fn(*const PaddlePredictorRaw, usize) -> *const c_char;
type PredictorGetHandle =
    unsafe extern "C" fn(*mut PaddlePredictorRaw, *const c_char) -> *mut PaddleTensorRaw;
type PredictorRun = unsafe extern "C" fn(*mut PaddlePredictorRaw) -> i32;
type TensorDestroy = unsafe extern "C" fn(*mut PaddleTensorRaw);
type TensorReshape = unsafe extern "C" fn(*mut PaddleTensorRaw, *const i64, usize) -> i32;
type TensorRank = unsafe extern "C" fn(*const PaddleTensorRaw) -> usize;
type TensorDimension = unsafe extern "C" fn(*const PaddleTensorRaw, usize) -> i64;
type TensorDtype = unsafe extern "C" fn(*const PaddleTensorRaw) -> i32;
type TensorCopyFrom = unsafe extern "C" fn(*mut PaddleTensorRaw, i32, *const c_void, usize) -> i32;
type TensorCopyTo = unsafe extern "C" fn(*const PaddleTensorRaw, i32, *mut c_void, usize) -> i32;

/// Resolved bridge symbols. Owning the library keeps every function pointer
/// alive until all configs, predictors, and tensor handles have been dropped.
pub struct PaddleApi {
    _library: Library,
    _dependencies: Vec<Library>,
    last_error: LastError,
    runtime_version: RuntimeVersion,
    config_create: ConfigCreate,
    config_destroy: ConfigDestroy,
    config_set_model: ConfigSetModel,
    config_set_threads: ConfigSetThreads,
    config_set_ir_optim: ConfigSetBool,
    config_set_memory_optim: ConfigSetBool,
    config_enable_gpu: ConfigEnableGpu,
    predictor_create: PredictorCreate,
    predictor_destroy: PredictorDestroy,
    predictor_input_count: PredictorCount,
    predictor_output_count: PredictorCount,
    predictor_input_name: PredictorName,
    predictor_output_name: PredictorName,
    predictor_get_input: PredictorGetHandle,
    predictor_get_output: PredictorGetHandle,
    predictor_run: PredictorRun,
    tensor_destroy: TensorDestroy,
    tensor_reshape: TensorReshape,
    tensor_rank: TensorRank,
    tensor_dimension: TensorDimension,
    tensor_dtype: TensorDtype,
    tensor_copy_from: TensorCopyFrom,
    tensor_copy_to: TensorCopyTo,
}

impl fmt::Debug for PaddleApi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaddleApi")
            .field("version", &self.version())
            .finish_non_exhaustive()
    }
}

impl PaddleApi {
    pub(crate) fn load(path: &Path) -> PaddleResult<Self> {
        // Paddle dynamically opens MKL/oneDNN after the bridge has loaded. On
        // Windows those bare-name loads do not inherit DLL_LOAD_DIR, so retain
        // explicitly preloaded packaged dependencies for the API lifetime.
        // SAFETY: Every candidate is an absolute sibling path of the trusted
        // bridge and is retained in PaddleApi.
        let dependencies = unsafe { open_runtime_dependencies(path) }?;
        // SAFETY: Native runtime loading is limited to the trusted runtime
        // registry. The resulting Library is retained by PaddleApi.
        let library = unsafe { open_library(path) }
            .map_err(|error| paddle_error(format!("failed to load native library: {error}")))?;
        // SAFETY: Symbol types match native/paddle_bridge.h exactly.
        unsafe { Self::from_library(library, dependencies) }
    }

    unsafe fn from_library(library: Library, dependencies: Vec<Library>) -> PaddleResult<Self> {
        macro_rules! required {
            ($name:literal, $type:ty) => {
                load_symbol::<$type>(&library, concat!($name, "\0").as_bytes())?
            };
        }

        let abi_version = required!("ls_paddle_abi_version", AbiVersion);
        // SAFETY: The symbol is resolved with its declared no-argument ABI.
        let actual_version = unsafe { abi_version() };
        if actual_version != PADDLE_BRIDGE_ABI_VERSION {
            return Err(paddle_error(format!(
                "unsupported Paddle bridge ABI {actual_version}; expected {PADDLE_BRIDGE_ABI_VERSION}"
            )));
        }

        Ok(Self {
            last_error: required!("ls_paddle_last_error", LastError),
            runtime_version: required!("ls_paddle_runtime_version", RuntimeVersion),
            config_create: required!("ls_paddle_config_create", ConfigCreate),
            config_destroy: required!("ls_paddle_config_destroy", ConfigDestroy),
            config_set_model: required!("ls_paddle_config_set_model", ConfigSetModel),
            config_set_threads: required!("ls_paddle_config_set_cpu_threads", ConfigSetThreads),
            config_set_ir_optim: required!("ls_paddle_config_set_ir_optim", ConfigSetBool),
            config_set_memory_optim: required!("ls_paddle_config_set_memory_optim", ConfigSetBool),
            config_enable_gpu: required!("ls_paddle_config_enable_gpu", ConfigEnableGpu),
            predictor_create: required!("ls_paddle_predictor_create", PredictorCreate),
            predictor_destroy: required!("ls_paddle_predictor_destroy", PredictorDestroy),
            predictor_input_count: required!("ls_paddle_predictor_input_count", PredictorCount),
            predictor_output_count: required!("ls_paddle_predictor_output_count", PredictorCount),
            predictor_input_name: required!("ls_paddle_predictor_input_name", PredictorName),
            predictor_output_name: required!("ls_paddle_predictor_output_name", PredictorName),
            predictor_get_input: required!("ls_paddle_predictor_input", PredictorGetHandle),
            predictor_get_output: required!("ls_paddle_predictor_output", PredictorGetHandle),
            predictor_run: required!("ls_paddle_predictor_run", PredictorRun),
            tensor_destroy: required!("ls_paddle_tensor_destroy", TensorDestroy),
            tensor_reshape: required!("ls_paddle_tensor_reshape", TensorReshape),
            tensor_rank: required!("ls_paddle_tensor_rank", TensorRank),
            tensor_dimension: required!("ls_paddle_tensor_dimension", TensorDimension),
            tensor_dtype: required!("ls_paddle_tensor_dtype", TensorDtype),
            tensor_copy_from: required!("ls_paddle_tensor_copy_from_cpu", TensorCopyFrom),
            tensor_copy_to: required!("ls_paddle_tensor_copy_to_cpu", TensorCopyTo),
            _dependencies: dependencies,
            _library: library,
        })
    }

    pub fn version(&self) -> Option<String> {
        // SAFETY: The bridge returns a thread-local, null-terminated string.
        let pointer = unsafe { (self.runtime_version)() };
        (!pointer.is_null()).then(|| {
            // SAFETY: The non-null bridge result is valid through this copy.
            unsafe { CStr::from_ptr(pointer) }
                .to_string_lossy()
                .into_owned()
        })
    }

    fn ensure_success(&self, status: i32, operation: &str) -> PaddleResult<()> {
        if status != 0 {
            return Ok(());
        }
        Err(paddle_error(format!(
            "{operation} failed: {}",
            self.last_error_message()
        )))
    }

    fn last_error_message(&self) -> String {
        // SAFETY: The function has no arguments and returns a borrowed C string.
        let pointer = unsafe { (self.last_error)() };
        if pointer.is_null() {
            return "native bridge returned no error detail".to_owned();
        }
        // SAFETY: The bridge guarantees null termination for a non-null result.
        let message = unsafe { CStr::from_ptr(pointer) }.to_string_lossy();
        if message.is_empty() {
            "native bridge returned no error detail".to_owned()
        } else {
            message.into_owned()
        }
    }
}

unsafe fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> PaddleResult<T> {
    // SAFETY: Caller supplies the exact signature from paddle_bridge.h.
    unsafe { library.get::<T>(name) }
        .map(|symbol| *symbol)
        .map_err(|error| {
            paddle_error(format!(
                "required Paddle bridge symbol '{}' is missing: {error}",
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
        // SAFETY: Same contract as Library::new. DLL_LOAD_DIR resolves Paddle
        // and its third-party dependencies packaged beside the bridge.
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

#[cfg(target_os = "windows")]
unsafe fn open_runtime_dependencies(path: &Path) -> PaddleResult<Vec<Library>> {
    let Some(directory) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(Vec::new());
    };
    let mut libraries = Vec::new();
    for name in ["libiomp5md.dll", "mklml.dll", "mkldnn.dll"] {
        let candidate = directory.join(name);
        if !candidate.is_file() {
            continue;
        }
        // SAFETY: Candidate belongs to the explicitly selected trusted runtime.
        let library = unsafe { Library::new(&candidate) }.map_err(|error| {
            paddle_error(format!(
                "failed to preload Paddle dependency {}: {error}",
                candidate.display()
            ))
        })?;
        libraries.push(library);
    }
    Ok(libraries)
}

#[cfg(not(target_os = "windows"))]
unsafe fn open_library(path: &Path) -> Result<Library, libloading::Error> {
    // SAFETY: Same caller contract as this function.
    unsafe { Library::new(path) }
}

#[cfg(not(target_os = "windows"))]
unsafe fn open_runtime_dependencies(_path: &Path) -> PaddleResult<Vec<Library>> {
    Ok(Vec::new())
}

pub(crate) struct PaddleConfig {
    api: Arc<PaddleApi>,
    raw: NonNull<PaddleConfigRaw>,
}

impl PaddleConfig {
    pub(crate) fn new(
        api: Arc<PaddleApi>,
        model_path: &Path,
        params_path: &Path,
        options: &PaddleOptions,
    ) -> PaddleResult<Self> {
        // SAFETY: Function pointer was verified during API construction.
        let raw = NonNull::new(unsafe { (api.config_create)() }).ok_or_else(|| {
            paddle_error(format!(
                "create config failed: {}",
                api.last_error_message()
            ))
        })?;
        let config = Self { api, raw };
        config.configure(model_path, params_path, options)?;
        Ok(config)
    }

    fn configure(
        &self,
        model_path: &Path,
        params_path: &Path,
        options: &PaddleOptions,
    ) -> PaddleResult<()> {
        let model = path_to_cstring(model_path)?;
        let params = path_to_cstring(params_path)?;
        // SAFETY: Config and both strings are live for the call.
        let status = unsafe {
            (self.api.config_set_model)(self.raw.as_ptr(), model.as_ptr(), params.as_ptr())
        };
        self.api.ensure_success(status, "set Paddle model")?;

        if options.cpu_threads > 0 {
            let threads = i32::try_from(options.cpu_threads)
                .map_err(|_| paddle_error("CPU thread count exceeds i32::MAX"))?;
            // SAFETY: Config is exclusively owned during construction.
            let status = unsafe { (self.api.config_set_threads)(self.raw.as_ptr(), threads) };
            self.api.ensure_success(status, "set Paddle CPU threads")?;
        }

        // SAFETY: Config is exclusively owned during construction.
        let status = unsafe {
            (self.api.config_set_ir_optim)(self.raw.as_ptr(), i32::from(options.enable_ir_optim))
        };
        self.api
            .ensure_success(status, "set Paddle IR optimization")?;
        // SAFETY: Config is exclusively owned during construction.
        let status = unsafe {
            (self.api.config_set_memory_optim)(
                self.raw.as_ptr(),
                i32::from(options.enable_memory_optim),
            )
        };
        self.api
            .ensure_success(status, "set Paddle memory optimization")?;

        if options.enable_gpu {
            // SAFETY: Config is exclusively owned during construction.
            let status = unsafe {
                (self.api.config_enable_gpu)(
                    self.raw.as_ptr(),
                    options.gpu_memory_pool_mb,
                    options.gpu_device_id,
                )
            };
            self.api.ensure_success(status, "enable Paddle GPU")?;
        }
        Ok(())
    }

    pub(crate) fn into_predictor(self) -> PaddleResult<PaddlePredictor> {
        // SAFETY: Config remains live for the duration of predictor creation.
        let pointer = unsafe { (self.api.predictor_create)(self.raw.as_ptr()) };
        let raw = NonNull::new(pointer).ok_or_else(|| {
            paddle_error(format!(
                "create predictor failed: {}",
                self.api.last_error_message()
            ))
        })?;
        Ok(PaddlePredictor {
            api: Arc::clone(&self.api),
            raw,
        })
    }
}

impl Drop for PaddleConfig {
    fn drop(&mut self) {
        // SAFETY: Config is owned and destroyed exactly once.
        unsafe { (self.api.config_destroy)(self.raw.as_ptr()) };
    }
}

pub(crate) struct PaddlePredictor {
    api: Arc<PaddleApi>,
    raw: NonNull<PaddlePredictorRaw>,
}

// SAFETY: PaddleSession serializes all access to this value with a Mutex.
unsafe impl Send for PaddlePredictor {}

impl PaddlePredictor {
    pub(crate) fn input_names(&self) -> PaddleResult<Vec<String>> {
        self.names(
            self.api.predictor_input_count,
            self.api.predictor_input_name,
        )
    }

    pub(crate) fn output_names(&self) -> PaddleResult<Vec<String>> {
        self.names(
            self.api.predictor_output_count,
            self.api.predictor_output_name,
        )
    }

    fn names(&self, count: PredictorCount, name: PredictorName) -> PaddleResult<Vec<String>> {
        // SAFETY: Predictor is live and read-only for these metadata calls.
        let count = unsafe { count(self.raw.as_ptr()) };
        (0..count)
            .map(|index| {
                // SAFETY: Index is within the count returned by the bridge.
                let pointer = unsafe { name(self.raw.as_ptr(), index) };
                if pointer.is_null() {
                    return Err(paddle_error(format!(
                        "get Paddle tensor name failed: {}",
                        self.api.last_error_message()
                    )));
                }
                // SAFETY: Name storage belongs to the live predictor and is terminated.
                Ok(unsafe { CStr::from_ptr(pointer) }
                    .to_string_lossy()
                    .into_owned())
            })
            .collect()
    }

    pub(crate) fn input_handle<'predictor>(
        &'predictor mut self,
        name: &str,
    ) -> PaddleResult<PaddleTensor<'predictor>> {
        self.handle(name, self.api.predictor_get_input)
    }

    pub(crate) fn output_handle<'predictor>(
        &'predictor mut self,
        name: &str,
    ) -> PaddleResult<PaddleTensor<'predictor>> {
        self.handle(name, self.api.predictor_get_output)
    }

    fn handle<'predictor>(
        &'predictor mut self,
        name: &str,
        get_handle: PredictorGetHandle,
    ) -> PaddleResult<PaddleTensor<'predictor>> {
        let name = CString::new(name)
            .map_err(|_| paddle_error("tensor name contains an interior NUL byte"))?;
        // SAFETY: Predictor is exclusively borrowed and name is terminated.
        let pointer = unsafe { get_handle(self.raw.as_ptr(), name.as_ptr()) };
        let raw = NonNull::new(pointer).ok_or_else(|| {
            paddle_error(format!(
                "get Paddle tensor handle failed: {}",
                self.api.last_error_message()
            ))
        })?;
        Ok(PaddleTensor {
            api: Arc::clone(&self.api),
            raw,
            _predictor: PhantomData,
        })
    }

    pub(crate) fn run(&mut self) -> PaddleResult<()> {
        // SAFETY: Predictor is exclusively borrowed for the native run.
        let status = unsafe { (self.api.predictor_run)(self.raw.as_ptr()) };
        self.api.ensure_success(status, "run Paddle predictor")
    }
}

impl Drop for PaddlePredictor {
    fn drop(&mut self) {
        // SAFETY: Predictor is owned and destroyed exactly once.
        unsafe { (self.api.predictor_destroy)(self.raw.as_ptr()) };
    }
}

pub(crate) struct PaddleTensor<'predictor> {
    api: Arc<PaddleApi>,
    raw: NonNull<PaddleTensorRaw>,
    _predictor: PhantomData<&'predictor mut PaddlePredictor>,
}

impl PaddleTensor<'_> {
    pub(crate) fn reshape(&mut self, shape: &[i64]) -> PaddleResult<()> {
        // SAFETY: Tensor is live and the shape slice is valid for this call.
        let status =
            unsafe { (self.api.tensor_reshape)(self.raw.as_ptr(), shape.as_ptr(), shape.len()) };
        self.api.ensure_success(status, "reshape Paddle tensor")
    }

    pub(crate) fn shape(&self) -> PaddleResult<Vec<i64>> {
        // SAFETY: Tensor is live and metadata access is read-only.
        let rank = unsafe { (self.api.tensor_rank)(self.raw.as_ptr()) };
        (0..rank)
            .map(|index| {
                // SAFETY: Index is within the returned rank.
                let dimension = unsafe { (self.api.tensor_dimension)(self.raw.as_ptr(), index) };
                if dimension == INVALID_DIMENSION {
                    Err(paddle_error(format!(
                        "get Paddle tensor shape failed: {}",
                        self.api.last_error_message()
                    )))
                } else {
                    Ok(dimension)
                }
            })
            .collect()
    }

    pub(crate) fn dtype(&self) -> PaddleResult<i32> {
        // SAFETY: Tensor is live and metadata access is read-only.
        let dtype = unsafe { (self.api.tensor_dtype)(self.raw.as_ptr()) };
        if dtype < 0 {
            Err(paddle_error(format!(
                "get Paddle tensor dtype failed: {}",
                self.api.last_error_message()
            )))
        } else {
            Ok(dtype)
        }
    }

    fn copy_from<T>(&mut self, dtype: i32, values: &[T]) -> PaddleResult<()> {
        // SAFETY: Slice remains live for the synchronous bridge call.
        let status = unsafe {
            (self.api.tensor_copy_from)(
                self.raw.as_ptr(),
                dtype,
                values.as_ptr().cast(),
                size_of_val(values),
            )
        };
        self.api.ensure_success(status, "copy Paddle input")
    }

    fn copy_to<T>(&self, dtype: i32, values: &mut [T]) -> PaddleResult<()> {
        // SAFETY: Mutable slice remains live and exclusively borrowed for the call.
        let status = unsafe {
            (self.api.tensor_copy_to)(
                self.raw.as_ptr(),
                dtype,
                values.as_mut_ptr().cast(),
                size_of_val(values),
            )
        };
        self.api.ensure_success(status, "copy Paddle output")
    }

    pub(crate) fn copy_from_f32(&mut self, values: &[f32]) -> PaddleResult<()> {
        self.copy_from(PADDLE_DATA_FLOAT32, values)
    }

    pub(crate) fn copy_from_f16(&mut self, values: &[u16]) -> PaddleResult<()> {
        self.copy_from(PADDLE_DATA_FLOAT16, values)
    }

    pub(crate) fn copy_from_i64(&mut self, values: &[i64]) -> PaddleResult<()> {
        self.copy_from(PADDLE_DATA_INT64, values)
    }

    pub(crate) fn copy_from_i32(&mut self, values: &[i32]) -> PaddleResult<()> {
        self.copy_from(PADDLE_DATA_INT32, values)
    }

    pub(crate) fn copy_from_u8(&mut self, values: &[u8]) -> PaddleResult<()> {
        self.copy_from(PADDLE_DATA_UINT8, values)
    }

    pub(crate) fn copy_from_bool(&mut self, values: &[u8]) -> PaddleResult<()> {
        self.copy_from(PADDLE_DATA_BOOL, values)
    }

    pub(crate) fn copy_to_f32(&self, values: &mut [f32]) -> PaddleResult<()> {
        self.copy_to(PADDLE_DATA_FLOAT32, values)
    }

    pub(crate) fn copy_to_f16(&self, values: &mut [u16]) -> PaddleResult<()> {
        self.copy_to(PADDLE_DATA_FLOAT16, values)
    }

    pub(crate) fn copy_to_i64(&self, values: &mut [i64]) -> PaddleResult<()> {
        self.copy_to(PADDLE_DATA_INT64, values)
    }

    pub(crate) fn copy_to_i32(&self, values: &mut [i32]) -> PaddleResult<()> {
        self.copy_to(PADDLE_DATA_INT32, values)
    }

    pub(crate) fn copy_to_u8(&self, values: &mut [u8]) -> PaddleResult<()> {
        self.copy_to(PADDLE_DATA_UINT8, values)
    }

    pub(crate) fn copy_to_bool(&self, values: &mut [u8]) -> PaddleResult<()> {
        self.copy_to(PADDLE_DATA_BOOL, values)
    }
}

impl Drop for PaddleTensor<'_> {
    fn drop(&mut self) {
        // SAFETY: Tensor handle is owned and destroyed exactly once.
        unsafe { (self.api.tensor_destroy)(self.raw.as_ptr()) };
    }
}

fn path_to_cstring(path: &Path) -> PaddleResult<CString> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|_| {
        paddle_error(format!(
            "path contains an interior NUL byte: {}",
            path.display()
        ))
    })
}
