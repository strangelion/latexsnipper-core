//! Browser bindings with optional recognition and model-free conversion profiles.
use wasm_bindgen::prelude::*;
mod api;
mod formula_api;

pub use formula_api::{
    api_info_v3, convert_formula_fragment_v3, convert_formula_v3, formula_capabilities_v3,
};

#[cfg(feature = "recognition")]
mod capabilities;
#[cfg(feature = "recognition")]
mod error;
#[cfg(feature = "recognition")]
mod profiles;
#[cfg(feature = "recognition")]
mod recognition;
#[cfg(feature = "recognition")]
mod state;
#[cfg(feature = "recognition")]
pub use recognition::*;

#[wasm_bindgen]
pub fn init() {
    log::info!("LaTeXSnipper WASM initialized");
}
