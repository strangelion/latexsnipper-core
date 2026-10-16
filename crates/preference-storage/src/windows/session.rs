//! Shared trusted-host initialization. No parsing of guest permission claims.
use super::{generate_host_keys, WindowsPreferenceCheckpoint, WindowsPreferencePersistence};
use crate::PreferenceProviderError;
use latexsnipper_conversion::formula_completion::{
    preferences::persistence::{
        transactions::*, FormulaPreferenceStorageError, FormulaPreferenceStoragePermission,
    },
    scoped::FormulaDefinitionScope,
};
use serde::{Deserialize, Deserializer};
use std::{path::Path, time::Duration};

/// Closed metadata for trusted adapters. The adapter must bound encoded input
/// before deserializing (C/Python use 16 KiB). No keys/Debug/Serialize are exposed.
/// Neither this request nor its booleans prove consent or guest-root isolation.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowsPreferenceOpenRequest {
    schema_version: u16,
    owner: String,
    identity: Identity,
    persistence: Persistence,
    operation_timeout_ms: u32,
    permission: FormulaPreferenceStoragePermission,
    action: Action,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Identity {
    user_sha256: String,
    project_sha256: Option<String>,
    document_sha256: Option<String>,
}
impl From<Identity> for FormulaPreferenceStorageIdentity {
    fn from(value: Identity) -> Self {
        Self {
            user_sha256: value.user_sha256,
            project_sha256: value.project_sha256,
            document_sha256: value.document_sha256,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Persistence {
    LogonSession,
    ThisUserThisComputer,
}
impl From<Persistence> for WindowsPreferencePersistence {
    fn from(value: Persistence) -> Self {
        match value {
            Persistence::LogonSession => Self::LogonSession,
            Persistence::ThisUserThisComputer => Self::ThisUserThisComputer,
        }
    }
}
struct Generation(u64);
impl<'de> Deserialize<'de> for Generation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty()
            || value.len() > 20
            || (value.len() > 1 && value.starts_with('0'))
            || !value.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(serde::de::Error::custom("Invalid canonical u64 string."));
        }
        value
            .parse()
            .map(Self)
            .map_err(|_| serde::de::Error::custom("Invalid canonical u64 string."))
    }
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Action {
    OpenExisting {
        root_path: String,
    },
    InitializeExisting {
        root_path: String,
        generation: Generation,
    },
    CreateNew {
        parent_path: String,
        directory_name: String,
        generation: Generation,
    },
}

/// Separate owner, not a global registry. Close/drop only releases memory and
/// capabilities; never publishes pending feedback or removes a protected head.
pub struct OpenedWindowsPreferenceStorage {
    pub session: FormulaPreferenceStorageSession<WindowsPreferenceCheckpoint>,
    /// Retained for explicit bounded cleanup and physical guest-grant overlap checks.
    pub backend: WindowsPreferenceCheckpoint,
}

/// Redacted diagnostics shared by trusted language adapters. Not permission grants.
#[derive(Debug)]
pub enum WindowsPreferenceSessionError {
    Provider(PreferenceProviderError),
    Transaction(FormulaPreferenceTransactionError),
    UnsupportedSchema,
    AlreadyInitialized,
}
impl From<PreferenceProviderError> for WindowsPreferenceSessionError {
    fn from(value: PreferenceProviderError) -> Self {
        Self::Provider(value)
    }
}
impl From<FormulaPreferenceTransactionError> for WindowsPreferenceSessionError {
    fn from(value: FormulaPreferenceTransactionError) -> Self {
        Self::Transaction(value)
    }
}
impl WindowsPreferenceSessionError {
    /// Stable static category; never return paths, private metadata or OS errors.
    pub fn code(&self) -> &'static str {
        use FormulaPreferenceStorageError as Storage;
        use FormulaPreferenceTransactionError as Transaction;
        use PreferenceProviderError as Provider;
        match self {
            Self::UnsupportedSchema => "VERSION_MISMATCH",
            Self::AlreadyInitialized => "PREFERENCE_ALREADY_INITIALIZED",
            Self::Provider(value) => match value {
                Provider::InvalidConfiguration => "INVALID_ARGUMENT",
                Provider::PrivateDirectoryRequired => "PRIVATE_DIRECTORY_REQUIRED",
                Provider::Unavailable => "PREFERENCE_PROVIDER_UNAVAILABLE",
                Provider::InvalidRecord => "INVALID_PROTECTED_RECORD",
                Provider::IntegrityMismatch => "PREFERENCE_INTEGRITY_MISMATCH",
                Provider::DeadlineExceeded => "PREFERENCE_DEADLINE_EXCEEDED",
                Provider::CleanupLimit => "PREFERENCE_CLEANUP_LIMIT",
            },
            Self::Transaction(value) => match value {
                Transaction::Storage(value) => match value {
                    Storage::Disabled => "PREFERENCE_STORAGE_DISABLED",
                    Storage::InvalidKey => "INVALID_ARGUMENT",
                    Storage::InvalidSnapshot => "INVALID_SNAPSHOT",
                    Storage::AuthenticationFailed => "AUTHENTICATION_FAILED",
                    Storage::ScopeMismatch => "SCOPE_MISMATCH",
                    Storage::StaleCheckpoint => "STALE_CHECKPOINT",
                    Storage::SensitiveStorageDenied => "SENSITIVE_STORAGE_DENIED",
                    Storage::LimitExceeded => "LIMIT_EXCEEDED",
                },
                Transaction::InvalidRecord => "INVALID_PROTECTED_RECORD",
                Transaction::IdentityMismatch => "PREFERENCE_IDENTITY_MISMATCH",
                Transaction::NotFound => "PREFERENCE_STORAGE_NOT_FOUND",
                Transaction::ProviderUnavailable => "PREFERENCE_PROVIDER_UNAVAILABLE",
                Transaction::StaleTransaction => "PREFERENCE_STALE_TRANSACTION",
                Transaction::Conflict => "PREFERENCE_CONFLICT",
                Transaction::NotCommitted => "PREFERENCE_NOT_COMMITTED",
                Transaction::Indeterminate => "PREFERENCE_INDETERMINATE",
                Transaction::RecoveryRequired => "PREFERENCE_RECOVERY_REQUIRED",
                Transaction::Exhausted => "LIMIT_EXCEEDED",
            },
        }
    }
}
impl std::fmt::Display for WindowsPreferenceSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provider(value) => std::fmt::Display::fmt(value, f),
            Self::Transaction(value) => std::fmt::Display::fmt(value, f),
            Self::UnsupportedSchema => f.write_str("Unsupported preference storage schema."),
            Self::AlreadyInitialized => f.write_str("Protected preference state already exists."),
        }
    }
}
impl std::error::Error for WindowsPreferenceSessionError {}

impl WindowsPreferenceOpenRequest {
    /// Opens only explicitly requested native state, with default-off learning.
    /// Storage consent precedes directory IO, including provisioning. Hosts must
    /// keep the returned root excluded from actual guest grants at execution.
    pub fn open(self) -> Result<OpenedWindowsPreferenceStorage, WindowsPreferenceSessionError> {
        if self.schema_version != 1 {
            return Err(WindowsPreferenceSessionError::UnsupportedSchema);
        }
        if !self.permission.storage_enabled {
            return Err(FormulaPreferenceTransactionError::Storage(
                FormulaPreferenceStorageError::Disabled,
            )
            .into());
        }
        let identity: FormulaPreferenceStorageIdentity = self.identity.into();
        let persistence = self.persistence.into();
        let timeout = Duration::from_millis(self.operation_timeout_ms.into());
        let (backend, generation) = match self.action {
            Action::OpenExisting { root_path } => (
                WindowsPreferenceCheckpoint::open_verified_private_root(
                    Path::new(&root_path),
                    &self.owner,
                    identity.clone(),
                    persistence,
                    timeout,
                )?,
                None,
            ),
            Action::InitializeExisting {
                root_path,
                generation,
            } => (
                WindowsPreferenceCheckpoint::open_verified_private_root(
                    Path::new(&root_path),
                    &self.owner,
                    identity.clone(),
                    persistence,
                    timeout,
                )?,
                Some(generation.0),
            ),
            Action::CreateNew {
                parent_path,
                directory_name,
                generation,
            } => (
                WindowsPreferenceCheckpoint::create_private_root(
                    Path::new(&parent_path),
                    &directory_name,
                    &self.owner,
                    identity.clone(),
                    persistence,
                    timeout,
                )?,
                Some(generation.0),
            ),
        };
        let owner = backend.owner_sha256().to_owned();
        let session = match generation {
            None => FormulaPreferenceStorageSession::open(
                backend.clone(),
                owner,
                identity,
                self.permission,
            )?,
            Some(generation) => {
                // Refuse an existing/corrupt head; later CAS(None) protects races.
                if backend.read_current_record()?.is_some() {
                    return Err(WindowsPreferenceSessionError::AlreadyInitialized);
                }
                let scope = FormulaDefinitionScope {
                    user_sha256: identity.user_sha256,
                    project_sha256: identity.project_sha256,
                    document_sha256: identity.document_sha256,
                    generation,
                };
                FormulaPreferenceStorageSession::new(
                    backend.clone(),
                    owner,
                    scope,
                    generate_host_keys()?,
                )?
            }
        };
        Ok(OpenedWindowsPreferenceStorage { session, backend })
    }
}
