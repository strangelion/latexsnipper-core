//! Bounded, explicit collection of this protected owner's unselected cache data.
use super::*;
use hmac::{Hmac, Mac};
use std::collections::BTreeSet;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const READ_BUDGET: usize = 8 * 1024 * 1024;
const NAME_DOMAIN: &[u8] = b"latexsnipper-approval-stage-name-v1\0";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ApprovalSnapshotCleanup {
    pub scanned_entries: usize,
    pub removed_snapshots: usize,
    pub removed_staging: usize,
    pub skipped_unverified: usize,
    pub budget_exhausted: bool,
}

impl ApprovalSnapshotCleanup {
    pub(super) fn removed(&self) -> usize {
        self.removed_snapshots + self.removed_staging
    }
}

struct CacheFile {
    bytes: Vec<u8>,
    metadata: cap_std::fs::Metadata,
}

impl WindowsProtectedApprovalCheckpoint {
    pub(super) fn staging_name(
        &self,
        key: &ApprovalPolicyKey,
        nonce: &[u8; 16],
    ) -> Result<String, WasiDiagnostic> {
        let nonce = hex::encode(nonce);
        let mac = name_mac(key, &self.owner_sha256, &nonce)?;
        let tag = hex::encode(mac.finalize().into_bytes());
        Ok(format!(
            "approval-stage-{}-{nonce}-{tag}.tmp",
            self.owner_sha256
        ))
    }

    fn owns_staging(&self, key: &ApprovalPolicyKey, name: &str) -> bool {
        let prefix = format!("approval-stage-{}-", self.owner_sha256);
        let Some(suffix) = name
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(".tmp"))
        else {
            return false;
        };
        let Some((nonce, tag)) = suffix.split_once('-') else {
            return false;
        };
        if !lower_hex(nonce, 32) || !lower_hex(tag, 64) {
            return false;
        }
        let Ok(tag) = hex::decode(tag) else {
            return false;
        };
        name_mac(key, &self.owner_sha256, nonce).is_ok_and(|mac| mac.verify_slice(&tag).is_ok())
    }

    /// Never collects the selected snapshot, directories, unrelated owners or
    /// unauthenticated snapshots. This is not a general cache/tree delete API.
    /// A timeout/error may leave some unselected objects already collected.
    pub fn collect_unselected_snapshots(
        &self,
        max_objects: usize,
        deadline: Instant,
    ) -> Result<ApprovalSnapshotCleanup, WasiDiagnostic> {
        if !(1..=128).contains(&max_objects) {
            return Err(failed("invalid approval cleanup object budget"));
        }
        let _lock = self.lock(deadline)?;
        let current = self
            .load()?
            .ok_or_else(|| failed("protected approval owner is missing"))?;
        let selected = self.snapshot_name(&current.checkpoint);
        // Preserve all recovery data if the protected latest object is invalid.
        let bytes = self.read_snapshot(&current.checkpoint, deadline)?;
        validate_candidate(
            &current.key,
            &SealedApprovalPolicies {
                bytes,
                checkpoint: current.checkpoint.clone(),
            },
        )?;
        let mut result = ApprovalSnapshotCleanup::default();
        let mut stages = Vec::new();
        let mut snapshots = Vec::new();
        let prefix = format!("approval-{}-", self.owner_sha256);
        for entry in self
            .directory
            .read_dir(".")
            .map_err(|_| failed("cannot enumerate approval cache"))?
        {
            check_deadline(deadline)?;
            result.scanned_entries += 1;
            if result.scanned_entries > 4096 {
                return Err(failed("approval cleanup scan budget exceeded"));
            }
            let entry = entry.map_err(|_| failed("cannot inspect approval cache entry"))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if self.owns_staging(&current.key, name) {
                stages.push(name.to_owned());
            } else if name != selected
                && name
                    .strip_prefix(&prefix)
                    .and_then(|s| s.strip_suffix(".lsap"))
                    .is_some_and(|digest| lower_hex(digest, 64))
            {
                snapshots.push(name.to_owned());
            }
        }
        let mut read_bytes = 0;
        let mut removed_names = BTreeSet::new();
        for name in stages {
            if result.removed() >= max_objects {
                result.budget_exhausted = true;
                break;
            }
            let Some(stage) = self.cache_file(&name, &mut read_bytes, deadline)? else {
                result.skipped_unverified += 1;
                continue;
            };
            match stage.metadata.nlink() {
                1 => {
                    // Authenticated random staging names identify app-created
                    // partial objects; payload need not be complete after a kill.
                    self.remove_cache_file(&name, &stage.metadata, 1, deadline)?;
                    result.removed_staging += 1;
                }
                2 => {
                    if result.removed() + 2 > max_objects {
                        result.budget_exhausted = true;
                        continue;
                    }
                    let Ok(checkpoint) = crate::remembered_approval::inspect_snapshot_for_cleanup(
                        &current.key,
                        &stage.bytes,
                        &current.checkpoint.store_id,
                    ) else {
                        result.skipped_unverified += 1;
                        continue;
                    };
                    let target = self.snapshot_name(&checkpoint);
                    if target == selected {
                        result.skipped_unverified += 1;
                        continue;
                    }
                    let Some(target_file) = self.cache_file(&target, &mut read_bytes, deadline)?
                    else {
                        result.skipped_unverified += 1;
                        continue;
                    };
                    if target_file.metadata.nlink() != 2
                        || !same_file(&stage.metadata, &target_file.metadata)
                        || target_file.bytes != stage.bytes
                    {
                        result.skipped_unverified += 1;
                        continue;
                    }
                    self.remove_cache_file(&name, &stage.metadata, 2, deadline)?;
                    result.removed_staging += 1;
                    self.remove_cache_file(&target, &target_file.metadata, 1, deadline)?;
                    result.removed_snapshots += 1;
                    removed_names.insert(target);
                }
                _ => {
                    result.skipped_unverified += 1;
                }
            }
        }
        for name in snapshots {
            if removed_names.contains(&name) {
                continue;
            }
            if result.removed() >= max_objects {
                result.budget_exhausted = true;
                break;
            }
            let Some(file) = self.cache_file(&name, &mut read_bytes, deadline)? else {
                result.skipped_unverified += 1;
                continue;
            };
            if file.metadata.nlink() != 1 {
                result.skipped_unverified += 1;
                continue;
            }
            let Ok(checkpoint) = crate::remembered_approval::inspect_snapshot_for_cleanup(
                &current.key,
                &file.bytes,
                &current.checkpoint.store_id,
            ) else {
                result.skipped_unverified += 1;
                continue;
            };
            if self.snapshot_name(&checkpoint) != name {
                result.skipped_unverified += 1;
                continue;
            }
            self.remove_cache_file(&name, &file.metadata, 1, deadline)?;
            result.removed_snapshots += 1;
        }
        check_deadline(deadline)?;
        Ok(result)
    }

    fn cache_file(
        &self,
        name: &str,
        read_bytes: &mut usize,
        deadline: Instant,
    ) -> Result<Option<CacheFile>, WasiDiagnostic> {
        check_deadline(deadline)?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let Ok(mut file) = self.directory.open_with(name, &options) else {
            return Ok(None);
        };
        let metadata = file
            .metadata()
            .map_err(|_| failed("cannot inspect approval cache object"))?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 || metadata.ino() == 0 {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            check_deadline(deadline)?;
            let count = file
                .read(&mut buffer)
                .map_err(|_| failed("cannot read approval cache object"))?;
            if count == 0 {
                break;
            }
            *read_bytes += count;
            if *read_bytes > READ_BUDGET {
                return Err(failed("approval cleanup read budget exceeded"));
            }
            bytes.extend_from_slice(&buffer[..count]);
            if bytes.len() > MAX_BYTES {
                return Ok(None);
            }
        }
        if bytes.len() as u64 != metadata.len() {
            return Ok(None);
        }
        Ok(Some(CacheFile { bytes, metadata }))
    }

    fn remove_cache_file(
        &self,
        name: &str,
        expected: &cap_std::fs::Metadata,
        links: u64,
        deadline: Instant,
    ) -> Result<(), WasiDiagnostic> {
        check_deadline(deadline)?;
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let file = self
            .directory
            .open_with(name, &options)
            .map_err(|_| failed("approval cleanup object changed"))?;
        let metadata = file
            .metadata()
            .map_err(|_| failed("cannot recheck approval cleanup object"))?;
        if !metadata.is_file()
            || metadata.nlink() != links
            || !same_file(&metadata, expected)
            || metadata.len() != expected.len()
        {
            return Err(conflict("approval cleanup object changed"));
        }
        drop(file);
        check_deadline(deadline)?;
        self.directory
            .remove_file(name)
            .map_err(|_| failed("cannot collect owned approval cache object"))
    }
}

fn name_mac(
    key: &ApprovalPolicyKey,
    owner: &str,
    nonce: &str,
) -> Result<Hmac<Sha256>, WasiDiagnostic> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.secret_bytes())
        .map_err(|_| failed("invalid approval staging key"))?;
    mac.update(NAME_DOMAIN);
    mac.update(owner.as_bytes());
    mac.update(nonce.as_bytes());
    Ok(mac)
}
fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn same_file(a: &cap_std::fs::Metadata, b: &cap_std::fs::Metadata) -> bool {
    a.ino() != 0 && a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OwnedCredential<'a>(&'a WindowsProtectedApprovalCheckpoint);
    impl Drop for OwnedCredential<'_> {
        fn drop(&mut self) {
            let _ = self.0.remove_test_credential();
        }
    }

    #[test]
    fn staging_name_authentication_is_bound_to_owner_key_and_nonce() {
        let temporary = tempfile::tempdir().unwrap();
        let owner = WindowsProtectedApprovalCheckpoint::open(
            temporary.path(),
            "fixture-owner-a",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        let other = WindowsProtectedApprovalCheckpoint::open(
            temporary.path(),
            "fixture-owner-b",
            WindowsApprovalPersistence::LogonSession,
        )
        .unwrap();
        let key = ApprovalPolicyKey::from_secret_bytes([42; 32]);
        let different_key = ApprovalPolicyKey::from_secret_bytes([43; 32]);
        let name = owner.staging_name(&key, &[7; 16]).unwrap();
        assert!(owner.owns_staging(&key, &name));
        assert!(!owner.owns_staging(&different_key, &name));
        assert!(!other.owns_staging(&key, &name));
        assert!(!owner.owns_staging(&key, &name.replace("0707", "0708")));
        assert!(!owner.owns_staging(&key, "approval-stage-legacy.tmp"));
        assert!(!owner.owns_staging(&key, &format!("../{name}")));
        assert!(!owner.owns_staging(&key, &name.to_uppercase()));
    }

    #[test]
    #[ignore = "explicit owned current-user cleanup fixture; unrelated data remains intact"]
    fn native_cleanup_preserves_selected_unowned_and_aliased_data() {
        let temporary = tempfile::tempdir().unwrap();
        let owner = WindowsProtectedApprovalCheckpoint::open(
            temporary.path(),
            "fixture-cleanup",
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
        let initial = policies.seal().unwrap();
        owner
            .initialize(&key, &initial, Instant::now() + Duration::from_secs(5))
            .unwrap();
        let make_object = |offset| {
            let now = initial.checkpoint.observed_unix_secs + offset;
            RememberedApprovalPolicies::restore_with_clock(
                ApprovalPolicyKey::from_secret_bytes(*key.secret_bytes()),
                &initial.bytes,
                &initial.checkpoint,
                Arc::new(move || Ok(now)),
            )
            .unwrap()
            .seal()
            .unwrap()
        };
        let aliased = make_object(1);
        let collectible = make_object(2);
        let wrong_store = RememberedApprovalPolicies::new(ApprovalPolicyKey::from_secret_bytes(
            *key.secret_bytes(),
        ))
        .unwrap()
        .seal()
        .unwrap();
        for object in [&aliased, &collectible, &wrong_store] {
            owner
                .save_snapshot(&key, object, Instant::now() + Duration::from_secs(5))
                .unwrap();
        }
        owner
            .directory
            .hard_link(
                owner.snapshot_name(&aliased.checkpoint),
                &owner.directory,
                "personal-alias.txt",
            )
            .unwrap();
        owner
            .directory
            .write("personal-notes.txt", b"preserve notes")
            .unwrap();
        let partial = owner.staging_name(&key, &[7; 16]).unwrap();
        owner
            .directory
            .write(&partial, b"partial staged data")
            .unwrap();
        let forged = owner
            .staging_name(&ApprovalPolicyKey::from_secret_bytes([43; 32]), &[8; 16])
            .unwrap();
        owner
            .directory
            .write(&forged, b"unverified staging name")
            .unwrap();
        owner
            .directory
            .write("approval-stage-legacy.tmp", b"legacy unknown ownership")
            .unwrap();
        let unsafe_link = owner.staging_name(&key, &[9; 16]).unwrap();
        owner
            .directory
            .hard_link("personal-notes.txt", &owner.directory, &unsafe_link)
            .unwrap();
        let malformed = format!("approval-{}-{}.lsap", owner.owner_sha256, "a".repeat(64));
        owner
            .directory
            .write(&malformed, b"not an authenticated snapshot")
            .unwrap();

        assert!(owner
            .collect_unselected_snapshots(0, Instant::now() + Duration::from_secs(5))
            .is_err());
        assert!(owner
            .collect_unselected_snapshots(128, Instant::now())
            .is_err());
        // A damaged selected object prevents all collection, retaining recovery data.
        owner
            .directory
            .write(
                owner.snapshot_name(&initial.checkpoint),
                b"damaged selected fixture",
            )
            .unwrap();
        assert!(owner
            .collect_unselected_snapshots(128, Instant::now() + Duration::from_secs(5))
            .is_err());
        assert!(owner
            .directory
            .symlink_metadata(owner.snapshot_name(&collectible.checkpoint))
            .is_ok());
        assert!(owner.directory.symlink_metadata(&partial).is_ok());
        owner
            .directory
            .write(owner.snapshot_name(&initial.checkpoint), &initial.bytes)
            .unwrap();
        let checkpoint_before = owner.load().unwrap().unwrap().checkpoint;
        let first = owner
            .collect_unselected_snapshots(1, Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(first.removed_staging, 1);
        assert!(first.budget_exhausted);
        let next = owner
            .collect_unselected_snapshots(128, Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert_eq!(next.removed_snapshots, 1);
        assert!(next.skipped_unverified >= 3);
        assert_eq!(owner.load().unwrap().unwrap().checkpoint, checkpoint_before);
        assert_eq!(
            owner.directory.read("personal-notes.txt").unwrap(),
            b"preserve notes"
        );
        assert_eq!(
            owner.directory.read("personal-alias.txt").unwrap(),
            aliased.bytes
        );
        for name in [
            &forged,
            &unsafe_link,
            &malformed,
            "approval-stage-legacy.tmp",
        ] {
            assert!(owner.directory.symlink_metadata(name).is_ok());
        }
        assert!(owner
            .directory
            .symlink_metadata(owner.snapshot_name(&wrong_store.checkpoint))
            .is_ok());
        assert!(owner
            .directory
            .symlink_metadata(owner.snapshot_name(&collectible.checkpoint))
            .is_err());
        assert_eq!(
            owner
                .directory
                .read(owner.snapshot_name(&initial.checkpoint))
                .unwrap(),
            initial.bytes
        );
        assert_eq!(
            owner
                .collect_unselected_snapshots(128, Instant::now() + Duration::from_secs(5))
                .unwrap()
                .removed(),
            0
        );
        // Aggregate IO exhaustion can follow partial collection, but cannot
        // touch the selected object or keep reading an unbounded cache.
        let block = vec![0_u8; MAX_BYTES];
        let mut large_stages = Vec::new();
        for index in 20..26 {
            let name = owner.staging_name(&key, &[index; 16]).unwrap();
            owner.directory.write(&name, &block).unwrap();
            large_stages.push(name);
        }
        assert!(owner
            .collect_unselected_snapshots(128, Instant::now() + Duration::from_secs(5))
            .is_err());
        let remaining = large_stages
            .iter()
            .filter(|name| owner.directory.symlink_metadata(name).is_ok())
            .count();
        assert!((2..=3).contains(&remaining));
        assert_eq!(owner.load().unwrap().unwrap().checkpoint, checkpoint_before);
        owner
            .collect_unselected_snapshots(128, Instant::now() + Duration::from_secs(5))
            .unwrap();
        assert!(large_stages
            .iter()
            .all(|name| owner.directory.symlink_metadata(name).is_err()));
        owner.remove_test_credential().unwrap();
        assert!(owner.load().unwrap().is_none());
        drop(cleanup);
        drop(owner);
        temporary.close().unwrap();
    }
}
