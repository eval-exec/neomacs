//! Safe-facing Windows security identity queries.
//!
//! Windows exposes file and process ownership as SIDs.  GNU Emacs maps the
//! final SID sub-authority (the RID) into its numeric uid/gid fields.  Keep the
//! necessary FFI and variable-sized, aligned buffers in this module so callers
//! cannot accidentally mix raw SID pointers with ordinary Rust ownership.

use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GROUP_SECURITY_INFORMATION, GetFileSecurityW, GetSecurityDescriptorGroup,
    GetSecurityDescriptorOwner, GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
    IsValidSid, LookupAccountSidW, OWNER_SECURITY_INFORMATION, PSID, SID_NAME_USE,
    TOKEN_PRIMARY_GROUP, TOKEN_QUERY, TOKEN_USER, TokenPrimaryGroup, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Principal {
    pub(crate) id: i64,
    pub(crate) name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Ownership {
    pub(crate) user: Principal,
    pub(crate) group: Principal,
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: `OwnedHandle` is constructed only from a successful
        // OpenProcessToken call and owns that handle exactly once.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn aligned_words(byte_len: u32) -> Vec<usize> {
    vec![0; (byte_len as usize).div_ceil(std::mem::size_of::<usize>())]
}

fn token_information(
    token: HANDLE,
    class: windows_sys::Win32::Security::TOKEN_INFORMATION_CLASS,
) -> Option<Vec<usize>> {
    let mut required = 0u32;
    // SAFETY: The null/zero call is the documented size query. `token` stays
    // alive in the caller's `OwnedHandle` for both calls.
    unsafe {
        GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut required);
    }
    if required == 0 {
        return None;
    }

    let mut buffer = aligned_words(required);
    // SAFETY: The machine-word buffer is suitably aligned and has at least
    // `required` writable bytes. The API initializes those bytes on success.
    let ok = unsafe {
        GetTokenInformation(
            token,
            class,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    };
    (ok != 0).then_some(buffer)
}

fn principal_name(sid: PSID) -> Option<String> {
    let mut name_len = 0u32;
    let mut domain_len = 0u32;
    let mut use_kind: SID_NAME_USE = 0;
    // SAFETY: This is the documented size query for a SID validated by the
    // caller. No output buffers are supplied in this first call.
    unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid,
            std::ptr::null_mut(),
            &mut name_len,
            std::ptr::null_mut(),
            &mut domain_len,
            &mut use_kind,
        );
    }
    if name_len == 0 {
        return None;
    }

    let mut name = vec![0u16; name_len as usize];
    let mut domain = vec![0u16; domain_len as usize];
    let domain_ptr = if domain.is_empty() {
        std::ptr::null_mut()
    } else {
        domain.as_mut_ptr()
    };
    // SAFETY: Both UTF-16 buffers have the capacities reported by the size
    // query, and `sid` remains owned by the surrounding token/descriptor.
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid,
            name.as_mut_ptr(),
            &mut name_len,
            domain_ptr,
            &mut domain_len,
            &mut use_kind,
        )
    };
    if ok == 0 {
        return None;
    }

    let name = &name[..usize::min(name_len as usize, name.len())];
    Some(String::from_utf16_lossy(
        name.strip_suffix(&[0]).unwrap_or(name),
    ))
}

fn principal(sid: PSID) -> Option<Principal> {
    // SAFETY: IsValidSid accepts an opaque SID pointer and performs the shape
    // validation needed before the sub-authority accessors below.
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return None;
    }
    // SAFETY: `sid` was validated above; these accessors return pointers into
    // the still-live SID allocation.
    let count = unsafe { GetSidSubAuthorityCount(sid).as_ref().copied() }?;
    if count == 0 {
        return None;
    }
    // SAFETY: `count - 1` is in range for this validated SID.
    let rid = unsafe { GetSidSubAuthority(sid, u32::from(count - 1)).as_ref() }.copied()?;
    Some(Principal {
        id: i64::from(rid),
        name: principal_name(sid),
    })
}

/// Return the current process token's user and primary-group principals.
pub(crate) fn current_process_ownership() -> Option<Ownership> {
    let mut raw_token: HANDLE = std::ptr::null_mut();
    // SAFETY: `raw_token` is a valid out-pointer. A successful handle is
    // immediately transferred to the RAII owner below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token) } == 0 {
        return None;
    }
    let token = OwnedHandle(raw_token);

    let user_buffer = token_information(token.0, TokenUser)?;
    if user_buffer.len() * std::mem::size_of::<usize>() < std::mem::size_of::<TOKEN_USER>() {
        return None;
    }
    // SAFETY: token_information returns a word-aligned buffer initialized as
    // TOKEN_USER for this information class.
    let token_user = unsafe { &*(user_buffer.as_ptr().cast::<TOKEN_USER>()) };
    let user = principal(token_user.User.Sid)?;

    let group_buffer = token_information(token.0, TokenPrimaryGroup)?;
    if group_buffer.len() * std::mem::size_of::<usize>()
        < std::mem::size_of::<TOKEN_PRIMARY_GROUP>()
    {
        return None;
    }
    // SAFETY: As above, with the TokenPrimaryGroup information class.
    let token_group = unsafe { &*(group_buffer.as_ptr().cast::<TOKEN_PRIMARY_GROUP>()) };
    let group = principal(token_group.PrimaryGroup)?;

    Some(Ownership { user, group })
}

/// Return the owner and primary-group principals from a file security
/// descriptor. Failure is intentionally optional: GNU falls back to the
/// current process identity when accurate security information is unavailable.
pub(crate) fn file_ownership(path: &Path) -> Option<Ownership> {
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let requested = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION;
    let mut required = 0u32;
    // SAFETY: This is the documented size query; no descriptor buffer is
    // supplied, and `wide_path` is NUL-terminated.
    unsafe {
        GetFileSecurityW(
            wide_path.as_ptr(),
            requested,
            std::ptr::null_mut(),
            0,
            &mut required,
        );
    }
    if required == 0 {
        return None;
    }

    let mut descriptor = aligned_words(required);
    // SAFETY: The aligned buffer contains at least `required` writable bytes
    // and lives until both returned SID pointers have been converted.
    if unsafe {
        GetFileSecurityW(
            wide_path.as_ptr(),
            requested,
            descriptor.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return None;
    }

    let descriptor_ptr = descriptor.as_mut_ptr().cast();
    let mut owner_sid: PSID = std::ptr::null_mut();
    let mut group_sid: PSID = std::ptr::null_mut();
    let mut defaulted = 0;
    // SAFETY: `descriptor_ptr` points to a successfully initialized security
    // descriptor and all out-pointers are valid for the duration of the calls.
    if unsafe { GetSecurityDescriptorOwner(descriptor_ptr, &mut owner_sid, &mut defaulted) } == 0
        || unsafe { GetSecurityDescriptorGroup(descriptor_ptr, &mut group_sid, &mut defaulted) }
            == 0
    {
        return None;
    }

    Some(Ownership {
        user: principal(owner_sid)?,
        group: principal(group_sid)?,
    })
}
