pub mod acceleration;
pub mod api_provider;
pub mod artifacts;
pub mod capabilities;
pub mod factory;
pub mod kind;
pub mod legacy;
pub mod model_handle;
pub mod model_package;
pub mod model_plugin;
pub mod model_registry;
pub mod model_resolver;
pub mod model_validation;
pub mod options;
pub mod plugin_loader;
pub mod providers;
pub mod resolver;
pub mod runtime_registry;
pub mod selection_policy;
pub mod session;

pub use acceleration::AccelerationMode;
pub use api_provider::{
    ApiKeyResolver, ApiProviderConfig, ModelProviderKind, PromptExample, PromptPreset,
    PromptProfile, PromptTask, UploadPolicy, UploadScope,
};
pub use artifacts::RuntimeArtifacts;
pub use capabilities::{RuntimeCapabilities, RuntimeDevice, RuntimeProbe};
pub use factory::RuntimeFactory;
pub use kind::RuntimeKind;
pub use legacy::{
    InferenceSession, RegistryRuntimeBackend, RuntimeBackend, RuntimeDiagnostics,
    RuntimeSessionCompatibility,
};
pub use model_handle::ModelHandle;
pub use model_package::{
    DetectionQuad, DetectionResult, FormulaResult, InferenceContext, LayoutResult, ModelDescriptor,
    ModelExecutionContext, ModelExecutor, ModelInput, ModelOutput, ModelPackage, ModelTask,
    PreparedModel, TableResult, TensorDtype, TensorSpec, TextResult,
};
pub use model_plugin::{ManifestAdapter, ModelManifestView, ModelPlugin, ModelPluginRegistry};
pub use model_registry::{
    ManifestDecoding, ManifestPreprocessing, ManifestResize, ManifestTensorSpec, ModelFiles,
    ModelManifest, ModelRegistry, ModelScanIssue, ModelScanReport,
};
pub use model_resolver::{
    normalize_key, FsModelResolver, MemoryModelResolver, ModelId, ModelResolver,
    SharedModelResolver,
};
pub use model_validation::{
    compute_bytes_checksum, compute_checksum, load_checksums, validate_all_models, validate_model,
    validate_model_bytes, ValidationReport,
};
pub use options::{DeviceKind, ExecutionProviderSpec, RuntimeOptions};
pub use plugin_loader::load_plugins_from_dir;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub use providers::onnx::OnnxRuntimeBackend;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
pub use providers::onnx::{Acceleration, Platform};
#[cfg(feature = "remote-api")]
pub use providers::remote::{RemoteApiProvider, RemoteApiResult};
pub use providers::stub::StubRuntime;
pub use resolver::{
    current_platform, platform_matches, ArtifactValidation, ResolvedRuntimeVariant,
    RuntimeResolutionAttempt, RuntimeResolver,
};
pub use runtime_registry::RuntimeRegistry;
pub use selection_policy::{
    ModelBackend, ModelCandidate, ModelCapability, ModelEvidence, ModelReadiness,
    ModelSelectionDecision, ModelSelectionMetadata, ModelSelectionPolicy, ModelSelectionRequest,
    SelectionPreference, SelectionReason,
};
pub use session::{
    RunRequest, RunResponse, RuntimeSession, SessionMetadata, TensorMap,
    TensorSpec as SessionTensorSpec,
};
