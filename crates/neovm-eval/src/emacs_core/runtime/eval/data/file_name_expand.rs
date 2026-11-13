//! Pure file-name expansion helpers (stage-4 K2 oracleb batch 4).
//!
//! The string-native `expand-file-name` chain and its HOME/`~` resolution
//! were moved here verbatim from `system/fileio` so the evaluator files that
//! need pure path expansion (treesit's inherited-directory predicate) do not
//! name the core system module.  The byte-native twins stay in the core
//! fileio module (the handler-aware Lisp entry drives them); the names the
//! remaining core callers use are re-exported there.

use neovm_value::system_vocab::file_names::file_name_absolute_bytes_p;
#[cfg(unix)]
use std::ffi::{CStr, CString};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

/// Expand FILE relative to DEFAULT_DIR (or the current working directory).
/// Handles `~` expansion and absolute path detection.
pub fn expand_file_name(name: &str, default_dir: Option<&str>) -> String {
    expand_file_name_with_home(name, default_dir, None)
}

pub fn expand_file_name_with_home(
    name: &str,
    default_dir: Option<&str>,
    home_override: Option<&str>,
) -> String {
    expand_file_name_with_home_inner(name, default_dir, home_override, true)
}

fn expand_file_name_with_home_inner(
    name: &str,
    default_dir: Option<&str>,
    home_override: Option<&str>,
    expand_default_dir: bool,
) -> String {
    #[cfg(windows)]
    let name_normalized = name.replace('\\', "/");
    #[cfg(windows)]
    let name = name_normalized.as_str();
    #[cfg(windows)]
    let default_dir_normalized = default_dir.map(|dir| dir.replace('\\', "/"));
    #[cfg(windows)]
    let default_dir = default_dir_normalized.as_deref();

    // Handle ~ expansion
    let expanded = if name.starts_with("~/") {
        if let Some(home) = home_override {
            format!("{}{}", home, &name[1..])
        } else if let Some(home) = home_env_string() {
            format!("{}{}", home, &name[1..])
        } else {
            name.to_string()
        }
    } else if name == "~" {
        if let Some(home) = home_override {
            home.to_string()
        } else if let Some(home) = home_env_string() {
            home
        } else {
            name.to_string()
        }
    } else if let Some((home, rest)) = expand_user_home_prefix(name) {
        format!("{home}{rest}")
    } else {
        name.to_string()
    };

    let preserve_trailing_slash = expanded.ends_with('/');

    // If already absolute, just clean it up.
    if file_name_absolute_p(&expanded) {
        let mut cleaned = clean_path(&expanded);
        if preserve_trailing_slash && !cleaned.ends_with('/') {
            cleaned.push('/');
        }
        return cleaned;
    }

    // Resolve relative to default_dir or cwd
    let base = if let Some(dir) = default_dir {
        if expand_default_dir {
            PathBuf::from(expand_file_name_with_home(dir, None, home_override))
        } else {
            PathBuf::from(dir)
        }
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
    };

    if expanded.is_empty() {
        let mut cleaned = clean_path(base.to_string_lossy().as_ref());
        trim_trailing_slashes_except_roots(&mut cleaned);
        return cleaned;
    }

    let joined = join_file_name(base.to_string_lossy().as_ref(), &expanded);
    let mut cleaned = clean_path(&joined);
    if preserve_trailing_slash && !cleaned.ends_with('/') {
        cleaned.push('/');
    }
    cleaned
}

/// Raw bytes of the home directory used for `~` expansion.
///
/// Unix: the `HOME` environment variable (byte-exact), or None if unset.
///
/// Windows: GNU `w32.c init_environment` guarantees HOME is set -- it keeps an
/// existing HOME, else (when `C:/.emacs` is absent) defaults it to the roaming
/// AppData folder via `SHGetFolderPath(CSIDL_APPDATA)` (the path the `APPDATA`
/// env var holds), else `"C:/"`. (GNU also consults an Emacs-specific registry
/// key, which we do not replicate.) Mirror that so `~` expands to a real
/// directory: a Windows session that sets APPDATA/USERPROFILE but not HOME would
/// otherwise leave `~` literal and `directory-files "~"` would fail at startup.
pub fn home_env_bytes() -> Option<Vec<u8>> {
    #[cfg(unix)]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(home.as_bytes().to_vec());
        }
        current_user_home_bytes()
    }
    #[cfg(not(unix))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return Some(home.to_string_lossy().into_owned().into_bytes());
        }
        current_user_home_bytes()
    }
}

/// String form of [`home_env_bytes`] for the string-native `~` expansion path,
/// so it honors the same Windows HOME fallback (APPDATA / USERPROFILE / "C:/").
fn home_env_string() -> Option<String> {
    home_env_bytes().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Byte-native twin of `expand_user_home_prefix`.
pub fn expand_user_home_prefix_bytes(name: &[u8]) -> Option<(Vec<u8>, &[u8])> {
    let rest = name.strip_prefix(b"~")?;
    let sep = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
    if sep == 0 {
        return None;
    }
    let user = &rest[..sep];
    let home = user_homedir_bytes(user)?;
    Some((home, &rest[sep..]))
}

/// Byte-native twin of `user_homedir`, returning the raw passwd `pw_dir` bytes.
fn user_homedir_bytes(user: &[u8]) -> Option<Vec<u8>> {
    #[cfg(unix)]
    {
        let user = CString::new(user).ok()?;
        let passwd = unsafe { libc::getpwnam(user.as_ptr()) };
        if passwd.is_null() {
            return None;
        }
        let pw_dir = unsafe { (*passwd).pw_dir };
        if pw_dir.is_null() {
            return None;
        }
        let dir = unsafe { CStr::from_ptr(pw_dir) }.to_bytes();
        if !dir.starts_with(b"/") {
            return None;
        }
        Some(dir.to_vec())
    }

    #[cfg(not(unix))]
    {
        let _ = user;
        None
    }
}

/// Home directory of the *current* user, mirroring GNU `get_homedir`'s fallback
/// when `HOME` is unset (fileio.c): on Unix, `getpwnam` on $LOGNAME then $USER,
/// else `getpwuid(getuid())`; on non-Unix, the OS-derived home where GNU
/// `w32.c init_environment` puts HOME -- the roaming AppData folder
/// (CSIDL_APPDATA == %APPDATA%), else %USERPROFILE%, else "C:/".
pub fn current_user_home_bytes() -> Option<Vec<u8>> {
    #[cfg(unix)]
    {
        for var in ["LOGNAME", "USER"] {
            if let Some(user) = std::env::var_os(var)
                && let Some(home) = user_homedir_bytes(user.as_bytes())
            {
                return Some(home);
            }
        }
        let passwd = unsafe { libc::getpwuid(libc::getuid()) };
        if passwd.is_null() {
            return None;
        }
        let pw_dir = unsafe { (*passwd).pw_dir };
        if pw_dir.is_null() {
            return None;
        }
        let dir = unsafe { CStr::from_ptr(pw_dir) }.to_bytes();
        if dir.is_empty() {
            return None;
        }
        Some(dir.to_vec())
    }

    #[cfg(not(unix))]
    {
        let home = std::env::var_os("APPDATA")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "C:/".to_string());
        Some(home.into_bytes())
    }
}

fn expand_user_home_prefix(name: &str) -> Option<(String, &str)> {
    let rest = name.strip_prefix('~')?;
    let sep = rest.find('/').unwrap_or(rest.len());
    if sep == 0 {
        return None;
    }
    let user = &rest[..sep];
    let home = user_homedir(user)?;
    Some((home, &rest[sep..]))
}

fn user_homedir(user: &str) -> Option<String> {
    #[cfg(unix)]
    {
        let user = CString::new(user).ok()?;
        let passwd = unsafe { libc::getpwnam(user.as_ptr()) };
        if passwd.is_null() {
            return None;
        }
        let pw_dir = unsafe { (*passwd).pw_dir };
        if pw_dir.is_null() {
            return None;
        }
        let dir = unsafe { CStr::from_ptr(pw_dir) };
        if !dir.to_bytes().starts_with(b"/") {
            return None;
        }
        Some(dir.to_string_lossy().into_owned())
    }

    #[cfg(not(unix))]
    {
        let _ = user;
        None
    }
}

fn join_file_name(base: &str, name: &str) -> String {
    if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

fn trim_trailing_slashes_except_roots(path: &mut String) {
    while path.len() > 1 && path.ends_with('/') && !(path.len() == 2 && path.starts_with("//")) {
        path.pop();
    }
}

/// Clean up a path by resolving `.` and `..` components without touching the
/// filesystem (no symlink resolution).  GNU `expand-file-name` does this as a
/// spelling-preserving byte pass, not via the platform path parser: an initial
/// `//` is preserved, later repeated slashes are collapsed, and the first
/// `/../` above root remains visible as the POSIX "superroot" spelling.
fn clean_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut p = 0;

    while p < bytes.len() {
        if bytes[p] != b'/' {
            out.push(bytes[p]);
            p += 1;
        } else if p + 1 < bytes.len()
            && bytes[p + 1] == b'.'
            && (p + 2 == bytes.len() || bytes[p + 2] == b'/')
        {
            if out.is_empty() && p + 2 == bytes.len() {
                out.push(b'/');
            }
            p += 2;
        } else if p + 2 < bytes.len()
            && bytes[p + 1] == b'.'
            && bytes[p + 2] == b'.'
            && !out.is_empty()
            && (p + 3 == bytes.len() || bytes[p + 3] == b'/')
        {
            let mut previous_sep = out.len();
            while previous_sep > 0 {
                previous_sep -= 1;
                if out[previous_sep] == b'/' {
                    break;
                }
            }

            if previous_sep == 0 && out.first() == Some(&b'/') && p + 3 == bytes.len() {
                out.truncate(1);
            } else {
                out.truncate(previous_sep);
            }
            p += 3;
        } else if p + 1 < bytes.len()
            && bytes[p + 1] == b'/'
            && (p != 0 || (p + 2 < bytes.len() && bytes[p + 2] == b'/'))
        {
            p += 1;
        } else {
            out.push(bytes[p]);
            p += 1;
        }
    }

    String::from_utf8(out).expect("path canonicalization preserves UTF-8")
}

/// Return true if FILENAME is an absolute file name.
/// Mirrors GNU Emacs `file_name_absolute_p` for Unix path syntax.
pub fn file_name_absolute_p(filename: &str) -> bool {
    file_name_absolute_bytes_p(filename.as_bytes())
}
