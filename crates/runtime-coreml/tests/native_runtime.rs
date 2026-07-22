#[cfg(target_vendor = "apple")]
mod apple {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use latexsnipper_runtime::{
        RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind, RuntimeOptions, TensorMap,
    };
    use latexsnipper_runtime_coreml::CoreMlFactory;
    use latexsnipper_tensor::{Tensor, TensorData};
    use serde_json::Value;

    #[test]
    fn coreml_probe_is_available_and_reports_serial_multiarray_support() {
        let probe = CoreMlFactory::new().probe();
        assert!(probe.available, "Core ML probe failed: {probe:?}");
        assert!(probe.capabilities.features.contains("serial-session"));
        assert!(probe.capabilities.features.contains("mlmultiarray"));
    }

    /// Opt-in parity test. Export a case with the official Python/Core ML
    /// reference implementation and point these variables at the native model
    /// and JSON case. Ordinary CI does not need external model assets.
    #[test]
    fn native_outputs_match_reference_case() {
        let Some(model) = std::env::var_os("LATEXSNIPPER_COREML_PARITY_MODEL") else {
            return;
        };
        let Some(case) = std::env::var_os("LATEXSNIPPER_COREML_PARITY_CASE") else {
            return;
        };
        let case: Value = serde_json::from_slice(&std::fs::read(case).unwrap()).unwrap();
        let inputs = parse_tensors(&case["inputs"]);
        let expected = parse_tensors(&case["expected"]);
        let artifacts =
            RuntimeArtifacts::new(RuntimeKind::CoreMl).with_file("model", PathBuf::from(model));
        let session = CoreMlFactory::new()
            .create_session(&artifacts, &RuntimeOptions::default())
            .unwrap();
        let actual = session.run(RunRequest::new(inputs)).unwrap().outputs;
        assert_eq!(
            actual.keys().collect::<Vec<_>>(),
            expected.keys().collect::<Vec<_>>()
        );
        let tolerance = case.get("atol").and_then(Value::as_f64).unwrap_or(1e-5) as f32;
        for (name, expected) in expected {
            let actual = &actual[&name];
            assert_eq!(actual.shape(), expected.shape(), "output {name} shape");
            assert_tensor_close(&name, actual, &expected, tolerance);
        }
    }

    fn parse_tensors(value: &Value) -> TensorMap {
        value
            .as_object()
            .expect("tensor map must be an object")
            .iter()
            .map(|(name, tensor)| {
                let shape = tensor["shape"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_u64().unwrap() as usize)
                    .collect::<Vec<_>>();
                let values = tensor["values"].as_array().unwrap();
                let tensor = match tensor["dtype"].as_str().unwrap() {
                    "f32" => Tensor::float32(
                        name,
                        shape,
                        values
                            .iter()
                            .map(|value| value.as_f64().unwrap() as f32)
                            .collect(),
                    ),
                    "f16" => Tensor::float16_bits(
                        name,
                        shape,
                        values
                            .iter()
                            .map(|value| value.as_u64().unwrap() as u16)
                            .collect(),
                    ),
                    "i32" => Tensor::int32(
                        name,
                        shape,
                        values
                            .iter()
                            .map(|value| value.as_i64().unwrap() as i32)
                            .collect(),
                    ),
                    dtype => panic!("unsupported parity dtype {dtype}"),
                };
                (name.clone(), tensor)
            })
            .collect::<BTreeMap<_, _>>()
    }

    fn assert_tensor_close(name: &str, actual: &Tensor, expected: &Tensor, tolerance: f32) {
        match (actual.data(), expected.data()) {
            (TensorData::Float32(actual), TensorData::Float32(expected)) => {
                let max_error = actual
                    .iter()
                    .zip(expected)
                    .map(|(actual, expected)| (actual - expected).abs())
                    .fold(0.0_f32, f32::max);
                assert!(
                    max_error <= tolerance,
                    "output {name} max_abs_error={max_error}, tolerance={tolerance}"
                );
            }
            (TensorData::Float16(actual), TensorData::Float16(expected)) => {
                assert_eq!(actual, expected, "output {name} f16 bits");
            }
            (TensorData::Int32(actual), TensorData::Int32(expected)) => {
                assert_eq!(actual, expected, "output {name} i32 values");
            }
            _ => panic!("output {name} dtype mismatch"),
        }
    }
}
