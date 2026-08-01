//! Opt-in ownership primitives for memory-mapped model sessions.
//!
//! The mapping is held in the same cache entry as the runtime session. This
//! makes the required drop order explicit and lets old and new model versions
//! coexist during an atomic cache switch.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use latexsnipper_foundation::{Result, SnipperError};
use memmap2::Mmap;
use sha2::{Digest, Sha256};

use crate::{RunRequest, RunResponse, RuntimeSession, SessionMetadata};

#[derive(Debug)]
pub struct ModelMapping {
    bytes: Mmap,
}

impl ModelMapping {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|error| {
            SnipperError::Model(format!(
                "failed to open mapped model '{}': {error}",
                path.display()
            ))
        })?;
        // SAFETY: the returned Mmap owns the operating-system mapping. Callers
        // cannot mutate it through this read-only type, and ModelMemoryOwner
        // keeps it alive for at least as long as its RuntimeSessionEntry.
        let bytes = unsafe { Mmap::map(&file) }.map_err(|error| {
            SnipperError::Model(format!("failed to map model '{}': {error}", path.display()))
        })?;
        Ok(Self { bytes })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelHash(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelVersion(pub String);

#[derive(Debug)]
pub struct ModelMemoryOwner {
    mapping: Arc<ModelMapping>,
    sha256: ModelHash,
    path: PathBuf,
}

impl ModelMemoryOwner {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mapping = Arc::new(ModelMapping::open(&path)?);
        let sha256 = ModelHash(format!("{:x}", Sha256::digest(mapping.as_bytes())));
        Ok(Self {
            mapping,
            sha256,
            path,
        })
    }

    pub fn mapping(&self) -> &Arc<ModelMapping> {
        &self.mapping
    }

    pub fn sha256(&self) -> &ModelHash {
        &self.sha256
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct RuntimeSessionEntry {
    // Drop the session before the owner. Field drop order is declaration order.
    session: Arc<dyn RuntimeSession>,
    memory_owner: Option<Arc<ModelMemoryOwner>>,
    model_version: ModelVersion,
}

impl RuntimeSessionEntry {
    pub fn new(
        session: Arc<dyn RuntimeSession>,
        memory_owner: Option<Arc<ModelMemoryOwner>>,
        model_version: ModelVersion,
    ) -> Self {
        Self {
            session,
            memory_owner,
            model_version,
        }
    }

    pub fn metadata(&self) -> &SessionMetadata {
        self.session.metadata()
    }

    pub fn run(&self, request: RunRequest) -> Result<RunResponse> {
        self.session.run(request)
    }

    pub fn memory_owner(&self) -> Option<&Arc<ModelMemoryOwner>> {
        self.memory_owner.as_ref()
    }

    pub fn model_version(&self) -> &ModelVersion {
        &self.model_version
    }
}

#[derive(Default)]
pub struct RuntimeSessionOwnerCache {
    entries: RwLock<HashMap<String, Arc<RuntimeSessionEntry>>>,
}

impl RuntimeSessionOwnerCache {
    /// Atomically publish a new version. Existing callers may retain the old
    /// Arc, so its session and mapping remain valid until their work completes.
    pub fn replace(
        &self,
        key: impl Into<String>,
        entry: RuntimeSessionEntry,
    ) -> Result<Option<Arc<RuntimeSessionEntry>>> {
        self.entries
            .write()
            .map_err(|_| SnipperError::Runtime("session owner cache poisoned".to_owned()))
            .map(|mut entries| entries.insert(key.into(), Arc::new(entry)))
    }

    pub fn get(&self, key: &str) -> Result<Option<Arc<RuntimeSessionEntry>>> {
        self.entries
            .read()
            .map_err(|_| SnipperError::Runtime("session owner cache poisoned".to_owned()))
            .map(|entries| entries.get(key).cloned())
    }

    pub fn clear_sessions(&self) {
        if let Ok(mut entries) = self.entries.write() {
            entries.clear();
        }
    }

    /// Report files kept alive by active cached model mappings. Callers can
    /// surface this list before upgrade/uninstall instead of hiding locks.
    pub fn occupied_files(&self) -> Result<Vec<PathBuf>> {
        let entries = self
            .entries
            .read()
            .map_err(|_| SnipperError::Runtime("session owner cache poisoned".to_owned()))?;
        let mut paths = entries
            .values()
            .filter_map(|entry| entry.memory_owner().map(|owner| owner.path().to_owned()))
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RunRequest, RunResponse, RuntimeKind, SessionMetadata, TensorMap};
    use tempfile::TempDir;

    struct NoopSession {
        metadata: SessionMetadata,
    }

    impl NoopSession {
        fn new(version: &str) -> Self {
            Self {
                metadata: SessionMetadata {
                    runtime: RuntimeKind::Custom("test".to_owned()),
                    model_id: Some(version.to_owned()),
                    requested_providers: Vec::new(),
                    effective_provider: Some("test".to_owned()),
                    fallback_chain: Vec::new(),
                    fallback_diagnostics: Vec::new(),
                    methods: Vec::new(),
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                },
            }
        }
    }

    impl RuntimeSession for NoopSession {
        fn metadata(&self) -> &SessionMetadata {
            &self.metadata
        }

        fn run(&self, _request: RunRequest) -> Result<RunResponse> {
            Ok(RunResponse {
                outputs: TensorMap::new(),
            })
        }
    }

    fn owner(root: &TempDir, version: &str) -> Arc<ModelMemoryOwner> {
        let path = root.path().join(format!("{version}.onnx"));
        std::fs::write(&path, version.as_bytes()).unwrap();
        Arc::new(ModelMemoryOwner::open(path).unwrap())
    }

    #[test]
    fn mapping_lives_with_session_and_clear_releases_cache_owner() {
        let root = TempDir::new().unwrap();
        let owner = owner(&root, "v1");
        let weak = Arc::downgrade(&owner);
        let cache = RuntimeSessionOwnerCache::default();
        cache
            .replace(
                "formula",
                RuntimeSessionEntry::new(
                    Arc::new(NoopSession::new("v1")),
                    Some(owner.clone()),
                    ModelVersion("v1".to_owned()),
                ),
            )
            .unwrap();
        drop(owner);
        assert!(weak.upgrade().is_some());
        cache.clear_sessions();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn hot_reload_keeps_old_and_new_versions_alive_for_active_callers() {
        let root = TempDir::new().unwrap();
        let cache = RuntimeSessionOwnerCache::default();
        cache
            .replace(
                "formula",
                RuntimeSessionEntry::new(
                    Arc::new(NoopSession::new("v1")),
                    Some(owner(&root, "v1")),
                    ModelVersion("v1".to_owned()),
                ),
            )
            .unwrap();
        let active_old = cache.get("formula").unwrap().unwrap();
        cache
            .replace(
                "formula",
                RuntimeSessionEntry::new(
                    Arc::new(NoopSession::new("v2")),
                    Some(owner(&root, "v2")),
                    ModelVersion("v2".to_owned()),
                ),
            )
            .unwrap();
        let active_new = cache.get("formula").unwrap().unwrap();
        assert_eq!(active_old.model_version().0, "v1");
        assert_eq!(active_new.model_version().0, "v2");
        assert_ne!(
            active_old.memory_owner().unwrap().sha256(),
            active_new.memory_owner().unwrap().sha256()
        );
        assert_eq!(cache.occupied_files().unwrap().len(), 1);
        assert_eq!(
            active_old
                .memory_owner()
                .unwrap()
                .path()
                .file_name()
                .unwrap(),
            "v1.onnx"
        );
        assert_eq!(
            active_new
                .memory_owner()
                .unwrap()
                .path()
                .file_name()
                .unwrap(),
            "v2.onnx"
        );
    }
}
