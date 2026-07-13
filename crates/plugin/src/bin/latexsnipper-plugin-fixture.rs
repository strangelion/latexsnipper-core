use std::path::PathBuf;
use std::time::Duration;

use latexsnipper_plugin::{
    ProcessPluginRequest, ProcessPluginResponse, PROCESS_PLUGIN_PROTOCOL_VERSION,
};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mode = option(&arguments, "--mode").unwrap_or_else(|| "echo".to_string());
    let request = PathBuf::from(required_option(&arguments, "--latexsnipper-plugin-request"));
    let response = PathBuf::from(required_option(
        &arguments,
        "--latexsnipper-plugin-response",
    ));

    match mode.as_str() {
        "infinite" => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        "panic" => panic!("isolated fixture panic"),
        "late-write" => {
            std::thread::sleep(Duration::from_millis(250));
            write_echo(&request, &response);
        }
        "oversize" => {
            std::fs::write(response, vec![b'x'; 1024 * 1024]).unwrap();
        }
        "echo" => write_echo(&request, &response),
        other => panic!("unsupported fixture mode: {other}"),
    }
}

fn write_echo(request_path: &PathBuf, response_path: &PathBuf) {
    let request: ProcessPluginRequest =
        serde_json::from_slice(&std::fs::read(request_path).unwrap()).unwrap();
    assert_eq!(request.protocol_version, PROCESS_PLUGIN_PROTOCOL_VERSION);
    let response = ProcessPluginResponse::success(latexsnipper_plugin::PluginResponse {
        document: request.request.document,
        metadata: request.request.metadata,
    });
    std::fs::write(response_path, serde_json::to_vec(&response).unwrap()).unwrap();
}

fn required_option(arguments: &[String], name: &str) -> String {
    option(arguments, name).unwrap_or_else(|| panic!("missing {name}"))
}

fn option(arguments: &[String], name: &str) -> Option<String> {
    arguments
        .windows(2)
        .find(|values| values[0] == name)
        .map(|values| values[1].clone())
}
