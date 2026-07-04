//! PDF page rendering using external tools.
//!
//! This module provides PDF rendering by shelling out to system tools:
//! - `pdftoppm` (from poppler-utils) - Linux/macOS
//! - `mutool` (from MuPDF) - cross-platform
//!
//! If neither tool is available, returns a clear error with installation instructions.

use latexsnipper_foundation::{Result, SnipperError};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::color::PixelFormat;
use crate::image::SnipperImage;

/// Detect available PDF rendering tool.
pub fn detect_pdf_tool() -> Option<PdfTool> {
    // Try pdftoppm first (Linux/macOS)
    if Command::new("pdftoppm").arg("--version").output().is_ok() {
        return Some(PdfTool::Pdftoppm);
    }

    // Try mutool (cross-platform)
    if Command::new("mutool").arg("-v").output().is_ok() {
        return Some(PdfTool::Mutool);
    }

    None
}

/// Available PDF rendering tools.
#[derive(Debug, Clone, Copy)]
pub enum PdfTool {
    Pdftoppm,
    Mutool,
}

impl PdfTool {
    pub fn name(&self) -> &'static str {
        match self {
            PdfTool::Pdftoppm => "pdftoppm",
            PdfTool::Mutool => "mutool",
        }
    }

    pub fn install_hint(&self) -> &'static str {
        match self {
            PdfTool::Pdftoppm => {
                "Install poppler-utils:\n  \
                 Linux: sudo apt install poppler-utils\n  \
                 macOS: brew install poppler\n  \
                 Windows: choco install poppler"
            }
            PdfTool::Mutool => {
                "Install MuPDF:\n  \
                 Linux: sudo apt install mupdf-tools\n  \
                 macOS: brew install mupdf\n  \
                 Windows: choco install mupdf"
            }
        }
    }
}

/// Render a PDF page to an image using an external tool.
pub fn render_pdf_page(pdf_path: &Path, page: u32, dpi: u32) -> Result<SnipperImage> {
    let tool = detect_pdf_tool().ok_or_else(|| {
        SnipperError::Image(
            "No PDF rendering tool found. Install one of:\n  \
             - poppler-utils (pdftoppm): sudo apt install poppler-utils\n  \
             - MuPDF (mutool): sudo apt install mupdf-tools\n  \
             Or convert PDF pages to images externally and use from_file()."
                .into(),
        )
    })?;

    match tool {
        PdfTool::Pdftoppm => render_with_pdftoppm(pdf_path, page, dpi),
        PdfTool::Mutool => render_with_mutool(pdf_path, page, dpi),
    }
}

/// Render using pdftoppm.
fn render_with_pdftoppm(pdf_path: &Path, page: u32, dpi: u32) -> Result<SnipperImage> {
    let tmp_dir = std::env::temp_dir().join(format!("latexsnipper-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| SnipperError::Image(format!("Failed to create temp dir: {}", e)))?;

    let output_prefix = tmp_dir.join("page");

    let status = Command::new("pdftoppm")
        .arg("-png")
        .arg("-r")
        .arg(dpi.to_string())
        .arg("-f")
        .arg(page.to_string())
        .arg("-l")
        .arg(page.to_string())
        .arg(pdf_path)
        .arg(&output_prefix)
        .status()
        .map_err(|e| SnipperError::Image(format!("Failed to run pdftoppm: {}", e)))?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(SnipperError::Image(format!(
            "pdftoppm failed with status: {}",
            status
        )));
    }

    // Find the output file
    let output_file = find_rendered_page(&tmp_dir)?;

    let img = image::open(&output_file)
        .map_err(|e| SnipperError::Image(format!("Failed to load rendered image: {}", e)))?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();

    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(SnipperImage::new(width, height, PixelFormat::Rgba, pixels))
}

/// Render using mutool.
fn render_with_mutool(pdf_path: &Path, page: u32, dpi: u32) -> Result<SnipperImage> {
    let tmp_dir = std::env::temp_dir().join(format!("latexsnipper-pdf-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| SnipperError::Image(format!("Failed to create temp dir: {}", e)))?;

    let output_file = tmp_dir.join("page.png");

    let status = Command::new("mutool")
        .arg("draw")
        .arg("-o")
        .arg(&output_file)
        .arg("-r")
        .arg(dpi.to_string())
        .arg(pdf_path)
        .arg(page.to_string())
        .status()
        .map_err(|e| SnipperError::Image(format!("Failed to run mutool: {}", e)))?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(SnipperError::Image(format!(
            "mutool failed with status: {}",
            status
        )));
    }

    let img = image::open(&output_file)
        .map_err(|e| SnipperError::Image(format!("Failed to load rendered image: {}", e)))?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let pixels = rgba.into_raw();

    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(SnipperImage::new(width, height, PixelFormat::Rgba, pixels))
}

/// Find the rendered page file in a directory.
fn find_rendered_page(dir: &Path) -> Result<PathBuf> {
    let entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| SnipperError::Image(format!("Failed to read temp dir: {}", e)))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "png" || ext == "ppm")
        })
        .collect();

    if entries.is_empty() {
        return Err(SnipperError::Image("No rendered page found".into()));
    }

    // Sort by name to get the correct page
    let mut sorted = entries;
    sorted.sort_by_key(|e| e.file_name());

    Ok(sorted.into_iter().next().unwrap().path())
}
