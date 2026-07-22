//! Runtime-only discovery and loading of the versioned Paddle bridge.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::error::{paddle_error, PaddleResult};
use crate::ffi::PaddleApi;

const PADDLE_ENV: &str = "LATEXSNIPPER_PADDLE_HOME";

// Paddle and oneDNN own process-global worker state. Unloading their DLLs
// after a transient probe can race native teardown, so successful APIs remain
// cached until process exit. Predictors and sessions are still independently
// owned and destroyed.
static API_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<PaddleApi>>>> = OnceLock::new();

#[derive(Debug, Clone, Default)]
pub struct PaddleLibraryLocator {
    explicit: Option<PathBuf>,
}

impl PaddleLibraryLocator {
    pub fn new(explicit: Option<PathBuf>) -> Self {
        Self { explicit }
    }

    pub fn load(&self) -> PaddleResult<Arc<PaddleApi>> {
        let candidates = self.candidates();
        let mut failures = Vec::new();
        for candidate in &candidates {
            match load_cached(candidate) {
                Ok(api) => {
                    log::info!(
                        "Loaded Paddle Inference runtime from {}",
                        candidate.display()
                    );
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
        Err(paddle_error(format!(
            "Paddle Inference runtime not installed or could not be loaded; set {PADDLE_ENV} or configure an explicit library path. Tried: {detail}"
        )))
    }

    pub fn candidates(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Some(explicit) = &self.explicit {
            roots.push(explicit.clone());
        }
        if let Some(home) = std::env::var_os(PADDLE_ENV).filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(home));
        }
        if let Ok(executable) = std::env::current_exe() {
            if let Some(application_dir) = executable.parent() {
                roots.push(application_dir.join("resources/runtime/paddle"));
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

fn load_cached(path: &Path) -> PaddleResult<Arc<PaddleApi>> {
    let cache = API_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .map_err(|_| paddle_error("Paddle runtime library cache was poisoned"))?;
    if let Some(api) = cache.get(path) {
        return Ok(Arc::clone(api));
    }
    let api = Arc::new(PaddleApi::load(path)?);
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
        Path::new("paddle").join("lib").join(name),
        Path::new("lib").join(name),
        Path::new("bin").join(name),
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
    "latexsnipper_paddle_bridge.dll"
}

#[cfg(target_os = "linux")]
const fn platform_library_name() -> &'static str {
    "liblatexsnipper_paddle_bridge.so"
}

#[cfg(target_os = "macos")]
const fn platform_library_name() -> &'static str {
    "liblatexsnipper_paddle_bridge.dylib"
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
const fn platform_library_name() -> &'static str {
    "latexsnipper_paddle_bridge"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_sdk_root_expands_common_layouts_first() {
        let root = PathBuf::from("sdk-root");
        let candidates = PaddleLibraryLocator::new(Some(root.clone())).candidates();
        assert_eq!(candidates[0], root.join(platform_library_name()));
        assert_eq!(
            candidates[1],
            root.join("paddle/lib").join(platform_library_name())
        );
    }

    #[test]
    fn explicit_library_is_not_expanded_as_a_directory() {
        let library = PathBuf::from(platform_library_name());
        let candidates = PaddleLibraryLocator::new(Some(library.clone())).candidates();
        assert_eq!(candidates[0], library);
    }
}
