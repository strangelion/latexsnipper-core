//! Canonical provider environment fingerprint collection.

use sha2::{Digest, Sha256};
use std::path::Path;

use crate::RuntimeProbe;

pub const FINGERPRINT_UNKNOWN: &str = "unknown";
pub const FINGERPRINT_UNAVAILABLE: &str = "unavailable";
pub const FINGERPRINT_NOT_APPLICABLE: &str = "not-applicable";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEnvironmentFingerprint {
    pub core_version: String,
    pub runtime_version: String,
    pub provider: String,
    pub provider_library_fingerprint: String,
    pub os: String,
    pub architecture: String,
    pub device_driver_fingerprint: String,
    pub smoke_model_sha256: String,
    pub runtime_binary_sha256: String,
    pub provider_library_sha256: String,
    pub device_identity: String,
}

impl ProviderEnvironmentFingerprint {
    /// Collect a stable key from observations that the runtime can actually
    /// report. Tagged version/device hashes are not binary or driver hashes.
    pub fn collect(
        core_version: impl Into<String>,
        provider: &str,
        probe: &RuntimeProbe,
        smoke_model_sha256: Option<&str>,
    ) -> Self {
        let provider = provider.trim().to_ascii_lowercase();
        let runtime_version = probe
            .version
            .as_deref()
            .filter(|version| !version.trim().is_empty())
            .unwrap_or(if probe.available {
                FINGERPRINT_UNKNOWN
            } else {
                FINGERPRINT_UNAVAILABLE
            })
            .to_owned();
        let provider_library_fingerprint = if !probe.available {
            FINGERPRINT_UNAVAILABLE.to_owned()
        } else if runtime_version == FINGERPRINT_UNKNOWN {
            FINGERPRINT_UNKNOWN.to_owned()
        } else {
            tagged_sha256(
                "runtime-version-sha256",
                &format!("{provider}\0{runtime_version}"),
            )
        };
        let device_driver_fingerprint = if provider == "cpu" {
            FINGERPRINT_NOT_APPLICABLE.to_owned()
        } else if probe.devices.is_empty() {
            FINGERPRINT_UNKNOWN.to_owned()
        } else {
            let mut devices = probe
                .devices
                .iter()
                .map(|device| {
                    format!(
                        "{:?}\0{}\0{}",
                        device.kind,
                        device.name,
                        device.memory_bytes.map_or_else(
                            || FINGERPRINT_UNKNOWN.to_owned(),
                            |bytes| bytes.to_string()
                        )
                    )
                })
                .collect::<Vec<_>>();
            devices.sort();
            tagged_sha256("runtime-device-sha256", &devices.join("\n"))
        };
        let runtime_binary_sha256 = std::env::var_os("ORT_DYLIB_PATH")
            .as_deref()
            .map(Path::new)
            .and_then(hash_file)
            .unwrap_or_else(|| FINGERPRINT_UNKNOWN.to_owned());
        let provider_library_sha256 = std::env::var_os("LATEXSNIPPER_PROVIDER_LIBRARY_PATH")
            .as_deref()
            .map(Path::new)
            .and_then(hash_file)
            .or_else(|| (provider == "cpu").then(|| runtime_binary_sha256.clone()))
            .unwrap_or_else(|| FINGERPRINT_UNKNOWN.to_owned());
        let device_identity = if provider == "cpu" {
            format!("cpu:{}:{}", std::env::consts::OS, std::env::consts::ARCH)
        } else {
            std::env::var("LATEXSNIPPER_DEVICE_IDENTITY")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| FINGERPRINT_UNKNOWN.to_owned())
        };

        Self {
            core_version: core_version.into(),
            runtime_version,
            provider,
            provider_library_fingerprint,
            os: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            device_driver_fingerprint,
            smoke_model_sha256: smoke_model_sha256
                .filter(|sha| is_sha256(sha))
                .map(str::to_ascii_lowercase)
                .unwrap_or_else(|| FINGERPRINT_UNAVAILABLE.to_owned()),
            runtime_binary_sha256,
            provider_library_sha256,
            device_identity,
        }
    }

    pub fn is_strongly_keyed(&self) -> bool {
        [
            self.runtime_version.as_str(),
            self.provider_library_fingerprint.as_str(),
            self.device_driver_fingerprint.as_str(),
            self.smoke_model_sha256.as_str(),
            self.runtime_binary_sha256.as_str(),
            self.provider_library_sha256.as_str(),
            self.device_identity.as_str(),
        ]
        .iter()
        .all(|value| !is_weak_observation(value))
    }
}

fn hash_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(format!("{:x}", Sha256::digest(bytes)))
}

pub fn is_weak_observation(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    matches!(
        value.as_str(),
        "" | FINGERPRINT_UNKNOWN | FINGERPRINT_UNAVAILABLE | "unverified" | "not-run"
    ) || value.starts_with("runtime-version-sha256:")
        || value.starts_with("runtime-device-sha256:")
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn tagged_sha256(tag: &str, value: &str) -> String {
    format!("{tag}:{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceKind, RuntimeDevice};

    #[test]
    fn cpu_fingerprint_uses_one_canonical_collection_rule() {
        let probe = RuntimeProbe::available(
            Some("ort-api-20".to_owned()),
            vec![RuntimeDevice {
                name: "CPU".to_owned(),
                kind: DeviceKind::Cpu,
                memory_bytes: None,
            }],
        );
        let fingerprint =
            ProviderEnvironmentFingerprint::collect("3.1.0", "CPU", &probe, Some(&"a".repeat(64)));
        assert_eq!(fingerprint.provider, "cpu");
        assert!(fingerprint
            .provider_library_fingerprint
            .starts_with("runtime-version-sha256:"));
        assert_eq!(
            fingerprint.device_driver_fingerprint,
            FINGERPRINT_NOT_APPLICABLE
        );
        assert!(!fingerprint.is_strongly_keyed());
        assert!(is_weak_observation(
            &fingerprint.provider_library_fingerprint
        ));
    }

    #[test]
    fn unavailable_observations_are_not_strong_keys() {
        let fingerprint = ProviderEnvironmentFingerprint::collect(
            "3.1.0",
            "cuda",
            &RuntimeProbe::unavailable("missing"),
            None,
        );
        assert!(!fingerprint.is_strongly_keyed());
    }
}
