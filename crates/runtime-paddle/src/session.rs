//! Paddle Inference session backed by one serialized Predictor instance.

use std::collections::BTreeSet;
use std::sync::Mutex;

use crate::error::{paddle_error, PaddleResult};
use crate::ffi::PaddlePredictor;
use crate::tensor;
use latexsnipper_foundation::Result;
use latexsnipper_runtime::{
    RunRequest, RunResponse, RuntimeKind, RuntimeSession, SessionMetadata, SessionTensorSpec,
    TensorMap,
};

pub struct PaddleSession {
    metadata: SessionMetadata,
    predictor: Mutex<PaddlePredictor>,
    input_names: Vec<String>,
    output_names: Vec<String>,
}

impl PaddleSession {
    pub(crate) fn new(
        model_id: Option<String>,
        mut predictor: PaddlePredictor,
    ) -> PaddleResult<Self> {
        let input_names = predictor.input_names()?;
        let output_names = predictor.output_names()?;
        if input_names.is_empty() {
            return Err(paddle_error("Paddle model declares no inputs"));
        }
        if output_names.is_empty() {
            return Err(paddle_error("Paddle model declares no outputs"));
        }

        let inputs = query_specs(&mut predictor, &input_names, true)?;
        let outputs = query_specs(&mut predictor, &output_names, false)?;
        Ok(Self {
            metadata: SessionMetadata {
                runtime: RuntimeKind::PaddleInference,
                model_id,
                methods: Vec::new(),
                inputs,
                outputs,
            },
            predictor: Mutex::new(predictor),
            input_names,
            output_names,
        })
    }
}

impl RuntimeSession for PaddleSession {
    fn metadata(&self) -> &SessionMetadata {
        &self.metadata
    }

    fn run(&self, request: RunRequest) -> Result<RunResponse> {
        if let Some(method) = request.method {
            return Err(paddle_error(format!(
                "Paddle Predictor does not expose named method '{method}'"
            )));
        }
        validate_inputs(&self.input_names, request.inputs.keys())?;
        let requested =
            validate_requested_outputs(&self.output_names, request.requested_outputs.as_deref())?;

        let mut predictor = self
            .predictor
            .lock()
            .map_err(|_| paddle_error("Paddle Predictor lock was poisoned"))?;
        for name in &self.input_names {
            let input = request
                .inputs
                .get(name)
                .expect("input set was validated before native inference");
            let mut handle = predictor.input_handle(name)?;
            tensor::copy_input(&mut handle, input)?;
        }

        predictor.run()?;

        let mut outputs = TensorMap::new();
        for name in requested {
            let handle = predictor.output_handle(&name)?;
            outputs.insert(name.clone(), tensor::copy_output(&name, &handle)?);
        }
        Ok(RunResponse { outputs })
    }
}

fn query_specs(
    predictor: &mut PaddlePredictor,
    names: &[String],
    input: bool,
) -> PaddleResult<Vec<SessionTensorSpec>> {
    names
        .iter()
        .map(|name| {
            let handle = if input {
                predictor.input_handle(name)?
            } else {
                predictor.output_handle(name)?
            };
            tensor::tensor_spec(name, &handle)
        })
        .collect()
}

fn validate_inputs<'name>(
    expected: &[String],
    provided: impl Iterator<Item = &'name String>,
) -> PaddleResult<()> {
    let expected: BTreeSet<&str> = expected.iter().map(String::as_str).collect();
    let provided: BTreeSet<&str> = provided.map(String::as_str).collect();
    if expected == provided {
        return Ok(());
    }

    let missing: Vec<_> = expected.difference(&provided).copied().collect();
    let unexpected: Vec<_> = provided.difference(&expected).copied().collect();
    Err(paddle_error(format!(
        "Paddle input names do not match model; missing={missing:?}, unexpected={unexpected:?}"
    )))
}

fn validate_requested_outputs(
    declared: &[String],
    requested: Option<&[String]>,
) -> PaddleResult<Vec<String>> {
    let Some(requested) = requested else {
        return Ok(declared.to_vec());
    };
    let declared_set: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
    let mut seen = BTreeSet::new();
    for name in requested {
        if !declared_set.contains(name.as_str()) {
            return Err(paddle_error(format!(
                "Paddle model does not declare requested output '{name}'"
            )));
        }
        if !seen.insert(name.as_str()) {
            return Err(paddle_error(format!(
                "Paddle output '{name}' was requested more than once"
            )));
        }
    }
    Ok(requested.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_validation_reports_both_differences() {
        let expected = vec!["image".to_owned(), "mask".to_owned()];
        let provided = ["image".to_owned(), "extra".to_owned()];
        let error = validate_inputs(&expected, provided.iter()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("mask"));
        assert!(message.contains("extra"));
    }

    #[test]
    fn requested_outputs_are_explicitly_validated() {
        let declared = vec!["tokens".to_owned()];
        let requested = vec!["logits".to_owned()];
        assert!(validate_requested_outputs(&declared, Some(&requested)).is_err());
    }
}
