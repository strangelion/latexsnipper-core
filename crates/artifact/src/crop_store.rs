use std::path::{Path, PathBuf};

use latexsnipper_ast::Rect;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropPrivacyConsent {
    ExplicitDebugOrBenchmark,
}

#[derive(Debug, Clone)]
pub struct CropArtifactPolicy {
    pub root: PathBuf,
    pub max_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableCropReference {
    pub artifact_ref: String,
    pub crop_hash: String,
    pub crop_bounds: Rect,
    pub source_image_hash: String,
    /// Relative to the explicitly configured debug artifact root.
    pub content_ref: String,
}

/// Opt-in debug/benchmark crop store.
///
/// Construction requires an explicit privacy-consent token. Production
/// pipelines have no store by default, and the reference is kept in runtime
/// evidence rather than embedded in the document AST.
#[derive(Debug)]
pub struct DebugCropStore {
    policy: CropArtifactPolicy,
}

impl DebugCropStore {
    pub fn new(policy: CropArtifactPolicy, _consent: CropPrivacyConsent) -> std::io::Result<Self> {
        if policy.max_files == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "max_files must be greater than zero",
            ));
        }
        if policy.root.as_os_str().is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "crop artifact root must be explicit",
            ));
        }
        std::fs::create_dir_all(policy.root.join("crops"))?;
        Ok(Self { policy })
    }

    pub fn save_png(
        &self,
        png: &[u8],
        crop_bounds: Rect,
        source_image_bytes: &[u8],
    ) -> std::io::Result<DurableCropReference> {
        let crop_hash = format!("{:x}", Sha256::digest(png));
        let source_image_hash = format!("{:x}", Sha256::digest(source_image_bytes));
        let file_name = format!("{crop_hash}.png");
        let content_ref = format!("crops/{file_name}");
        let destination = self.policy.root.join(&content_ref);
        if !destination.exists() {
            let temporary = destination.with_extension(format!("png.tmp-{}", std::process::id()));
            std::fs::write(&temporary, png)?;
            match std::fs::rename(&temporary, &destination) {
                Ok(()) => {}
                Err(_error) if destination.exists() => {
                    let _ = std::fs::remove_file(&temporary);
                }
                Err(error) => {
                    let _ = std::fs::remove_file(&temporary);
                    return Err(error);
                }
            }
        }
        self.enforce_retention()?;
        Ok(DurableCropReference {
            artifact_ref: format!("table-crop:{crop_hash}"),
            crop_hash,
            crop_bounds,
            source_image_hash,
            content_ref,
        })
    }

    pub fn root(&self) -> &Path {
        &self.policy.root
    }

    pub fn cleanup_all(&self) -> std::io::Result<usize> {
        let mut removed = 0;
        for entry in std::fs::read_dir(self.policy.root.join("crops"))? {
            let path = entry?.path();
            if path.is_file() {
                std::fs::remove_file(path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn enforce_retention(&self) -> std::io::Result<()> {
        let mut files = std::fs::read_dir(self.policy.root.join("crops"))?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let modified = entry.metadata().ok()?.modified().ok()?;
                entry.path().is_file().then_some((modified, entry.path()))
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|item| item.0);
        let remove_count = files.len().saturating_sub(self.policy.max_files);
        for (_, path) in files.into_iter().take(remove_count) {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_store_is_explicit_hash_addressed_and_cleanable() {
        let temporary = tempfile::tempdir().unwrap();
        let store = DebugCropStore::new(
            CropArtifactPolicy {
                root: temporary.path().to_path_buf(),
                max_files: 1,
            },
            CropPrivacyConsent::ExplicitDebugOrBenchmark,
        )
        .unwrap();
        let reference = store
            .save_png(
                b"not-a-real-png-but-hashable",
                Rect::new(1.0, 2.0, 3.0, 4.0),
                b"source",
            )
            .unwrap();
        assert!(reference.artifact_ref.starts_with("table-crop:"));
        assert_eq!(reference.crop_hash.len(), 64);
        assert_eq!(reference.source_image_hash.len(), 64);
        assert_eq!(store.cleanup_all().unwrap(), 1);
    }
}
