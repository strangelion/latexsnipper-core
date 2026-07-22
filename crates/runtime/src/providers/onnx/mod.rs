pub mod backend;
pub mod platform;
mod provider;

pub use backend::OnnxRuntimeBackend;
pub use platform::{Acceleration, Platform};
