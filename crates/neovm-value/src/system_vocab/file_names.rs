//! Pure Lisp file-name <-> host path conversions and `substitute-in-file-name`
//! (stage-4 W1/E3: moved from `neovm-core`'s system `fileio` module).
//!
//! GNU keeps file names as Lisp strings until the system-call boundary; these
//! helpers only spell paths into Lisp syntax (and substitute environment
//! variables), so no evaluator, display or host authority is involved.  Code
//! that runs Lisp (expansion against the default directory, file-name handler
//! dispatch, locking) stays in the system layer.

#[cfg(unix)]
use std::ffi::{CStr, CString};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::heap_types::LispString;

#[cfg(windows)]
fn host_path_to_lisp_file_name_string_inner(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(not(windows))]
fn host_path_to_lisp_file_name_string_inner(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Convert a host path spelling into GNU's Lisp-visible file-name syntax.
///
/// GNU's Windows port presents file names to Lisp with `/` directory
/// separators, and converts to DOS separators only at the system-call layer
/// (`dostounix_filename`, `unixtodos_filename`, `map_w32_filename`).  Keep the
/// same boundary in Rust code that seeds Lisp variables from host paths.
pub fn host_path_to_lisp_file_name_string(path: &Path) -> String {
    host_path_to_lisp_file_name_string_inner(path)
}

/// Convert an OS path back to a Lisp file-name string at the filesystem boundary.
///
/// On Unix keep the raw bytes intact, matching GNU's byte-preserving file-name
/// handling for resolved paths.
pub fn path_to_lisp_file_name(path: &Path) -> LispString {
    #[cfg(unix)]
    {
        LispString::from_unibyte(path.as_os_str().as_bytes().to_vec())
    }

    #[cfg(not(unix))]
    {
        crate::strings_core::plain_str_to_lisp_string(
            &host_path_to_lisp_file_name_string(path),
            true,
        )
    }
}

fn user_homedir_absolute_p(name: &[u8]) -> bool {
    let end = name
        .iter()
        .position(|&b| b == b'\0' || b == b'/')
        .unwrap_or(name.len());
    if end == 0 {
        return false;
    }

    #[cfg(unix)]
    {
        let Ok(user) = CString::new(&name[..end]) else {
            return false;
        };
        let passwd = unsafe { libc::getpwnam(user.as_ptr()) };
        if passwd.is_null() {
            return false;
        }
        let pw_dir = unsafe { (*passwd).pw_dir };
        if pw_dir.is_null() {
            return false;
        }
        unsafe { CStr::from_ptr(pw_dir) }
            .to_bytes()
            .starts_with(b"/")
    }

    #[cfg(not(unix))]
    {
        let _ = name;
        false
    }
}

pub fn file_name_absolute_bytes_p(filename: &[u8]) -> bool {
    #[cfg(windows)]
    {
        if matches!(
            filename,
            [drive, b':', sep, ..] if drive.is_ascii_alphabetic() && (*sep == b'/' || *sep == b'\\')
        ) {
            return true;
        }
        if matches!(filename, [b'/', b'/', third, ..] if *third != b'/')
            || matches!(filename, [b'\\', b'\\', third, ..] if *third != b'\\')
        {
            return true;
        }
    }
    match filename {
        [b'/', ..] => true,
        [b'~'] => true,
        [b'~', b'/', ..] => true,
        [b'~', rest @ ..] => user_homedir_absolute_p(rest),
        _ => false,
    }
}

fn env_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Byte index of the first embedded absolute file name (`/` followed by an
/// absolute-name start), as GNU's `substitute-in-file-name` trims it.
pub fn embedded_absfilename_start(bytes: &[u8]) -> Option<usize> {
    let mut i = 1usize;
    while i < bytes.len() {
        if bytes[i - 1] == b'/' && file_name_absolute_bytes_p(&bytes[i..]) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn trim_embedded_absfilename(path: String) -> String {
    let mut current = path;
    loop {
        if let Some(idx) = embedded_absfilename_start(current.as_bytes()) {
            current = current[idx..].to_string();
        } else {
            return current;
        }
    }
}

/// Drop every prefix before an embedded absolute file name (byte form).
pub fn trim_embedded_absfilename_bytes(path: Vec<u8>) -> Vec<u8> {
    let mut current = path;
    loop {
        if let Some(idx) = embedded_absfilename_start(&current) {
            current = current[idx..].to_vec();
        } else {
            return current;
        }
    }
}

/// Substitute environment variables in FILENAME.
/// Mirrors Emacs `substitute-in-file-name` behavior for local path forms.
pub fn substitute_in_file_name(filename: &str) -> String {
    let bytes = filename.as_bytes();
    let mut out = String::with_capacity(filename.len());
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'$' {
            // Safe because i is always at a valid UTF-8 boundary.
            let ch = filename[i..]
                .chars()
                .next()
                .expect("index points at valid char boundary");
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }

        if i + 1 >= bytes.len() {
            out.push('$');
            i += 1;
            continue;
        }

        match bytes[i + 1] {
            b'$' => {
                out.push('$');
                i += 2;
            }
            b'{' => {
                if let Some(rel_end) = bytes[i + 2..].iter().position(|&b| b == b'}') {
                    let end = i + 2 + rel_end;
                    let var = &filename[i + 2..end];
                    if let Ok(value) = std::env::var(var) {
                        out.push_str(&value);
                    } else {
                        out.push_str(&filename[i..=end]);
                    }
                    i = end + 1;
                } else {
                    // Unclosed ${... keeps '$' literal; rest passes through.
                    out.push('$');
                    i += 1;
                }
            }
            next if env_name_char(next) => {
                let mut end = i + 1;
                while end < bytes.len() && env_name_char(bytes[end]) {
                    end += 1;
                }
                let var = &filename[i + 1..end];
                if let Ok(value) = std::env::var(var) {
                    out.push_str(&value);
                } else {
                    out.push_str(&filename[i..end]);
                }
                i = end;
            }
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }

    trim_embedded_absfilename(out)
}

pub fn substitute_in_file_name_lisp(filename: &LispString) -> LispString {
    #[cfg(unix)]
    {
        let bytes = filename.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0usize;

        while i < bytes.len() {
            if bytes[i] != b'$' {
                out.push(bytes[i]);
                i += 1;
                continue;
            }

            if i + 1 >= bytes.len() {
                out.push(b'$');
                i += 1;
                continue;
            }

            match bytes[i + 1] {
                b'$' => {
                    out.push(b'$');
                    i += 2;
                }
                b'{' => {
                    if let Some(rel_end) = bytes[i + 2..].iter().position(|&b| b == b'}') {
                        let end = i + 2 + rel_end;
                        let var = String::from_utf8_lossy(&bytes[i + 2..end]);
                        if let Some(value) = std::env::var_os(var.as_ref()) {
                            out.extend_from_slice(value.as_bytes());
                        } else {
                            out.extend_from_slice(&bytes[i..=end]);
                        }
                        i = end + 1;
                    } else {
                        out.push(b'$');
                        i += 1;
                    }
                }
                next if env_name_char(next) => {
                    let mut end = i + 1;
                    while end < bytes.len() && env_name_char(bytes[end]) {
                        end += 1;
                    }
                    let var = String::from_utf8_lossy(&bytes[i + 1..end]);
                    if let Some(value) = std::env::var_os(var.as_ref()) {
                        out.extend_from_slice(value.as_bytes());
                    } else {
                        out.extend_from_slice(&bytes[i..end]);
                    }
                    i = end;
                }
                _ => {
                    out.push(b'$');
                    i += 1;
                }
            }
        }

        LispString::from_unibyte(trim_embedded_absfilename_bytes(out))
    }

    #[cfg(not(unix))]
    {
        let substituted =
            substitute_in_file_name(&crate::emacs_char::to_utf8_lossy(filename.as_bytes()));
        crate::strings_core::plain_str_to_lisp_string(&substituted, !substituted.is_ascii())
    }
}
