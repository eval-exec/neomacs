use super::{Ownership, Principal};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub(super) fn query(_path: &Path, metadata: &fs::Metadata) -> Ownership {
    let uid = metadata.uid();
    let gid = metadata.gid();
    Ownership {
        user: Principal {
            id: i64::from(uid),
            name: uid_to_name(uid),
        },
        group: Principal {
            id: i64::from(gid),
            name: gid_to_name(gid),
        },
    }
}

use std::ffi::CStr;
#[cfg(unix)]
fn uid_to_name(uid: u32) -> Option<String> {
    unsafe {
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let mut buf_len = 1024usize;

        loop {
            let mut buf = vec![0u8; buf_len];
            let rc = libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr().cast(), buf_len, &mut result);

            if rc == 0 {
                if result.is_null() || pwd.pw_name.is_null() {
                    return None;
                }
                return Some(CStr::from_ptr(pwd.pw_name).to_string_lossy().into_owned());
            }

            if rc == libc::ERANGE && buf_len < (1 << 20) {
                buf_len *= 2;
                continue;
            }

            return None;
        }
    }
}

#[cfg(unix)]
fn gid_to_name(gid: u32) -> Option<String> {
    unsafe {
        let mut grp: libc::group = std::mem::zeroed();
        let mut result: *mut libc::group = std::ptr::null_mut();
        let mut buf_len = 1024usize;

        loop {
            let mut buf = vec![0u8; buf_len];
            let rc = libc::getgrgid_r(gid, &mut grp, buf.as_mut_ptr().cast(), buf_len, &mut result);

            if rc == 0 {
                if result.is_null() || grp.gr_name.is_null() {
                    return None;
                }
                return Some(CStr::from_ptr(grp.gr_name).to_string_lossy().into_owned());
            }

            if rc == libc::ERANGE && buf_len < (1 << 20) {
                buf_len *= 2;
                continue;
            }

            return None;
        }
    }
}
