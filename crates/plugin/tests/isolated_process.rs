use std::path::Path;
use std::time::Duration;

use latexsnipper_ast::Document;
use latexsnipper_plugin::{
    IsolatedProcessHost, IsolatedProcessLimits, IsolatedProcessStatus, PluginClass, PluginManifest,
    PluginRequest, PluginStore, PLUGIN_ABI_VERSION,
};
use sha2::{Digest, Sha256};

fn fixture() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_latexsnipper-plugin-fixture"))
}

fn limits(timeout_millis: u64, output_limit_bytes: u64) -> IsolatedProcessLimits {
    IsolatedProcessLimits {
        timeout: Duration::from_millis(timeout_millis),
        memory_limit_bytes: 128 * 1024 * 1024,
        output_limit_bytes,
    }
}

fn execute(
    mode: &str,
    limits: IsolatedProcessLimits,
) -> latexsnipper_plugin::IsolatedProcessResult {
    IsolatedProcessHost::execute(
        fixture(),
        &["--mode".to_string(), mode.to_string()],
        &PluginRequest::new("transform", Document::new()),
        limits,
    )
    .unwrap()
}

#[cfg(unix)]
fn execute_with_arguments(
    arguments: Vec<String>,
    limits: IsolatedProcessLimits,
) -> latexsnipper_plugin::IsolatedProcessResult {
    IsolatedProcessHost::execute(
        fixture(),
        &arguments,
        &PluginRequest::new("transform", Document::new()),
        limits,
    )
    .unwrap()
}

#[test]
fn hard_timeout_terminates_infinite_process_and_host_recovers() {
    let timed_out = execute("infinite", limits(50, 64 * 1024));
    assert_eq!(timed_out.status, IsolatedProcessStatus::TimedOut);
    assert!(timed_out.terminated);
    assert_eq!(
        timed_out.diagnostic_code.as_deref(),
        Some("PLUGIN_HARD_TIMEOUT")
    );

    let healthy = execute("echo", limits(1_000, 64 * 1024));
    assert_eq!(healthy.status, IsolatedProcessStatus::Completed);
    assert!(healthy.response.is_some());
}

#[test]
fn panic_is_contained_as_structured_process_failure() {
    let result = execute("panic", limits(1_000, 64 * 1024));
    assert_eq!(result.status, IsolatedProcessStatus::ProcessFailed);
    assert_eq!(
        result.diagnostic_code.as_deref(),
        Some("PLUGIN_PROCESS_EXIT")
    );
}

#[test]
fn late_write_is_prevented_by_process_termination() {
    let result = execute("late-write", limits(30, 64 * 1024));
    assert_eq!(result.status, IsolatedProcessStatus::TimedOut);
    assert!(result.terminated);
    assert_eq!(result.output_bytes, 0);
}

#[test]
fn output_budget_terminates_or_rejects_oversized_response() {
    let result = execute("oversize", limits(1_000, 1024));
    assert_eq!(result.status, IsolatedProcessStatus::OutputLimitExceeded);
    assert!(result.output_bytes > 1024);
}

#[test]
fn incomplete_or_ambiguous_protocol_responses_are_rejected() {
    for mode in ["empty-response", "mixed-response", "code-only-response"] {
        let result = execute(mode, limits(1_000, 64 * 1024));
        assert_eq!(result.status, IsolatedProcessStatus::ProtocolFailed);
        assert_eq!(
            result.diagnostic_code.as_deref(),
            Some("PLUGIN_PROTOCOL_SHAPE")
        );
        assert!(result.response.is_none());
    }
}

#[cfg(unix)]
#[test]
fn hard_timeout_terminates_parent_and_spawned_descendant() {
    let pid_file = std::env::temp_dir().join(format!(
        "latexsnipper-plugin-descendant-{}-{}.pid",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let result = execute_with_arguments(
        vec![
            "--mode".to_string(),
            "spawn-descendant".to_string(),
            "--pid-file".to_string(),
            pid_file.to_string_lossy().to_string(),
        ],
        limits(500, 64 * 1024),
    );
    assert_eq!(result.status, IsolatedProcessStatus::TimedOut);
    let descendant_pid: libc::pid_t = std::fs::read_to_string(&pid_file).unwrap().parse().unwrap();
    let mut gone = false;
    for _ in 0..50 {
        // SAFETY: Signal zero only probes whether the recorded process still exists.
        let probe = unsafe { libc::kill(descendant_pid, 0) };
        if probe == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = std::fs::remove_file(pid_file);
    assert!(
        gone,
        "spawned descendant survived process-group termination"
    );
}

#[test]
fn verified_store_executes_enabled_process_plugin() {
    let root = std::env::temp_dir().join(format!(
        "latexsnipper-isolated-store-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let package = root.join("package");
    std::fs::create_dir_all(&package).unwrap();
    let file_name = fixture().file_name().unwrap();
    let entrypoint = package.join(file_name);
    std::fs::copy(fixture(), &entrypoint).unwrap();
    let bytes = std::fs::read(&entrypoint).unwrap();
    let mut manifest = PluginManifest::built_in("process.echo", "1.0.0");
    manifest.class = PluginClass::IsolatedProcess;
    manifest.abi_version = Some(PLUGIN_ABI_VERSION);
    manifest.entrypoint = Some(file_name.to_string_lossy().to_string());
    manifest.checksum_sha256 = Some(hex::encode(Sha256::digest(bytes)));
    manifest.permissions.timeout_millis = Some(1_000);
    manifest.permissions.memory_limit_bytes = Some(128 * 1024 * 1024);
    manifest.permissions.output_limit_bytes = Some(64 * 1024);
    std::fs::write(
        package.join("plugin.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let store = PluginStore::new(root.join("store"));
    store.install(&package).unwrap();
    store.set_enabled("process.echo", true).unwrap();
    let result = store
        .execute_isolated(
            "process.echo",
            &PluginRequest::new("transform", Document::new()),
        )
        .unwrap();
    assert_eq!(result.status, IsolatedProcessStatus::Completed);
    assert!(result.response.is_some());

    std::fs::remove_dir_all(root).unwrap();
}
