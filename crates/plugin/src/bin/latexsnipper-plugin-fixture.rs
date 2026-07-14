use std::path::PathBuf;
use std::time::Duration;

use latexsnipper_plugin::{
    ProcessPluginRequest, ProcessPluginResponse, PROCESS_PLUGIN_PROTOCOL_VERSION,
};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mode = option(&arguments, "--mode").unwrap_or_else(|| "echo".to_string());
    if mode == "grandchild" {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
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
        "spawn-descendant" => {
            let pid_file = PathBuf::from(required_option(&arguments, "--pid-file"));
            let mut grandchild = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--mode", "grandchild"])
                .spawn()
                .unwrap();
            std::fs::write(pid_file, grandchild.id().to_string()).unwrap();
            grandchild.wait().unwrap();
        }
        "empty-response" => write_protocol_response(&response, None, None, None),
        "mixed-response" => {
            let request = read_request(&request);
            write_protocol_response(
                &response,
                Some(latexsnipper_plugin::PluginResponse {
                    document: request.request.document,
                    metadata: request.request.metadata,
                }),
                Some("PLUGIN_FIXTURE_ERROR".to_string()),
                Some("mixed response".to_string()),
            );
        }
        "code-only-response" => write_protocol_response(
            &response,
            None,
            Some("PLUGIN_FIXTURE_ERROR".to_string()),
            None,
        ),
        "echo" => write_echo(&request, &response),
        other => panic!("unsupported fixture mode: {other}"),
    }
}

fn write_echo(request_path: &PathBuf, response_path: &PathBuf) {
    let request = read_request(request_path);
    assert_eq!(request.protocol_version, PROCESS_PLUGIN_PROTOCOL_VERSION);
    let response = ProcessPluginResponse::success(latexsnipper_plugin::PluginResponse {
        document: request.request.document,
        metadata: request.request.metadata,
    });
    std::fs::write(response_path, serde_json::to_vec(&response).unwrap()).unwrap();
}

fn read_request(request_path: &PathBuf) -> ProcessPluginRequest {
    serde_json::from_slice(&std::fs::read(request_path).unwrap()).unwrap()
}

fn write_protocol_response(
    response_path: &PathBuf,
    response: Option<latexsnipper_plugin::PluginResponse>,
    error_code: Option<String>,
    error_message: Option<String>,
) {
    let response = ProcessPluginResponse {
        protocol_version: PROCESS_PLUGIN_PROTOCOL_VERSION,
        response,
        error_code,
        error_message,
    };
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
