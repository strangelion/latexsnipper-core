#[cfg(target_os = "windows")]
pub mod onnx;
pub mod stub;

#[cfg(feature = "remote-api")]
pub mod remote;
