use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{tensorrt_error, TensorRtResult};
use crate::flavor::TensorRtFlavor;
use crate::options::TensorRtOptions;

static TEMPORARY_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
struct CacheIdentity<'a> {
    schema: u32,
    runtime_id: &'a str,
    model_sha256: String,
    runtime_version: &'a str,
    device_fingerprint: &'a str,
    precision: crate::options::TensorRtPrecision,
    workspace_bytes: u64,
    profiles: &'a std::collections::BTreeMap<String, crate::options::ShapeProfile>,
}

pub(crate) fn cache_key(
    model: &[u8],
    flavor: TensorRtFlavor,
    runtime_version: &str,
    device_fingerprint: &str,
    options: &TensorRtOptions,
) -> TensorRtResult<String> {
    let identity = CacheIdentity {
        schema: 1,
        runtime_id: flavor.runtime_id(),
        model_sha256: hex::encode(Sha256::digest(model)),
        runtime_version,
        device_fingerprint,
        precision: options.precision,
        workspace_bytes: options.workspace_bytes,
        profiles: &options.profiles,
    };
    let encoded = serde_json::to_vec(&identity)
        .map_err(|error| tensorrt_error(format!("failed to encode engine cache key: {error}")))?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

pub(crate) struct EngineCache {
    root: PathBuf,
}

impl EngineCache {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.engine"))
    }

    pub(crate) fn load(&self, key: &str) -> TensorRtResult<Option<Vec<u8>>> {
        let path = self.path(key);
        match fs::read(&path) {
            Ok(bytes) if bytes.is_empty() => {
                self.invalidate(key)?;
                Ok(None)
            }
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(tensorrt_error(format!(
                "failed to read engine cache {}: {error}",
                path.display()
            ))),
        }
    }

    pub(crate) fn store(&self, key: &str, bytes: &[u8]) -> TensorRtResult<PathBuf> {
        if bytes.is_empty() {
            return Err(tensorrt_error("refusing to cache an empty TensorRT engine"));
        }
        fs::create_dir_all(&self.root).map_err(|error| {
            tensorrt_error(format!(
                "failed to create engine cache {}: {error}",
                self.root.display()
            ))
        })?;
        let destination = self.path(key);
        if destination.is_file() {
            return Ok(destination);
        }
        let temporary = self.root.join(format!(
            ".{key}.{}.{}.tmp",
            std::process::id(),
            TEMPORARY_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| {
                tensorrt_error(format!(
                    "failed to create temporary engine cache {}: {error}",
                    temporary.display()
                ))
            })?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| {
                tensorrt_error(format!(
                    "failed to write temporary engine cache {}: {error}",
                    temporary.display()
                ))
            })?;
        match fs::rename(&temporary, &destination) {
            Ok(()) => Ok(destination),
            Err(_) if destination.is_file() => {
                let _ = fs::remove_file(&temporary);
                Ok(destination)
            }
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(tensorrt_error(format!(
                    "failed to publish engine cache {}: {error}",
                    destination.display()
                )))
            }
        }
    }

    pub(crate) fn invalidate(&self, key: &str) -> TensorRtResult<()> {
        let path = self.path(key);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(tensorrt_error(format!(
                "failed to invalidate engine cache {}: {error}",
                path.display()
            ))),
        }
    }
}

pub(crate) fn read_artifact(path: &Path, kind: &str) -> TensorRtResult<Vec<u8>> {
    fs::read(path).map_err(|error| {
        tensorrt_error(format!(
            "failed to read {kind} artifact {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::options::{ShapeProfile, TensorRtPrecision};

    fn options(precision: TensorRtPrecision) -> TensorRtOptions {
        TensorRtOptions {
            library_path: None,
            cache: true,
            cache_dir: PathBuf::from("cache"),
            precision,
            workspace_bytes: 1024,
            device_id: 0,
            profiles: BTreeMap::from([(
                "x".to_owned(),
                ShapeProfile {
                    min: vec![1, 1],
                    opt: vec![1, 2],
                    max: vec![4, 4],
                },
            )]),
        }
    }

    #[test]
    fn cache_key_changes_with_every_compatibility_dimension() {
        let fp32 = cache_key(
            b"model",
            TensorRtFlavor::Standard,
            "10.0",
            "gpu-a",
            &options(TensorRtPrecision::Fp32),
        )
        .unwrap();
        let fp16 = cache_key(
            b"model",
            TensorRtFlavor::Standard,
            "10.0",
            "gpu-a",
            &options(TensorRtPrecision::Fp16),
        )
        .unwrap();
        let runtime = cache_key(
            b"model",
            TensorRtFlavor::Standard,
            "10.1",
            "gpu-a",
            &options(TensorRtPrecision::Fp32),
        )
        .unwrap();
        let gpu = cache_key(
            b"model",
            TensorRtFlavor::Standard,
            "10.0",
            "gpu-b",
            &options(TensorRtPrecision::Fp32),
        )
        .unwrap();
        let rtx = cache_key(
            b"model",
            TensorRtFlavor::Rtx,
            "10.0",
            "gpu-a",
            &options(TensorRtPrecision::Fp32),
        )
        .unwrap();
        assert_ne!(fp32, fp16);
        assert_ne!(fp32, runtime);
        assert_ne!(fp32, gpu);
        assert_ne!(fp32, rtx);
    }
}
