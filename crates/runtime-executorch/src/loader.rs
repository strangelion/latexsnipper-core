//! Runtime-only discovery of the versioned ExecuTorch bridge.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::error::{executorch_error, ExecuTorchResult};
use crate::ffi::ExecuTorchApi;

const EXECUTORCH_ENV: &str = "LATEXSNIPPER_EXECUTORCH_HOME";

// Native runtimes may own process-global backend registries. Keep every
// successfully loaded bridge alive until process exit, while sessions remain
// independently owned and destroyed.
static API_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<ExecuTorchApi>>>> = OnceLock::new();

#[derive(Debug, Clone, Default)]
pub struct ExecuTorchLibraryLocator {
    explicit: Option<PathBuf>,
}

impl ExecuTorchLibraryLocator {
    pub fn new(explicit: Option<PathBuf>) -> Self {
        Self { explicit }
    }

    pub(crate) fn load(&self) -> ExecuTorchResult<Arc<ExecuTorchApi>> {
        let candidates = self.candidates();
        let mut failures = Vec::new();
        for candidate in &candidates {
            match load_cached(candidate) {
                Ok(api) => {
                    log::info!("Loaded ExecuTorch runtime from {}", candidate.display());
                    return Ok(api);
                }
                Err(error) => failures.push(format!("{} ({error})", candidate.display())),
            }
        }

        let detail = if failures.is_empty() {
            "no candidate library paths were generated".to_owned()
        } else {
            failures.join("; ")
        };
        Err(executorch_error(format!(
            "ExecuTorch runtime not installed or could not be loaded; set {EXECUTORCH_ENV} or configure an explicit library path. Tried: {detail}"
        )))
    }

    pub fn candidates(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(explicit) = &self.explicit {
            roots.push(explicit.clone());
        }
        if let Some(home) = std::env::var_os(EXECUTORCH_ENV).filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(home));
        }
        if let Ok(executable) = std::env::current_exe() {
            if let Some(application_dir) = executable.parent() {
                roots.push(application_dir.join("resources/runtime/executorch"));
            }
        }

        let mut candidates = Vec::new();
        for root in roots {
            expand_root(&root, &mut candidates);
        }
        candidates.push(PathBuf::from(platform_library_name()));
        deduplicate(candidates)
    }
}

fn load_cached(path: &Path) -> ExecuTorchResult<Arc<ExecuTorchApi>> {
    let cache = API_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .map_err(|_| executorch_error("ExecuTorch runtime library cache was poisoned"))?;
    if let Some(api) = cache.get(path) {
        return Ok(Arc::clone(api));
    }
    let api = Arc::new(ExecuTorchApi::load(path)?);
    cache.insert(path.to_path_buf(), Arc::clone(&api));
    Ok(api)
}

fn expand_root(root: &Path, candidates: &mut Vec<PathBuf>) {
    if root.is_file() || root.extension().is_some_and(is_dynamic_library_extension) {
        candidates.push(root.to_path_buf());
        return;
    }

    let name = platform_library_name();
    for relative in [
        Path::new(name).to_path_buf(),
        Path::new("bin").join(name),
        Path::new("lib").join(name),
        Path::new("executorch").join("lib").join(name),
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

#[cfg(target_os = "windows")]
const fn platform_library_name() -> &'static str {
    "latexsnipper_executorch_bridge.dll"
}

#[cfg(target_os = "linux")]
const fn platform_library_name() -> &'static str {
    "liblatexsnipper_executorch_bridge.so"
}

#[cfg(target_os = "macos")]
const fn platform_library_name() -> &'static str {
    "liblatexsnipper_executorch_bridge.dylib"
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
const fn platform_library_name() -> &'static str {
    "latexsnipper_executorch_bridge"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_runtime_root_expands_packaged_layouts() {
        let root = PathBuf::from("runtime-root");
        let candidates = ExecuTorchLibraryLocator::new(Some(root.clone())).candidates();
        assert_eq!(candidates[0], root.join(platform_library_name()));
        assert_eq!(
            candidates[1],
            root.join("bin").join(platform_library_name())
        );
    }

    #[test]
    fn explicit_library_is_not_expanded_as_a_directory() {
        let library = PathBuf::from(platform_library_name());
        let candidates = ExecuTorchLibraryLocator::new(Some(library.clone())).candidates();
        assert_eq!(candidates[0], library);
    }
}
