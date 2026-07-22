//! Core ML session with serialized access to one native `MLModel` instance.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use latexsnipper_foundation::Result;
use latexsnipper_runtime::{
    RunRequest, RunResponse, RuntimeKind, RuntimeSession, SessionMetadata, SessionTensorSpec,
    TensorMap,
};
use latexsnipper_tensor::Tensor;

use crate::error::{coreml_error, CoreMlResult};
use crate::ffi::CoreMlProgram;
use crate::options::CoreMlOptions;
use crate::tensor::{copy_output, PreparedInput};

pub(crate) struct TemporaryCompiledModel {
    path: PathBuf,
    root: PathBuf,
}

impl TemporaryCompiledModel {
    pub(crate) fn new(path: PathBuf, root: PathBuf) -> Self {
        Self { path, root }
    }
}

impl Drop for TemporaryCompiledModel {
    fn drop(&mut self) {
        let is_owned_temporary = self.path.parent() == Some(self.root.as_path())
            && self
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.') && name.ends_with(".mlmodelc"));
        if !is_owned_temporary {
            log::error!(
                "Refusing to remove unrecognized temporary Core ML path '{}'",
                self.path.display()
            );
            return;
        }
        if let Err(error) = std::fs::remove_dir_all(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "Failed to remove temporary Core ML model '{}': {error}",
                    self.path.display()
                );
            }
        }
    }
}

pub(crate) struct CoreMlSession {
    metadata: SessionMetadata,
    program: Mutex<CoreMlProgram>,
    _temporary_model: Option<TemporaryCompiledModel>,
}

impl CoreMlSession {
    pub(crate) fn load(
        path: &Path,
        options: &CoreMlOptions,
        temporary_model: Option<TemporaryCompiledModel>,
    ) -> CoreMlResult<Self> {
        let program = CoreMlProgram::load(path, options.compute_units.native_code())?;
        let inputs = program.inputs()?;
        let outputs = program.outputs()?;
        if inputs.is_empty() || outputs.is_empty() {
            return Err(coreml_error(
                "Core ML model must declare at least one MLMultiArray input and output",
            ));
        }
        validate_unique_names(&inputs, "input")?;
        validate_unique_names(&outputs, "output")?;
        Ok(Self {
            metadata: SessionMetadata {
                runtime: RuntimeKind::CoreMl,
                model_id: Some(path.to_string_lossy().into_owned()),
                methods: vec!["predict".to_owned()],
                inputs,
                outputs,
            },
            program: Mutex::new(program),
            _temporary_model: temporary_model,
        })
    }
}

impl RuntimeSession for CoreMlSession {
    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        if request
            .method
            .as_deref()
            .is_some_and(|method| method != "predict")
        {
            return Err(coreml_error(format!(
                "Core ML exposes only the 'predict' method, not '{}'",
                request.method.as_deref().unwrap_or_default()
            )));
        }
        validate_inputs(&self.metadata.inputs, &request.inputs)?;
        let requested = validate_requested_outputs(
            &self.metadata.outputs,
            request.requested_outputs.as_deref(),
        )?;
        let prepared = self
            .metadata
            .inputs
            .iter()
            .map(|spec| {
                let tensor = request
                    .inputs
                    .get(&spec.name)
                    .expect("input names were validated before preparation");
                validate_tensor(spec, tensor)?;
                PreparedInput::new(&spec.name, tensor)
            })
            .collect::<CoreMlResult<Vec<_>>>()?;
        let views = prepared.iter().map(PreparedInput::view).collect::<Vec<_>>();
        let native_outputs = self
            .program
            .lock()
            .map_err(|_| coreml_error("Core ML model lock was poisoned"))?
            .run(&views)?;
        if native_outputs.count() != self.metadata.outputs.len() {
            return Err(coreml_error(format!(
                "Core ML model declared {} outputs but returned {}",
                self.metadata.outputs.len(),
                native_outputs.count()
            )));
        }

        let requested = requested.into_iter().collect::<BTreeSet<_>>();
        let declared = self
            .metadata
            .outputs
            .iter()
            .map(|spec| spec.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut outputs = TensorMap::new();
        for index in 0..native_outputs.count() {
            let native = native_outputs.info(index)?;
            if !declared.contains(native.name.as_str()) {
                return Err(coreml_error(format!(
                    "Core ML returned undeclared output '{}'",
                    native.name
                )));
            }
            if requested.contains(native.name.as_str()) {
                let name = native.name.clone();
                if outputs.insert(name.clone(), copy_output(native)?).is_some() {
                    return Err(coreml_error(format!(
                        "Core ML returned duplicate output '{name}'"
                    )));
                }
            }
        }
        if outputs.len() != requested.len() {
            return Err(coreml_error(
                "Core ML did not return every requested output",
            ));
        }
        Ok(RunResponse { outputs })
    }
}

fn validate_unique_names(specs: &[SessionTensorSpec], direction: &str) -> CoreMlResult<()> {
    let mut names = BTreeSet::new();
    for spec in specs {
        if !names.insert(spec.name.as_str()) {
            return Err(coreml_error(format!(
                "Core ML model declares duplicate {direction} '{}'",
                spec.name
            )));
        }
    }
    Ok(())
}

fn validate_inputs(expected: &[SessionTensorSpec], provided: &TensorMap) -> CoreMlResult<()> {
    let expected = expected
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();
    let provided = provided.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if expected == provided {
        return Ok(());
    }
    let missing = expected.difference(&provided).copied().collect::<Vec<_>>();
    let unexpected = provided.difference(&expected).copied().collect::<Vec<_>>();
    Err(coreml_error(format!(
        "Core ML input names do not match model; missing={missing:?}, unexpected={unexpected:?}"
    )))
}

fn validate_requested_outputs(
    declared: &[SessionTensorSpec],
    requested: Option<&[String]>,
) -> CoreMlResult<Vec<String>> {
    let Some(requested) = requested else {
        return Ok(declared.iter().map(|spec| spec.name.clone()).collect());
    };
    let declared = declared
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for name in requested {
        if !declared.contains(name.as_str()) {
            return Err(coreml_error(format!(
                "Core ML model does not declare requested output '{name}'"
            )));
        }
        if !seen.insert(name.as_str()) {
            return Err(coreml_error(format!(
                "Core ML output '{name}' was requested more than once"
            )));
        }
    }
    Ok(requested.to_vec())
}

fn validate_tensor(spec: &SessionTensorSpec, tensor: &Tensor) -> CoreMlResult<()> {
    if tensor.dtype().as_str() != spec.dtype {
        return Err(coreml_error(format!(
            "Core ML input '{}' requires dtype {}, got {}",
            spec.name,
            spec.dtype,
            tensor.dtype().as_str()
        )));
    }
    if tensor.shape().len() != spec.shape.len()
        || spec
            .shape
            .iter()
            .zip(tensor.shape())
            .any(|(expected, actual)| {
                expected.is_some_and(|value| i64::try_from(*actual).ok() != Some(value))
            })
    {
        return Err(coreml_error(format!(
            "Core ML input '{}' requires shape {:?}, got {:?}",
            spec.name,
            spec.shape,
            tensor.shape()
        )));
    }
    Ok(())
}
