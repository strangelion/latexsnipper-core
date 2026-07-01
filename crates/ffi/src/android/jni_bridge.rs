use std::sync::Mutex;
use once_cell::sync::Lazy;

use latexsnipper_engine::{SnipperEngine, EngineConfig, RecognizeMode};
use latexsnipper_runtime::{StubRuntime, OnnxRuntimeBackend};

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

    let models_path = std::path::PathBuf::from(dir);
    let config = EngineConfig {
        models_dir: models_path.clone(),
        ..Default::default()
    };

    // Try OnnxRuntimeBackend first, fall back to StubRuntime
    let runtime: Box<dyn latexsnipper_runtime::RuntimeBackend> =
        match OnnxRuntimeBackend::new(models_path) {
            Ok(backend) => {
                log::info!("Using ONNX Runtime backend");
                Box::new(backend)
            }
            Err(e) => {
                log::warn!("ONNX Runtime not available ({}), using StubRuntime", e);
                Box::new(StubRuntime::new())
            }
        };

    let engine = SnipperEngine::new(config, runtime);
    *ENGINE.lock().unwrap() = Some(engine);
    1
}

/// Recognize formula in an image.
/// image_data: raw RGB pixel data
/// width, height: image dimensions
/// Returns a JSON string with the result.
#[no_mangle]
pub extern "C" fn Java_com_latexsnipper_core_NativeBridge_nativeRecognizeFormula(
    image_data: *const u8,
    image_len: usize,
    width: u32,
    height: u32,
) -> *mut std::os::raw::c_char {
    let response = recognize_sync(image_data, image_len, width, height, RecognizeMode::Formula);
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
    width: u32,
    height: u32,
) -> *mut std::os::raw::c_char {
    let response = recognize_sync(image_data, image_len, width, height, RecognizeMode::Text);
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
    width: u32,
    height: u32,
) -> *mut std::os::raw::c_char {
    let response = recognize_sync(image_data, image_len, width, height, RecognizeMode::Mixed);
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

fn recognize_sync(
    image_data: *const u8,
    image_len: usize,
    width: u32,
    height: u32,
    mode: RecognizeMode,
) -> FfiResponse {
    let engine = ENGINE.lock().unwrap();
    let engine = match engine.as_ref() {
        Some(e) => e,
        None => return FfiResponse::error("Engine not initialized"),
    };

    // Safety: validate pointer and length before creating slice
    if image_data.is_null() || image_len == 0 || image_len > 100 * 1024 * 1024 {
        return FfiResponse::error("Invalid image data: null pointer, empty, or too large (>100MB)");
    }

    // Validate dimensions
    if width == 0 || height == 0 || width > 10000 || height > 10000 {
        return FfiResponse::error("Invalid image dimensions");
    }

    // Validate expected data length (RGB = 3 bytes per pixel)
    let expected_len = (width * height * 3) as usize;
    if image_len < expected_len {
        return FfiResponse::error("Image data too short for given dimensions");
    }

    let data = unsafe { std::slice::from_raw_parts(image_data, expected_len) };

    // Create SnipperImage from actual pixel data
    let image = latexsnipper_image::SnipperImage::new(
        width,
        height,
        latexsnipper_image::color::PixelFormat::Rgb,
        data.to_vec(),
    );

    let start = std::time::Instant::now();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let doc = match rt.block_on(engine.recognize(image, mode)) {
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
