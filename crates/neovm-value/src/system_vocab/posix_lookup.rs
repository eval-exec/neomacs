//! Thread-safe POSIX `*_r` lookup helpers (`getpwuid_r`, `getgrgid_r`, and the
//! shared `ERANGE` retry loop).
//!
//! Follow-up callers outside this wave's audited scope (`file_name_expand.rs`,
//! `file_names.rs`) can route `getpwnam_r`/`getpwuid_r` through
//! [`retry_erange_lookup`] in a subsequent cleanup.

#[cfg(all(unix, not(miri)))]
use std::ffi::CStr;
#[cfg(unix)]
use std::mem::MaybeUninit;

/// Run a POSIX `*_r` entry lookup that populates an output struct `T` using a
/// caller-supplied scratch buffer and returns `libc::ERANGE` when that buffer
/// is too small.
///
/// The scratch buffer starts at 1 KiB and doubles up to 1 MiB. When `call`
/// returns `0` with a non-null `result` pointer, `extract` runs while the
/// scratch buffer is still live so any C string pointers inside `entry` remain
/// valid.
///
/// # Safety
/// - `T` must be a C struct valid to initialize via `call` starting from
///   zeroed bytes (e.g. `libc::passwd`, `libc::group`, `libc::servent`).
/// - Whenever `call` returns `0` and writes a non-null `result` pointer, the
///   struct at `entry` must be validly initialized for reading by `extract`.
#[cfg(unix)]
pub unsafe fn retry_erange_lookup<T, R>(
    mut call: impl FnMut(*mut T, *mut libc::c_char, libc::size_t, *mut *mut T) -> libc::c_int,
    extract: impl FnOnce(&T) -> Option<R>,
) -> Option<R> {
    let mut buf_len = 1024usize;
    loop {
        let mut buf = vec![0u8; buf_len];
        let mut entry = MaybeUninit::<T>::zeroed();
        let mut result: *mut T = std::ptr::null_mut();
        let rc = call(
            entry.as_mut_ptr(),
            buf.as_mut_ptr().cast(),
            buf_len,
            &mut result,
        );
        if rc == 0 {
            if result.is_null() {
                return None;
            }
            // SAFETY: guaranteed by the caller's `# Safety` contract when
            // `rc == 0` and `result` is non-null; `buf` remains live across
            // `extract`.
            let initialized = unsafe { entry.assume_init_ref() };
            return extract(initialized);
        }
        if rc == libc::ERANGE && buf_len < (1 << 20) {
            buf_len *= 2;
            continue;
        }
        return None;
    }
}

/// Thread-safe `getpwuid_r` lookup returning `pw_name` as a UTF-8 lossy
/// `String`.
///
/// Under Miri (`#[cfg(miri)]`), returns `None`: Miri does not implement POSIX
/// `getpwuid_r`/`getgrgid_r` shims (same convention as
/// `runtime_identity/mod.rs`), and callers treat `None` as an absent lookup.
#[cfg(all(unix, not(miri)))]
pub fn lookup_user_name(uid: u32) -> Option<String> {
    // SAFETY: `libc::passwd` is a C POD struct; `getpwuid_r` initializes
    // `entry` and writes string data into `buf` when it returns `0` with a
    // non-null `result`.
    unsafe {
        retry_erange_lookup::<libc::passwd, String>(
            |entry, buf, len, result| {
                // SAFETY: `entry`, `buf` (with `len` bytes), and `result` are
                // valid out-parameters supplied by `retry_erange_lookup`.
                unsafe { libc::getpwuid_r(uid as libc::uid_t, entry, buf, len, result) }
            },
            |pwd| {
                if pwd.pw_name.is_null() {
                    return None;
                }
                // SAFETY: `pwd.pw_name` is a NUL-terminated C string inside
                // the live scratch buffer.
                Some(
                    unsafe { CStr::from_ptr(pwd.pw_name) }
                        .to_string_lossy()
                        .into_owned(),
                )
            },
        )
    }
}

#[cfg(any(not(unix), miri))]
pub fn lookup_user_name(_uid: u32) -> Option<String> {
    None
}

/// Thread-safe `getgrgid_r` lookup returning `gr_name` as a UTF-8 lossy
/// `String`.
///
/// Under Miri (`#[cfg(miri)]`), returns `None`: Miri does not implement POSIX
/// `getpwuid_r`/`getgrgid_r` shims (same convention as
/// `runtime_identity/mod.rs`), and callers treat `None` as an absent lookup.
#[cfg(all(unix, not(miri)))]
pub fn lookup_group_name(gid: u32) -> Option<String> {
    // SAFETY: `libc::group` is a C POD struct; `getgrgid_r` initializes
    // `entry` and writes string data into `buf` when it returns `0` with a
    // non-null `result`.
    unsafe {
        retry_erange_lookup::<libc::group, String>(
            |entry, buf, len, result| {
                // SAFETY: `entry`, `buf` (with `len` bytes), and `result` are
                // valid out-parameters supplied by `retry_erange_lookup`.
                unsafe { libc::getgrgid_r(gid as libc::gid_t, entry, buf, len, result) }
            },
            |grp| {
                if grp.gr_name.is_null() {
                    return None;
                }
                // SAFETY: `grp.gr_name` is a NUL-terminated C string inside
                // the live scratch buffer.
                Some(
                    unsafe { CStr::from_ptr(grp.gr_name) }
                        .to_string_lossy()
                        .into_owned(),
                )
            },
        )
    }
}

#[cfg(any(not(unix), miri))]
pub fn lookup_group_name(_gid: u32) -> Option<String> {
    None
}
