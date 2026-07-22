//! Runtime factory trait — creates sessions for a specific runtime kind.

use crate::kind::RuntimeKind;
use crate::{RuntimeArtifacts, RuntimeOptions, RuntimeProbe, RuntimeSession};
use latexsnipper_foundation::Result;

/// Factory for creating runtime sessions.
///
/// Each runtime kind has one factory. The factory is responsible for:
/// 1. Probing availability on the current machine
/// 2. Creating inference sessions from model artifacts
///
/// Factories are registered in the [`RuntimeRegistry`](crate::RuntimeRegistry).
pub trait RuntimeFactory: Send + Sync {
    /// Which runtime this factory creates sessions for.
    fn kind(&self) -> RuntimeKind;

    /// Probe whether this runtime is available on the current machine.
    ///
    /// Should be fast and never panic. Returns detailed availability info.
    fn probe(&self) -> RuntimeProbe;

    /// Create a new inference session.
    ///
    /// # Arguments
    /// * `artifacts` - Model files and their roles for this runtime variant
    /// * `options` - Runtime configuration (device, providers, threads, etc.)
    fn create_session(
        &self,
        artifacts: &RuntimeArtifacts,
        options: &RuntimeOptions,
    ) -> Result<Box<dyn RuntimeSession>>;

    /// Release factory-owned session caches. Most factories do not cache.
    fn clear_sessions(&self) {}
}
