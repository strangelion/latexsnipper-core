/// Platform detection and ONNX Runtime library selection.

/// Detected platform type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    WindowsX64,
    WindowsArm64,
    LinuxX64,
    LinuxAarch64,
    MacOsArm64,
    Unknown,
}

/// Detected acceleration capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceleration {
    CpuOnly,
    Cuda12,
    Cuda13,
    Directml,
    Tensorrt,
}

impl Platform {
    /// Detect current platform at compile time.
    pub fn detect() -> Self {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            Platform::WindowsX64
        }
        #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
        {
            Platform::WindowsArm64
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            Platform::LinuxX64
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            Platform::LinuxAarch64
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            Platform::MacOsArm64
        }
        #[cfg(not(any(
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "windows", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "aarch64"),
            all(target_os = "macos", target_arch = "aarch64"),
        )))]
        {
            Platform::Unknown
        }
    }

    /// Get the ONNX Runtime download URL for this platform.
    pub fn onnx_url(&self, version: &str, accel: Acceleration) -> String {
        let base = format!(
            "https://github.com/microsoft/onnxruntime/releases/download/v{}",
            version
        );
        match (self, accel) {
            (Platform::WindowsX64, Acceleration::CpuOnly) => {
                format!("{}/onnxruntime-win-x64-{}.zip", base, version)
            }
            (Platform::WindowsX64, Acceleration::Cuda12) => {
                format!("{}/onnxruntime-win-x64-gpu_cuda12-{}.zip", base, version)
            }
            (Platform::WindowsX64, Acceleration::Cuda13) => {
                format!("{}/onnxruntime-win-x64-gpu_cuda13-{}.zip", base, version)
            }
            (Platform::WindowsX64, Acceleration::Directml) => {
                format!("{}/onnxruntime-win-x64-{}.zip", base, version)
            }
            (Platform::WindowsX64, Acceleration::Tensorrt) => {
                format!("{}/onnxruntime-win-x64-gpu_cuda12-{}.zip", base, version)
            } // TensorRT uses CUDA backend
            (Platform::WindowsArm64, _) => {
                format!("{}/onnxruntime-win-arm64-{}.zip", base, version)
            }
            (Platform::LinuxX64, Acceleration::CpuOnly) => {
                format!("{}/onnxruntime-linux-x64-{}.tgz", base, version)
            }
            (Platform::LinuxX64, Acceleration::Cuda12) => {
                format!("{}/onnxruntime-linux-x64-gpu_cuda12-{}.tgz", base, version)
            }
            (Platform::LinuxX64, Acceleration::Cuda13) => {
                format!("{}/onnxruntime-linux-x64-gpu_cuda13-{}.tgz", base, version)
            }
            (Platform::LinuxX64, Acceleration::Directml) => {
                format!("{}/onnxruntime-linux-x64-{}.tgz", base, version)
            }
            (Platform::LinuxX64, Acceleration::Tensorrt) => {
                format!("{}/onnxruntime-linux-x64-gpu_cuda12-{}.tgz", base, version)
            } // TensorRT uses CUDA backend
            (Platform::LinuxAarch64, _) => {
                format!("{}/onnxruntime-linux-aarch64-{}.tgz", base, version)
            }
            (Platform::MacOsArm64, _) => format!("{}/onnxruntime-osx-arm64-{}.tgz", base, version),
            (Platform::Unknown, _) => format!("{}/onnxruntime-win-x64-{}.zip", base, version),
        }
    }

    /// Check if GPU acceleration is available on this platform.
    pub fn detect_gpu() -> Acceleration {
        // Check for TensorRT (highest priority on NVIDIA)
        if cfg!(target_os = "windows") || cfg!(target_os = "linux") {
            if std::env::var("TENSORRT_PATH").is_ok()
                || std::path::Path::new("/usr/local/tensorrt").exists()
                || std::path::Path::new("C:\\Program Files\\NVIDIA TensorRT").exists()
            {
                return Acceleration::Tensorrt;
            }
        }

        // Check for CUDA
        if cfg!(target_os = "windows") || cfg!(target_os = "linux") {
            if std::env::var("CUDA_PATH").is_ok()
                || std::path::Path::new("/usr/local/cuda").exists()
            {
                return Acceleration::Cuda12;
            }
        }

        // Check for DirectML on Windows
        if cfg!(target_os = "windows") {
            return Acceleration::Directml;
        }

        Acceleration::CpuOnly
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::WindowsX64 => write!(f, "win-x64"),
            Platform::WindowsArm64 => write!(f, "win-arm64"),
            Platform::LinuxX64 => write!(f, "linux-x64"),
            Platform::LinuxAarch64 => write!(f, "linux-aarch64"),
            Platform::MacOsArm64 => write!(f, "osx-arm64"),
            Platform::Unknown => write!(f, "unknown"),
        }
    }
}
