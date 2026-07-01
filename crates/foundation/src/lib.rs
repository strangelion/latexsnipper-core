pub mod config;
pub mod error;
pub mod event;
pub mod logging;

pub use config::{AccelerationMode, CoreConfig};
pub use error::{IntoSnipper, Result, SnipperError};
pub use event::{Event, EventBus, EventType};
pub use logging::{init_logger, CoreLogger};
