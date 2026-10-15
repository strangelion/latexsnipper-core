use super::*;
use latexsnipper_conversion::formula_completion::{
    fragments::{FormulaFragmentCompletionApplyRequest, FormulaFragmentCompletionRequest},
    preferences::persistence::transactions::FormulaPreferenceStorageSession,
    preferences::{persistence::FormulaPreferenceStoragePermission, FormulaPreferencePolicy},
    scoped::FormulaDefinitionScope,
};
use serde_json::json;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

type Session = FormulaPreferenceStorageSession<WindowsPreferenceCheckpoint>;
fn temporary() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix("snipper-preference-fixture-");
    match std::env::var_os("SNIPPER_PREFERENCE_FIXTURE_PARENT") {
        Some(parent) => builder.tempdir_in(parent).unwrap(),
        None => builder.tempdir().unwrap(),
    }
}
fn permission() -> FormulaPreferenceStoragePermission {
    FormulaPreferenceStoragePermission {
        storage_enabled: true,
        allow_sensitive_storage: false,
    }
}
fn enabled() -> FormulaPreferencePolicy {
    FormulaPreferencePolicy {
        ranking_enabled: true,
        feedback_enabled: true,
        allow_sensitive_feedback: false,
    }
}
fn identity() -> FormulaPreferenceStorageIdentity {
    FormulaPreferenceStorageIdentity {
        user_sha256: "a".repeat(64),
        project_sha256: Some("b".repeat(64)),
        document_sha256: Some("c".repeat(64)),
    }
}
fn scope() -> FormulaDefinitionScope {
    FormulaDefinitionScope {
        user_sha256: "a".repeat(64),
        project_sha256: Some("b".repeat(64)),
        document_sha256: Some("c".repeat(64)),
        generation: 0,
    }
}
fn query() -> FormulaFragmentCompletionRequest {
    serde_json::from_value(json!({"source":"中😀\\lo","inputFormat":"latex","cursor":10,"revision":"18446744073709551615",
      "catalog":{"commands":{"scope":{"userSha256":"a".repeat(64),"projectSha256":"b".repeat(64),"documentSha256":"c".repeat(64),"generation":"0"},"definitions":[]},
        "policy":{"retrievalEnabled":true,"sensitiveDocument":false,"allowInSensitiveDocument":false},
        "fragments":[{"id":"a","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localfirst","label":"PRIVATE_LABEL","source":"x"},
          {"id":"z","revision":"0","userSha256":"a".repeat(64),"projectSha256":null,"documentSha256":null,"trigger":"localsecond","label":"PRIVATE_LABEL","source":"y^2"}]}})).unwrap()
}
fn record(session: &mut Session, event: u64, candidate: &str) {
    let request = query();
    let apply = FormulaFragmentCompletionApplyRequest {
        snapshot: request.execute().unwrap().snapshot,
        source: request.source,
        cursor: request.cursor,
        revision: request.revision,
        input_format: request.input_format,
        catalog: request.catalog,
        candidate_id: candidate.into(),
    };
    let core = session.preferences_mut();
    core.record_acceptance_at(&core.capture_context(), &apply, event)
        .unwrap()
        .unwrap();
}
fn reopen(root: &Path) -> WindowsPreferenceCheckpoint {
    WindowsPreferenceCheckpoint::open_verified_private_root(
        root,
        "native-provider-fixture",
        identity(),
        WindowsPreferencePersistence::LogonSession,
        Duration::from_secs(10),
    )
    .unwrap()
}
fn open(backend: WindowsPreferenceCheckpoint) -> Session {
    let owner = backend.owner_sha256().into();
    Session::open(backend, owner, identity(), permission()).unwrap()
}
fn initialize(backend: &WindowsPreferenceCheckpoint) -> Session {
    let mut session = Session::new(
        backend.clone(),
        backend.owner_sha256().into(),
        scope(),
        generate_host_keys().unwrap(),
    )
    .unwrap();
    session.preferences_mut().set_policy(enabled()).unwrap();
    record(&mut session, 1, "fragment:z");
    let mut stage = session.prepare(permission()).unwrap();
    session.commit(&mut stage, permission()).unwrap();
    session
}

struct Fixture {
    backend: Option<WindowsPreferenceCheckpoint>,
    temporary: Option<tempfile::TempDir>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_persistence(WindowsPreferencePersistence::LogonSession)
    }
    fn with_persistence(persistence: WindowsPreferencePersistence) -> Self {
        let temporary = temporary();
        let backend = WindowsPreferenceCheckpoint::create_private_root(
            temporary.path(),
            "private",
            "native-provider-fixture",
            identity(),
            persistence,
            Duration::from_secs(10),
        )
        .unwrap();
        Self {
            backend: Some(backend),
            temporary: Some(temporary),
        }
    }
    fn backend(&self) -> &WindowsPreferenceCheckpoint {
        self.backend.as_ref().unwrap()
    }
    fn root(&self) -> PathBuf {
        self.temporary.as_ref().unwrap().path().join("private")
    }
    fn control(&self) -> &Path {
        self.temporary.as_ref().unwrap().path()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(backend) = self.backend.take() {
            // SAFETY: exact random fixture namespace, no credential enumeration
            // or deletion of application/user-owned credentials is performed.
            let ok = unsafe {
                windows_sys::Win32::Security::Credentials::CredDeleteW(
                    backend.inner.target.as_ptr(),
                    CRED_TYPE_GENERIC,
                    0,
                )
            } != 0
                || unsafe { GetLastError() } == ERROR_NOT_FOUND;
            drop(backend);
            if !std::thread::panicking() {
                assert!(ok, "cannot remove the exact owned fixture credential");
            }
        }
        if let Some(temporary) = self.temporary.take() {
            let result = temporary.close();
            if !std::thread::panicking() {
                result.unwrap();
            }
        }
    }
}

#[test]
fn explicit_dpapi_publication_restores_from_a_fresh_handle_with_learning_off() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    assert!(backend.load().unwrap().is_none());
    let session = initialize(backend);
    assert_eq!(session.preferences().event_count(), 1);
    let head = backend.head(backend.deadline()).unwrap().unwrap();
    let cipher = backend
        .read_object(&head.object_sha256, backend.deadline())
        .unwrap();
    for private in [b"feedbackKey".as_slice(), b"snapshotJson", b"PRIVATE_LABEL"] {
        assert!(!cipher
            .windows(private.len())
            .any(|window| window == private));
    }
    let restored = open(reopen(&fixture.root()));
    assert_eq!(restored.preferences().event_count(), 1);
    assert_eq!(
        restored.preferences().policy(),
        FormulaPreferencePolicy::default()
    );
    assert_eq!(
        backend
            .head(backend.deadline())
            .unwrap()
            .unwrap()
            .object_sha256,
        head.object_sha256
    );
}

#[test]
fn explicit_current_user_persistent_mode_round_trips_without_claiming_a_logon_test() {
    let fixture = Fixture::with_persistence(WindowsPreferencePersistence::ThisUserThisComputer);
    initialize(fixture.backend());
    let same = WindowsPreferenceCheckpoint::open_verified_private_root(
        &fixture.root(),
        "native-provider-fixture",
        identity(),
        WindowsPreferencePersistence::ThisUserThisComputer,
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(same.owner_sha256(), fixture.backend().owner_sha256());
    assert_eq!(open(same).preferences().event_count(), 1);
    let separate = reopen(&fixture.root());
    assert_ne!(separate.owner_sha256(), fixture.backend().owner_sha256());
    assert!(separate.load().unwrap().is_none());
}

#[test]
fn explicit_clear_keeps_an_empty_protected_epoch_and_bounded_cleanup_keeps_current_objects() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    let mut session = initialize(backend);
    let mut stale = open(reopen(&fixture.root()));
    let mut old = stale.prepare(permission()).unwrap();
    for _ in 0..3 {
        let mut stage = session.prepare(permission()).unwrap();
        session.commit(&mut stage, permission()).unwrap();
    }
    let mut clear = session
        .prepare_clear(1, generate_host_keys().unwrap())
        .unwrap();
    session
        .commit(&mut clear, FormulaPreferenceStoragePermission::default())
        .unwrap();
    assert_eq!(
        stale.commit(&mut old, permission()).unwrap_err(),
        FormulaPreferenceTransactionError::Conflict
    );
    let first = backend.cleanup_unselected(1).unwrap();
    assert_eq!(first.removed, 1);
    assert!(first.more);
    assert!(backend.cleanup_unselected(0).is_err());
    assert!(backend.cleanup_unselected(129).is_err());
    assert_eq!(backend.cleanup_unselected(128).unwrap().removed, 3);
    let restored = open(reopen(&fixture.root()));
    assert_eq!(restored.preferences().scope().generation, 1);
    assert_eq!(restored.preferences().event_count(), 0);
    assert_eq!(
        restored.preferences().policy(),
        FormulaPreferencePolicy::default()
    );
    let current = backend.head(backend.deadline()).unwrap().unwrap();
    assert!(fixture
        .root()
        .join(backend.object_name(&current.object_sha256))
        .is_file());
    assert!(fixture.root().join(backend.lock_name()).is_file());
    assert_eq!(backend.cleanup_unselected(128).unwrap().removed, 0);
}

#[test]
fn selected_object_tampering_missing_bytes_and_hard_links_never_choose_an_older_object() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    let mut session = initialize(backend);
    let old = backend.head(backend.deadline()).unwrap().unwrap();
    let old_cipher = backend
        .read_object(&old.object_sha256, backend.deadline())
        .unwrap();
    let mut stage = session.prepare(permission()).unwrap();
    session.commit(&mut stage, permission()).unwrap();
    let current = backend.head(backend.deadline()).unwrap().unwrap();
    let path = fixture
        .root()
        .join(backend.object_name(&current.object_sha256));
    let cipher = std::fs::read(&path).unwrap();
    std::fs::write(&path, &old_cipher).unwrap();
    assert!(backend.load().is_err());
    assert_eq!(
        backend.read_current_record().err(),
        Some(Error::IntegrityMismatch)
    );
    assert!(backend.cleanup_unselected(128).is_err());
    let mut invalid = cipher.clone();
    let last = invalid.len() - 1;
    invalid[last] ^= 1;
    std::fs::write(&path, invalid).unwrap();
    assert!(backend.load().is_err());
    std::fs::write(&path, &cipher).unwrap();
    let extra = fixture.root().join("fixture-hardlink");
    std::fs::hard_link(&path, &extra).unwrap();
    assert!(backend.load().is_err());
    std::fs::remove_file(&extra).unwrap();
    let held = fixture.root().join("fixture-held");
    std::fs::rename(&path, &held).unwrap();
    assert!(backend.load().is_err());
    std::fs::rename(&held, &path).unwrap();
    assert_eq!(open(reopen(&fixture.root())).preferences().event_count(), 1);
    assert_eq!(
        backend
            .head(backend.deadline())
            .unwrap()
            .unwrap()
            .object_sha256,
        current.object_sha256
    );
}

#[test]
fn privacy_acl_is_rechecked_without_repair_or_permanent_blacklisting() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    let mut session = initialize(backend);
    let mut stage = session.prepare(permission()).unwrap();
    private_root::fixture_dacl(&fixture.root(), true);
    assert!(backend.load().is_err());
    assert!(WindowsPreferenceCheckpoint::open_verified_private_root(
        &fixture.root(),
        "native-provider-fixture",
        identity(),
        WindowsPreferencePersistence::LogonSession,
        Duration::from_secs(10)
    )
    .is_err());
    assert_eq!(
        session.commit(&mut stage, permission()).unwrap_err(),
        FormulaPreferenceTransactionError::NotCommitted
    );
    assert_eq!(session.preferences().policy(), enabled());
    private_root::fixture_dacl(&fixture.root(), false);
    session.commit(&mut stage, permission()).unwrap();
    assert_eq!(open(reopen(&fixture.root())).preferences().event_count(), 1);
}

#[test]
fn invalid_or_existing_roots_are_not_created_or_repaired_and_identity_namespaces_are_separate() {
    let temporary = temporary();
    for name in ["", ".", "..", "a/b", "a\\b", "a:", "trailing."] {
        assert!(WindowsPreferenceCheckpoint::create_private_root(
            temporary.path(),
            name,
            "fixture",
            identity(),
            WindowsPreferencePersistence::LogonSession,
            Duration::from_secs(1)
        )
        .is_err());
    }
    assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 0);
    assert!(WindowsPreferenceCheckpoint::open_verified_private_root(
        temporary.path(),
        "fixture",
        identity(),
        WindowsPreferencePersistence::LogonSession,
        Duration::from_secs(1)
    )
    .is_err());
    let fixture = Fixture::new();
    initialize(fixture.backend());
    assert!(WindowsPreferenceCheckpoint::create_private_root(
        fixture.control(),
        "private",
        "fixture",
        identity(),
        WindowsPreferencePersistence::LogonSession,
        Duration::from_secs(1)
    )
    .is_err());
    assert!(WindowsPreferenceCheckpoint::open_verified_private_root(
        &fixture.root(),
        "fixture",
        identity(),
        WindowsPreferencePersistence::LogonSession,
        Duration::ZERO
    )
    .is_err());
    let mut other = identity();
    other.document_sha256 = None;
    let isolated = WindowsPreferenceCheckpoint::open_verified_private_root(
        &fixture.root(),
        "native-provider-fixture",
        other,
        WindowsPreferencePersistence::LogonSession,
        Duration::from_secs(1),
    )
    .unwrap();
    assert_ne!(isolated.owner_sha256(), fixture.backend().owner_sha256());
    assert!(isolated.load().unwrap().is_none());
    assert_eq!(open(reopen(&fixture.root())).preferences().event_count(), 1);
}

#[test]
fn publication_lock_is_bounded_and_temporary_contention_does_not_destroy_the_head() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    initialize(backend);
    let held = backend.lock(backend.deadline()).unwrap();
    let contender = WindowsPreferenceCheckpoint::open_verified_private_root(
        &fixture.root(),
        "native-provider-fixture",
        identity(),
        WindowsPreferencePersistence::LogonSession,
        Duration::from_millis(5),
    )
    .unwrap();
    assert!(contender.load().is_err());
    drop(held);
    assert!(backend.load().unwrap().is_some());
}

#[test]
fn provider_checks_inner_authentication_and_dpapi_namespace_before_accepting_a_record() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    initialize(backend);
    let current = backend.load().unwrap().unwrap();
    let expected = current.witness().unwrap();
    let plaintext = Zeroizing::new(current.encode_for_protected_storage().unwrap());
    let cipher = backend.crypt(&plaintext, true).unwrap();
    let other = WindowsPreferenceCheckpoint::open_verified_private_root(
        &fixture.root(),
        "different-host-owner",
        identity(),
        WindowsPreferencePersistence::LogonSession,
        Duration::from_secs(10),
    )
    .unwrap();
    assert!(other.crypt(&cipher, false).is_err());
    let mut root: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
    root["revision"] = json!("2");
    let mut snapshot: serde_json::Value =
        serde_json::from_str(root["snapshotJson"].as_str().unwrap()).unwrap();
    let authentication = snapshot["authentication"].as_str().unwrap();
    let first = if authentication.starts_with('a') {
        "b"
    } else {
        "a"
    };
    snapshot["authentication"] = json!(format!("{first}{}", &authentication[1..]));
    root["snapshotJson"] = json!(snapshot.to_string());
    let forged = ProtectedPreferenceRecord::decode_from_protected_storage(
        &serde_json::to_vec(&root).unwrap(),
    )
    .unwrap();
    assert!(forged.validate_checkpoint_integrity().is_err());
    assert_eq!(
        backend.compare_exchange(Some(&expected), &forged),
        PreferencePublication::NotCommitted
    );
    assert_eq!(
        backend.load().unwrap().unwrap().witness().unwrap(),
        expected
    );
}

#[test]
fn physical_directory_overlap_rejects_private_parents_children_and_case_aliases() {
    let fixture = Fixture::new();
    let backend = fixture.backend();
    std::fs::create_dir(fixture.root().join("child")).unwrap();
    std::fs::create_dir(fixture.control().join("workspace")).unwrap();
    let parent = Dir::open_ambient_dir(fixture.control(), cap_std::ambient_authority()).unwrap();
    let child =
        Dir::open_ambient_dir(fixture.root().join("child"), cap_std::ambient_authority()).unwrap();
    let sibling = Dir::open_ambient_dir(
        fixture.control().join("workspace"),
        cap_std::ambient_authority(),
    )
    .unwrap();
    for grant in [&parent, &child, &backend.inner.directory] {
        assert!(backend.check_no_directory_overlap(&[grant]).is_err());
    }
    backend.check_no_directory_overlap(&[&sibling]).unwrap();
    backend.check_no_directory_overlap(&[]).unwrap();
    let alias = PathBuf::from(fixture.root().to_str().unwrap().to_uppercase());
    assert_eq!(reopen(&alias).owner_sha256(), backend.owner_sha256());
    let alias = Dir::open_ambient_dir(alias, cap_std::ambient_authority()).unwrap();
    assert!(backend.check_no_directory_overlap(&[&alias]).is_err());
    assert!(backend
        .check_no_directory_overlap(&vec![&sibling; 129])
        .is_err());
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn spawn_child(fixture: &Fixture, mode: &str, name: &str) -> OwnedChild {
    OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "windows::tests::child_process_entry",
                "--nocapture",
            ])
            .env("SNIPPER_PREFERENCE_STORAGE_CHILD", mode)
            .env("SNIPPER_PREFERENCE_STORAGE_ROOT", fixture.root())
            .env("SNIPPER_PREFERENCE_STORAGE_CONTROL", fixture.control())
            .env("SNIPPER_PREFERENCE_STORAGE_NAME", name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "fixture child did not reach its bounded barrier"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[test]
fn two_real_processes_cas_one_winner_without_overwriting_the_other() {
    let fixture = Fixture::new();
    initialize(fixture.backend());
    let mut children = [
        spawn_child(&fixture, "race", "a"),
        spawn_child(&fixture, "race", "z"),
    ];
    wait_until(|| {
        ["a", "z"]
            .iter()
            .all(|name| fixture.control().join(format!("ready-{name}")).is_file())
    });
    std::fs::write(fixture.control().join("release"), b"go").unwrap();
    for child in &mut children {
        wait_until(|| child.0.try_wait().unwrap().is_some());
        assert!(child.0.wait().unwrap().success());
    }
    let mut outcomes: Vec<_> = ["a", "z"]
        .iter()
        .map(|name| {
            std::fs::read_to_string(fixture.control().join(format!("result-{name}"))).unwrap()
        })
        .collect();
    outcomes.sort();
    assert_eq!(outcomes, ["applied", "conflict"]);
    assert_eq!(open(reopen(&fixture.root())).preferences().event_count(), 2);
}

#[test]
fn forced_process_termination_at_five_publication_boundaries_preserves_the_selected_head() {
    for (cut, selected) in [
        ("created", false),
        ("chunk", false),
        ("synced", false),
        ("before", false),
        ("after", true),
    ] {
        let fixture = Fixture::new();
        initialize(fixture.backend());
        let mut child = spawn_child(&fixture, "cut", cut);
        wait_until(|| fixture.control().join("cut-ready").is_file());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let restored = open(reopen(&fixture.root()));
        assert_eq!(
            restored.preferences().event_count(),
            if selected { 1024 } else { 1 }
        );
        assert_eq!(
            restored.preferences().policy(),
            FormulaPreferencePolicy::default()
        );
        assert_eq!(
            fixture.backend().cleanup_unselected(128).unwrap().removed,
            1
        );
    }
}

#[test]
fn child_process_entry() {
    let Ok(mode) = std::env::var("SNIPPER_PREFERENCE_STORAGE_CHILD") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("SNIPPER_PREFERENCE_STORAGE_ROOT").unwrap());
    let control = PathBuf::from(std::env::var_os("SNIPPER_PREFERENCE_STORAGE_CONTROL").unwrap());
    let name = std::env::var("SNIPPER_PREFERENCE_STORAGE_NAME").unwrap();
    let mut backend = reopen(&root);
    if mode == "cut" {
        let requested = match name.as_str() {
            "created" => PublicationCut::ObjectCreated,
            "chunk" => PublicationCut::ChunkWritten,
            "synced" => PublicationCut::ObjectSynced,
            "before" => PublicationCut::BeforeSelection,
            "after" => PublicationCut::AfterSelection,
            _ => panic!("unknown public fixture cut"),
        };
        Arc::get_mut(&mut backend.inner).unwrap().probe = Some(Arc::new(move |cut| {
            if cut == requested {
                std::fs::write(control.join("cut-ready"), b"ready").unwrap();
                loop {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }));
        let mut session = open(backend);
        session.preferences_mut().set_policy(enabled()).unwrap();
        for event in 2..=1024 {
            record(&mut session, event, "fragment:z");
        }
        let mut stage = session.prepare(permission()).unwrap();
        session.commit(&mut stage, permission()).unwrap();
        panic!("fixture cut was not reached");
    }
    assert_eq!(mode, "race");
    let mut session = open(backend);
    session.preferences_mut().set_policy(enabled()).unwrap();
    record(&mut session, 2, &format!("fragment:{name}"));
    let mut stage = session.prepare(permission()).unwrap();
    std::fs::write(control.join(format!("ready-{name}")), b"ready").unwrap();
    wait_until(|| control.join("release").is_file());
    let outcome = match session.commit(&mut stage, permission()) {
        Ok(()) => "applied",
        Err(FormulaPreferenceTransactionError::Conflict) => "conflict",
        Err(_) => panic!("unexpected redacted publication result"),
    };
    std::fs::write(control.join(format!("result-{name}")), outcome).unwrap();
}
