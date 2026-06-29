use std::sync::Mutex;
use once_cell::sync::Lazy;

use latexsnipper_engine::{SnipperEngine, EngineConfig, RecognizeMode};
use latexsnipper_runtime::{StubRuntime, AccelerationMode};
use latexsnipper_foundation::{SnipperError, Result};

use crate::common::FfiResponse;

/// Global engine instance (protected by Mutex for thread safety).
static ENGINE: Lazy<Mutex<Option<SnipperEngine>>> = Lazy::new(|| Mutex::new(None));

/// Initialize the engine with a models directory.
/// Returns 1 on success, 0 on failure.
#[no_mangle]
pub extern "C" fn Java_com_latexsnipper_core_NativeBridge_nativeInit(
    models_dir: *const std::os::raw::c_char,
) -> i32 {
    let dir = match unsafe { crate::common::cstr_to_string(models_dir) } {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let config = EngineConfig {
        models_dir: std::path::PathBuf::from(dir),
        ..Default::default()
    };

    let runtime = Box::new(StubRuntime::new());
    let engine = SnipperEngine::new(config, runtime);
    *ENGINE.lock().unwrap() = Some(engine);
    1
}

/// Recognize formula in an image.
/// Returns a JSON string with the result.
#[no_mangle]
pub extern "C" fn Java_com_latexsnipper_core_NativeBridge_nativeRecognizeFormula(
    image_data: *const u8,
    image_len: usize,
) -> *mut std::os::raw::c_char {
    let response = recognize_sync(image_data, image_len, RecognizeMode::Formula);
    match std::ffi::CString::new(response.to_json()) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Recognize text in an image.
#[no_mangle]
pub extern "C" fn Java_com_latexsnipper_core_NativeBridge_nativeRecognizeText(
    image_data: *const u8,
    image_len: usize,
) -> *mut std::os::raw::c_char {
    let response = recognize_sync(image_data, image_len, RecognizeMode::Text);
    match std::ffi::CString::new(response.to_json()) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Recognize mixed in an image.
#[no_mangle]
pub extern "C" fn Java_com_latexsnipper_core_NativeBridge_nativeRecognizeMixed(
    image_data: *const u8,
    image_len: usize,
) -> *mut std::os::raw::c_char {
    let response = recognize_sync(image_data, image_len, RecognizeMode::Mixed);
    match std::ffi::CString::new(response.to_json()) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Release the engine and free resources.
#[no_mangle]
pub extern "C" fn Java_com_latexsnipper_core_NativeBridge_nativeRelease() {
    *ENGINE.lock().unwrap() = None;
}

/// Free a string returned by the bridge.
#[no_mangle]
pub extern "C" fn Java_com_latexsnipper_core_NativeBridge_nativeFreeString(
    ptr: *mut std::os::raw::c_char,
) {
    if !ptr.is_null() {
        unsafe { drop(std::ffi::CString::from_raw(ptr)); }
    }
}

fn recognize_sync(image_data: *const u8, image_len: usize, mode: RecognizeMode) -> FfiResponse {
    let engine = ENGINE.lock().unwrap();
    let engine = match engine.as_ref() {
        Some(e) => e,
        None => return FfiResponse::error("Engine not initialized"),
    };

    // Convert raw bytes to SnipperImage (placeholder — real impl decodes JPEG/PNG)
    let data = unsafe { std::slice::from_raw_parts(image_data, image_len) };

    let start = std::time::Instant::now();

    // For now, use tokio runtime to run async recognize
    let rt = tokio::runtime::Runtime::new().unwrap();
    let doc = match rt.block_on(engine.recognize(
        latexsnipper_image::SnipperImage::new(100, 100, latexsnipper_image::color::PixelFormat::Rgb, vec![128u8; 30000]),
        mode,
    )) {
        Ok(d) => d,
        Err(e) => return FfiResponse::error(&e.to_string()),
    };

    let elapsed = start.elapsed().as_millis() as u64;

    // Extract text from document
    let text: String = doc.pages.iter().flat_map(|p| &p.blocks).filter_map(|b| {
        match b {
            latexsnipper_ast::Block::Formula(f) => Some(f.formula.as_latex().to_string()),
            latexsnipper_ast::Block::Paragraph(p) => {
                let t: String = p.inlines.iter().filter_map(|i| {
                    if let latexsnipper_ast::Inline::Text(t) = i { Some(t.text.as_str()) } else { None }
                }).collect();
                Some(t)
            }
            _ => None,
        }
    }).collect::<Vec<_>>().join("\n");

    FfiResponse::success(&text, doc.pages.first().and_then(|p| p.blocks.first()).map_or(0.0, |b| {
        match b {
            latexsnipper_ast::Block::Formula(f) => f.formula.confidence,
            _ => 0.0,
        }
    }), elapsed)
}
