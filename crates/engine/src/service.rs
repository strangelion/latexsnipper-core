use latexsnipper_foundation::Result;

/// Trait for services that have a lifecycle (init, start, stop).
pub trait Service: Send + Sync {
    /// Get the service name.
    fn name(&self) -> &str;

    /// Initialize the service.
    fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// Start the service.
    fn start(&mut self) -> Result<()> {
        Ok(())
    }

    /// Stop the service.
    fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    /// Check if the service is running.
    fn is_running(&self) -> bool {
        false
    }
}

/// Status of a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Uninitialized,
    Initialized,
    Running,
    Stopped,
    Error,
}
