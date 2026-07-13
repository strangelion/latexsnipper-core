use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use latexsnipper_foundation::{Result, SnipperError};
use serde::{Deserialize, Serialize};

use crate::{PluginRequest, PluginResponse, PLUGIN_ABI_VERSION};

pub const PROCESS_PLUGIN_PROTOCOL_VERSION: u32 = PLUGIN_ABI_VERSION;

#[derive(Debug, Clone, Copy)]
pub struct IsolatedProcessLimits {
    pub timeout: Duration,
    pub memory_limit_bytes: u64,
    pub output_limit_bytes: u64,
}

impl Default for IsolatedProcessLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            memory_limit_bytes: 256 * 1024 * 1024,
            output_limit_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessPluginRequest {
    pub protocol_version: u32,
    pub request: PluginRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessPluginResponse {
    pub protocol_version: u32,
    pub response: Option<PluginResponse>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

impl ProcessPluginResponse {
    pub fn success(response: PluginResponse) -> Self {
        Self {
            protocol_version: PROCESS_PLUGIN_PROTOCOL_VERSION,
            response: Some(response),
            error_code: None,
            error_message: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolatedProcessStatus {
    Completed,
    TimedOut,
    OutputLimitExceeded,
    ProcessFailed,
    ProtocolFailed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsolatedProcessResult {
    pub status: IsolatedProcessStatus,
    pub response: Option<PluginResponse>,
    pub exit_code: Option<i32>,
    pub terminated: bool,
    pub elapsed_millis: u128,
    pub output_bytes: u64,
    pub diagnostic_code: Option<String>,
    pub diagnostic_message: Option<String>,
}

/// Versioned child-process plugin host with hard timeout and resource cleanup.
///
/// The process receives only request/response file paths and an empty environment.
/// Closing or timing out always kills and waits for the child before returning.
pub struct IsolatedProcessHost;

impl IsolatedProcessHost {
    pub fn execute(
        entrypoint: &Path,
        arguments: &[String],
        request: &PluginRequest,
        limits: IsolatedProcessLimits,
    ) -> Result<IsolatedProcessResult> {
        if !entrypoint.is_file() {
            return Err(plugin_error(format!(
                "Isolated plugin entrypoint does not exist: {}",
                entrypoint.display()
            )));
        }
        if limits.timeout.is_zero()
            || limits.memory_limit_bytes == 0
            || limits.output_limit_bytes == 0
        {
            return Err(plugin_error("Isolated plugin limits must be non-zero"));
        }

        let workspace = ProcessWorkspace::create()?;
        let request_path = workspace.path.join("request.json");
        let response_path = workspace.path.join("response.json");
        let payload = ProcessPluginRequest {
            protocol_version: PROCESS_PLUGIN_PROTOCOL_VERSION,
            request: request.clone(),
        };
        let request_bytes =
            serde_json::to_vec(&payload).map_err(|error| plugin_error(error.to_string()))?;
        if request_bytes.len() as u64 > limits.output_limit_bytes {
            return Err(plugin_error("Plugin request exceeds the IPC byte budget"));
        }
        std::fs::write(&request_path, request_bytes)
            .map_err(|error| plugin_error(error.to_string()))?;

        let mut command = Command::new(entrypoint);
        command
            .args(arguments)
            .arg("--latexsnipper-plugin-request")
            .arg(&request_path)
            .arg("--latexsnipper-plugin-response")
            .arg(&response_path)
            .current_dir(&workspace.path)
            .env_clear()
            .env(
                "LATEXSNIPPER_PLUGIN_PROTOCOL",
                PROCESS_PLUGIN_PROTOCOL_VERSION.to_string(),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_command_memory_limit(&mut command, limits.memory_limit_bytes)?;
        let mut child = command
            .spawn()
            .map_err(|error| plugin_error(format!("Could not start plugin process: {error}")))?;
        let _memory_guard = attach_process_memory_limit(&child, limits.memory_limit_bytes)?;
        let started = Instant::now();

        loop {
            if let Some(status) = child
                .try_wait()
                .map_err(|error| plugin_error(format!("Could not poll plugin process: {error}")))?
            {
                let elapsed = started.elapsed().as_millis();
                let output_bytes = file_size(&response_path);
                if !status.success() {
                    return Ok(runtime_result(
                        IsolatedProcessStatus::ProcessFailed,
                        status.code(),
                        false,
                        elapsed,
                        output_bytes,
                        "PLUGIN_PROCESS_EXIT",
                        "Isolated plugin exited unsuccessfully",
                    ));
                }
                if output_bytes > limits.output_limit_bytes {
                    return Ok(runtime_result(
                        IsolatedProcessStatus::OutputLimitExceeded,
                        status.code(),
                        false,
                        elapsed,
                        output_bytes,
                        "PLUGIN_OUTPUT_LIMIT",
                        "Isolated plugin response exceeded its byte budget",
                    ));
                }
                return read_response(&response_path, status.code(), elapsed, output_bytes);
            }

            let output_bytes = file_size(&response_path);
            if output_bytes > limits.output_limit_bytes {
                terminate_and_wait(&mut child)?;
                return Ok(runtime_result(
                    IsolatedProcessStatus::OutputLimitExceeded,
                    None,
                    true,
                    started.elapsed().as_millis(),
                    output_bytes,
                    "PLUGIN_OUTPUT_LIMIT",
                    "Isolated plugin response exceeded its byte budget",
                ));
            }
            if started.elapsed() >= limits.timeout {
                terminate_and_wait(&mut child)?;
                return Ok(runtime_result(
                    IsolatedProcessStatus::TimedOut,
                    None,
                    true,
                    started.elapsed().as_millis(),
                    output_bytes,
                    "PLUGIN_HARD_TIMEOUT",
                    "Isolated plugin process was terminated at its deadline",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn read_response(
    path: &Path,
    exit_code: Option<i32>,
    elapsed_millis: u128,
    output_bytes: u64,
) -> Result<IsolatedProcessResult> {
    let bytes = std::fs::read(path).map_err(|error| {
        plugin_error(format!(
            "Isolated plugin did not produce a response: {error}"
        ))
    })?;
    let response: ProcessPluginResponse = serde_json::from_slice(&bytes)
        .map_err(|error| plugin_error(format!("Invalid isolated plugin response: {error}")))?;
    if response.protocol_version != PROCESS_PLUGIN_PROTOCOL_VERSION {
        return Ok(runtime_result(
            IsolatedProcessStatus::ProtocolFailed,
            exit_code,
            false,
            elapsed_millis,
            output_bytes,
            "PLUGIN_PROTOCOL_VERSION",
            "Isolated plugin returned an incompatible protocol version",
        ));
    }
    if let Some(message) = response.error_message {
        return Ok(IsolatedProcessResult {
            status: IsolatedProcessStatus::ProtocolFailed,
            response: None,
            exit_code,
            terminated: false,
            elapsed_millis,
            output_bytes,
            diagnostic_code: response
                .error_code
                .or_else(|| Some("PLUGIN_ERROR".to_string())),
            diagnostic_message: Some(message),
        });
    }
    Ok(IsolatedProcessResult {
        status: IsolatedProcessStatus::Completed,
        response: response.response,
        exit_code,
        terminated: false,
        elapsed_millis,
        output_bytes,
        diagnostic_code: None,
        diagnostic_message: None,
    })
}

fn runtime_result(
    status: IsolatedProcessStatus,
    exit_code: Option<i32>,
    terminated: bool,
    elapsed_millis: u128,
    output_bytes: u64,
    code: &str,
    message: &str,
) -> IsolatedProcessResult {
    IsolatedProcessResult {
        status,
        response: None,
        exit_code,
        terminated,
        elapsed_millis,
        output_bytes,
        diagnostic_code: Some(code.to_string()),
        diagnostic_message: Some(message.to_string()),
    }
}

fn terminate_and_wait(child: &mut std::process::Child) -> Result<()> {
    match child.kill() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
        Err(error) => return Err(plugin_error(format!("Could not terminate plugin: {error}"))),
    }
    child
        .wait()
        .map_err(|error| plugin_error(format!("Could not reap plugin process: {error}")))?;
    Ok(())
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

struct ProcessWorkspace {
    path: PathBuf,
}

impl ProcessWorkspace {
    fn create() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "latexsnipper-plugin-host-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        std::fs::create_dir(&path).map_err(|error| plugin_error(error.to_string()))?;
        Ok(Self { path })
    }
}

impl Drop for ProcessWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(unix)]
fn configure_command_memory_limit(command: &mut Command, limit: u64) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let resource_limit = libc::rlimit {
        rlim_cur: limit as libc::rlim_t,
        rlim_max: limit as libc::rlim_t,
    };
    // SAFETY: pre_exec only calls the async-signal-safe setrlimit function and
    // captures a Copy value. No allocation or locking occurs in the child hook.
    unsafe {
        command.pre_exec(move || {
            if libc::setrlimit(libc::RLIMIT_AS, &resource_limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn configure_command_memory_limit(_command: &mut Command, _limit: u64) -> Result<()> {
    Ok(())
}

#[cfg(windows)]
struct ProcessMemoryGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for ProcessMemoryGuard {
    fn drop(&mut self) {
        // SAFETY: The handle is created by CreateJobObjectW and owned by this guard.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn attach_process_memory_limit(
    child: &std::process::Child,
    limit: u64,
) -> Result<Option<ProcessMemoryGuard>> {
    use std::mem::{size_of, zeroed};
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };

    // SAFETY: All pointers reference initialized values for the duration of each call.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(plugin_error(std::io::Error::last_os_error().to_string()));
        }
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        information.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        information.ProcessMemoryLimit = limit as usize;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const information).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            windows_sys::Win32::Foundation::CloseHandle(job);
            return Err(plugin_error(std::io::Error::last_os_error().to_string()));
        }
        if AssignProcessToJobObject(job, child.as_raw_handle() as _) == 0 {
            windows_sys::Win32::Foundation::CloseHandle(job);
            return Err(plugin_error(std::io::Error::last_os_error().to_string()));
        }
        Ok(Some(ProcessMemoryGuard(job)))
    }
}

#[cfg(not(windows))]
fn attach_process_memory_limit(_child: &std::process::Child, _limit: u64) -> Result<Option<()>> {
    Ok(None)
}

fn plugin_error(message: impl Into<String>) -> SnipperError {
    SnipperError::Plugin(message.into())
}
