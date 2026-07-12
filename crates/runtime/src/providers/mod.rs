#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub mod onnx;
pub mod stub;

#[cfg(feature = "remote-api")]
pub mod remote;
