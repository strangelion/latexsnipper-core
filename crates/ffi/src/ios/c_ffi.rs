use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Mutex;
use once_cell::sync::Lazy;

use latexsnipper_engine::{SnipperEngine, EngineConfig, RecognizeMode};
use latexsnipper_runtime::{StubRuntime, OnnxRuntimeBackend};
use latexsnipper_foundation::{SnipperError, Result};

use crate::common::FfiResponse;

/// Global engine instance for iOS.
static ENGINE: Lazy<Mutex<Option<SnipperEngine>>> = Lazy::new(|| Mutex::new(None));

/// Initialize the engine.
/// Returns 1 on success, 0 on failure.
#[no_mangle]
pub extern "C" fn latexsnipper_init(models_dir: *const c_char) -> i32 {
    let dir = match unsafe { cstr_to_string(models_dir) } {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let models_path = std::path::PathBuf::from(dir);
    let config = EngineConfig {
        models_dir: models_path.clone(),
        ..Default::default()
    };

    let runtime: Box<dyn latexsnipper_runtime::RuntimeBackend> =
        match OnnxRuntimeBackend::new(models_path) {
            Ok(backend) => Box::new(backend),
            Err(_) => Box::new(StubRuntime::new()),
        };

    let engine = SnipperEngine::new(config, runtime);
    *ENGINE.lock().unwrap() = Some(engine);
    1
}

/// Recognize formula.
/// data: raw RGB pixel data
/// width, height: image dimensions
/// Returns a JSON string that must be freed with latexsnipper_free_string.
#[no_mangle]
pub extern "C" fn latexsnipper_recognize_formula(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
) -> *mut c_char {
    let response = recognize_sync(data, len, width, height, RecognizeMode::Formula);
    match CString::new(response.to_json()) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Recognize text.
#[no_mangle]
pub extern "C" fn latexsnipper_recognize_text(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
) -> *mut c_char {
    let response = recognize_sync(data, len, width, height, RecognizeMode::Text);
    match CString::new(response.to_json()) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Recognize mixed.
#[no_mangle]
pub extern "C" fn latexsnipper_recognize_mixed(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
) -> *mut c_char {
    let response = recognize_sync(data, len, width, height, RecognizeMode::Mixed);
    match CString::new(response.to_json()) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Release engine.
#[no_mangle]
pub extern "C" fn latexsnipper_release() {
    *ENGINE.lock().unwrap() = None;
}

/// Free a string.
#[no_mangle]
pub extern "C" fn latexsnipper_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)); }
    }
}

unsafe fn cstr_to_string(ptr: *const c_char) -> Result<String> {
    if ptr.is_null() {
        return Err(SnipperError::Other("Null pointer".into()));
    }
    CStr::from_ptr(ptr)
        .to_str()
        .map(|s| s.to_string())
        .map_err(|e| SnipperError::Other(e.to_string()))
}

fn recognize_sync(
    data: *const u8,
    len: usize,
    width: u32,
    height: u32,
    mode: RecognizeMode,
) -> FfiResponse {
    let engine = ENGINE.lock().unwrap();
    let engine = match engine.as_ref() {
        Some(e) => e,
        None => return FfiResponse::error("Engine not initialized"),
    };

    if data.is_null() || len == 0 || len > 100 * 1024 * 1024 {
        return FfiResponse::error("Invalid image data: null pointer, empty, or too large (>100MB)");
    }

    if width == 0 || height == 0 || width > 10000 || height > 10000 {
        return FfiResponse::error("Invalid image dimensions");
    }

    let expected_len = (width * height * 3) as usize;
    if len < expected_len {
        return FfiResponse::error("Image data too short for given dimensions");
    }

    let data = unsafe { std::slice::from_raw_parts(data, expected_len) };

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
