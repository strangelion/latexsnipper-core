//! Explicit trusted native storage, separate from conversion and WASM dependencies.
//! No provider or consent is enabled automatically. Currently Windows-only.

#[cfg(windows)]
pub mod windows;

/// Redacted native diagnostics. Never include paths, keys or record contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceProviderError {
    InvalidConfiguration,
    PrivateDirectoryRequired,
    Unavailable,
    InvalidRecord,
    IntegrityMismatch,
    DeadlineExceeded,
    CleanupLimit,
}
impl std::fmt::Display for PreferenceProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidConfiguration => "Invalid explicit preference provider configuration.",
            Self::PrivateDirectoryRequired => {
                "Preference provider requires a verified private directory."
            }
            Self::Unavailable => "Native preference provider is unavailable.",
            Self::InvalidRecord => "Native preference provider record is invalid or absent.",
            Self::IntegrityMismatch => {
                "Native preference object integrity does not match its protected head."
            }
            Self::DeadlineExceeded => "Preference provider operation exceeded its deadline.",
            Self::CleanupLimit => "Preference cleanup exceeded its inspection budget.",
        })
    }
}
impl std::error::Error for PreferenceProviderError {}
