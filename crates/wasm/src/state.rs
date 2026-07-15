use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use latexsnipper_runtime::{normalize_key, MemoryModelResolver};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{WasmError, WasmErrorCode};

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLimits {
    pub per_artifact_bytes: u64,
    pub total_model_bytes: u64,
    pub max_image_width: u32,
    pub max_image_height: u32,
    pub max_image_pixels: u64,
    pub max_table_elements: u64,
    pub max_result_bytes: u64,
    pub profile: &'static str,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            per_artifact_bytes: 128 * MIB,
            total_model_bytes: 256 * MIB,
            max_image_width: 8_192,
            max_image_height: 8_192,
            max_image_pixels: 40_000_000,
            max_table_elements: 4_096,
            max_result_bytes: 16 * MIB,
            profile: "balanced",
        }
    }
}

impl MemoryLimits {
    pub fn low_memory() -> Self {
        Self {
            per_artifact_bytes: 64 * MIB,
            total_model_bytes: 128 * MIB,
            max_image_width: 4_096,
            max_image_height: 4_096,
            max_image_pixels: 16_000_000,
            max_table_elements: 2_048,
            max_result_bytes: 8 * MIB,
            profile: "low-memory",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryUsage {
    pub artifact_count: usize,
    pub total_model_bytes: u64,
    pub pending_bytes: u64,
    pub session_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArtifactInfo {
    pub name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub last_access: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadOutcome {
    pub name: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub duplicate: bool,
    pub duplicate_of: Option<String>,
    pub staged: bool,
    pub evicted: Vec<String>,
}

#[derive(Debug, Clone)]
struct ArtifactMetadata {
    size_bytes: u64,
    sha256: String,
    last_access: u64,
}

#[derive(Debug)]
enum PendingChange {
    Store {
        name: String,
        bytes: Vec<u8>,
        metadata: ArtifactMetadata,
    },
    Remove(String),
}

pub struct WasmState {
    pub resolver: Arc<MemoryModelResolver>,
    artifacts: HashMap<String, ArtifactMetadata>,
    limits: MemoryLimits,
    pending: Option<Vec<PendingChange>>,
    access_clock: u64,
    cancellation_requested: bool,
}

impl WasmState {
    pub fn new() -> Self {
        Self {
            resolver: Arc::new(MemoryModelResolver::new()),
            artifacts: HashMap::new(),
            limits: MemoryLimits::default(),
            pending: None,
            access_clock: 0,
            cancellation_requested: false,
        }
    }

    pub fn limits(&self) -> MemoryLimits {
        self.limits.clone()
    }

    pub fn usage(&self) -> MemoryUsage {
        MemoryUsage {
            artifact_count: self.artifacts.len(),
            total_model_bytes: self.artifacts.values().map(|value| value.size_bytes).sum(),
            pending_bytes: self
                .pending
                .as_ref()
                .into_iter()
                .flatten()
                .filter_map(|change| match change {
                    PendingChange::Store { metadata, .. } => Some(metadata.size_bytes),
                    PendingChange::Remove(_) => None,
                })
                .sum(),
            session_bytes: None,
        }
    }

    pub fn list(&self) -> Vec<ModelArtifactInfo> {
        let mut values: Vec<_> = self
            .artifacts
            .iter()
            .map(|(name, metadata)| ModelArtifactInfo {
                name: name.clone(),
                size_bytes: metadata.size_bytes,
                sha256: metadata.sha256.clone(),
                last_access: metadata.last_access,
            })
            .collect();
        values.sort_by(|left, right| left.name.cmp(&right.name));
        values
    }

    pub fn is_loaded(&self, name: &str) -> bool {
        self.artifacts.contains_key(&normalize_key(name))
    }

    pub fn load(
        &mut self,
        name: &str,
        bytes: Vec<u8>,
        expected_sha256: Option<&str>,
    ) -> Result<LoadOutcome, WasmError> {
        let name = normalize_key(name);
        if name.is_empty() || name.ends_with('/') {
            return Err(WasmError::new(
                WasmErrorCode::InvalidArgument,
                "Model artifact name must be a non-empty file key",
            ));
        }
        let size_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if size_bytes > self.limits.per_artifact_bytes {
            return Err(memory_error(
                "Artifact exceeds the per-artifact memory limit",
                size_bytes,
                &self.limits,
            ));
        }

        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        if let Some(expected) = expected_sha256 {
            let expected = expected
                .strip_prefix("sha256:")
                .unwrap_or(expected)
                .to_ascii_lowercase();
            if expected != sha256 {
                return Err(WasmError::new(
                    WasmErrorCode::ModelChecksumMismatch,
                    format!("Checksum mismatch for {name}"),
                )
                .with_details(serde_json::json!({
                    "expectedSha256": expected,
                    "actualSha256": sha256,
                })));
            }
        }

        self.access_clock = self.access_clock.saturating_add(1);
        if self
            .artifacts
            .get(&name)
            .is_some_and(|metadata| metadata.sha256 == sha256)
        {
            if let Some(metadata) = self.artifacts.get_mut(&name) {
                metadata.last_access = self.access_clock;
            }
            return Ok(LoadOutcome {
                name,
                size_bytes,
                sha256,
                duplicate: true,
                duplicate_of: None,
                staged: false,
                evicted: Vec::new(),
            });
        }

        let duplicate_of = self
            .artifacts
            .iter()
            .find_map(|(key, metadata)| (metadata.sha256 == sha256).then(|| key.clone()));
        let metadata = ArtifactMetadata {
            size_bytes,
            sha256: sha256.clone(),
            last_access: self.access_clock,
        };
        let staged = self.pending.is_some();
        let evicted = if let Some(pending) = self.pending.as_mut() {
            pending.push(PendingChange::Store {
                name: name.clone(),
                bytes,
                metadata,
            });
            Vec::new()
        } else {
            self.apply_changes(vec![PendingChange::Store {
                name: name.clone(),
                bytes,
                metadata,
            }])?
        };

        Ok(LoadOutcome {
            name,
            size_bytes,
            sha256,
            duplicate: duplicate_of.is_some(),
            duplicate_of,
            staged,
            evicted,
        })
    }

    pub fn unload(&mut self, name: &str) -> Result<bool, WasmError> {
        let name = normalize_key(name);
        if let Some(pending) = self.pending.as_mut() {
            let existed = self.artifacts.contains_key(&name);
            pending.push(PendingChange::Remove(name));
            return Ok(existed);
        }
        if !self.artifacts.contains_key(&name) {
            return Ok(false);
        }
        self.apply_changes(vec![PendingChange::Remove(name)])?;
        Ok(true)
    }

    pub fn clear(&mut self) {
        self.pending = None;
        self.artifacts.clear();
        self.resolver.clear();
    }

    pub fn begin_update(&mut self) -> Result<(), WasmError> {
        if self.pending.is_some() {
            return Err(WasmError::new(
                WasmErrorCode::UpdateAlreadyActive,
                "A model update transaction is already active",
            ));
        }
        self.pending = Some(Vec::new());
        Ok(())
    }

    pub fn commit_update(&mut self) -> Result<Vec<String>, WasmError> {
        let changes = self.pending.take().ok_or_else(|| {
            WasmError::new(
                WasmErrorCode::UpdateNotActive,
                "No model update transaction is active",
            )
        })?;
        self.apply_changes(changes)
    }

    pub fn rollback_update(&mut self) -> Result<(), WasmError> {
        if self.pending.take().is_none() {
            return Err(WasmError::new(
                WasmErrorCode::UpdateNotActive,
                "No model update transaction is active",
            ));
        }
        Ok(())
    }

    pub fn set_limits(&mut self, limits: MemoryLimits) -> Result<Vec<String>, WasmError> {
        if limits.per_artifact_bytes == 0
            || limits.total_model_bytes == 0
            || limits.max_image_width == 0
            || limits.max_image_height == 0
            || limits.max_image_pixels == 0
            || limits.max_table_elements == 0
            || limits.max_result_bytes == 0
            || limits.per_artifact_bytes > limits.total_model_bytes
        {
            return Err(WasmError::new(
                WasmErrorCode::InvalidArgument,
                "Memory limits must be positive and per-artifact must not exceed total",
            ));
        }
        if self
            .artifacts
            .values()
            .any(|artifact| artifact.size_bytes > limits.per_artifact_bytes)
        {
            return Err(WasmError::new(
                WasmErrorCode::ModelMemoryLimit,
                "An existing artifact exceeds the requested per-artifact limit",
            ));
        }
        let previous = self.limits.clone();
        self.limits = limits;
        match self.apply_changes(Vec::new()) {
            Ok(evicted) => Ok(evicted),
            Err(error) => {
                self.limits = previous;
                Err(error)
            }
        }
    }

    pub fn touch_all(&mut self) {
        self.access_clock = self.access_clock.saturating_add(1);
        for metadata in self.artifacts.values_mut() {
            metadata.last_access = self.access_clock;
        }
    }

    pub fn request_cancellation(&mut self) {
        self.cancellation_requested = true;
    }

    pub fn cancellation_requested(&self) -> bool {
        self.cancellation_requested
    }

    pub fn reset_cancellation(&mut self) {
        self.cancellation_requested = false;
    }

    fn apply_changes(&mut self, changes: Vec<PendingChange>) -> Result<Vec<String>, WasmError> {
        let mut projected = self.artifacts.clone();
        let protected: HashSet<String> = changes
            .iter()
            .filter_map(|change| match change {
                PendingChange::Store { name, .. } => Some(name.clone()),
                PendingChange::Remove(_) => None,
            })
            .collect();

        for change in &changes {
            match change {
                PendingChange::Store { name, metadata, .. } => {
                    projected.insert(name.clone(), metadata.clone());
                }
                PendingChange::Remove(name) => {
                    projected.remove(name);
                }
            }
        }

        let mut evicted = Vec::new();
        while projected
            .values()
            .map(|value| value.size_bytes)
            .sum::<u64>()
            > self.limits.total_model_bytes
        {
            let candidate = projected
                .iter()
                .filter(|(name, _)| !protected.contains(*name))
                .min_by_key(|(_, metadata)| metadata.last_access)
                .map(|(name, _)| name.clone())
                .ok_or_else(|| {
                    memory_error(
                        "Model update exceeds the total memory limit",
                        projected.values().map(|value| value.size_bytes).sum(),
                        &self.limits,
                    )
                })?;
            projected.remove(&candidate);
            evicted.push(candidate);
        }

        for name in &evicted {
            self.resolver.remove(name);
        }
        for change in changes {
            match change {
                PendingChange::Store { name, bytes, .. } => self.resolver.store(name, bytes),
                PendingChange::Remove(name) => {
                    self.resolver.remove(&name);
                }
            }
        }
        self.artifacts = projected;
        Ok(evicted)
    }
}

impl Default for WasmState {
    fn default() -> Self {
        Self::new()
    }
}

fn memory_error(message: &str, requested_bytes: u64, limits: &MemoryLimits) -> WasmError {
    WasmError::new(WasmErrorCode::ModelMemoryLimit, message).with_details(serde_json::json!({
        "requestedBytes": requested_bytes,
        "perArtifactBytes": limits.per_artifact_bytes,
        "totalModelBytes": limits.total_model_bytes,
    }))
}

thread_local! {
    pub static STATE: RefCell<WasmState> = RefCell::new(WasmState::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_limits(total: u64) -> MemoryLimits {
        MemoryLimits {
            per_artifact_bytes: total,
            total_model_bytes: total,
            max_image_width: 10,
            max_image_height: 10,
            max_image_pixels: 100,
            max_table_elements: 100,
            max_result_bytes: 1_024,
            profile: "test",
        }
    }

    #[test]
    fn checksum_and_per_artifact_limits_are_enforced() {
        let mut state = WasmState::new();
        state.set_limits(small_limits(4)).unwrap();
        let mismatch = state.load("a.bin", vec![1], Some("00")).unwrap_err();
        assert_eq!(mismatch.code, WasmErrorCode::ModelChecksumMismatch);
        let too_large = state.load("a.bin", vec![0; 5], None).unwrap_err();
        assert_eq!(too_large.code, WasmErrorCode::ModelMemoryLimit);
    }

    #[test]
    fn total_limit_evicts_the_least_recently_used_artifact() {
        let mut state = WasmState::new();
        state.set_limits(small_limits(4)).unwrap();
        state.load("old.bin", vec![1, 2], None).unwrap();
        let outcome = state.load("new.bin", vec![3, 4, 5], None).unwrap();
        assert_eq!(outcome.evicted, vec!["old.bin"]);
        assert!(!state.is_loaded("old.bin"));
        assert!(state.is_loaded("new.bin"));
    }

    #[test]
    fn failed_transaction_does_not_modify_live_artifacts() {
        let mut state = WasmState::new();
        state.set_limits(small_limits(4)).unwrap();
        state.load("live.bin", vec![1, 2], None).unwrap();
        state.begin_update().unwrap();
        state.load("first.bin", vec![3, 4, 5], None).unwrap();
        state.load("second.bin", vec![6, 7, 8], None).unwrap();

        let error = state.commit_update().unwrap_err();
        assert_eq!(error.code, WasmErrorCode::ModelMemoryLimit);
        assert!(state.is_loaded("live.bin"));
        assert!(!state.is_loaded("first.bin"));
        assert!(!state.is_loaded("second.bin"));
    }
}
