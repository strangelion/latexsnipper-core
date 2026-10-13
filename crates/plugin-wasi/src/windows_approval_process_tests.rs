//! Explicit owned-fixture process coordination checks, not a runtime CLI.
use super::*;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};

const OWNER: &str = "fixture-process-owner";
const ROOT_ENV: &str = "LATEXSNIPPER_TEST_OWNER_ROOT";
const COOKIE_ENV: &str = "LATEXSNIPPER_TEST_OWNER_COOKIE";
const SLOT_ENV: &str = "LATEXSNIPPER_TEST_OWNER_SLOT";
const CUT_ENV: &str = "LATEXSNIPPER_TEST_OWNER_CUT";
const WORKER: &str = "windows_approval_checkpoint::process_tests::native_owner_process_worker";

struct OwnedChild(Option<Child>);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            // Only the exact child created by this fixture can be stopped.
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

struct OwnedCredential<'a>(&'a WindowsProtectedApprovalCheckpoint);
impl Drop for OwnedCredential<'_> {
    fn drop(&mut self) {
        let _ = self.0.remove_test_credential();
    }
}

fn write_marker(owner: &WindowsProtectedApprovalCheckpoint, name: &str, bytes: &[u8]) {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut file = owner.directory.open_with(name, &options).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}

fn marker(owner: &WindowsProtectedApprovalCheckpoint, name: &str) -> Option<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = owner.directory.open_with(name, &options).ok()?;
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file() && metadata.nlink() == 1 && metadata.len() <= 128);
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(129)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 128);
    Some(bytes)
}

fn fixture_request(path: &str) -> crate::ApprovalRequest {
    crate::ApprovalRequest {
        schema_version: 1,
        id: "a".repeat(64),
        plugin_id: "fixture.component".into(),
        plugin_version: "1.0.0".into(),
        component_sha256: "b".repeat(64),
        host_context_sha256: "c".repeat(64),
        remembered_binding_sha256: Some("d".repeat(64)),
        action: crate::SensitiveAction::FileWrite {
            grant_id: "path-0".into(),
            root: "fixture-root".into(),
            relative_path: path.into(),
            target_sha256: None,
            payload_sha256: "e".repeat(64),
            payload_bytes: 1,
        },
    }
}

// Transport helper, deliberately ignored by default. Only the parent fixture
// supplies a checked random root/cookie. A standalone invocation does no IO.
#[test]
#[ignore = "child transport harness; run the parent process coordination test"]
fn native_owner_process_worker() {
    let Some(root) = std::env::var_os(ROOT_ENV) else {
        return;
    };
    let cookie = std::env::var(COOKIE_ENV).unwrap();
    assert!(cookie.len() == 32 && cookie.bytes().all(|b| b.is_ascii_hexdigit()));
    let slot: usize = std::env::var(SLOT_ENV).unwrap().parse().unwrap();
    assert!(slot < 2);
    let mut owner = WindowsProtectedApprovalCheckpoint::open(
        Path::new(&root),
        OWNER,
        WindowsApprovalPersistence::LogonSession,
    )
    .unwrap();
    assert_eq!(
        marker(&owner, "process-fixture-cookie").unwrap(),
        cookie.as_bytes()
    );
    let expected = owner.load().unwrap().unwrap().checkpoint;
    let policies = owner
        .restore_policies(Instant::now() + Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let rows = policies.policies().unwrap();
    assert_eq!(
        rows.len(),
        if std::env::var(CUT_ENV).as_deref() == Ok("written") {
            128
        } else {
            2
        }
    );
    assert!(policies.revoke(&rows[slot].id).unwrap());
    let candidate = policies.seal().unwrap();
    if let Ok(cut) = std::env::var(CUT_ENV) {
        let phase = cut_phase(&cut);
        let mut options = OpenOptions::new();
        options
            .write(true)
            .create_new(true)
            .follow(FollowSymlinks::No);
        let mut file = owner
            .directory
            .open_with("fixture-candidate.lsap", &options)
            .unwrap();
        file.write_all(&candidate.bytes).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let probe_owner = Arc::new(owner.duplicate().unwrap());
        owner.publication_probe = Some(Arc::new(move |actual| {
            if actual == phase {
                write_marker(&probe_owner, "crash-ready", b"parked-at-cut");
                loop {
                    std::thread::park_timeout(Duration::from_secs(1));
                }
            }
        }));
        owner
            .publish(
                &expected,
                &candidate,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
        panic!("publication did not reach the requested fixture cut");
    }
    write_marker(
        &owner,
        &format!("process-ready-{slot}"),
        expected.snapshot_sha256.as_bytes(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while marker(&owner, "process-go").is_none() {
        assert!(
            Instant::now() < deadline,
            "parent fixture did not release barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let published = match owner.publish(
        &expected,
        &candidate,
        Instant::now() + Duration::from_secs(5),
    ) {
        Ok(()) => true,
        Err(error) if error.code == WasiDiagnosticCode::PluginWasiWriteConflict => false,
        Err(_) => panic!("unexpected owner publication error"),
    };
    let remaining = policies.policies().unwrap();
    assert_eq!(remaining.len(), 1);
    let crate::SensitiveAction::FileWrite { relative_path, .. } = &remaining[0].action else {
        panic!("invalid fixture action");
    };
    let request = fixture_request(relative_path);
    if published {
        assert!(policies
            .authorize(&request)
            .unwrap()
            .unwrap()
            .check()
            .is_ok());
        write_marker(&owner, &format!("process-result-{slot}"), b"published-live");
    } else {
        // A real separate process still holds an authenticated old view, but
        // the winning process removed its remaining policy from protected state.
        assert_eq!(
            policies.authorize(&request).err().unwrap().code,
            WasiDiagnosticCode::PluginWasiApprovalRevoked
        );
        write_marker(
            &owner,
            &format!("process-result-{slot}"),
            b"conflict-vetoed",
        );
    }
}

fn cut_phase(value: &str) -> PublicationCut {
    match value {
        "created" => PublicationCut::StagingCreated,
        "written" => PublicationCut::StagingChunkWritten,
        "synced" => PublicationCut::StagingSynced,
        "linked" => PublicationCut::ObjectLinked,
        "ready" => PublicationCut::ObjectReady,
        "checkpoint" => PublicationCut::CheckpointWritten,
        _ => panic!("invalid fixture publication cut"),
    }
}

#[test]
#[ignore = "explicit owned child termination at six publication cuts and cleanup"]
fn native_forced_termination_recovers_selected_state_and_collects_owned_orphans() {
    for cut in [
        "created",
        "written",
        "synced",
        "linked",
        "ready",
        "checkpoint",
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let owner = WindowsProtectedApprovalCheckpoint::open(
            temporary.path(),
            OWNER,
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        assert!(owner.load().unwrap().is_none());
        let cleanup = OwnedCredential(&owner);
        let key = ApprovalPolicyKey::generate().unwrap();
        let policies = RememberedApprovalPolicies::new(ApprovalPolicyKey::from_secret_bytes(
            *key.secret_bytes(),
        ))
        .unwrap();
        let row_count = if cut == "written" { 128 } else { 2 };
        for index in 0..row_count {
            let path = if cut == "written" {
                format!("fixture-{index:03}-{}.txt", "x".repeat(128))
            } else {
                format!("fixture-{index:03}.txt")
            };
            policies
                .remember_request(
                    &fixture_request(&path),
                    crate::ApprovalScope::Exact,
                    Duration::from_secs(3600),
                )
                .unwrap();
        }
        let initial = policies.seal().unwrap();
        owner
            .initialize(&key, &initial, Instant::now() + Duration::from_secs(5))
            .unwrap();
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).unwrap();
        let cookie = hex::encode(random);
        write_marker(&owner, "process-fixture-cookie", cookie.as_bytes());
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", WORKER, "--ignored"])
            .env(ROOT_ENV, temporary.path())
            .env(COOKIE_ENV, &cookie)
            .env(SLOT_ENV, "0")
            .env(CUT_ENV, cut)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(0x0800_0000);
        let mut child = OwnedChild(Some(command.spawn().unwrap()));
        let deadline = Instant::now() + Duration::from_secs(10);
        while marker(&owner, "crash-ready").is_none() {
            assert!(
                child.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "owned cut worker exited early"
            );
            assert!(
                Instant::now() < deadline,
                "owned cut worker readiness timeout"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        child.0.as_mut().unwrap().kill().unwrap();
        while child.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "owned cut worker termination timeout"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let output = child.0.take().unwrap().wait_with_output().unwrap();
        assert!(!output.status.success());
        let restored = owner
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(
            restored.policies().unwrap().len(),
            if cut == "checkpoint" {
                row_count - 1
            } else {
                row_count
            }
        );
        drop(restored);
        let selected = owner.load().unwrap().unwrap().checkpoint;
        assert_eq!(
            selected.revision,
            initial.checkpoint.revision + u64::from(cut == "checkpoint")
        );
        let mut file = owner.directory.open("fixture-candidate.lsap").unwrap();
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        drop(file);
        let checkpoint = crate::remembered_approval::inspect_snapshot_for_cleanup(
            &key,
            &bytes,
            &initial.checkpoint.store_id,
        )
        .unwrap();
        let candidate = SealedApprovalPolicies { bytes, checkpoint };
        if cut == "written" {
            assert!(candidate.bytes.len() > 64 * 1024);
        }
        if cut == "linked" {
            assert!(owner
                .publish(
                    &initial.checkpoint,
                    &candidate,
                    Instant::now() + Duration::from_secs(5)
                )
                .is_err());
        }
        let collected = owner
            .collect_unselected_snapshots(128, Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(owner.load().unwrap().unwrap().checkpoint, selected);
        match cut {
            "created" | "written" | "synced" => {
                assert_eq!(collected.removed_staging, 1);
                assert_eq!(collected.removed_snapshots, 0);
            }
            "linked" => {
                assert_eq!(collected.removed_staging, 1);
                assert_eq!(collected.removed_snapshots, 1);
            }
            _ => {
                assert_eq!(collected.removed_staging, 0);
                assert_eq!(collected.removed_snapshots, 1);
            }
        }
        assert!(marker(&owner, "process-fixture-cookie").is_some());
        if cut == "checkpoint" {
            assert_eq!(
                owner
                    .publish(
                        &initial.checkpoint,
                        &candidate,
                        Instant::now() + Duration::from_secs(5)
                    )
                    .unwrap_err()
                    .code,
                WasiDiagnosticCode::PluginWasiWriteConflict
            );
        } else {
            // Explicit metadata retry only after reading protected state and GC.
            owner
                .publish(
                    &initial.checkpoint,
                    &candidate,
                    Instant::now() + Duration::from_secs(5),
                )
                .unwrap();
        }
        let restored = owner
            .restore_policies(Instant::now() + Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(restored.policies().unwrap().len(), row_count - 1);
        drop(restored);
        owner
            .collect_unselected_snapshots(128, Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            owner
                .collect_unselected_snapshots(128, Instant::now() + Duration::from_secs(5))
                .unwrap()
                .removed(),
            0
        );
        owner.remove_test_credential().unwrap();
        assert!(owner.load().unwrap().is_none());
        drop(child);
        drop(cleanup);
        drop(owner);
        temporary.close().unwrap();
    }
}

#[test]
#[ignore = "explicit two-process owned current-user credential fixture and cleanup"]
fn native_process_race_revokes_stale_client_and_cleans_owned_resources() {
    let temporary = tempfile::tempdir().unwrap();
    let owner = WindowsProtectedApprovalCheckpoint::open(
        temporary.path(),
        OWNER,
        WindowsApprovalPersistence::LogonSession,
    )
    .unwrap();
    assert!(owner.load().unwrap().is_none());
    let cleanup = OwnedCredential(&owner);
    let key = ApprovalPolicyKey::generate().unwrap();
    let policies =
        RememberedApprovalPolicies::new(ApprovalPolicyKey::from_secret_bytes(*key.secret_bytes()))
            .unwrap();
    for path in ["one.txt", "two.txt"] {
        policies
            .remember_request(
                &fixture_request(path),
                crate::ApprovalScope::Exact,
                Duration::from_secs(3600),
            )
            .unwrap();
    }
    let initial = policies.seal().unwrap();
    owner
        .initialize(&key, &initial, Instant::now() + Duration::from_secs(5))
        .unwrap();
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).unwrap();
    let cookie = hex::encode(random);
    write_marker(&owner, "process-fixture-cookie", cookie.as_bytes());
    let executable = std::env::current_exe().unwrap();
    let mut children = Vec::new();
    for slot in 0..2 {
        let mut command = Command::new(&executable);
        command
            .args(["--exact", WORKER, "--ignored"])
            .env(ROOT_ENV, temporary.path())
            .env(COOKIE_ENV, &cookie)
            .env(SLOT_ENV, slot.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        // CREATE_NO_WINDOW, from the Windows SDK process creation flags.
        command.creation_flags(0x0800_0000);
        children.push(OwnedChild(Some(command.spawn().unwrap())));
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let ready = (0..2).all(|slot| {
            marker(&owner, &format!("process-ready-{slot}"))
                .is_some_and(|bytes| bytes == initial.checkpoint.snapshot_sha256.as_bytes())
        });
        if ready {
            break;
        }
        for child in &mut children {
            assert!(
                child.0.as_mut().unwrap().try_wait().unwrap().is_none(),
                "owned worker exited before the publication barrier"
            );
        }
        assert!(
            Instant::now() < deadline,
            "owned process fixture readiness timeout"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(same_snapshot(
        &owner.load().unwrap().unwrap().checkpoint,
        &initial.checkpoint
    ));
    write_marker(&owner, "process-go", b"go");
    for child in &mut children {
        while child.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "owned process fixture completion timeout"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let output = child.0.take().unwrap().wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "owned process fixture worker failed"
        );
    }
    let results: Vec<_> = (0..2)
        .map(|slot| marker(&owner, &format!("process-result-{slot}")).unwrap())
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|value| value.as_slice() == b"published-live")
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|value| value.as_slice() == b"conflict-vetoed")
            .count(),
        1
    );
    let latest = owner.load().unwrap().unwrap();
    assert_eq!(latest.checkpoint.revision, initial.checkpoint.revision + 1);
    let restored = owner
        .restore_policies(Instant::now() + Duration::from_secs(5))
        .unwrap()
        .unwrap();
    assert_eq!(restored.policies().unwrap().len(), 1);
    drop(restored);
    owner.remove_test_credential().unwrap();
    assert!(owner.load().unwrap().is_none());
    drop(children);
    drop(cleanup);
    drop(owner);
    temporary.close().unwrap();
}
