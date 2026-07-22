use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::error::{tensorrt_error, TensorRtResult};
use crate::ffi::TensorRtApi;
use crate::flavor::TensorRtFlavor;

type ApiCache = HashMap<(PathBuf, TensorRtFlavor), Arc<TensorRtApi>>;
static API_CACHE: OnceLock<Mutex<ApiCache>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct TensorRtLibraryLocator {
    explicit: Option<PathBuf>,
    flavor: TensorRtFlavor,
}

impl TensorRtLibraryLocator {
    pub(crate) fn new(explicit: Option<PathBuf>, flavor: TensorRtFlavor) -> Self {
        Self { explicit, flavor }
    }

    pub(crate) fn load(&self) -> TensorRtResult<Arc<TensorRtApi>> {
        let candidates = self.candidates();
        let mut failures = Vec::new();
        for candidate in &candidates {
            match load_cached(candidate, self.flavor) {
                Ok(api) => {
                    log::info!(
                        "Loaded {} runtime bridge from {}",
                        self.flavor.display_name(),
                        candidate.display()
                    );
                    return Ok(api);
                }
                Err(error) => failures.push(format!("{} ({error})", candidate.display())),
            }
        }
        let environment = self.flavor.environment();
        Err(tensorrt_error(format!(
            "{} runtime not installed or could not be loaded; set {environment} or configure libraryPath. Tried: {}",
            self.flavor.display_name(),
            failures.join("; ")
        )))
    }

    pub fn candidates(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(explicit) = &self.explicit {
            roots.push(explicit.clone());
        }
        if let Some(home) =
            std::env::var_os(self.flavor.environment()).filter(|value| !value.is_empty())
        {
            roots.push(PathBuf::from(home));
        }
        if let Ok(executable) = std::env::current_exe() {
            if let Some(application_dir) = executable.parent() {
                roots.push(
                    application_dir
                        .join("resources/runtime")
                        .join(self.flavor.resource_directory()),
                );
            }
        }
        let mut candidates = Vec::new();
        for root in roots {
            expand_root(&root, self.flavor, &mut candidates);
        }
        candidates.push(PathBuf::from(self.flavor.bridge_name()));
        deduplicate(candidates)
    }
}

fn load_cached(path: &Path, flavor: TensorRtFlavor) -> TensorRtResult<Arc<TensorRtApi>> {
    let cache = API_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .map_err(|_| tensorrt_error("TensorRT runtime library cache was poisoned"))?;
    let key = (path.to_path_buf(), flavor);
    if let Some(api) = cache.get(&key) {
        return Ok(Arc::clone(api));
    }
    let api = Arc::new(TensorRtApi::load(path, flavor)?);
    cache.insert(key, Arc::clone(&api));
    Ok(api)
}

fn expand_root(root: &Path, flavor: TensorRtFlavor, candidates: &mut Vec<PathBuf>) {
    if root.is_file() || root.extension().is_some_and(is_dynamic_library_extension) {
        candidates.push(root.to_path_buf());
        return;
    }
    let name = flavor.bridge_name();
    for relative in [
        Path::new(name).to_path_buf(),
        Path::new("bin").join(name),
        Path::new("lib").join(name),
        Path::new(flavor.resource_directory())
            .join("lib")
            .join(name),
    ] {
        candidates.push(root.join(relative));
    }
}

fn is_dynamic_library_extension(extension: &std::ffi::OsStr) -> bool {
    let extension = extension.to_string_lossy();
    extension.eq_ignore_ascii_case("dll")
        || extension.eq_ignore_ascii_case("so")
        || extension.eq_ignore_ascii_case("dylib")
}

fn deduplicate(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_root_expands_packaged_layout() {
        let root = PathBuf::from("runtime-root");
        let candidates =
            TensorRtLibraryLocator::new(Some(root.clone()), TensorRtFlavor::Standard).candidates();
        assert_eq!(
            candidates[0],
            root.join(TensorRtFlavor::Standard.bridge_name())
        );
        assert_eq!(
            candidates[1],
            root.join("bin")
                .join(TensorRtFlavor::Standard.bridge_name())
        );
    }

    #[test]
    fn rtx_uses_an_independent_environment_and_bridge_name() {
        let root = PathBuf::from("rtx-root");
        let candidates =
            TensorRtLibraryLocator::new(Some(root.clone()), TensorRtFlavor::Rtx).candidates();
        assert_eq!(candidates[0], root.join(TensorRtFlavor::Rtx.bridge_name()));
        assert_ne!(
            TensorRtFlavor::Rtx.environment(),
            TensorRtFlavor::Standard.environment()
        );
    }
}
