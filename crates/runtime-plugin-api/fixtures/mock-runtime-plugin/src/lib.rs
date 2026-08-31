//! Real dynamic-library fixture for exercising runtime plugin ABI v1.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use latexsnipper_runtime_plugin_api::abi::*;

const RUNTIME_ID: &[u8] = b"mock-runtime";
const PLUGIN_VERSION: &[u8] = b"1.0.0";
const RUNTIME_VERSION: &[u8] = b"mock-runtime/1";
const DEVICE_NAME: &[u8] = b"mock-cpu";
const CAPABILITIES: &[u8] = br#"{"tensorDtypes":["f32"],"executionProviders":["mock-cpu"],"methods":["predict","free-count","active-sessions","malformed","fail-after-allocation"],"features":["mock-runtime","cpu-copy"]}"#;
const METADATA: &[u8] = br#"{"runtime":"custom:mock-runtime","modelId":"mock-model","methods":["predict","free-count","active-sessions","malformed","fail-after-allocation"],"inputs":[{"name":"x","shape":[null],"dtype":"f32"}],"outputs":[{"name":"y","shape":[null],"dtype":"f32"}]}"#;

static ACTIVE_SESSIONS: AtomicUsize = AtomicUsize::new(0);
static FREED_OUTPUTS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static LAST_ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

struct MockSession;

struct MockOutput {
    name: Vec<u8>,
    shape: Vec<i64>,
    data: Vec<u8>,
    views: Vec<LatexSnipperTensorViewV1>,
}

unsafe extern "C" fn probe(output: *mut LatexSnipperRuntimeProbeV1) -> i32 {
    if output.is_null() {
        set_error("probe output is null");
        return LS_RUNTIME_ERROR;
    }
    static DEVICE: LatexSnipperRuntimeDeviceV1 = LatexSnipperRuntimeDeviceV1 {
        name: bytes(DEVICE_NAME),
        kind: LS_DEVICE_CPU,
        has_memory_bytes: 0,
        memory_bytes: 0,
    };
    // SAFETY: Caller provided a writable output pointer.
    unsafe {
        *output = LatexSnipperRuntimeProbeV1 {
            available: 1,
            version: bytes(RUNTIME_VERSION),
            reason_unavailable: LatexSnipperBytesV1::empty(),
            devices: &DEVICE,
            device_count: 1,
            capabilities_json: bytes(CAPABILITIES),
        };
    }
    LS_RUNTIME_OK
}

unsafe extern "C" fn create_session(
    request: *const LatexSnipperSessionCreateRequestV1,
    output: *mut LatexSnipperSessionV1,
) -> i32 {
    if request.is_null() || output.is_null() {
        set_error("create request or output is null");
        return LS_RUNTIME_ERROR;
    }
    let session = Box::new(MockSession);
    ACTIVE_SESSIONS.fetch_add(1, Ordering::SeqCst);
    // SAFETY: Caller provided writable output and takes ownership of the handle.
    unsafe {
        *output = LatexSnipperSessionV1 {
            handle: Box::into_raw(session).cast::<c_void>(),
            metadata_json: bytes(METADATA),
        };
    }
    LS_RUNTIME_OK
}

unsafe extern "C" fn destroy_session(session: *mut c_void) {
    if session.is_null() {
        return;
    }
    // SAFETY: Host transfers each successful create handle exactly once.
    drop(unsafe { Box::from_raw(session.cast::<MockSession>()) });
    ACTIVE_SESSIONS.fetch_sub(1, Ordering::SeqCst);
}

unsafe extern "C" fn run(
    session: *mut c_void,
    request: *const LatexSnipperRunRequestV1,
    output: *mut LatexSnipperOwnedTensorListV1,
) -> i32 {
    if session.is_null() || request.is_null() || output.is_null() {
        set_error("run session, request, or output is null");
        return LS_RUNTIME_ERROR;
    }
    // SAFETY: Host supplies a live request for the synchronous call.
    let request = unsafe { &*request };
    if request.input_count != 1 || request.inputs.is_null() {
        set_error("mock runtime expects one input");
        return LS_RUNTIME_ERROR;
    }
    // SAFETY: Input count is one and host promises one live view.
    let input = unsafe { &*request.inputs };
    if input.dtype != LS_DTYPE_FLOAT32
        || input.rank != 1
        || input.shape.is_null()
        || (input.byte_len != 0 && input.data.is_null())
        || input.byte_len % 4 != 0
    {
        set_error("mock runtime expects a rank-one f32 tensor");
        return LS_RUNTIME_ERROR;
    }
    let method = if request.has_method == 0 {
        "predict".to_owned()
    } else {
        match utf8(request.method) {
            Some(method) => method,
            None => {
                set_error("method is not valid UTF-8");
                return LS_RUNTIME_ERROR;
            }
        }
    };
    // SAFETY: Shape pointer is non-null for rank one.
    let shape = vec![unsafe { *input.shape }];
    // SAFETY: Input data pointer and length were validated above.
    let input_bytes = if input.byte_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(input.data.cast::<u8>(), input.byte_len) }
    };
    let values = input_bytes
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(f32::from_ne_bytes)
        .collect::<Vec<_>>();
    let result = match method.as_str() {
        "predict" => values.into_iter().map(|value| value * 2.0).collect(),
        "free-count" => vec![FREED_OUTPUTS.load(Ordering::SeqCst) as f32],
        "active-sessions" => vec![ACTIVE_SESSIONS.load(Ordering::SeqCst) as f32],
        "malformed" | "fail-after-allocation" => vec![1.0],
        _ => {
            set_error("unknown mock method");
            return LS_RUNTIME_ERROR;
        }
    };
    let output_shape = if matches!(method.as_str(), "free-count" | "active-sessions") {
        vec![1]
    } else {
        shape
    };
    let malformed = method == "malformed";
    let owned = allocate_output(output_shape, result, malformed);
    // SAFETY: Caller provided writable output and receives ownership.
    unsafe { *output = owned };
    if method == "fail-after-allocation" {
        set_error("intentional failure after output allocation");
        LS_RUNTIME_ERROR
    } else {
        LS_RUNTIME_OK
    }
}

unsafe extern "C" fn free_output(
    _session: *mut c_void,
    output: *mut LatexSnipperOwnedTensorListV1,
) {
    if output.is_null() {
        return;
    }
    // SAFETY: Caller passes the live list previously returned by this plugin.
    let output = unsafe { &mut *output };
    if !output.owner.is_null() {
        // SAFETY: Owner was created by `Box::into_raw` in `allocate_output`.
        drop(unsafe { Box::from_raw(output.owner.cast::<MockOutput>()) });
        FREED_OUTPUTS.fetch_add(1, Ordering::SeqCst);
    }
    *output = LatexSnipperOwnedTensorListV1::empty();
}

unsafe extern "C" fn last_error() -> LatexSnipperBytesV1 {
    LAST_ERROR.with(|error| {
        let error = error.borrow();
        LatexSnipperBytesV1::from_slice(&error)
    })
}

fn allocate_output(
    shape: Vec<i64>,
    values: Vec<f32>,
    malformed: bool,
) -> LatexSnipperOwnedTensorListV1 {
    let data = values
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
    let mut output = Box::new(MockOutput {
        name: b"y".to_vec(),
        shape,
        data,
        views: Vec::new(),
    });
    output.views.push(LatexSnipperTensorViewV1 {
        name: LatexSnipperBytesV1::from_slice(&output.name),
        dtype: LS_DTYPE_FLOAT32,
        shape: output.shape.as_ptr(),
        rank: output.shape.len(),
        data: output.data.as_ptr().cast::<c_void>(),
        byte_len: if malformed { 3 } else { output.data.len() },
    });
    let tensors = output.views.as_ptr();
    LatexSnipperOwnedTensorListV1 {
        owner: Box::into_raw(output).cast::<c_void>(),
        tensors,
        tensor_count: 1,
    }
}

fn set_error(message: &str) {
    LAST_ERROR.with(|error| {
        let mut error = error.borrow_mut();
        error.clear();
        error.extend_from_slice(message.as_bytes());
    });
}

fn utf8(view: LatexSnipperBytesV1) -> Option<String> {
    if view.len == 0 {
        return Some(String::new());
    }
    if view.data.is_null() {
        return None;
    }
    // SAFETY: Host promises request byte views are live for the call.
    std::str::from_utf8(unsafe { std::slice::from_raw_parts(view.data, view.len) })
        .ok()
        .map(str::to_owned)
}

const fn bytes(value: &'static [u8]) -> LatexSnipperBytesV1 {
    LatexSnipperBytesV1 {
        data: value.as_ptr(),
        len: value.len(),
    }
}

static PLUGIN: LatexSnipperRuntimePluginV1 = LatexSnipperRuntimePluginV1 {
    struct_size: std::mem::size_of::<LatexSnipperRuntimePluginV1>(),
    abi_version: LATEXSNIPPER_RUNTIME_PLUGIN_ABI_V1,
    runtime_id: bytes(RUNTIME_ID),
    plugin_version: bytes(PLUGIN_VERSION),
    probe: Some(probe),
    create_session: Some(create_session),
    destroy_session: Some(destroy_session),
    run: Some(run),
    free_output: Some(free_output),
    last_error: Some(last_error),
};

#[no_mangle]
pub extern "C" fn latexsnipper_runtime_plugin_entry_v1() -> *const LatexSnipperRuntimePluginV1 {
    &PLUGIN
}
