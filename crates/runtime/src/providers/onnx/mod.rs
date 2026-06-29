pub mod backend;
pub mod platform;

pub use backend::OnnxRuntimeBackend;
pub use platform::{Platform, Acceleration};
