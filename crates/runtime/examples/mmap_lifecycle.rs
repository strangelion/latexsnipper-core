//! Experimental, opt-in evidence for memory-mapped ONNX model lifecycle behavior.
//!
//! This example deliberately does not change the production model loader. It proves
//! the ownership and update behavior needed before `memoryMapModel` can be enabled.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use memmap2::Mmap;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const INPUT_SHAPE: [usize; 4] = [1, 3, 48, 320];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LifecycleEvidence {
    schema_version: u32,
    feature: &'static str,
    production_default_changed: bool,
    model_v1_sha256: String,
    model_v2_sha256: String,
    artifact_read_ms: f64,
    mmap_create_ms: f64,
    page_touch_ms: f64,
    session_create_and_optimize_ms: f64,
    first_inference_ms: f64,
    warm_inference_ms: f64,
    reload_session_create_ms: f64,
    reload_first_inference_ms: f64,
    old_session_survived_pointer_update: bool,
    old_session_survived_artifact_delete: bool,
    version_pointer_update_atomic: bool,
    old_artifact_delete_while_mapped: FileOperationEvidence,
    old_artifact_replace_while_mapped: FileOperationEvidence,
    process_memory: ProcessMemoryEvidence,
    cleanup_succeeded: bool,
    limitations: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileOperationEvidence {
    succeeded: bool,
    error_kind: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessMemoryEvidence {
    working_set_bytes: Option<u64>,
    peak_working_set_bytes: Option<u64>,
    private_bytes: Option<u64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let source_model = PathBuf::from(
        args.next()
            .ok_or("usage: mmap_lifecycle MODEL OUTPUT_JSON")?,
    );
    let output = PathBuf::from(
        args.next()
            .ok_or("usage: mmap_lifecycle MODEL OUTPUT_JSON")?,
    );
    let artifact_read_started = Instant::now();
    let source_bytes = fs::read(&source_model)?;
    let artifact_read_ms = elapsed_ms(artifact_read_started);
    let model_v1_sha256 = sha256(&source_bytes);

    // Protobuf parsers preserve forward compatibility by ignoring this unknown
    // length-delimited field. It gives v2 a different content identity without
    // changing the executable graph.
    let mut version_two_bytes = source_bytes.clone();
    version_two_bytes.extend_from_slice(&[0xfa, 0x01, 0x0b]);
    version_two_bytes.extend_from_slice(b"artifact-v2");
    let model_v2_sha256 = sha256(&version_two_bytes);

    let sandbox = TempDir::new()?;
    let v1_path = sandbox.path().join(format!("model-{model_v1_sha256}.onnx"));
    let v2_path = sandbox.path().join(format!("model-{model_v2_sha256}.onnx"));
    fs::write(&v1_path, &source_bytes)?;
    fs::write(&v2_path, &version_two_bytes)?;
    let pointer_path = sandbox.path().join("current-model");
    atomic_write_pointer(
        &pointer_path,
        v1_path.file_name().unwrap().to_string_lossy().as_bytes(),
    )?;

    let mmap_started = Instant::now();
    let v1_file = File::open(&v1_path)?;
    let v1_map = unsafe { Mmap::map(&v1_file)? };
    let mmap_create_ms = elapsed_ms(mmap_started);
    let page_touch_started = Instant::now();
    let page_checksum = touch_pages(&v1_map);
    std::hint::black_box(page_checksum);
    let page_touch_ms = elapsed_ms(page_touch_started);

    let session_started = Instant::now();
    let mut v1_builder =
        Session::builder()?.with_optimization_level(GraphOptimizationLevel::All)?;
    let mut v1_session = v1_builder.commit_from_memory_directly(&v1_map)?;
    let session_create_and_optimize_ms = elapsed_ms(session_started);

    let first_started = Instant::now();
    let first_hash = run_fixed_input(&mut v1_session)?;
    let first_inference_ms = elapsed_ms(first_started);
    let warm_started = Instant::now();
    let warm_hash = run_fixed_input(&mut v1_session)?;
    let warm_inference_ms = elapsed_ms(warm_started);
    if first_hash != warm_hash {
        return Err("warm inference output differs from first inference".into());
    }

    atomic_write_pointer(
        &pointer_path,
        v2_path.file_name().unwrap().to_string_lossy().as_bytes(),
    )?;
    let selected_after_update = fs::read_to_string(&pointer_path)?;
    let version_pointer_update_atomic =
        selected_after_update == v2_path.file_name().unwrap().to_string_lossy();
    let old_after_pointer_hash = run_fixed_input(&mut v1_session)?;
    let old_session_survived_pointer_update = old_after_pointer_hash == first_hash;

    let delete_result = match fs::remove_file(&v1_path) {
        Ok(()) => FileOperationEvidence {
            succeeded: true,
            error_kind: None,
        },
        Err(error) => FileOperationEvidence {
            succeeded: false,
            error_kind: Some(format!("{:?}", error.kind())),
        },
    };
    let old_after_delete_hash = run_fixed_input(&mut v1_session)?;
    let old_session_survived_artifact_delete = old_after_delete_hash == first_hash;
    let replacement_path = sandbox.path().join("replacement-v1.onnx");
    fs::write(&replacement_path, &version_two_bytes)?;
    let replace_result = match fs::rename(&replacement_path, &v1_path) {
        Ok(()) => FileOperationEvidence {
            succeeded: true,
            error_kind: None,
        },
        Err(error) => FileOperationEvidence {
            succeeded: false,
            error_kind: Some(format!("{:?}", error.kind())),
        },
    };

    let v2_file = File::open(&v2_path)?;
    let v2_map = unsafe { Mmap::map(&v2_file)? };
    let reload_started = Instant::now();
    let mut v2_builder =
        Session::builder()?.with_optimization_level(GraphOptimizationLevel::All)?;
    let mut v2_session = v2_builder.commit_from_memory_directly(&v2_map)?;
    let reload_session_create_ms = elapsed_ms(reload_started);
    let reload_inference_started = Instant::now();
    let reload_hash = run_fixed_input(&mut v2_session)?;
    let reload_first_inference_ms = elapsed_ms(reload_inference_started);
    if reload_hash != first_hash {
        return Err("versioned model update changed inference output".into());
    }

    let process_memory = process_memory();
    drop(v2_session);
    drop(v1_session);
    drop(v2_map);
    drop(v1_map);
    drop(v2_file);
    drop(v1_file);
    let sandbox_path = sandbox.path().to_path_buf();
    drop(sandbox);
    let cleanup_succeeded = !sandbox_path.exists();

    let evidence = LifecycleEvidence {
        schema_version: 1,
        feature: "runtime-mmap-experimental",
        production_default_changed: false,
        model_v1_sha256,
        model_v2_sha256,
        artifact_read_ms,
        mmap_create_ms,
        page_touch_ms,
        session_create_and_optimize_ms,
        first_inference_ms,
        warm_inference_ms,
        reload_session_create_ms,
        reload_first_inference_ms,
        old_session_survived_pointer_update,
        old_session_survived_artifact_delete,
        version_pointer_update_atomic,
        old_artifact_delete_while_mapped: delete_result,
        old_artifact_replace_while_mapped: replace_result,
        process_memory,
        cleanup_succeeded,
        limitations: vec![
            "This is an isolated CPU experiment, not the production loader.",
            "ONNX graph optimization time is included in session creation.",
            "Process memory includes the example process, ONNX Runtime, and both sessions.",
            "File deletion and replacement semantics are operating-system dependent.",
        ],
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_vec_pretty(&evidence)?)?;
    Ok(())
}

fn run_fixed_input(
    session: &mut ort::session::InMemorySession<'_>,
) -> Result<String, Box<dyn std::error::Error>> {
    let data = (0..INPUT_SHAPE.iter().product::<usize>())
        .map(|index| (index % 17) as f32 / 17.0)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let input: Value = Value::from_array((INPUT_SHAPE.to_vec(), data))?.into();
    let outputs = session.run(HashMap::from([("x".to_owned(), input)]))?;
    let mut hasher = Sha256::new();
    for (name, value) in outputs {
        hasher.update(name.as_bytes());
        let (shape, values) = value.try_extract_tensor::<f32>()?;
        for dimension in shape.iter() {
            hasher.update(dimension.to_le_bytes());
        }
        for value in values {
            hasher.update(value.to_le_bytes());
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

fn touch_pages(bytes: &[u8]) -> u8 {
    let mut checksum = 0_u8;
    for offset in (0..bytes.len()).step_by(4096) {
        checksum ^= bytes[offset];
    }
    checksum ^ bytes.last().copied().unwrap_or_default()
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn atomic_write_pointer(path: &Path, value: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension("tmp");
    let mut file = File::create(&temporary)?;
    file.write_all(value)?;
    file.sync_all()?;
    replace_file(&temporary, path)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn process_memory() -> ProcessMemoryEvidence {
    use std::ffi::c_void;
    #[repr(C)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCountersEx,
            size: u32,
        ) -> i32;
    }
    let mut counters = ProcessMemoryCountersEx {
        cb: std::mem::size_of::<ProcessMemoryCountersEx>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
        private_usage: 0,
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCountersEx>() as u32,
        )
    };
    if ok == 0 {
        return ProcessMemoryEvidence {
            working_set_bytes: None,
            peak_working_set_bytes: None,
            private_bytes: None,
        };
    }
    ProcessMemoryEvidence {
        working_set_bytes: Some(counters.working_set_size as u64),
        peak_working_set_bytes: Some(counters.peak_working_set_size as u64),
        private_bytes: Some(counters.private_usage as u64),
    }
}

#[cfg(not(windows))]
fn process_memory() -> ProcessMemoryEvidence {
    ProcessMemoryEvidence {
        working_set_bytes: None,
        peak_working_set_bytes: None,
        private_bytes: None,
    }
}
