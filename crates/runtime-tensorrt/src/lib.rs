//! Native TensorRT runtime backend.
//!
//! TensorRT is discovered through a separately packaged, versioned C bridge,
//! so ordinary workspace builds have no NVIDIA link-time dependency.

pub mod cache;
pub mod error;
pub mod factory;
pub mod ffi;
mod flavor;
pub mod loader;
pub mod options;
pub mod session;
pub mod tensor;

pub use factory::{TensorRtFactory, TensorRtRtxFactory};
pub use options::{ShapeProfile, TensorRtOptions, TensorRtPrecision};
