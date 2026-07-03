//! Tract-based ONNX RuntimeBackend for WASM-compatible inference.
//!
//! This crate provides a `RuntimeBackend` implementation using the `tract`
//! pure-Rust ONNX runtime, suitable for WebAssembly environments where the
//! native ONNX Runtime C++ library is not available.

mod backend;
mod session;

pub use backend::TractBackend;
pub use session::TractSession;
