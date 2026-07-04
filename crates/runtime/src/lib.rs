pub mod acceleration;
pub mod backend;
pub mod model_handle;
pub mod model_package;
pub mod model_plugin;
pub mod model_registry;
pub mod model_resolver;
pub mod model_validation;
pub mod plugin_loader;
pub mod providers;
pub mod session;

pub use acceleration::AccelerationMode;
pub use backend::RuntimeBackend;
pub use model_handle::ModelHandle;
pub use model_package::{
    DetectionResult, FormulaResult, InferenceContext, ModelDescriptor, ModelExecutor, ModelInput,
    ModelOutput, ModelPackage, ModelTask, TableResult, TensorDtype, TensorSpec, TextResult,
};
pub use model_plugin::{ManifestAdapter, ModelManifestView, ModelPlugin, ModelPluginRegistry};
pub use plugin_loader::load_plugins_from_dir;
pub use model_registry::{
    ManifestDecoding, ManifestPreprocessing, ManifestResize, ManifestTensorSpec, ModelFiles,
    ModelManifest, ModelRegistry,
};
pub use model_resolver::{FsModelResolver, MemoryModelResolver, ModelId, ModelResolver, SharedModelResolver};
pub use model_validation::{
    compute_bytes_checksum, compute_checksum, load_checksums, validate_all_models,
    validate_model, validate_model_bytes, ValidationReport,
};
pub use providers::onnx::OnnxRuntimeBackend;
pub use providers::onnx::{Acceleration, Platform};
pub use providers::stub::StubRuntime;
pub use session::InferenceSession;
