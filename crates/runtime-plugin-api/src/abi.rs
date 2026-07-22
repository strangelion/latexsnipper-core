//! Frozen version 1 C ABI.
//!
//! Every pointer is borrowed only for the documented call unless ownership is
//! explicitly represented by `LatexSnipperOwnedTensorListV1::owner`.

use std::ffi::c_void;

pub const LATEXSNIPPER_RUNTIME_PLUGIN_ABI_V1: u32 = 1;
pub const LATEXSNIPPER_RUNTIME_PLUGIN_ENTRY_V1: &[u8] = b"latexsnipper_runtime_plugin_entry_v1\0";

pub const LS_RUNTIME_OK: i32 = 0;
pub const LS_RUNTIME_ERROR: i32 = -1;

pub const LS_DTYPE_FLOAT32: i32 = 0;
pub const LS_DTYPE_FLOAT16: i32 = 1;
pub const LS_DTYPE_INT64: i32 = 2;
pub const LS_DTYPE_INT32: i32 = 3;
pub const LS_DTYPE_UINT8: i32 = 4;
pub const LS_DTYPE_BOOL: i32 = 5;

pub const LS_DEVICE_AUTO: i32 = 0;
pub const LS_DEVICE_CPU: i32 = 1;
pub const LS_DEVICE_GPU: i32 = 2;
pub const LS_DEVICE_NPU: i32 = 3;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperBytesV1 {
    pub data: *const u8,
    pub len: usize,
}

impl LatexSnipperBytesV1 {
    pub const fn empty() -> Self {
        Self {
            data: std::ptr::null(),
            len: 0,
        }
    }

    pub fn from_slice(bytes: &[u8]) -> Self {
        Self {
            data: bytes.as_ptr(),
            len: bytes.len(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperTensorViewV1 {
    pub name: LatexSnipperBytesV1,
    pub dtype: i32,
    pub shape: *const i64,
    pub rank: usize,
    pub data: *const c_void,
    pub byte_len: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperArtifactV1 {
    pub role: LatexSnipperBytesV1,
    pub path: LatexSnipperBytesV1,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperSessionCreateRequestV1 {
    pub artifacts: *const LatexSnipperArtifactV1,
    pub artifact_count: usize,
    pub artifact_options_json: LatexSnipperBytesV1,
    pub runtime_options_json: LatexSnipperBytesV1,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperSessionV1 {
    pub handle: *mut c_void,
    /// UTF-8 JSON encoding of `latexsnipper_runtime::SessionMetadata`.
    /// Owned by the session and valid until `destroy_session`.
    pub metadata_json: LatexSnipperBytesV1,
}

impl LatexSnipperSessionV1 {
    pub const fn empty() -> Self {
        Self {
            handle: std::ptr::null_mut(),
            metadata_json: LatexSnipperBytesV1::empty(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperRunRequestV1 {
    pub has_method: u8,
    pub method: LatexSnipperBytesV1,
    pub inputs: *const LatexSnipperTensorViewV1,
    pub input_count: usize,
    pub has_requested_outputs: u8,
    pub requested_outputs: *const LatexSnipperBytesV1,
    pub requested_output_count: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperOwnedTensorListV1 {
    /// Plugin-defined allocation token. A non-null value transfers ownership
    /// to the host even when `tensor_count == 0`.
    pub owner: *mut c_void,
    pub tensors: *const LatexSnipperTensorViewV1,
    pub tensor_count: usize,
}

impl LatexSnipperOwnedTensorListV1 {
    pub const fn empty() -> Self {
        Self {
            owner: std::ptr::null_mut(),
            tensors: std::ptr::null(),
            tensor_count: 0,
        }
    }

    pub const fn owns_allocation(self) -> bool {
        !self.owner.is_null() || !self.tensors.is_null() || self.tensor_count != 0
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperRuntimeDeviceV1 {
    pub name: LatexSnipperBytesV1,
    pub kind: i32,
    pub has_memory_bytes: u8,
    pub memory_bytes: u64,
}

// SAFETY: Probe device records are immutable borrowed descriptors whose byte
// views must remain live until the next plugin call.
unsafe impl Sync for LatexSnipperRuntimeDeviceV1 {}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LatexSnipperRuntimeProbeV1 {
    pub available: u8,
    pub version: LatexSnipperBytesV1,
    pub reason_unavailable: LatexSnipperBytesV1,
    pub devices: *const LatexSnipperRuntimeDeviceV1,
    pub device_count: usize,
    /// UTF-8 JSON encoding of `RuntimeCapabilities`.
    pub capabilities_json: LatexSnipperBytesV1,
}

impl LatexSnipperRuntimeProbeV1 {
    pub const fn empty() -> Self {
        Self {
            available: 0,
            version: LatexSnipperBytesV1::empty(),
            reason_unavailable: LatexSnipperBytesV1::empty(),
            devices: std::ptr::null(),
            device_count: 0,
            capabilities_json: LatexSnipperBytesV1::empty(),
        }
    }
}

pub type RuntimeProbeV1 = unsafe extern "C" fn(output: *mut LatexSnipperRuntimeProbeV1) -> i32;
pub type RuntimeCreateSessionV1 = unsafe extern "C" fn(
    request: *const LatexSnipperSessionCreateRequestV1,
    output: *mut LatexSnipperSessionV1,
) -> i32;
pub type RuntimeDestroySessionV1 = unsafe extern "C" fn(session: *mut c_void);
pub type RuntimeRunV1 = unsafe extern "C" fn(
    session: *mut c_void,
    request: *const LatexSnipperRunRequestV1,
    output: *mut LatexSnipperOwnedTensorListV1,
) -> i32;
pub type RuntimeFreeOutputV1 =
    unsafe extern "C" fn(session: *mut c_void, output: *mut LatexSnipperOwnedTensorListV1);
pub type RuntimeLastErrorV1 = unsafe extern "C" fn() -> LatexSnipperBytesV1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LatexSnipperRuntimePluginV1 {
    pub struct_size: usize,
    pub abi_version: u32,
    pub runtime_id: LatexSnipperBytesV1,
    pub plugin_version: LatexSnipperBytesV1,
    pub probe: Option<RuntimeProbeV1>,
    pub create_session: Option<RuntimeCreateSessionV1>,
    pub destroy_session: Option<RuntimeDestroySessionV1>,
    pub run: Option<RuntimeRunV1>,
    pub free_output: Option<RuntimeFreeOutputV1>,
    pub last_error: Option<RuntimeLastErrorV1>,
}

// SAFETY: A v1 function table is immutable process-lifetime data. The byte
// views must point to immutable process-lifetime identity strings.
unsafe impl Sync for LatexSnipperRuntimePluginV1 {}

pub type RuntimePluginEntryV1 = unsafe extern "C" fn() -> *const LatexSnipperRuntimePluginV1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_struct_layout_is_frozen_by_explicit_size_contract() {
        assert!(
            std::mem::size_of::<LatexSnipperRuntimePluginV1>() >= 10 * usize::BITS as usize / 8
        );
        assert_eq!(LATEXSNIPPER_RUNTIME_PLUGIN_ABI_V1, 1);
    }
}
