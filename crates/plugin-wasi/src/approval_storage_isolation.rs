//! Trusted-host private storage vetoes. Never grants permissions or opens UI.
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::permissions::ComponentPermissions;
use crate::{WasiDiagnostic, WasiDiagnosticCode};

pub trait ApprovalStorageIsolation: Send + Sync {
    fn check_permissions(
        &self,
        permissions: &ComponentPermissions,
        deadline: Instant,
    ) -> Result<(), WasiDiagnostic>;
}

pub(crate) fn check_isolations(
    guards: &[Arc<dyn ApprovalStorageIsolation>],
    permissions: &ComponentPermissions,
    deadline: Instant,
) -> Result<(), WasiDiagnostic> {
    for guard in guards {
        let check_deadline = deadline.min(Instant::now() + Duration::from_millis(50));
        if Instant::now() >= check_deadline {
            return Err(timeout());
        }
        let result = guard.check_permissions(permissions, check_deadline);
        if Instant::now() >= check_deadline {
            return Err(timeout());
        }
        result?;
    }
    Ok(())
}

fn timeout() -> WasiDiagnostic {
    WasiDiagnostic::new(
        WasiDiagnosticCode::PluginWasiTimeout,
        "private approval storage isolation exceeded its deadline",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_success_and_expired_checks_do_not_bypass_isolation() {
        struct Late;
        impl ApprovalStorageIsolation for Late {
            fn check_permissions(
                &self,
                _: &ComponentPermissions,
                deadline: Instant,
            ) -> Result<(), WasiDiagnostic> {
                std::thread::sleep(
                    deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(2),
                );
                Ok(())
            }
        }
        let guards: Vec<Arc<dyn ApprovalStorageIsolation>> = vec![Arc::new(Late)];
        assert_eq!(
            check_isolations(&guards, &ComponentPermissions::deny_all(), Instant::now())
                .unwrap_err()
                .code,
            WasiDiagnosticCode::PluginWasiTimeout
        );
        assert_eq!(
            check_isolations(
                &guards,
                &ComponentPermissions::deny_all(),
                Instant::now() + Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            WasiDiagnosticCode::PluginWasiTimeout
        );
    }
}
