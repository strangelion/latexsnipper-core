pub mod error;
pub mod logging;
pub mod config;
pub mod event;

pub use error::{SnipperError, Result, IntoSnipper};
pub use logging::{CoreLogger, init_logger};
pub use config::{CoreConfig, AccelerationMode};
pub use event::{EventBus, Event, EventType};
