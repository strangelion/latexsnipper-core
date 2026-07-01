pub mod backend;
pub mod platform;

pub use backend::OnnxRuntimeBackend;
pub use platform::{Acceleration, Platform};
