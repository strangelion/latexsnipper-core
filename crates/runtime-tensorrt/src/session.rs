use std::collections::BTreeSet;
use std::sync::Mutex;

use latexsnipper_foundation::Result;
use latexsnipper_runtime::{
    RunRequest, RunResponse, RuntimeKind, RuntimeSession, SessionMetadata, SessionTensorSpec,
    TensorMap,
};
use latexsnipper_tensor::Tensor;

use crate::error::{tensorrt_error, TensorRtResult};
use crate::ffi::TensorRtProgram;
use crate::tensor::{copy_output, PreparedInput};

pub struct TensorRtSession {
    metadata: SessionMetadata,
    program: Mutex<TensorRtProgram>,
}

impl TensorRtSession {
    pub(crate) fn new(
        runtime: RuntimeKind,
        model_id: Option<String>,
        program: TensorRtProgram,
    ) -> TensorRtResult<Self> {
        let inputs = program.tensor_specs(0)?;
        let outputs = program.tensor_specs(1)?;
        if inputs.is_empty() || outputs.is_empty() {
            return Err(tensorrt_error(format!(
                "engine must declare at least one input and one output, got {}/{}",
                inputs.len(),
                outputs.len()
            )));
        }
        validate_unique_names(&inputs, "input")?;
        validate_unique_names(&outputs, "output")?;
        Ok(Self {
            metadata: SessionMetadata {
                runtime,
                model_id,
                methods: Vec::new(),
                inputs,
                outputs,
            },
            program: Mutex::new(program),
        })
    }
}

impl RuntimeSession for TensorRtSession {
    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        if let Some(method) = request.method {
            return Err(tensorrt_error(format!(
                "native TensorRT session has no named method '{method}'"
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
            .collect::<TensorRtResult<Vec<_>>>()?;
        let views = prepared.iter().map(PreparedInput::view).collect::<Vec<_>>();
        let native = self
            .program
            .lock()
            .map_err(|_| tensorrt_error("TensorRT execution context lock was poisoned"))?
            .run(&views)?;
        if native.count() != self.metadata.outputs.len() {
            return Err(tensorrt_error(format!(
                "engine declares {} outputs but returned {}",
                self.metadata.outputs.len(),
                native.count()
            )));
        }
        let requested: BTreeSet<_> = requested.into_iter().collect();
        let mut outputs = TensorMap::new();
        for (index, spec) in self.metadata.outputs.iter().enumerate() {
            if requested.contains(spec.name.as_str()) {
                outputs.insert(
                    spec.name.clone(),
                    copy_output(&spec.name, native.info(index)?)?,
                );
            }
        }
        Ok(RunResponse { outputs })
    }
}

fn validate_unique_names(specs: &[SessionTensorSpec], direction: &str) -> TensorRtResult<()> {
    let mut names = BTreeSet::new();
    for spec in specs {
        if !names.insert(spec.name.as_str()) {
            return Err(tensorrt_error(format!(
                "engine has duplicate {direction} tensor '{}'",
                spec.name
            )));
        }
    }
    Ok(())
}

fn validate_inputs(expected: &[SessionTensorSpec], provided: &TensorMap) -> TensorRtResult<()> {
    let expected: BTreeSet<_> = expected.iter().map(|spec| spec.name.as_str()).collect();
    let provided: BTreeSet<_> = provided.keys().map(String::as_str).collect();
    if expected == provided {
        return Ok(());
    }
    let missing: Vec<_> = expected.difference(&provided).copied().collect();
    let unexpected: Vec<_> = provided.difference(&expected).copied().collect();
    Err(tensorrt_error(format!(
        "input names do not match engine; missing={missing:?}, unexpected={unexpected:?}"
    )))
}

fn validate_requested_outputs(
    declared: &[SessionTensorSpec],
    requested: Option<&[String]>,
) -> TensorRtResult<Vec<String>> {
    let Some(requested) = requested else {
        return Ok(declared.iter().map(|spec| spec.name.clone()).collect());
    };
    let declared: BTreeSet<_> = declared.iter().map(|spec| spec.name.as_str()).collect();
    let mut seen = BTreeSet::new();
    for name in requested {
        if !declared.contains(name.as_str()) {
            return Err(tensorrt_error(format!(
                "engine does not declare requested output '{name}'"
            )));
        }
        if !seen.insert(name.as_str()) {
            return Err(tensorrt_error(format!(
                "output '{name}' was requested more than once"
            )));
        }
    }
    Ok(requested.to_vec())
}

fn validate_tensor(spec: &SessionTensorSpec, tensor: &Tensor) -> TensorRtResult<()> {
    if tensor.dtype().as_str() != spec.dtype {
        return Err(tensorrt_error(format!(
            "input '{}' requires dtype {}, got {}",
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
        return Err(tensorrt_error(format!(
            "input '{}' requires shape {:?}, got {:?}",
            spec.name,
            spec.shape,
            tensor.shape()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> SessionTensorSpec {
        SessionTensorSpec {
            name: name.to_owned(),
            shape: vec![Some(1), None],
            dtype: "f32".to_owned(),
        }
    }

    #[test]
    fn dynamic_shape_validation_accepts_runtime_dimension() {
        let tensor = Tensor::float32("x", vec![1, 9], vec![0.0; 9]);
        assert!(validate_tensor(&spec("x"), &tensor).is_ok());
    }

    #[test]
    fn requested_outputs_reject_duplicates() {
        let error =
            validate_requested_outputs(&[spec("y")], Some(&["y".to_owned(), "y".to_owned()]))
                .unwrap_err();
        assert!(error.to_string().contains("more than once"));
    }
}
