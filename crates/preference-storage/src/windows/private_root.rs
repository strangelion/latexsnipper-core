//! Restrictive current-user directory provisioning and handle-based rechecks.
//! No ACL repair, recursive creation, credential access or guest authorization.
use super::{Error, Result};
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::ptr::{addr_of, null_mut};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, LocalFree, ERROR_NO_TOKEN, HANDLE},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetSecurityInfo, SE_FILE_OBJECT,
        },
        *,
    },
    Storage::FileSystem::{CreateDirectoryW, FILE_ALL_ACCESS},
    System::Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
};

struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        // SAFETY: owns a successful LocalFree-family allocation.
        unsafe {
            LocalFree(self.0);
        }
    }
}
struct Token(HANDLE);
impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: owns one successful token-open handle, never a pseudohandle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct UserSid {
    buffer: Vec<usize>,
}
impl UserSid {
    fn current() -> Result<Self> {
        let mut handle = null_mut();
        // SAFETY: outputs are writable; effective impersonation token is honored.
        if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut handle) } == 0
            && (unsafe { GetLastError() } != ERROR_NO_TOKEN
                || unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0)
        {
            return Err(denied());
        }
        let token = Token(handle);
        let mut needed = 0;
        // SAFETY: size query writes no token data to the null zero-byte buffer.
        unsafe {
            GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut needed);
        }
        if needed < std::mem::size_of::<TOKEN_USER>() as u32 || needed > 4096 {
            return Err(denied());
        }
        let mut buffer = vec![0_usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        let capacity = buffer.len() * std::mem::size_of::<usize>();
        // SAFETY: aligned stable storage has at least the queried byte capacity.
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                capacity as u32,
                &mut needed,
            )
        } == 0
            || needed as usize > capacity
        {
            return Err(denied());
        }
        let result = Self { buffer };
        let offset = (result.as_ptr() as usize)
            .checked_sub(result.buffer.as_ptr() as usize)
            .ok_or_else(denied)?;
        if offset > needed as usize {
            return Err(denied());
        }
        bounded_sid(result.as_ptr(), needed as usize - offset)?;
        Ok(result)
    }
    fn as_ptr(&self) -> PSID {
        // SAFETY: successful bounded token query returned an aligned TOKEN_USER.
        unsafe { (*self.buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }
    fn text(&self) -> Result<String> {
        let mut raw = null_mut();
        // SAFETY: valid bounded SID; output is an owned terminated OS string.
        if unsafe { ConvertSidToStringSidW(self.as_ptr(), &mut raw) } == 0 || raw.is_null() {
            return Err(denied());
        }
        let _allocation = Allocation(raw.cast());
        let mut length = 0;
        // SAFETY: successful SID conversion returns a NUL-terminated string.
        unsafe {
            while length < 256 && *raw.add(length) != 0 {
                length += 1;
            }
        }
        if length == 256 {
            return Err(denied());
        }
        String::from_utf16(unsafe { std::slice::from_raw_parts(raw, length) }).map_err(|_| denied())
    }
}
fn descriptor(value: &str) -> Result<Allocation> {
    let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
    let mut raw = null_mut();
    let mut bytes = 0;
    // SAFETY: terminated input and writable output pointers remain live.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(wide.as_ptr(), 1, &mut raw, &mut bytes)
    } == 0
        || raw.is_null()
    {
        return Err(denied());
    }
    let allocation = Allocation(raw);
    if bytes == 0 || bytes > 64 * 1024 {
        return Err(denied());
    }
    Ok(allocation)
}
pub(super) fn create(parent: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.len() > 128
        || name.chars().any(char::is_control)
        || name.ends_with(['.', ' '])
        || name.contains(['/', '\\', ':'])
        || !matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
    {
        return Err(Error::InvalidConfiguration);
    }
    let parent = parent.canonicalize().map_err(|_| denied())?;
    if !parent.is_dir() {
        return Err(denied());
    }
    let parent_handle = cap_std::fs::Dir::open_ambient_dir(&parent, cap_std::ambient_authority())
        .map_err(|_| denied())?;
    super::isolation::local_path(&parent_handle)?;
    let root = parent.join(name);
    let sid = UserSid::current()?.text()?;
    let security = descriptor(&format!(
        "O:{sid}D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
    ))?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: descriptor and path remain live; existing children are refused.
    if unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) } == 0 {
        return Err(denied());
    }
    Ok(root)
}
fn bounded_sid(sid: PSID, available: usize) -> Result<()> {
    if sid.is_null() || available < 8 {
        return Err(denied());
    }
    // SAFETY: caller bounds this OS-returned pointer to an owned allocation.
    let header = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), 8) };
    if header[1] > 15 || 8 + usize::from(header[1]) * 4 > available {
        return Err(denied());
    }
    // SAFETY: the complete variable-length SID has now been bounded.
    if unsafe { IsValidSid(sid) } == 0 {
        return Err(denied());
    }
    Ok(())
}
pub(super) fn verify(handle: HANDLE, directory: bool) -> Result<()> {
    let user = UserSid::current()?;
    let mut owner = null_mut();
    let mut acl = null_mut();
    let mut raw = null_mut();
    // SAFETY: borrowed live handle; all outputs belong to one OS allocation.
    if unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut raw,
        )
    } != 0
        || raw.is_null()
    {
        return Err(denied());
    }
    let _allocation = Allocation(raw);
    // SAFETY: successful GetSecurityInfo returned a valid OS descriptor.
    let length = unsafe { GetSecurityDescriptorLength(raw) } as usize;
    if length == 0 || length > 64 * 1024 || owner.is_null() || acl.is_null() {
        return Err(denied());
    }
    if unsafe { EqualSid(owner, user.as_ptr()) } == 0 {
        return Err(denied());
    }
    let mut control = 0;
    let mut revision = 0;
    if unsafe { GetSecurityDescriptorControl(raw, &mut control, &mut revision) } == 0
        || (directory && control & SE_DACL_PROTECTED == 0)
    {
        return Err(denied());
    }
    let mut info = ACL_SIZE_INFORMATION::default();
    if unsafe {
        GetAclInformation(
            acl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
        || info.AceCount != 3
        || info.AclBytesInUse as usize > length
    {
        return Err(denied());
    }
    let mut found = [false; 3];
    let inherit_flags = if directory {
        OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
    } else {
        0
    };
    for index in 0..info.AceCount {
        let mut ace = null_mut();
        if unsafe { GetAce(acl, index, &mut ace) } == 0 || ace.is_null() {
            return Err(denied());
        }
        let offset = (ace as usize)
            .checked_sub(raw as usize)
            .ok_or_else(denied)?;
        if offset + std::mem::size_of::<ACCESS_ALLOWED_ACE>() > length {
            return Err(denied());
        }
        // SAFETY: the complete fixed ACE prefix is bounded before access.
        let value = unsafe { std::ptr::read_unaligned(ace.cast::<ACCESS_ALLOWED_ACE>()) };
        if value.Header.AceType != 0
            || value.Mask != FILE_ALL_ACCESS
            || offset + value.Header.AceSize as usize > length
            || value.Header.AceSize < 16
            || (u32::from(value.Header.AceFlags) & !INHERITED_ACE) != inherit_flags
        {
            return Err(denied());
        }
        let sid = unsafe { addr_of!((*ace.cast::<ACCESS_ALLOWED_ACE>()).SidStart) }
            .cast_mut()
            .cast();
        bounded_sid(sid, usize::from(value.Header.AceSize) - 8)?;
        if unsafe { EqualSid(sid, user.as_ptr()) } != 0 {
            found[0] = true;
        } else if unsafe { IsWellKnownSid(sid, WinLocalSystemSid) } != 0 {
            found[1] = true;
        } else if unsafe { IsWellKnownSid(sid, WinBuiltinAdministratorsSid) } != 0 {
            found[2] = true;
        } else {
            return Err(denied());
        }
    }
    if unsafe { IsWellKnownSid(user.as_ptr(), WinLocalSystemSid) } != 0 {
        found[1] = found[0];
    }
    if found.iter().all(|value| *value) {
        Ok(())
    } else {
        Err(denied())
    }
}
fn denied() -> Error {
    Error::PrivateDirectoryRequired
}

#[cfg(test)]
pub(super) fn fixture_dacl(path: &Path, broad: bool) {
    use windows_sys::Win32::Security::Authorization::SetNamedSecurityInfoW;
    let sid = UserSid::current().unwrap().text().unwrap();
    let extra = if broad { "(A;OICI;GR;;;WD)" } else { "" };
    let security = descriptor(&format!(
        "D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA){extra}"
    ))
    .unwrap();
    let mut acl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    // SAFETY: modifies only a freshly created fixture-owned root, never its parent.
    unsafe {
        assert_ne!(
            GetSecurityDescriptorDacl(security.0, &mut present, &mut acl, &mut defaulted),
            0
        );
        assert_ne!(present, 0);
        let mut path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        assert_eq!(
            SetNamedSecurityInfoW(
                path.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut()
            ),
            0
        );
    }
}
