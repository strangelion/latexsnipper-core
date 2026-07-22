//! Deterministic cache identity for compiled Core ML models.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{coreml_error, CoreMlResult};

const CACHE_SCHEMA: &[u8] = b"latexsnipper-coreml-cache-v1\0";

#[derive(Debug, Clone)]
pub(crate) struct CoreMlCache {
    root: PathBuf,
}

impl CoreMlCache {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn compiled_path(
        &self,
        source: &Path,
        runtime_version: &str,
    ) -> CoreMlResult<PathBuf> {
        let mut hash = Sha256::new();
        hash.update(CACHE_SCHEMA);
        hash.update(runtime_version.as_bytes());
        hash.update([0]);
        hash.update(std::env::consts::ARCH.as_bytes());
        hash.update([0]);
        hash_artifact(source, &mut hash)?;
        Ok(self
            .root
            .join(format!("{}.mlmodelc", hex::encode(hash.finalize()))))
    }

    pub(crate) fn prepare_root(&self) -> CoreMlResult<()> {
        fs::create_dir_all(&self.root).map_err(|error| {
            coreml_error(format!(
                "create Core ML cache directory '{}': {error}",
                self.root.display()
            ))
        })?;
        let metadata = fs::symlink_metadata(&self.root).map_err(|error| {
            coreml_error(format!(
                "inspect Core ML cache directory '{}': {error}",
                self.root.display()
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(coreml_error(format!(
                "Core ML cache root must be a real directory: {}",
                self.root.display()
            )));
        }
        Ok(())
    }

    pub(crate) fn temporary_path(&self, final_path: &Path) -> CoreMlResult<PathBuf> {
        let name = final_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| coreml_error("compiled Core ML cache path has no UTF-8 file name"))?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| coreml_error(format!("system clock precedes Unix epoch: {error}")))?
            .as_nanos();
        Ok(self.root.join(format!(
            ".{name}.{}.{}.tmp.mlmodelc",
            std::process::id(),
            nonce
        )))
    }

    pub(crate) fn is_compiled_model(path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mlmodelc"))
            && path.is_dir()
    }
}

fn hash_artifact(path: &Path, hash: &mut Sha256) -> CoreMlResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        coreml_error(format!(
            "inspect Core ML artifact '{}': {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(coreml_error(format!(
            "Core ML artifacts may not contain symbolic links: {}",
            path.display()
        )));
    }
    if metadata.is_file() {
        hash.update(b"file\0");
        hash.update(metadata.len().to_le_bytes());
        hash_file(path, hash)?;
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(coreml_error(format!(
            "Core ML artifact is neither a file nor a directory: {}",
            path.display()
        )));
    }

    hash.update(b"directory\0");
    let mut entries = fs::read_dir(path)
        .map_err(|error| {
            coreml_error(format!(
                "read Core ML package directory '{}': {error}",
                path.display()
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            coreml_error(format!(
                "enumerate Core ML package directory '{}': {error}",
                path.display()
            ))
        })?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_str().ok_or_else(|| {
            coreml_error(format!(
                "Core ML package contains a non-UTF-8 path below '{}'",
                path.display()
            ))
        })?;
        hash.update(name.len().to_le_bytes());
        hash.update(name.as_bytes());
        hash_artifact(&entry.path(), hash)?;
    }
    Ok(())
}

fn hash_file(path: &Path, hash: &mut Sha256) -> CoreMlResult<()> {
    let mut file = File::open(path).map_err(|error| {
        coreml_error(format!(
            "open Core ML artifact '{}': {error}",
            path.display()
        ))
    })?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            coreml_error(format!(
                "read Core ML artifact '{}': {error}",
                path.display()
            ))
        })?;
        if read == 0 {
            return Ok(());
        }
        hash.update(&buffer[..read]);
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn fixture_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "latexsnipper-coreml-cache-test-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn package_hash_is_deterministic_and_content_sensitive() {
        let root = fixture_root("hash");
        let package = root.join("model.mlpackage");
        fs::create_dir_all(package.join("Data")).unwrap();
        File::create(package.join("Manifest.json"))
            .unwrap()
            .write_all(b"manifest")
            .unwrap();
        File::create(package.join("Data").join("weights.bin"))
            .unwrap()
            .write_all(b"weights-a")
            .unwrap();

        let cache = CoreMlCache::new(root.join("cache"));
        let first = cache.compiled_path(&package, "macOS-test").unwrap();
        let second = cache.compiled_path(&package, "macOS-test").unwrap();
        assert_eq!(first, second);

        File::create(package.join("Data").join("weights.bin"))
            .unwrap()
            .write_all(b"weights-b")
            .unwrap();
        let changed = cache.compiled_path(&package, "macOS-test").unwrap();
        assert_ne!(first, changed);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_version_participates_in_cache_identity() {
        let root = fixture_root("version");
        fs::create_dir_all(&root).unwrap();
        let model = root.join("model.mlmodel");
        File::create(&model).unwrap().write_all(b"model").unwrap();
        let cache = CoreMlCache::new(root.join("cache"));
        assert_ne!(
            cache.compiled_path(&model, "macOS-a").unwrap(),
            cache.compiled_path(&model, "macOS-b").unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
