//! End-to-end table recognition test.
//! Pipeline: TATR det -> table crop -> structure (tatr|slanet) -> grid -> per-cell OCR.
//! Run: cargo test --test table_e2e -- --nocapture
//! Select backend by setting STRUCT_BACKEND env var: "tatr" (default) or "slanet".
//! Requires Windows (ONNX Runtime).

#![cfg(target_os = "windows")]

use latexsnipper_ast::*;
use latexsnipper_image::operations;
use latexsnipper_image::ImageSource;
use latexsnipper_inference::{
    load_keys, recognize_table_structure, recognize_table_transformer, recognize_text_with_keys,
    TextRecParams,
};
use latexsnipper_runtime::{AccelerationMode, ModelHandle, OnnxRuntimeBackend, RuntimeBackend};

fn models_dir() -> std::path::PathBuf {
    std::env::current_dir().unwrap().join("models")
}

fn require_real_models() -> bool {
    std::env::var_os("LATEXSNIPPER_REQUIRE_REAL_MODELS").is_some()
}

fn load_image(name: &str) -> latexsnipper_image::SnipperImage {
    let path = std::env::current_dir().unwrap().join("fixtures").join(name);
    latexsnipper_image::decode::decode(ImageSource::File(&path)).unwrap()
}

fn struct_backend_path(models: &std::path::Path, backend: &str) -> Option<std::path::PathBuf> {
    match backend {
        "tatr" => {
            let p = models.join("table-struct/tatr-structure/model.onnx");
            if p.exists() {
                Some(p)
            } else {
                None
            }
        }
        "slanet" => {
            let p = models.join("table-struct/slanet-plus/model.onnx");
            if p.exists() {
                Some(p)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[test]
fn full_pipeline() {
    let models = models_dir();
    let det_path = models.join("table-det/tatr-detection/model.onnx");
    let tr_path = models.join("text-rec/v6-small/inference.onnx");

    // Backend selection
    let struct_backend = std::env::var("STRUCT_BACKEND").unwrap_or_else(|_| "tatr".into());
    let struct_path = struct_backend_path(&models, &struct_backend);

    if !det_path.exists() || !tr_path.exists() {
        assert!(
            !require_real_models(),
            "required table detection or text recognition model is missing"
        );
        eprintln!("SKIP: detection or text-rec model not found");
        return;
    }
    if struct_backend != "projection" && struct_path.is_none() {
        assert!(
            !require_real_models(),
            "required table structure model '{}' is missing",
            struct_backend
        );
        eprintln!("SKIP: structure model '{}' not found", struct_backend);
        return;
    }

    let image = load_image("table.jpg");
    eprintln!("Image: {}x{}", image.width(), image.height());
    let backend = OnnxRuntimeBackend::new(models.clone()).unwrap();

    // 1. TATR detection -> table crop
    let table_rect = {
        let h = ModelHandle::with_path("tatr-det", det_path);
        let s = backend.create_session(&h, AccelerationMode::Cpu).unwrap();
        let dets = recognize_table_transformer(&image, &*s).unwrap();
        match dets.iter().filter(|d| d.class_id != 0).max_by(|a, b| {
            let aa = (a.bbox[2] - a.bbox[0]) * (a.bbox[3] - a.bbox[1]);
            let bb = (b.bbox[2] - b.bbox[0]) * (b.bbox[3] - b.bbox[1]);
            aa.partial_cmp(&bb).unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Some(d) => {
                let r = Rect::new(
                    d.bbox[0],
                    d.bbox[1],
                    d.bbox[2] - d.bbox[0],
                    d.bbox[3] - d.bbox[1],
                );
                eprintln!(
                    "Table: ({:.0},{:.0},{:.0},{:.0})",
                    r.x, r.y, r.width, r.height
                );
                r
            }
            None => {
                eprintln!("No table region, using full image");
                Rect::new(0.0, 0.0, image.width() as f32, image.height() as f32)
            }
        }
    };
    let table_image = operations::crop(&image, table_rect);
    eprintln!(
        "Table crop: {}x{}",
        table_image.width(),
        table_image.height()
    );

    // 2. Table structure recognition
    let grid = if let Some(ref model_path) = struct_path {
        let h = ModelHandle::with_path(&struct_backend, model_path.clone());
        let s = backend.create_session(&h, AccelerationMode::Cpu).unwrap();
        recognize_table_structure(&table_image, &struct_backend, Some(&*s)).unwrap()
    } else {
        recognize_table_structure(&table_image, "projection", None).unwrap()
    };
    let grid = grid.unwrap_or_default();
    eprintln!("Backend '{}': {} grid cells", struct_backend, grid.len());

    if grid.is_empty() {
        assert!(
            !require_real_models(),
            "real table model returned no grid cells"
        );
        eprintln!("No grid cells generated, skipping OCR");
        return;
    }

    // 3. Text recognition per cell
    let tr_handle = ModelHandle::with_path("text-rec", tr_path);
    let tr_session = backend
        .create_session(&tr_handle, AccelerationMode::Cpu)
        .unwrap();
    let (keys, first_char_id) = if let Some(chars) = tr_session.get_character_list() {
        (chars, 1)
    } else {
        load_keys(&models.join("text-rec/v6-small/inference.yml")).unwrap_or_default()
    };
    let tr_params = TextRecParams::default();

    let max_row = grid.iter().map(|c| c.row).max().unwrap_or(0);
    let max_col = grid.iter().map(|c| c.col).max().unwrap_or(0);
    let mut table_rows: Vec<Vec<String>> = vec![vec![String::new(); max_col + 1]; max_row + 1];

    for cell in &grid {
        let pad = 3.0;
        let cx = (cell.rect.x - pad).max(0.0);
        let cy = (cell.rect.y - pad).max(0.0);
        let cw = (cell.rect.width + pad * 2.0).min(table_image.width() as f32 - cx);
        let ch = (cell.rect.height + pad * 2.0).min(table_image.height() as f32 - cy);
        if cw < 4.0 || ch < 4.0 {
            continue;
        }

        let cropped = operations::crop(&table_image, Rect::new(cx, cy, cw, ch));
        let text = match recognize_text_with_keys(
            &cropped,
            &*tr_session,
            &keys,
            first_char_id,
            &tr_params,
        ) {
            Ok(r) if !r.text.trim().is_empty() => r.text,
            _ => String::new(),
        };
        if cell.row <= max_row && cell.col <= max_col {
            table_rows[cell.row][cell.col] = text;
        }
    }

    eprintln!(
        "\n=== Recognized Table ({}x{}) ===",
        max_row + 1,
        max_col + 1
    );
    for (ri, row) in table_rows.iter().enumerate() {
        let cells: Vec<&str> = row
            .iter()
            .map(|s| if s.is_empty() { "?" } else { s })
            .collect();
        eprintln!("  Row {}: {}", ri, cells.join(" | "));
    }

    let merged: Vec<_> = grid
        .iter()
        .filter(|c| c.rowspan > 1 || c.colspan > 1)
        .collect();
    if !merged.is_empty() {
        eprintln!("\n  Merged cells:");
        for c in &merged {
            eprintln!(
                "    ({},{}) rowspan={} colspan={}",
                c.row, c.col, c.rowspan, c.colspan
            );
        }
    }

    assert!(!grid.is_empty(), "No grid cells generated");
}
