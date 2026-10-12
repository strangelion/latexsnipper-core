//! Trusted-host live policy verification. Descriptions cannot grant authority.
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{RememberedApprovalPolicy, WasiDiagnostic, WasiDiagnosticCode};

const CHECK_BUDGET: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RememberedPolicyIdentity {
    pub schema_version: u16,
    pub store_id: String,
    pub policy_id: String,
    pub policy_sha256: String,
}

impl RememberedPolicyIdentity {
    /// Hash authenticated policy metadata for a protected owner index. This
    /// computes a description only; it does not verify a snapshot or approve IO.
    pub fn for_policy(
        store_id: &str,
        policy: &RememberedApprovalPolicy,
    ) -> Result<Self, WasiDiagnostic> {
        if store_id.len() != 32
            || !store_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(failed("invalid live policy store identity"));
        }
        let bytes = serde_json::to_vec(&("core-wasi-live-policy-v1", store_id, policy))
            .map_err(|_| failed("cannot compute live policy identity"))?;
        if bytes.len() > 16 * 1024 {
            return Err(failed("live policy identity metadata exceeds budget"));
        }
        Ok(Self {
            schema_version: 1,
            store_id: store_id.to_owned(),
            policy_id: policy.id.clone(),
            policy_sha256: hex::encode(Sha256::digest(bytes)),
        })
    }
}

/// The trusted host must consult a protected authoritative owner, not a guest or
/// replayable sidecar. Checks must honor the deadline, never show UI or approve
/// actions. A native callback is trusted code, not a preemptible sandbox.
pub trait RememberedApprovalAuthorityGuard: Send + Sync {
    fn check_current(
        &self,
        policy: &RememberedPolicyIdentity,
        deadline: Instant,
    ) -> Result<(), WasiDiagnostic>;
}

#[derive(Clone)]
pub(crate) struct AuthorityBinding {
    guard: Arc<dyn RememberedApprovalAuthorityGuard>,
    identity: RememberedPolicyIdentity,
}

impl AuthorityBinding {
    pub(crate) fn new(
        guard: Arc<dyn RememberedApprovalAuthorityGuard>,
        store_id: &str,
        policy: &RememberedApprovalPolicy,
    ) -> Result<Self, WasiDiagnostic> {
        Ok(Self {
            guard,
            identity: RememberedPolicyIdentity::for_policy(store_id, policy)?,
        })
    }

    pub(crate) fn check(&self, use_deadline: Instant) -> Result<(), WasiDiagnostic> {
        let deadline = Instant::now()
            .checked_add(CHECK_BUDGET)
            .ok_or_else(|| failed("live policy check deadline overflow"))?
            .min(use_deadline);
        if Instant::now() >= deadline {
            return Err(timeout());
        }
        let result = self.guard.check_current(&self.identity, deadline);
        if Instant::now() >= deadline {
            return Err(timeout());
        }
        result
    }
}

fn timeout() -> WasiDiagnostic {
    WasiDiagnostic::new(
        WasiDiagnosticCode::PluginWasiTimeout,
        "live policy authority check exceeded its deadline",
    )
}
fn failed(message: &str) -> WasiDiagnostic {
    WasiDiagnostic::new(WasiDiagnosticCode::PluginWasiHostFailure, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_native_success_is_rejected_and_expired_deadline_does_not_call_guard() {
        struct Late {
            calls: std::sync::atomic::AtomicUsize,
        }
        impl RememberedApprovalAuthorityGuard for Late {
            fn check_current(
                &self,
                _: &RememberedPolicyIdentity,
                deadline: Instant,
            ) -> Result<(), WasiDiagnostic> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                std::thread::sleep(
                    deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(2),
                );
                Ok(())
            }
        }
        let guard = Arc::new(Late {
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let binding = AuthorityBinding {
            guard: guard.clone(),
            identity: RememberedPolicyIdentity {
                schema_version: 1,
                store_id: "a".repeat(32),
                policy_id: "b".repeat(64),
                policy_sha256: "c".repeat(64),
            },
        };
        assert_eq!(
            binding.check(Instant::now()).unwrap_err().code,
            WasiDiagnosticCode::PluginWasiTimeout
        );
        assert_eq!(guard.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(
            binding
                .check(Instant::now() + Duration::from_secs(1))
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiTimeout
        );
        assert_eq!(guard.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
