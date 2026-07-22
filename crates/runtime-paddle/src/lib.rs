//! Paddle Inference Runtime backend.
//!
//! Provides a [`RuntimeFactory`] implementation that creates inference
//! sessions through a versioned C bridge over Paddle Inference.
//!
//! The native library is discovered dynamically, so merely depending on this
//! crate never adds a Paddle link-time requirement.

pub mod error;
pub mod factory;
pub mod ffi;
pub mod loader;
pub mod options;
pub mod session;
pub mod tensor;

pub use factory::PaddleInferenceFactory;
pub use options::PaddleOptions;
