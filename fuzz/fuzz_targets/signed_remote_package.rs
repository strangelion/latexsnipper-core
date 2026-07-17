#![no_main]

use latexsnipper_plugin::{
    extract_and_verify_remote_package, PluginExecutionClassV3, RegistryTarget, RemotePackageLimits,
};
use libfuzzer_sys::fuzz_target;
use sha2::{Digest, Sha256};

fuzz_target!(|data: &[u8]| {
    let package = &data[..data.len().min(1024 * 1024)];
    let target = RegistryTarget {
        plugin_id: "fuzz.plugin".to_string(),
        version: "1.0.0".to_string(),
        package_path: "packages/fuzz.plugin-1.0.0.zip".to_string(),
        length: package.len() as u64,
        sha256: hex::encode(Sha256::digest(package)),
        execution_class: PluginExecutionClassV3::WasiComponent,
        core_version_requirement: ">=3.0.0, <4".to_string(),
        revoked: false,
        revocation_reason: None,
    };
    let Ok(temporary) = tempfile::tempdir() else {
        return;
    };
    let limits = RemotePackageLimits {
        compressed_bytes: 1024 * 1024,
        decompressed_bytes: 2 * 1024 * 1024,
        files: 64,
        single_file_bytes: 1024 * 1024,
    };
    let _ = extract_and_verify_remote_package(
        package,
        &target,
        &temporary.path().join("staged"),
        limits,
    );
});
