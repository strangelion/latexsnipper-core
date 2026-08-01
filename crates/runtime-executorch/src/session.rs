//! ExecuTorch session with serialized access to one native Module.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use latexsnipper_foundation::Result;
use latexsnipper_runtime::{
    RunRequest, RunResponse, RuntimeKind, RuntimeSession, SessionMetadata, SessionTensorSpec,
    TensorMap,
};
use latexsnipper_tensor::Tensor;

use crate::error::{executorch_error, ExecuTorchResult};
use crate::ffi::{ExecuTorchProgram, NativeMethodMetadata};
use crate::options::ExecuTorchOptions;
use crate::tensor::{copy_output, PreparedInput};

pub struct ExecuTorchSession {
    metadata: SessionMetadata,
    default_method: String,
    methods: BTreeMap<String, NativeMethodMetadata>,
    program: Mutex<ExecuTorchProgram>,
}

impl ExecuTorchSession {
    pub(crate) fn new(
        model_id: Option<String>,
        program: ExecuTorchProgram,
        options: &ExecuTorchOptions,
    ) -> ExecuTorchResult<Self> {
        let mut methods = program
            .methods()?
            .into_iter()
            .map(|method| (method.name.clone(), method))
            .collect::<BTreeMap<_, _>>();
        if methods.is_empty() {
            return Err(executorch_error("ExecuTorch program declares no methods"));
        }
        if !methods.contains_key(&options.default_method) {
            return Err(executorch_error(format!(
                "ExecuTorch program has no method '{}'; available methods: {:?}",
                options.default_method,
                methods.keys().collect::<Vec<_>>()
            )));
        }
        let default = methods
            .get_mut(&options.default_method)
            .expect("default method existence was checked");
        apply_name_overrides(&mut default.inputs, options.input_names.as_deref(), "input")?;
        apply_name_overrides(
            &mut default.outputs,
            options.output_names.as_deref(),
            "output",
        )?;
        for method in methods.values() {
            validate_unique_names(&method.name, &method.inputs, "input")?;
            validate_unique_names(&method.name, &method.outputs, "output")?;
        }
        let default = &methods[&options.default_method];
        let metadata = SessionMetadata {
            runtime: RuntimeKind::ExecuTorch,
            model_id,
            requested_providers: vec!["executorch-native".to_owned()],
            effective_provider: Some("executorch-native".to_owned()),
            fallback_chain: Vec::new(),
            fallback_diagnostics: Vec::new(),
            methods: methods.keys().cloned().collect(),
            inputs: default.inputs.clone(),
            outputs: default.outputs.clone(),
        };
        Ok(Self {
            metadata,
            default_method: options.default_method.clone(),
            methods,
            program: Mutex::new(program),
        })
    }
}

impl RuntimeSession for ExecuTorchSession {
    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        let method_name = request.method.as_deref().unwrap_or(&self.default_method);
        let method = self.methods.get(method_name).ok_or_else(|| {
            executorch_error(format!(
                "ExecuTorch program has no method '{method_name}'; available methods: {:?}",
                self.methods.keys().collect::<Vec<_>>()
            ))
        })?;
        validate_inputs(&method.inputs, &request.inputs)?;
        let requested =
            validate_requested_outputs(&method.outputs, request.requested_outputs.as_deref())?;

        let prepared = method
            .inputs
            .iter()
            .map(|spec| {
                let tensor = request
                    .inputs
                    .get(&spec.name)
                    .expect("input names were validated before preparation");
                validate_tensor(spec, tensor)?;
                PreparedInput::new(tensor)
            })
            .collect::<ExecuTorchResult<Vec<_>>>()?;
        let views = prepared.iter().map(PreparedInput::view).collect::<Vec<_>>();
        let native_outputs = self
            .program
            .lock()
            .map_err(|_| executorch_error("ExecuTorch Module lock was poisoned"))?
            .run(method_name, &views)?;
        if native_outputs.count() != method.outputs.len() {
            return Err(executorch_error(format!(
                "method '{method_name}' declared {} outputs but returned {}",
                method.outputs.len(),
                native_outputs.count()
            )));
        }

        let requested = requested.into_iter().collect::<BTreeSet<_>>();
        let mut outputs = TensorMap::new();
        for (index, spec) in method.outputs.iter().enumerate() {
            if requested.contains(spec.name.as_str()) {
                let output = copy_output(&spec.name, native_outputs.info(index)?)?;
                outputs.insert(spec.name.clone(), output);
            }
        }
        Ok(RunResponse { outputs })
    }
}

fn apply_name_overrides(
    specs: &mut [SessionTensorSpec],
    overrides: Option<&[String]>,
    direction: &str,
) -> ExecuTorchResult<()> {
    let Some(overrides) = overrides else {
        return Ok(());
    };
    if overrides.len() != specs.len() {
        return Err(executorch_error(format!(
            "ExecuTorch {direction}Names contains {} names, but the default method declares {} {direction}s",
            overrides.len(),
            specs.len()
        )));
    }
    for (spec, name) in specs.iter_mut().zip(overrides) {
        if name.trim().is_empty() {
            return Err(executorch_error(format!(
                "ExecuTorch {direction}Names contains an empty name"
            )));
        }
        spec.name.clone_from(name);
    }
    Ok(())
}

fn validate_unique_names(
    method: &str,
    specs: &[SessionTensorSpec],
    direction: &str,
) -> ExecuTorchResult<()> {
    let mut names = BTreeSet::new();
    for spec in specs {
        if !names.insert(spec.name.as_str()) {
            return Err(executorch_error(format!(
                "method '{method}' has duplicate {direction} name '{}'",
                spec.name
            )));
        }
    }
    Ok(())
}

fn validate_inputs(expected: &[SessionTensorSpec], provided: &TensorMap) -> ExecuTorchResult<()> {
    let expected: BTreeSet<_> = expected.iter().map(|spec| spec.name.as_str()).collect();
    let provided: BTreeSet<_> = provided.keys().map(String::as_str).collect();
    if expected == provided {
        return Ok(());
    }
    let missing: Vec<_> = expected.difference(&provided).copied().collect();
    let unexpected: Vec<_> = provided.difference(&expected).copied().collect();
    Err(executorch_error(format!(
        "ExecuTorch input names do not match method; missing={missing:?}, unexpected={unexpected:?}"
    )))
}

fn validate_requested_outputs(
    declared: &[SessionTensorSpec],
    requested: Option<&[String]>,
) -> ExecuTorchResult<Vec<String>> {
    let Some(requested) = requested else {
        return Ok(declared.iter().map(|spec| spec.name.clone()).collect());
    };
    let declared: BTreeSet<_> = declared.iter().map(|spec| spec.name.as_str()).collect();
    let mut seen = BTreeSet::new();
    for name in requested {
        if !declared.contains(name.as_str()) {
            return Err(executorch_error(format!(
                "ExecuTorch method does not declare requested output '{name}'"
            )));
        }
        if !seen.insert(name.as_str()) {
            return Err(executorch_error(format!(
                "ExecuTorch output '{name}' was requested more than once"
            )));
        }
    }
    Ok(requested.to_vec())
}

fn validate_tensor(spec: &SessionTensorSpec, tensor: &Tensor) -> ExecuTorchResult<()> {
    if tensor.dtype().as_str() != spec.dtype {
        return Err(executorch_error(format!(
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
        return Err(executorch_error(format!(
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
    fn validates_dynamic_tensor_shape_and_dtype() {
        let tensor = Tensor::float32("x", vec![1, 2], vec![1.0, 2.0]);
        assert!(validate_tensor(&spec("x"), &tensor).is_ok());
        let wrong = Tensor::int64("x", vec![1, 2], vec![1, 2]);
        assert!(validate_tensor(&spec("x"), &wrong).is_err());
    }

    #[test]
    fn input_validation_reports_both_differences() {
        let expected = vec![spec("image"), spec("mask")];
        let provided = BTreeMap::from([
            (
                "image".to_owned(),
                Tensor::float32("image", vec![1, 1], vec![0.0]),
            ),
            (
                "extra".to_owned(),
                Tensor::float32("extra", vec![1, 1], vec![0.0]),
            ),
        ]);
        let message = validate_inputs(&expected, &provided)
            .unwrap_err()
            .to_string();
        assert!(message.contains("mask") && message.contains("extra"));
    }
}
