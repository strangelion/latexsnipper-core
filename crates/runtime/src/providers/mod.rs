pub mod legacy_adapter;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub mod onnx;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub mod onnx_factory;
pub mod stub;

#[cfg(feature = "remote-api")]
pub mod remote;
