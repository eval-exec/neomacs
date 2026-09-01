//! Runtime-owned host and user identity.
//!
//! Portable dumps preserve Lisp state from the build process, but identity
//! values describe the process that loaded the image.  This module is the one
//! lifecycle seam that captures those values, installs their Lisp variables,
//! and tracks GNU's owned-versus-overridden `system-name` distinction.

use super::eval::Context;
use super::value::{Value, eq_value};

/// Which kernel credential a Lisp identity primitive requests.
///
/// Keeping effective and real credentials exhaustive prevents the six Lisp
/// entry points from encoding that distinction as duplicated platform calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CredentialScope {
    Effective,
    Real,
}

/// A process user ID, kept distinct from a group ID at the host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UserId(i64);

impl From<UserId> for i64 {
    fn from(id: UserId) -> Self {
        id.0
    }
}

/// A process group ID, kept distinct from a user ID at the host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GroupId(i64);

impl From<GroupId> for i64 {
    fn from(id: GroupId) -> Self {
        id.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PasswdEntry {
    pub(crate) login: String,
    pub(crate) gecos: String,
}

std::cfg_select! {
    unix => {
        use std::ffi::{CStr, CString};

        const SECONDARY_LOGIN_ENV: &str = "USER";

        fn passwd_entry_from_raw(passwd: &libc::passwd) -> Option<PasswdEntry> {
            if passwd.pw_name.is_null() {
                return None;
            }
            let login = unsafe { CStr::from_ptr(passwd.pw_name) }
                .to_string_lossy()
                .into_owned();
            let gecos = if passwd.pw_gecos.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(passwd.pw_gecos) }
                    .to_string_lossy()
                    .into_owned()
            };
            Some(PasswdEntry { login, gecos })
        }

        fn lookup_passwd_by_uid(uid: i64) -> Option<PasswdEntry> {
            let uid = libc::uid_t::try_from(uid).ok()?;
            let mut buffer_len = 1024usize;
            loop {
                let mut buffer = vec![0u8; buffer_len];
                let mut passwd = unsafe { std::mem::zeroed::<libc::passwd>() };
                let mut result = std::ptr::null_mut();
                let status = unsafe {
                    libc::getpwuid_r(
                        uid,
                        &mut passwd,
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                        &mut result,
                    )
                };
                if status == 0 {
                    return (!result.is_null())
                        .then(|| passwd_entry_from_raw(&passwd))
                        .flatten();
                }
                if status != libc::ERANGE || buffer_len >= 1 << 20 {
                    return None;
                }
                buffer_len *= 2;
            }
        }

        fn lookup_passwd_by_login(login: &str) -> Option<PasswdEntry> {
            let login = CString::new(login).ok()?;
            let mut buffer_len = 1024usize;
            loop {
                let mut buffer = vec![0u8; buffer_len];
                let mut passwd = unsafe { std::mem::zeroed::<libc::passwd>() };
                let mut result = std::ptr::null_mut();
                let status = unsafe {
                    libc::getpwnam_r(
                        login.as_ptr(),
                        &mut passwd,
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                        &mut result,
                    )
                };
                if status == 0 {
                    return (!result.is_null())
                        .then(|| passwd_entry_from_raw(&passwd))
                        .flatten();
                }
                if status != libc::ERANGE || buffer_len >= 1 << 20 {
                    return None;
                }
                buffer_len *= 2;
            }
        }

        pub(crate) fn effective_uid() -> i64 {
            unsafe { libc::geteuid() as i64 }
        }

        fn real_uid() -> i64 {
            unsafe { libc::getuid() as i64 }
        }

        fn effective_gid() -> i64 {
            unsafe { libc::getegid() as i64 }
        }

        fn real_gid() -> i64 {
            unsafe { libc::getgid() as i64 }
        }

        fn capture_platform_identity() -> PlatformIdentity {
            let environment_login = login_name_from_env();
            PlatformIdentity {
                effective_passwd: lookup_passwd_by_uid(effective_uid()),
                environment_passwd: environment_login
                    .as_deref()
                    .and_then(lookup_passwd_by_login),
                environment_login,
                real_passwd: lookup_passwd_by_uid(real_uid()),
            }
        }
    }
    windows => {
        const SECONDARY_LOGIN_ENV: &str = "USERNAME";

        fn windows_token_account() -> Option<(String, i64)> {
            use windows_sys::Win32::Foundation::{
                CloseHandle, ERROR_INSUFFICIENT_BUFFER, GetLastError, HANDLE,
            };
            use windows_sys::Win32::Security::{
                GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation,
                LookupAccountSidW, SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
            };
            use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

            let mut token: HANDLE = std::ptr::null_mut();
            if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
                return None;
            }

            let result = (|| {
                let mut required = 0u32;
                let status = unsafe {
                    GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut required)
                };
                if status != 0
                    || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
                    || required < std::mem::size_of::<TOKEN_USER>() as u32
                {
                    return None;
                }

                // TOKEN_USER requires pointer alignment stronger than Vec<u8>
                // promises. Store the variable-sized result in machine words.
                let word_size = std::mem::size_of::<usize>();
                let word_count = (required as usize).div_ceil(word_size);
                let mut token_buffer = vec![0usize; word_count];
                if unsafe {
                    GetTokenInformation(
                        token,
                        TokenUser,
                        token_buffer.as_mut_ptr().cast(),
                        required,
                        &mut required,
                    )
                } == 0
                {
                    return None;
                }

                let token_user = unsafe { &*(token_buffer.as_ptr().cast::<TOKEN_USER>()) };
                let sid = token_user.User.Sid;
                let count = unsafe { GetSidSubAuthorityCount(sid).as_ref().copied() }?;
                if count == 0 {
                    return None;
                }
                let rid = unsafe { GetSidSubAuthority(sid, u32::from(count - 1)).as_ref() }
                    .copied()?;

                let mut name_len = 0u32;
                let mut domain_len = 0u32;
                let mut sid_name_use: SID_NAME_USE = 0;
                let status = unsafe {
                    LookupAccountSidW(
                        std::ptr::null(),
                        sid,
                        std::ptr::null_mut(),
                        &mut name_len,
                        std::ptr::null_mut(),
                        &mut domain_len,
                        &mut sid_name_use,
                    )
                };
                if status != 0
                    || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
                    || name_len == 0
                {
                    return None;
                }

                let mut name = vec![0u16; name_len as usize];
                let mut domain = vec![0u16; domain_len as usize];
                let domain_ptr = if domain.is_empty() {
                    std::ptr::null_mut()
                } else {
                    domain.as_mut_ptr()
                };
                if unsafe {
                    LookupAccountSidW(
                        std::ptr::null(),
                        sid,
                        name.as_mut_ptr(),
                        &mut name_len,
                        domain_ptr,
                        &mut domain_len,
                        &mut sid_name_use,
                    )
                } == 0
                {
                    return None;
                }

                let name = &name[..usize::min(name_len as usize, name.len())];
                let name = name.strip_suffix(&[0]).unwrap_or(name);
                let login = String::from_utf16_lossy(name);
                let uid = if login.eq_ignore_ascii_case("administrator") {
                    500
                } else {
                    i64::from(rid)
                };
                Some((login, uid))
            })();

            unsafe {
                CloseHandle(token);
            }
            result
        }

        fn windows_account() -> (PasswdEntry, i64) {
            let (login, uid) = windows_token_account().unwrap_or_else(|| {
                // This is GNU w32.c's fallback after token/SID lookup fails.
                let login = whoami::username().unwrap_or_else(|_| "unknown".to_string());
                let uid = if login.eq_ignore_ascii_case("administrator") {
                    0
                } else {
                    123
                };
                (login, uid)
            });
            // GNU w32.c leaves its synthetic passwd `pw_gecos` empty.
            (
                PasswdEntry {
                    login,
                    gecos: String::new(),
                },
                uid,
            )
        }

        fn lookup_passwd_by_uid(uid: i64) -> Option<PasswdEntry> {
            let (entry, effective_uid) = windows_account();
            (uid == effective_uid).then_some(entry)
        }

        fn lookup_passwd_by_login(login: &str) -> Option<PasswdEntry> {
            let (mut entry, _) = windows_account();
            let environment_alias = login_name_from_env();
            if !entry.login.eq_ignore_ascii_case(login)
                && environment_alias
                    .as_deref()
                    .is_none_or(|alias| !alias.eq_ignore_ascii_case(login))
            {
                return None;
            }
            entry.login = login.to_string();
            Some(entry)
        }

        pub(crate) fn effective_uid() -> i64 {
            windows_account().1
        }

        fn real_uid() -> i64 {
            effective_uid()
        }

        // GNU's Windows compatibility layer uses a synthetic passwd/group
        // entry whose gid is zero; effective and real IDs are identical.
        fn effective_gid() -> i64 {
            0
        }

        fn real_gid() -> i64 {
            effective_gid()
        }

        fn capture_platform_identity() -> PlatformIdentity {
            let environment_login = login_name_from_env();
            let (native_passwd, _) = windows_account();
            let environment_passwd = environment_login.as_ref().map(|login| {
                let mut entry = native_passwd.clone();
                entry.login = login.clone();
                entry
            });
            PlatformIdentity {
                effective_passwd: Some(native_passwd.clone()),
                environment_login,
                environment_passwd,
                real_passwd: Some(native_passwd),
            }
        }
    }
    _ => {
        const SECONDARY_LOGIN_ENV: &str = "USER";

        fn lookup_passwd_by_uid(_uid: i64) -> Option<PasswdEntry> {
            None
        }

        fn lookup_passwd_by_login(_login: &str) -> Option<PasswdEntry> {
            None
        }

        pub(crate) fn effective_uid() -> i64 {
            0
        }

        fn real_uid() -> i64 {
            effective_uid()
        }

        fn effective_gid() -> i64 {
            0
        }

        fn real_gid() -> i64 {
            effective_gid()
        }

        fn capture_platform_identity() -> PlatformIdentity {
            let environment_login = login_name_from_env();
            PlatformIdentity {
                effective_passwd: lookup_passwd_by_uid(effective_uid()),
                environment_passwd: environment_login
                    .as_deref()
                    .and_then(lookup_passwd_by_login),
                environment_login,
                real_passwd: lookup_passwd_by_uid(real_uid()),
            }
        }
    }
}

/// Read the process user credential directly from the platform identity seam.
pub(crate) fn process_user_id(scope: CredentialScope) -> UserId {
    UserId(match scope {
        CredentialScope::Effective => effective_uid(),
        CredentialScope::Real => real_uid(),
    })
}

/// Read the process group credential directly from the platform identity seam.
pub(crate) fn process_group_id(scope: CredentialScope) -> GroupId {
    GroupId(match scope {
        CredentialScope::Effective => effective_gid(),
        CredentialScope::Real => real_gid(),
    })
}

fn login_name_from_env() -> Option<String> {
    std::env::var_os("LOGNAME")
        .or_else(|| std::env::var_os(SECONDARY_LOGIN_ENV))
        .map(|name| name.to_string_lossy().into_owned())
}

pub(crate) fn lookup_login_by_uid(uid: i64) -> Option<String> {
    lookup_passwd_by_uid(uid).map(|entry| entry.login)
}

pub(crate) fn canonical_full_name(entry: &PasswdEntry) -> String {
    entry.gecos.split(',').next().unwrap_or("").to_string()
}

pub(crate) fn lookup_full_name_by_uid(uid: i64) -> Option<String> {
    lookup_passwd_by_uid(uid).map(|entry| canonical_full_name(&entry))
}

pub(crate) fn lookup_full_name_by_login(login: &str) -> Option<String> {
    lookup_passwd_by_login(login).map(|entry| canonical_full_name(&entry))
}

fn normalized_system_name() -> String {
    hostname::get()
        .map(|os| os.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "localhost".to_string())
        .chars()
        .map(|character| match character {
            ' ' | '\t' => '-',
            other => other,
        })
        .collect()
}

pub(crate) fn operating_system_release_value() -> Value {
    sysinfo::System::kernel_version()
        .map(Value::string)
        .unwrap_or(Value::NIL)
}

struct PlatformIdentity {
    effective_passwd: Option<PasswdEntry>,
    environment_login: Option<String>,
    environment_passwd: Option<PasswdEntry>,
    real_passwd: Option<PasswdEntry>,
}

impl PlatformIdentity {
    fn capture() -> Self {
        capture_platform_identity()
    }

    fn login_name(&self) -> String {
        self.environment_login
            .clone()
            .or_else(|| {
                self.effective_passwd
                    .as_ref()
                    .map(|entry| entry.login.clone())
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn real_login_name(&self) -> String {
        self.real_passwd
            .as_ref()
            .map(|entry| entry.login.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn full_name(&self) -> String {
        if let Some(name) = std::env::var_os("NAME") {
            return name.to_string_lossy().into_owned();
        }

        let login = self.login_name();
        let entry = if login == self.real_login_name() {
            self.environment_passwd
                .as_ref()
                .or_else(|| {
                    self.effective_passwd
                        .as_ref()
                        .filter(|entry| entry.login == login)
                })
                .or_else(|| {
                    self.real_passwd
                        .as_ref()
                        .filter(|entry| entry.login == login)
                })
        } else {
            self.effective_passwd.as_ref()
        };
        entry
            .map(canonical_full_name)
            .unwrap_or_else(|| "unknown".to_string())
    }
}

pub(crate) struct RuntimeIdentity {
    operating_system_release: Value,
    system_name: String,
    user_full_name: Value,
    user_login_name: Value,
    user_real_login_name: Value,
}

impl RuntimeIdentity {
    pub(crate) fn capture() -> Self {
        let platform = PlatformIdentity::capture();
        Self {
            operating_system_release: operating_system_release_value(),
            system_name: normalized_system_name(),
            user_full_name: Value::string(platform.full_name()),
            user_login_name: Value::string(platform.login_name()),
            user_real_login_name: Value::string(platform.real_login_name()),
        }
    }

    pub(crate) fn install(self, eval: &mut Context) {
        install_system_name(eval, self.system_name);
        for (name, value) in [
            ("operating-system-release", self.operating_system_release),
            ("user-full-name", self.user_full_name),
            ("user-login-name", self.user_login_name),
            ("user-real-login-name", self.user_real_login_name),
        ] {
            eval.set_variable(name, value);
            eval.obarray_mut().make_special(name);
        }
    }
}

fn install_system_name(eval: &mut Context, name: String) {
    // GNU sysdep.c:init_system_name retains Vsystem_name when its bytes still
    // equal the current hostname.  This preserves object identity across
    // unchanged refreshes; only the refresh permission check below uses `eq`.
    let value = eval
        .obarray()
        .symbol_value("system-name")
        .copied()
        .filter(|value| value.as_utf8_str() == Some(name.as_str()))
        .unwrap_or_else(|| Value::string(name));
    eval.set_variable("system-name", value);
    eval.obarray_mut().make_special("system-name");
    eval.cached_system_name = value;
}

fn refresh_system_name_from(eval: &mut Context, name: String) {
    let visible = eval
        .obarray()
        .symbol_value("system-name")
        .copied()
        .unwrap_or(Value::NIL);
    if eq_value(&visible, &eval.cached_system_name) {
        install_system_name(eval, name);
    }
}

pub(crate) fn refresh_system_name(eval: &mut Context) {
    refresh_system_name_from(eval, normalized_system_name());
}

pub(crate) fn install(eval: &mut Context) {
    RuntimeIdentity::capture().install(eval);
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
