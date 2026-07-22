//! ExecuTorch runtime backend.
//!
//! The crate talks to a separately installed, versioned C bridge and therefore
//! adds no ExecuTorch link-time dependency to ordinary LaTeXSnipper builds.

pub mod error;
pub mod factory;
pub mod ffi;
pub mod loader;
pub mod options;
pub mod session;
pub mod tensor;

pub use factory::ExecuTorchFactory;
pub use options::ExecuTorchOptions;
