//! Trusted dynamic-library host implementing the common runtime traits.

use std::collections::{BTreeSet, HashSet};
use std::ffi::c_void;
use std::fmt;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

use latexsnipper_foundation::Result;
use latexsnipper_runtime::{
    DeviceKind, RunRequest, RunResponse, RuntimeArtifacts, RuntimeCapabilities, RuntimeDevice,
    RuntimeFactory, RuntimeKind, RuntimeOptions, RuntimeProbe, RuntimeSession, SessionMetadata,
    TensorMap,
};
use latexsnipper_tensor::{Tensor, TensorData};
use libloading::Library;

use crate::abi::*;
use crate::descriptor::RuntimePluginDescriptor;
use crate::error::{plugin_runtime_error, PluginRuntimeResult};

const MAX_STRING_BYTES: usize = 1024 * 1024;
const MAX_TENSORS: usize = 4096;
const MAX_DEVICES: usize = 256;

struct LoadedPlugin {
    _library: Library,
    table: LatexSnipperRuntimePluginV1,
    runtime_id: String,
    plugin_version: String,
    library_path: PathBuf,
}

// SAFETY: `Library` keeps code and static plugin data resident. Calls that
// touch a session are serialized by `RuntimePluginSession`; probe and create
// are required by ABI v1 to be thread-safe.
unsafe impl Send for LoadedPlugin {}
unsafe impl Sync for LoadedPlugin {}

impl LoadedPlugin {
    unsafe fn load(
        library_path: &Path,
        descriptor: &RuntimePluginDescriptor,
    ) -> PluginRuntimeResult<Arc<Self>> {
        // SAFETY: Discovery has verified the canonical path and exact digest,
        // and explicit trust authorizes executing this native library.
        let library = unsafe { Library::new(library_path) }.map_err(|error| {
            plugin_runtime_error(format!(
                "load trusted runtime plugin '{}': {error}",
                library_path.display()
            ))
        })?;
        // SAFETY: The library remains owned below and the symbol name includes
        // its required terminator.
        let entry = unsafe { library.get::<RuntimePluginEntryV1>(LATEXSNIPPER_RUNTIME_PLUGIN_ENTRY_V1) }
            .map_err(|error| {
                plugin_runtime_error(format!(
                    "trusted library '{}' does not export latexsnipper_runtime_plugin_entry_v1: {error}",
                    library_path.display()
                ))
            })?;
        // SAFETY: ABI contract requires a process-lifetime table pointer.
        let table_pointer = unsafe { entry() };
        if table_pointer.is_null() {
            return Err(plugin_runtime_error(
                "runtime plugin entry returned a null function table",
            ));
        }
        // SAFETY: Every ABI table begins with `struct_size`; reading only that
        // prefix is valid even when a plugin returns an older/smaller table.
        let struct_size = unsafe { std::ptr::addr_of!((*table_pointer).struct_size).read() };
        if struct_size < std::mem::size_of::<LatexSnipperRuntimePluginV1>() {
            return Err(plugin_runtime_error(format!(
                "runtime plugin function table is too small: expected at least {}, got {}",
                std::mem::size_of::<LatexSnipperRuntimePluginV1>(),
                struct_size
            )));
        }
        // SAFETY: The declared size now covers the complete frozen v1 table.
        let table = unsafe { table_pointer.read() };
        if table.abi_version != LATEXSNIPPER_RUNTIME_PLUGIN_ABI_V1 {
            return Err(plugin_runtime_error(format!(
                "runtime plugin ABI mismatch: host supports v1, plugin reports v{}",
                table.abi_version
            )));
        }
        require_functions(&table)?;
        let runtime_id = copy_utf8(table.runtime_id, "runtime id", 128)?;
        let plugin_version = copy_utf8(table.plugin_version, "plugin version", 128)?;
        if runtime_id != descriptor.runtime_id || plugin_version != descriptor.plugin_version {
            return Err(plugin_runtime_error(format!(
                "runtime plugin identity differs from descriptor: descriptor={}/{}, library={}/{}",
                descriptor.runtime_id, descriptor.plugin_version, runtime_id, plugin_version
            )));
        }
        Ok(Arc::new(Self {
            _library: library,
            table,
            runtime_id,
            plugin_version,
            library_path: library_path.to_path_buf(),
        }))
    }

    fn error_message(&self, operation: &str) -> String {
        let Some(last_error) = self.table.last_error else {
            return format!("{operation} failed without a plugin error message");
        };
        // SAFETY: Required v1 function has no arguments and returns borrowed bytes.
        let view = unsafe { last_error() };
        match copy_utf8(view, "last error", 16 * 1024) {
            Ok(message) if !message.is_empty() => message,
            _ => format!("{operation} failed with an invalid plugin error message"),
        }
    }
}

fn require_functions(table: &LatexSnipperRuntimePluginV1) -> PluginRuntimeResult<()> {
    let missing = [
        (table.probe.is_none(), "probe"),
        (table.create_session.is_none(), "create_session"),
        (table.destroy_session.is_none(), "destroy_session"),
        (table.run.is_none(), "run"),
        (table.free_output.is_none(), "free_output"),
        (table.last_error.is_none(), "last_error"),
    ]
    .into_iter()
    .filter_map(|(missing, name)| missing.then_some(name))
    .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(plugin_runtime_error(format!(
            "runtime plugin function table omits required functions: {}",
            missing.join(", ")
        )))
    }
}

#[derive(Clone)]
pub struct RuntimePluginFactory {
    plugin: Arc<LoadedPlugin>,
}

impl fmt::Debug for RuntimePluginFactory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimePluginFactory")
            .field("runtime_id", &self.plugin.runtime_id)
            .field("plugin_version", &self.plugin.plugin_version)
            .field("library_path", &self.plugin.library_path)
            .finish()
    }
}

impl RuntimePluginFactory {
    pub(crate) unsafe fn load_trusted(
        library_path: &Path,
        descriptor: &RuntimePluginDescriptor,
    ) -> PluginRuntimeResult<Self> {
        // SAFETY: Caller proves explicit trust and byte verification.
        let plugin = unsafe { LoadedPlugin::load(library_path, descriptor) }?;
        Ok(Self { plugin })
    }

    pub fn runtime_id(&self) -> &str {
        &self.plugin.runtime_id
    }

    pub fn plugin_version(&self) -> &str {
        &self.plugin.plugin_version
    }
}

impl RuntimeFactory for RuntimePluginFactory {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::Custom(self.plugin.runtime_id.clone())
    }

    fn probe(&self) -> RuntimeProbe {
        match probe_plugin(&self.plugin) {
            Ok(probe) => probe,
            Err(error) => RuntimeProbe::unavailable(error.to_string()),
        }
    }

    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>> {
        let expected = RuntimeKind::Custom(self.plugin.runtime_id.clone());
        if artifacts.runtime != expected {
            return Err(plugin_runtime_error(format!(
                "custom runtime '{}' received '{}' artifacts",
                self.plugin.runtime_id, artifacts.runtime
            )));
        }
        if !artifacts.buffers.is_empty() {
            return Err(plugin_runtime_error(
                "runtime plugin ABI v1 accepts installed file artifacts, not in-memory model buffers",
            ));
        }

        let owned_artifacts = artifacts
            .files
            .iter()
            .map(|(role, path)| {
                let path = path.to_str().ok_or_else(|| {
                    plugin_runtime_error(format!(
                        "runtime plugin artifact path is not UTF-8: {}",
                        path.display()
                    ))
                })?;
                Ok((role.as_bytes().to_vec(), path.as_bytes().to_vec()))
            })
            .collect::<PluginRuntimeResult<Vec<_>>>()?;
        let artifact_views = owned_artifacts
            .iter()
            .map(|(role, path)| LatexSnipperArtifactV1 {
                role: LatexSnipperBytesV1::from_slice(role),
                path: LatexSnipperBytesV1::from_slice(path),
            })
            .collect::<Vec<_>>();
        let artifact_options = serde_json::to_vec(&artifacts.options)
            .map_err(|error| plugin_runtime_error(error.to_string()))?;
        let runtime_options =
            serde_json::to_vec(options).map_err(|error| plugin_runtime_error(error.to_string()))?;
        let request = LatexSnipperSessionCreateRequestV1 {
            artifacts: artifact_views.as_ptr(),
            artifact_count: artifact_views.len(),
            artifact_options_json: LatexSnipperBytesV1::from_slice(&artifact_options),
            runtime_options_json: LatexSnipperBytesV1::from_slice(&runtime_options),
        };
        let mut output = LatexSnipperSessionV1::empty();
        let create = self
            .plugin
            .table
            .create_session
            .expect("required function was validated while loading plugin");
        // SAFETY: All request views remain live through this synchronous call.
        let status = unsafe { create(&request, &mut output) };
        let handle = NonNull::new(output.handle);
        if status != LS_RUNTIME_OK {
            let message = self.plugin.error_message("create session");
            if let Some(handle) = handle {
                destroy_handle(&self.plugin, handle);
            }
            return Err(plugin_runtime_error(message));
        }
        let handle = handle.ok_or_else(|| {
            plugin_runtime_error("runtime plugin create_session succeeded with a null handle")
        })?;
        let metadata_bytes =
            match copy_bytes(output.metadata_json, "session metadata", MAX_STRING_BYTES) {
                Ok(bytes) => bytes,
                Err(error) => {
                    destroy_handle(&self.plugin, handle);
                    return Err(error);
                }
            };
        let metadata: SessionMetadata = match serde_json::from_slice(&metadata_bytes) {
            Ok(metadata) => metadata,
            Err(error) => {
                destroy_handle(&self.plugin, handle);
                return Err(plugin_runtime_error(format!(
                    "runtime plugin returned invalid session metadata JSON: {error}"
                )));
            }
        };
        if metadata.runtime != expected {
            destroy_handle(&self.plugin, handle);
            return Err(plugin_runtime_error(format!(
                "runtime plugin session metadata reports '{}', expected '{}'",
                metadata.runtime, expected
            )));
        }
        validate_metadata(&metadata).inspect_err(|_| destroy_handle(&self.plugin, handle))?;
        Ok(Box::new(RuntimePluginSession {
            metadata,
            plugin: Arc::clone(&self.plugin),
            handle,
            execution: Mutex::new(()),
        }))
    }
}

fn probe_plugin(plugin: &LoadedPlugin) -> PluginRuntimeResult<RuntimeProbe> {
    let probe = plugin
        .table
        .probe
        .expect("required function was validated while loading plugin");
    let mut native = LatexSnipperRuntimeProbeV1::empty();
    // SAFETY: Output points to writable host memory for this synchronous call.
    let status = unsafe { probe(&mut native) };
    if status != LS_RUNTIME_OK {
        return Err(plugin_runtime_error(plugin.error_message("probe")));
    }
    if native.device_count > MAX_DEVICES || (native.device_count != 0 && native.devices.is_null()) {
        return Err(plugin_runtime_error(
            "runtime plugin probe returned an invalid device list",
        ));
    }
    let native_devices = if native.device_count == 0 {
        &[]
    } else {
        // SAFETY: Count is bounded and non-null pointer was checked.
        unsafe { std::slice::from_raw_parts(native.devices, native.device_count) }
    };
    let devices = native_devices
        .iter()
        .map(|device| {
            Ok(RuntimeDevice {
                name: copy_utf8(device.name, "device name", 1024)?,
                kind: device_kind(device.kind)?,
                memory_bytes: (device.has_memory_bytes != 0).then_some(device.memory_bytes),
            })
        })
        .collect::<PluginRuntimeResult<Vec<_>>>()?;
    let capabilities = if native.capabilities_json.len == 0 {
        RuntimeCapabilities::default()
    } else {
        let bytes = copy_bytes(
            native.capabilities_json,
            "probe capabilities",
            MAX_STRING_BYTES,
        )?;
        serde_json::from_slice(&bytes).map_err(|error| {
            plugin_runtime_error(format!(
                "runtime plugin returned invalid capabilities JSON: {error}"
            ))
        })?
    };
    let version = optional_utf8(native.version, "runtime version", 1024)?;
    let reason = optional_utf8(native.reason_unavailable, "unavailable reason", 16 * 1024)?;
    let available = native.available != 0;
    Ok(RuntimeProbe {
        available,
        version,
        devices,
        reason_unavailable: if available {
            None
        } else {
            Some(reason.unwrap_or_else(|| "plugin runtime reported unavailable".to_owned()))
        },
        capabilities,
    })
}

struct RuntimePluginSession {
    metadata: SessionMetadata,
    plugin: Arc<LoadedPlugin>,
    handle: NonNull<c_void>,
    execution: Mutex<()>,
}

// SAFETY: The opaque handle is only touched while `execution` is locked, and
// the ABI requires `destroy_session` after all calls have returned.
unsafe impl Send for RuntimePluginSession {}
unsafe impl Sync for RuntimePluginSession {}

impl RuntimeSession for RuntimePluginSession {
    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        let _execution = self
            .execution
            .lock()
            .map_err(|_| plugin_runtime_error("runtime plugin session lock was poisoned"))?;
        validate_run_request(&self.metadata, &request)?;
        let prepared = self
            .metadata
            .inputs
            .iter()
            .map(|spec| {
                let tensor = request
                    .inputs
                    .get(&spec.name)
                    .expect("input names were validated before preparation");
                PreparedTensor::new(&spec.name, tensor)
            })
            .collect::<PluginRuntimeResult<Vec<_>>>()?;
        let input_views = prepared
            .iter()
            .map(PreparedTensor::view)
            .collect::<Vec<_>>();
        let method = request.method.as_deref().map(str::as_bytes);
        let requested_views = request
            .requested_outputs
            .as_ref()
            .map(|names| {
                names
                    .iter()
                    .map(|name| LatexSnipperBytesV1::from_slice(name.as_bytes()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let native_request = LatexSnipperRunRequestV1 {
            has_method: u8::from(method.is_some()),
            method: method
                .map(LatexSnipperBytesV1::from_slice)
                .unwrap_or_else(LatexSnipperBytesV1::empty),
            inputs: input_views.as_ptr(),
            input_count: input_views.len(),
            has_requested_outputs: u8::from(request.requested_outputs.is_some()),
            requested_outputs: requested_views.as_ptr(),
            requested_output_count: requested_views.len(),
        };
        let mut native_output = LatexSnipperOwnedTensorListV1::empty();
        let run = self
            .plugin
            .table
            .run
            .expect("required function was validated while loading plugin");
        // SAFETY: Request storage and session are live; calls are serialized.
        let status = unsafe { run(self.handle.as_ptr(), &native_request, &mut native_output) };
        let output_guard = PluginOutputGuard::new(&self.plugin, self.handle, native_output);
        if status != LS_RUNTIME_OK {
            return Err(plugin_runtime_error(self.plugin.error_message("run")));
        }
        let views = output_guard.views()?;
        let declared = self
            .metadata
            .outputs
            .iter()
            .map(|spec| spec.name.as_str())
            .collect::<BTreeSet<_>>();
        let requested = request
            .requested_outputs
            .as_ref()
            .map(|names| names.iter().map(String::as_str).collect::<BTreeSet<_>>())
            .unwrap_or_else(|| declared.clone());
        let mut outputs = TensorMap::new();
        for view in views {
            let tensor = copy_native_tensor(view)?;
            let name = tensor.name().to_owned();
            let Some(spec) = self.metadata.outputs.iter().find(|spec| spec.name == name) else {
                return Err(plugin_runtime_error(format!(
                    "runtime plugin returned undeclared output '{name}'"
                )));
            };
            if tensor.dtype().as_str() != spec.dtype
                || tensor.shape().len() != spec.shape.len()
                || spec
                    .shape
                    .iter()
                    .zip(tensor.shape())
                    .any(|(expected, actual)| {
                        expected.is_some_and(|value| i64::try_from(*actual).ok() != Some(value))
                    })
            {
                return Err(plugin_runtime_error(format!(
                    "runtime plugin output '{name}' does not match dtype/shape metadata"
                )));
            }
            if requested.contains(name.as_str()) && outputs.insert(name.clone(), tensor).is_some() {
                return Err(plugin_runtime_error(format!(
                    "runtime plugin returned duplicate output '{name}'"
                )));
            }
        }
        if outputs.keys().map(String::as_str).collect::<BTreeSet<_>>() != requested {
            return Err(plugin_runtime_error(
                "runtime plugin did not return every requested output",
            ));
        }
        Ok(RunResponse { outputs })
    }
}

impl Drop for RuntimePluginSession {
    fn drop(&mut self) {
        destroy_handle(&self.plugin, self.handle);
    }
}

fn destroy_handle(plugin: &LoadedPlugin, handle: NonNull<c_void>) {
    let destroy = plugin
        .table
        .destroy_session
        .expect("required function was validated while loading plugin");
    // SAFETY: Host owns this live handle exactly once.
    unsafe { destroy(handle.as_ptr()) };
}

struct PluginOutputGuard<'plugin> {
    plugin: &'plugin LoadedPlugin,
    session: NonNull<c_void>,
    output: LatexSnipperOwnedTensorListV1,
    active: bool,
}

impl<'plugin> PluginOutputGuard<'plugin> {
    fn new(
        plugin: &'plugin LoadedPlugin,
        session: NonNull<c_void>,
        output: LatexSnipperOwnedTensorListV1,
    ) -> Self {
        Self {
            plugin,
            session,
            active: output.owns_allocation(),
            output,
        }
    }

    fn views(&self) -> PluginRuntimeResult<&[LatexSnipperTensorViewV1]> {
        if self.output.tensor_count > MAX_TENSORS
            || (self.output.tensor_count != 0 && self.output.tensors.is_null())
        {
            return Err(plugin_runtime_error(
                "runtime plugin returned an invalid output tensor list",
            ));
        }
        if self.output.tensor_count == 0 {
            Ok(&[])
        } else {
            // SAFETY: Count is bounded, pointer checked, and allocation remains
            // owned by this guard for the returned borrow.
            Ok(
                unsafe {
                    std::slice::from_raw_parts(self.output.tensors, self.output.tensor_count)
                },
            )
        }
    }
}

impl Drop for PluginOutputGuard<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.active = false;
        let free = self
            .plugin
            .table
            .free_output
            .expect("required function was validated while loading plugin");
        // SAFETY: The output ownership token is consumed exactly once while
        // its originating session remains live and locked.
        unsafe { free(self.session.as_ptr(), &mut self.output) };
        self.output = LatexSnipperOwnedTensorListV1::empty();
    }
}

struct PreparedTensor {
    name: Vec<u8>,
    shape: Vec<i64>,
    dtype: i32,
    bytes: Vec<u8>,
}

impl PreparedTensor {
    fn new(name: &str, tensor: &Tensor) -> PluginRuntimeResult<Self> {
        let count = checked_element_count(tensor.shape())?;
        if tensor_data_len(tensor.data()) != count {
            return Err(plugin_runtime_error(format!(
                "input tensor '{name}' shape/data length mismatch"
            )));
        }
        let shape = tensor
            .shape()
            .iter()
            .map(|dimension| {
                i64::try_from(*dimension)
                    .map_err(|_| plugin_runtime_error("tensor dimension exceeds i64::MAX"))
            })
            .collect::<PluginRuntimeResult<Vec<_>>>()?;
        let (dtype, bytes) = encode_tensor_data(tensor.data());
        Ok(Self {
            name: name.as_bytes().to_vec(),
            shape,
            dtype,
            bytes,
        })
    }

    fn view(&self) -> LatexSnipperTensorViewV1 {
        LatexSnipperTensorViewV1 {
            name: LatexSnipperBytesV1::from_slice(&self.name),
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

fn copy_native_tensor(view: &LatexSnipperTensorViewV1) -> PluginRuntimeResult<Tensor> {
    let name = copy_utf8(view.name, "output tensor name", 4096)?;
    let shape = copy_shape(view.shape, view.rank, &name)?;
    let count = checked_element_count(&shape)?;
    let width = dtype_width(view.dtype)?;
    let expected = count
        .checked_mul(width)
        .ok_or_else(|| plugin_runtime_error("output tensor byte length overflow"))?;
    if view.byte_len != expected || (view.byte_len != 0 && view.data.is_null()) {
        return Err(plugin_runtime_error(format!(
            "output tensor '{name}' requires {expected} bytes, plugin returned {}",
            view.byte_len
        )));
    }
    let bytes = if view.byte_len == 0 {
        &[]
    } else {
        // SAFETY: Output guard owns this bounded allocation through conversion.
        unsafe { std::slice::from_raw_parts(view.data.cast::<u8>(), view.byte_len) }
    };
    match view.dtype {
        LS_DTYPE_FLOAT32 => Ok(Tensor::float32(
            &name,
            shape,
            decode::<4, f32>(bytes, f32::from_ne_bytes),
        )),
        LS_DTYPE_FLOAT16 => Ok(Tensor::float16_bits(
            &name,
            shape,
            decode::<2, u16>(bytes, u16::from_ne_bytes),
        )),
        LS_DTYPE_INT64 => Ok(Tensor::int64(
            &name,
            shape,
            decode::<8, i64>(bytes, i64::from_ne_bytes),
        )),
        LS_DTYPE_INT32 => Ok(Tensor::int32(
            &name,
            shape,
            decode::<4, i32>(bytes, i32::from_ne_bytes),
        )),
        LS_DTYPE_UINT8 => Ok(Tensor::u8(&name, shape, bytes.to_vec())),
        LS_DTYPE_BOOL => Ok(Tensor::boolean(
            &name,
            shape,
            bytes.iter().map(|byte| *byte != 0).collect(),
        )),
        _ => unreachable!("dtype width validation accepted only known codes"),
    }
}

fn validate_metadata(metadata: &SessionMetadata) -> PluginRuntimeResult<()> {
    for (direction, specs) in [("input", &metadata.inputs), ("output", &metadata.outputs)] {
        let mut names = HashSet::new();
        for spec in specs {
            if spec.name.is_empty() || !names.insert(spec.name.as_str()) {
                return Err(plugin_runtime_error(format!(
                    "runtime plugin metadata contains an empty or duplicate {direction} name"
                )));
            }
            if dtype_code(&spec.dtype).is_none() {
                return Err(plugin_runtime_error(format!(
                    "runtime plugin metadata uses unsupported dtype '{}'",
                    spec.dtype
                )));
            }
        }
    }
    let mut methods = HashSet::new();
    if metadata
        .methods
        .iter()
        .any(|method| method.is_empty() || !methods.insert(method.as_str()))
    {
        return Err(plugin_runtime_error(
            "runtime plugin metadata contains an empty or duplicate method",
        ));
    }
    Ok(())
}

fn validate_run_request(
    metadata: &SessionMetadata,
    request: &RunRequest,
) -> PluginRuntimeResult<()> {
    if let Some(method) = &request.method {
        if !metadata.methods.iter().any(|candidate| candidate == method) {
            return Err(plugin_runtime_error(format!(
                "runtime plugin session does not declare method '{method}'"
            )));
        }
    }
    let expected = metadata
        .inputs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();
    let provided = request
        .inputs
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if expected != provided {
        return Err(plugin_runtime_error(format!(
            "runtime plugin input names differ; missing={:?}, unexpected={:?}",
            expected.difference(&provided).collect::<Vec<_>>(),
            provided.difference(&expected).collect::<Vec<_>>()
        )));
    }
    for spec in &metadata.inputs {
        let tensor = &request.inputs[&spec.name];
        if tensor.dtype().as_str() != spec.dtype
            || tensor.shape().len() != spec.shape.len()
            || spec
                .shape
                .iter()
                .zip(tensor.shape())
                .any(|(expected, actual)| {
                    expected.is_some_and(|value| i64::try_from(*actual).ok() != Some(value))
                })
        {
            return Err(plugin_runtime_error(format!(
                "runtime plugin input '{}' does not match dtype/shape metadata",
                spec.name
            )));
        }
    }
    if let Some(requested) = &request.requested_outputs {
        let declared = metadata
            .outputs
            .iter()
            .map(|spec| spec.name.as_str())
            .collect::<HashSet<_>>();
        let mut seen = HashSet::new();
        if requested
            .iter()
            .any(|name| !declared.contains(name.as_str()) || !seen.insert(name.as_str()))
        {
            return Err(plugin_runtime_error(
                "runtime plugin request contains an undeclared or duplicate output",
            ));
        }
    }
    Ok(())
}

fn copy_shape(pointer: *const i64, rank: usize, name: &str) -> PluginRuntimeResult<Vec<usize>> {
    if rank > 64 || (rank != 0 && pointer.is_null()) {
        return Err(plugin_runtime_error(format!(
            "output tensor '{name}' has an invalid shape view"
        )));
    }
    if rank == 0 {
        return Ok(Vec::new());
    }
    // SAFETY: Rank is bounded and pointer checked under the output guard.
    unsafe { std::slice::from_raw_parts(pointer, rank) }
        .iter()
        .map(|dimension| {
            usize::try_from(*dimension).map_err(|_| {
                plugin_runtime_error(format!(
                    "output tensor '{name}' has invalid dimension {dimension}"
                ))
            })
        })
        .collect()
}

fn checked_element_count(shape: &[usize]) -> PluginRuntimeResult<usize> {
    shape.iter().try_fold(1usize, |count, dimension| {
        count
            .checked_mul(*dimension)
            .ok_or_else(|| plugin_runtime_error("tensor element count overflow"))
    })
}

fn tensor_data_len(data: &TensorData) -> usize {
    match data {
        TensorData::Float32(values) => values.len(),
        TensorData::Float16(values) => values.len(),
        TensorData::Int64(values) => values.len(),
        TensorData::Int32(values) => values.len(),
        TensorData::UInt8(values) => values.len(),
        TensorData::Bool(values) => values.len(),
    }
}

fn encode_tensor_data(data: &TensorData) -> (i32, Vec<u8>) {
    match data {
        TensorData::Float32(values) => (LS_DTYPE_FLOAT32, encode(values, f32::to_ne_bytes)),
        TensorData::Float16(values) => (LS_DTYPE_FLOAT16, encode(values, u16::to_ne_bytes)),
        TensorData::Int64(values) => (LS_DTYPE_INT64, encode(values, i64::to_ne_bytes)),
        TensorData::Int32(values) => (LS_DTYPE_INT32, encode(values, i32::to_ne_bytes)),
        TensorData::UInt8(values) => (LS_DTYPE_UINT8, values.clone()),
        TensorData::Bool(values) => (
            LS_DTYPE_BOOL,
            values.iter().copied().map(u8::from).collect(),
        ),
    }
}

fn encode<T, const WIDTH: usize>(values: &[T], to_bytes: impl Fn(T) -> [u8; WIDTH]) -> Vec<u8>
where
    T: Copy,
{
    values.iter().copied().flat_map(to_bytes).collect()
}

fn decode<const WIDTH: usize, T>(bytes: &[u8], from_bytes: impl Fn([u8; WIDTH]) -> T) -> Vec<T> {
    bytes
        .as_chunks::<WIDTH>()
        .0
        .iter()
        .copied()
        .map(from_bytes)
        .collect()
}

fn dtype_width(dtype: i32) -> PluginRuntimeResult<usize> {
    match dtype {
        LS_DTYPE_FLOAT32 | LS_DTYPE_INT32 => Ok(4),
        LS_DTYPE_FLOAT16 => Ok(2),
        LS_DTYPE_INT64 => Ok(8),
        LS_DTYPE_UINT8 | LS_DTYPE_BOOL => Ok(1),
        other => Err(plugin_runtime_error(format!(
            "runtime plugin returned unknown dtype code {other}"
        ))),
    }
}

fn dtype_code(dtype: &str) -> Option<i32> {
    match dtype {
        "f32" => Some(LS_DTYPE_FLOAT32),
        "f16" => Some(LS_DTYPE_FLOAT16),
        "i64" => Some(LS_DTYPE_INT64),
        "i32" => Some(LS_DTYPE_INT32),
        "u8" => Some(LS_DTYPE_UINT8),
        "bool" => Some(LS_DTYPE_BOOL),
        _ => None,
    }
}

fn device_kind(kind: i32) -> PluginRuntimeResult<DeviceKind> {
    match kind {
        LS_DEVICE_AUTO => Ok(DeviceKind::Auto),
        LS_DEVICE_CPU => Ok(DeviceKind::Cpu),
        LS_DEVICE_GPU => Ok(DeviceKind::Gpu),
        LS_DEVICE_NPU => Ok(DeviceKind::Npu),
        other => Err(plugin_runtime_error(format!(
            "runtime plugin returned unknown device kind {other}"
        ))),
    }
}

fn optional_utf8(
    view: LatexSnipperBytesV1,
    label: &str,
    maximum: usize,
) -> PluginRuntimeResult<Option<String>> {
    if view.len == 0 {
        Ok(None)
    } else {
        copy_utf8(view, label, maximum).map(Some)
    }
}

fn copy_utf8(
    view: LatexSnipperBytesV1,
    label: &str,
    maximum: usize,
) -> PluginRuntimeResult<String> {
    let bytes = copy_bytes(view, label, maximum)?;
    String::from_utf8(bytes)
        .map_err(|_| plugin_runtime_error(format!("runtime plugin {label} is not valid UTF-8")))
}

fn copy_bytes(
    view: LatexSnipperBytesV1,
    label: &str,
    maximum: usize,
) -> PluginRuntimeResult<Vec<u8>> {
    if view.len > maximum || (view.len != 0 && view.data.is_null()) {
        return Err(plugin_runtime_error(format!(
            "runtime plugin {label} byte view is invalid or exceeds {maximum} bytes"
        )));
    }
    if view.len == 0 {
        Ok(Vec::new())
    } else {
        // SAFETY: Trusted plugin promises the bounded view remains live for
        // this call; bytes are immediately copied into host ownership.
        Ok(unsafe { std::slice::from_raw_parts(view.data, view.len) }.to_vec())
    }
}
