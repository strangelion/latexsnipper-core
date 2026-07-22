//! ExecuTorch-specific options derived from the common runtime options.

use std::path::PathBuf;

use latexsnipper_runtime::RuntimeOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecuTorchOptions {
    /// Explicit bridge library or packaged runtime root.
    pub library_path: Option<PathBuf>,
    /// Method used when `RunRequest::method` is absent.
    pub default_method: String,
    /// Optional stable names for the default method's positional inputs.
    pub input_names: Option<Vec<String>>,
    /// Optional stable names for the default method's positional outputs.
    pub output_names: Option<Vec<String>>,
}

impl Default for ExecuTorchOptions {
    fn default() -> Self {
        Self {
            library_path: None,
            default_method: "forward".to_owned(),
            input_names: None,
            output_names: None,
        }
    }
}

impl ExecuTorchOptions {
    pub fn from_runtime(options: &RuntimeOptions) -> Self {
        Self {
            library_path: string_option(options, "libraryPath")
                .or_else(|| string_option(options, "executorchHome"))
                .map(PathBuf::from),
            default_method: string_option(options, "method")
                .filter(|method| !method.trim().is_empty())
                .unwrap_or_else(|| "forward".to_owned()),
            input_names: string_array_option(options, "inputNames"),
            output_names: string_array_option(options, "outputNames"),
        }
    }
}

fn string_option(options: &RuntimeOptions, key: &str) -> Option<String> {
    options
        .extra
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn string_array_option(options: &RuntimeOptions, key: &str) -> Option<Vec<String>> {
    let names = options.extra.get(key)?.as_array()?;
    names
        .iter()
        .map(|name| name.as_str().map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_method_and_tensor_name_overrides() {
        let mut options = RuntimeOptions::default();
        options.extra.insert("method".to_owned(), "encode".into());
        options.extra.insert(
            "inputNames".to_owned(),
            serde_json::json!(["image", "mask"]),
        );

        let parsed = ExecuTorchOptions::from_runtime(&options);
        assert_eq!(parsed.default_method, "encode");
        assert_eq!(
            parsed.input_names,
            Some(vec!["image".to_owned(), "mask".to_owned()])
        );
    }
}
