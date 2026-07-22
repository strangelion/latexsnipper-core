//! Native Core ML runtime.
//!
//! The Objective-C++ bridge is compiled and linked only for Apple targets.
//! Other platforms retain a factory that probes as unavailable, allowing one
//! manifest and one workspace feature set to remain portable.

#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
pub mod cache;
pub mod error;
pub mod factory;
pub mod options;

#[cfg(target_vendor = "apple")]
mod ffi;
#[cfg(target_vendor = "apple")]
mod session;
#[cfg(target_vendor = "apple")]
mod tensor;

pub use factory::CoreMlFactory;
pub use options::{CoreMlComputeUnits, CoreMlOptions};
