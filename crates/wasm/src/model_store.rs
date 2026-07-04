use std::collections::HashMap;
use std::sync::Mutex;

use once_cell::sync::Lazy;

static MODEL_STORE: Lazy<Mutex<HashMap<String, Vec<u8>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Store a model by name. Overwrites if name already exists.
pub fn store_model(name: &str, bytes: Vec<u8>) {
    if let Ok(mut store) = MODEL_STORE.lock() {
        store.insert(name.to_string(), bytes);
    }
}

/// Check if a model is loaded.
pub fn has_model(name: &str) -> bool {
    MODEL_STORE
        .lock()
        .map(|s| s.contains_key(name))
        .unwrap_or(false)
}

/// List all loaded model names.
pub fn list_models() -> Vec<String> {
    MODEL_STORE
        .lock()
        .map(|s| s.keys().cloned().collect())
        .unwrap_or_default()
}