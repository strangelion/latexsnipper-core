use std::path::PathBuf;

use latexsnipper_runtime::{
    RunRequest, RuntimeArtifacts, RuntimeFactory, RuntimeKind, RuntimeOptions, TensorMap,
};
use latexsnipper_runtime_executorch::ExecuTorchFactory;
use latexsnipper_tensor::{Tensor, TensorData};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let runtime_home = PathBuf::from(arguments.next().ok_or("missing runtime home argument")?);
    let program_path = PathBuf::from(arguments.next().ok_or("missing program path argument")?);
    if arguments.next().is_some() {
        return Err("expected: executorch_parity <runtime-home> <program.pte>".into());
    }

    let factory = ExecuTorchFactory::with_library_path(runtime_home);
    eprintln!("stage: probe");
    let probe = factory.probe();
    if !probe.available {
        return Err(format!("ExecuTorch probe failed: {probe:?}").into());
    }
    let artifacts =
        RuntimeArtifacts::new(RuntimeKind::ExecuTorch).with_file("program", program_path);
    eprintln!("stage: load program");
    let session = factory.create_session(&artifacts, &RuntimeOptions::default())?;
    let input_name = session
        .metadata()
        .inputs
        .first()
        .ok_or("forward method declares no inputs")?
        .name
        .clone();

    let input = (0..64)
        .map(|index| (index as f32 - 31.5) / 16.0)
        .collect::<Vec<_>>();
    let mut methods = serde_json::Map::new();
    for method in ["forward", "encode"] {
        eprintln!("stage: run {method}");
        let tensor = Tensor::float32(&input_name, vec![1, 1, 8, 8], input.clone());
        let response = session.run(RunRequest {
            method: Some(method.to_owned()),
            inputs: TensorMap::from([(input_name.clone(), tensor)]),
            requested_outputs: None,
        })?;
        let outputs = response
            .outputs
            .into_iter()
            .map(|(name, tensor)| {
                let TensorData::Float32(values) = tensor.data() else {
                    return Err(format!("{method} output '{name}' is not f32"));
                };
                Ok((
                    name,
                    serde_json::json!({
                        "shape": tensor.shape(),
                        "values": values,
                    }),
                ))
            })
            .collect::<Result<serde_json::Map<_, _>, String>>()?;
        methods.insert(method.to_owned(), serde_json::Value::Object(outputs));
    }
    eprintln!("stage: complete");
    println!("{}", serde_json::to_string_pretty(&methods)?);
    Ok(())
}
