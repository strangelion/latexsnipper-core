//! Physical directory comparisons, independent of untrusted grant path labels.
use super::*;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Component, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{
    GetDriveTypeW, GetFinalPathNameByHandleW, VOLUME_NAME_GUID,
};
use windows_sys::Win32::System::WindowsProgramming::DRIVE_FIXED;

fn physical_path(directory: &Dir) -> Result<PathBuf> {
    // SAFETY: live borrowed handle; the two calls query size then fill our buffer.
    let needed = unsafe {
        GetFinalPathNameByHandleW(directory.as_raw_handle(), null_mut(), 0, VOLUME_NAME_GUID)
    };
    if needed == 0 || needed > 32768 {
        return Err(Error::PrivateDirectoryRequired);
    }
    let mut wide = vec![0_u16; needed as usize];
    let count = unsafe {
        GetFinalPathNameByHandleW(
            directory.as_raw_handle(),
            wide.as_mut_ptr(),
            needed,
            VOLUME_NAME_GUID,
        )
    };
    if count == 0 || count >= needed {
        return Err(Error::PrivateDirectoryRequired);
    }
    wide.truncate(count as usize);
    let path = PathBuf::from(OsString::from_wide(&wide));
    let text = path.to_str().ok_or(Error::PrivateDirectoryRequired)?;
    // A per-computer credential head must not coordinate a shared/remote disk
    // with other machines' independent credentials and cleanup decisions.
    if !text.starts_with(r"\\?\Volume{") {
        return Err(Error::PrivateDirectoryRequired);
    }
    Ok(path)
}
pub(super) fn local_path(directory: &Dir) -> Result<PathBuf> {
    let path = physical_path(directory)?;
    let text = path.to_str().ok_or(Error::PrivateDirectoryRequired)?;
    let end = text.find("}\\").ok_or(Error::PrivateDirectoryRequired)? + 2;
    let root: Vec<u16> = text[..end].encode_utf16().chain(Some(0)).collect();
    if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
        return Err(Error::PrivateDirectoryRequired);
    }
    Ok(path)
}
fn identity(directory: &Dir) -> Result<(u64, u64)> {
    let metadata = directory.dir_metadata().map_err(|_| Error::Unavailable)?;
    if !metadata.is_dir() || metadata.ino() == 0 {
        return Err(Error::PrivateDirectoryRequired);
    }
    Ok((metadata.dev(), metadata.ino()))
}
fn ancestors(directory: &Dir, deadline: Instant) -> Result<Vec<(u64, u64)>> {
    let path = physical_path(directory)?;
    let mut identities = Vec::new();
    for ancestor in path.ancestors().filter(|part| {
        part.components()
            .any(|value| matches!(value, Component::RootDir))
    }) {
        check_deadline(deadline)?;
        if identities.len() >= 128 {
            return Err(Error::PrivateDirectoryRequired);
        }
        let opened = Dir::open_ambient_dir(ancestor, cap_std::ambient_authority())
            .map_err(|_| Error::PrivateDirectoryRequired)?;
        identities.push(identity(&opened)?);
    }
    if identities.first() != Some(&identity(directory)?) {
        return Err(Error::PrivateDirectoryRequired);
    }
    Ok(identities)
}
impl WindowsPreferenceCheckpoint {
    /// Call from the trusted host before guest execution and after grant/root
    /// changes. Inspect real directory capabilities, not plugin-supplied labels.
    /// This check does not itself grant, revoke or intercept any filesystem IO.
    pub fn check_no_directory_overlap(&self, granted: &[&Dir]) -> Result<()> {
        if granted.len() > 128 {
            return Err(Error::InvalidConfiguration);
        }
        let deadline = self.deadline();
        self.check_private_root()?;
        let private = identity(&self.inner.directory)?;
        let private_parents = ancestors(&self.inner.directory, deadline)?;
        for grant in granted {
            check_deadline(deadline)?;
            // Do not skip an unresolved SMB/remote alias merely because its
            // transport reports a different device ID for the same storage.
            physical_path(grant)?;
            let grant_identity = identity(grant)?;
            if grant_identity.0 == private.0
                && (private_parents.contains(&grant_identity)
                    || ancestors(grant, deadline)?.contains(&private))
            {
                return Err(Error::PrivateDirectoryRequired);
            }
        }
        check_deadline(deadline)
    }
}
