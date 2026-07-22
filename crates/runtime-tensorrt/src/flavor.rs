use latexsnipper_runtime::RuntimeKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TensorRtFlavor {
    Standard,
    Rtx,
}

impl TensorRtFlavor {
    pub(crate) const fn runtime_id(self) -> &'static str {
        match self {
            Self::Standard => "tensorrt",
            Self::Rtx => "tensorrt-rtx",
        }
    }

    pub(crate) const fn display_name(self) -> &'static str {
        match self {
            Self::Standard => "TensorRT",
            Self::Rtx => "TensorRT-RTX",
        }
    }

    pub(crate) const fn environment(self) -> &'static str {
        match self {
            Self::Standard => "LATEXSNIPPER_TENSORRT_HOME",
            Self::Rtx => "LATEXSNIPPER_TENSORRT_RTX_HOME",
        }
    }

    pub(crate) const fn resource_directory(self) -> &'static str {
        match self {
            Self::Standard => "tensorrt",
            Self::Rtx => "tensorrt-rtx",
        }
    }

    pub(crate) const fn runtime_kind(self) -> RuntimeKind {
        match self {
            Self::Standard => RuntimeKind::TensorRt,
            Self::Rtx => RuntimeKind::TensorRtRtx,
        }
    }

    #[cfg(target_os = "windows")]
    pub(crate) const fn bridge_name(self) -> &'static str {
        match self {
            Self::Standard => "latexsnipper_tensorrt_bridge.dll",
            Self::Rtx => "latexsnipper_tensorrt_rtx_bridge.dll",
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) const fn bridge_name(self) -> &'static str {
        match self {
            Self::Standard => "liblatexsnipper_tensorrt_bridge.so",
            Self::Rtx => "liblatexsnipper_tensorrt_rtx_bridge.so",
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    pub(crate) const fn bridge_name(self) -> &'static str {
        match self {
            Self::Standard => "latexsnipper_tensorrt_bridge",
            Self::Rtx => "latexsnipper_tensorrt_rtx_bridge",
        }
    }
}
