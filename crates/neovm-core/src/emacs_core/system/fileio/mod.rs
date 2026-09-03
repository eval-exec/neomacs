//! File I/O primitives for the Elisp VM.
//!
//! Provides path manipulation, file predicates, read/write operations,
//! directory operations, and file attribute queries.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_fixnum, expect_max_args, expect_min_args};
use std::collections::{HashMap, VecDeque};
#[cfg(unix)]
use std::ffi::{CStr, CString};
use std::fs;
use std::io::{ErrorKind, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::buffer::{
    BufferId, CharPos0, EmacsByteLen, EmacsBytePos, EmacsByteRange, LispCharPos1,
    TextPositionAnchor, VisitedFileModtime, text_props::TextPropertyTable,
};
use crate::heap_types::LispString;

use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::intern::{intern, resolve_sym};
use super::symbol::Obarray;
use super::value::{
    OrderedRuntimeBindingMap, Value, ValueKind, VecLikeType, eq_value, list_to_vec,
};

// ===========================================================================
// Path operations (pure, no evaluator needed)
// ===========================================================================

/// Expand FILE relative to DEFAULT_DIR (or the current working directory).
/// Handles `~` expansion and absolute path detection.
pub fn expand_file_name(name: &str, default_dir: Option<&str>) -> String {
    expand_file_name_with_home(name, default_dir, None)
}

pub(crate) fn expand_file_name_with_home(
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

/// Byte-native `expand-file-name` core.
///
/// Operates on Emacs internal-encoding bytes so raw eight-bit file-name bytes
/// survive byte-exactly (GNU keeps file names as Lisp strings; on Unix
/// ENCODE_FILE / DECODE_FILE are byte-identity).  Path manipulation is
/// ASCII-structural (`/`, `~`, `.`); non-ASCII byte8 multibyte sequences are
/// non-special and pass through untouched.  This mirrors
/// `expand_file_name_with_home_inner` but never round-trips through a
/// storage-`&str`.
fn expand_file_name_bytes_with_home(
    name: &[u8],
    default_dir: Option<&[u8]>,
    home_override: Option<&[u8]>,
    expand_default_dir: bool,
) -> Vec<u8> {
    #[cfg(windows)]
    let name_normalized: Vec<u8> = name
        .iter()
        .map(|&b| if b == b'\\' { b'/' } else { b })
        .collect();
    #[cfg(windows)]
    let name: &[u8] = &name_normalized;
    #[cfg(windows)]
    let default_dir_normalized: Option<Vec<u8>> = default_dir.map(|dir| {
        dir.iter()
            .map(|&b| if b == b'\\' { b'/' } else { b })
            .collect()
    });
    #[cfg(windows)]
    let default_dir: Option<&[u8]> = default_dir_normalized.as_deref();

    // Handle ~ expansion
    let expanded: Vec<u8> = if name.starts_with(b"~/") {
        let mut out = Vec::new();
        if let Some(home) = home_override {
            out.extend_from_slice(home);
        } else if let Some(home) = home_env_bytes() {
            out.extend_from_slice(&home);
        } else {
            out.push(b'~');
        }
        out.extend_from_slice(&name[1..]);
        out
    } else if name == b"~" {
        if let Some(home) = home_override {
            home.to_vec()
        } else if let Some(home) = home_env_bytes() {
            home
        } else {
            name.to_vec()
        }
    } else if let Some((home, rest)) = expand_user_home_prefix_bytes(name) {
        let mut out = home;
        out.extend_from_slice(rest);
        out
    } else {
        name.to_vec()
    };

    let preserve_trailing_slash = expanded.last() == Some(&b'/');

    // If already absolute, just clean it up.
    if file_name_absolute_bytes_p(&expanded) {
        let mut cleaned = clean_path_bytes(&expanded);
        if preserve_trailing_slash && cleaned.last() != Some(&b'/') {
            cleaned.push(b'/');
        }
        return cleaned;
    }

    // Resolve relative to default_dir or cwd
    let base: Vec<u8> = if let Some(dir) = default_dir {
        if expand_default_dir {
            expand_file_name_bytes_with_home(dir, None, home_override, true)
        } else {
            dir.to_vec()
        }
    } else {
        current_dir_bytes()
    };

    if expanded.is_empty() {
        let mut cleaned = clean_path_bytes(&base);
        trim_trailing_slashes_except_roots_bytes(&mut cleaned);
        return cleaned;
    }

    let joined = join_file_name_bytes(&base, &expanded);
    let mut cleaned = clean_path_bytes(&joined);
    if preserve_trailing_slash && cleaned.last() != Some(&b'/') {
        cleaned.push(b'/');
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
fn home_env_bytes() -> Option<Vec<u8>> {
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

/// Raw bytes of the current working directory, byte-exact on Unix.
fn current_dir_bytes() -> Vec<u8> {
    match std::env::current_dir() {
        Ok(dir) => {
            #[cfg(unix)]
            {
                dir.as_os_str().as_bytes().to_vec()
            }
            #[cfg(not(unix))]
            {
                dir.to_string_lossy().into_owned().into_bytes()
            }
        }
        Err(_) => b"/".to_vec(),
    }
}

/// Byte-native twin of `expand_user_home_prefix`.
fn expand_user_home_prefix_bytes(name: &[u8]) -> Option<(Vec<u8>, &[u8])> {
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
fn current_user_home_bytes() -> Option<Vec<u8>> {
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

/// Byte-native twin of `join_file_name`.
fn join_file_name_bytes(base: &[u8], name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(base.len() + 1 + name.len());
    out.extend_from_slice(base);
    if base.last() != Some(&b'/') {
        out.push(b'/');
    }
    out.extend_from_slice(name);
    out
}

/// Byte-native twin of `trim_trailing_slashes_except_roots`.
fn trim_trailing_slashes_except_roots_bytes(path: &mut Vec<u8>) {
    while path.len() > 1
        && path.last() == Some(&b'/')
        && !(path.len() == 2 && path.starts_with(b"//"))
    {
        path.pop();
    }
}

/// Byte-native twin of `clean_path`.  Resolves `.`/`..` as a spelling-preserving
/// byte pass (no filesystem / symlink resolution), preserving the leading `//`
/// root marker and the POSIX superroot spelling exactly like the `&str` version.
fn clean_path_bytes(bytes: &[u8]) -> Vec<u8> {
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

    out
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

/// Convert a Lisp file-name string to an OS path at the real filesystem boundary.
///
/// GNU keeps file names as Lisp strings until ENCODE_FILE / platform I/O.
/// On Unix, preserve the original bytes exactly so raw unibyte file names
/// survive intact.
pub(crate) fn lisp_file_name_to_path_buf(filename: &crate::heap_types::LispString) -> PathBuf {
    #[cfg(unix)]
    {
        PathBuf::from(std::ffi::OsString::from_vec(filename.as_bytes().to_vec()))
    }

    #[cfg(not(unix))]
    {
        let name = crate::emacs_core::emacs_char::to_utf8_lossy(filename.as_bytes());
        PathBuf::from(lisp_file_name_to_host_path_string(&name))
    }
}

#[cfg(not(unix))]
fn lisp_file_name_to_host_path_string(filename: &str) -> String {
    #[cfg(windows)]
    {
        filename.replace('/', "\\")
    }

    #[cfg(not(windows))]
    {
        filename.to_string()
    }
}

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
pub(crate) fn host_path_to_lisp_file_name_string(path: &Path) -> String {
    host_path_to_lisp_file_name_string_inner(path)
}

/// Convert an OS path back to a Lisp file-name string at the filesystem boundary.
///
/// On Unix keep the raw bytes intact, matching GNU's byte-preserving file-name
/// handling for resolved paths.
pub(crate) fn path_to_lisp_file_name(path: &Path) -> crate::heap_types::LispString {
    #[cfg(unix)]
    {
        crate::heap_types::LispString::from_unibyte(path.as_os_str().as_bytes().to_vec())
    }

    #[cfg(not(unix))]
    {
        crate::emacs_core::builtins::plain_str_to_lisp_string(
            &host_path_to_lisp_file_name_string(path),
            true,
        )
    }
}

fn canonicalize_with_missing_suffix(path: &Path) -> PathBuf {
    if let Ok(canon) = fs::canonicalize(path) {
        return canon;
    }

    let mut prefix = path.to_path_buf();
    let mut suffix = VecDeque::new();
    loop {
        if let Ok(canon_prefix) = fs::canonicalize(&prefix) {
            let mut resolved = canon_prefix;
            for part in suffix {
                resolved.push(part);
            }
            return resolved;
        }

        let Some(name) = prefix.file_name().map(|s| s.to_os_string()) else {
            break;
        };
        suffix.push_front(name);
        if !prefix.pop() {
            break;
        }
    }

    path.to_path_buf()
}

/// Resolve FILENAME to a true name, preserving trailing slash marker semantics.
pub fn file_truename(filename: &str, default_dir: Option<&str>) -> String {
    let expanded = expand_file_name(filename, default_dir);
    let preserve_trailing_slash = expanded.ends_with('/');
    let mut resolved = canonicalize_with_missing_suffix(Path::new(&expanded))
        .to_string_lossy()
        .into_owned();

    if preserve_trailing_slash && resolved != "/" && !resolved.ends_with('/') {
        resolved.push('/');
    }

    resolved
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn file_truename_lisp_inner(
    filename: &crate::heap_types::LispString,
    default_dir: &crate::heap_types::LispString,
    remaining_links: &mut i64,
    prev_dirs: &mut HashMap<Vec<u8>, crate::heap_types::LispString>,
) -> Result<crate::heap_types::LispString, Flow> {
    let filename = lisp_file_name_normalize_directory_separators(filename);
    let default_dir = lisp_file_name_normalize_directory_separators(default_dir);
    let mut filename = if lisp_file_name_absolute_system_p(&filename) {
        filename
    } else {
        expand_file_name_lisp(&filename, Some(&default_dir))
    };

    loop {
        *remaining_links -= 1;
        if *remaining_links < 0 {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "Apparent cycle of symbolic links for {}",
                    crate::emacs_core::emacs_char::to_utf8_lossy(filename.as_bytes())
                ))],
            ));
        }

        let mut dir = lisp_file_name_directory(&filename).unwrap_or_else(|| default_dir.clone());
        let dirfile = lisp_directory_file_name(&dir);
        if !lisp_string_runtime_eq(&dir, &dirfile) {
            let dir_key = dir.as_bytes().to_vec();
            if let Some(cached) = prev_dirs.get(&dir_key).cloned() {
                dir = cached;
            } else {
                let new = lisp_file_name_as_directory(&file_truename_lisp_inner(
                    &dirfile,
                    &default_dir,
                    remaining_links,
                    prev_dirs,
                )?);
                prev_dirs.insert(dir_key, new.clone());
                dir = new;
            }
        }

        let filename_no_dir = lisp_file_name_nondirectory(&filename);
        if lisp_file_name_is_ascii_text(&filename_no_dir, b"..") {
            let parent = lisp_directory_file_name(&dir);
            return Ok(match lisp_file_name_directory(&parent) {
                Some(parent_dir) => lisp_directory_file_name(&parent_dir),
                None => parent,
            });
        }
        if lisp_file_name_is_ascii_text(&filename_no_dir, b".") {
            return Ok(lisp_directory_file_name(&dir));
        }

        filename = concat_file_name_lisp(&dir, &filename_no_dir);
        match file_symlink_target_lisp(&filename) {
            Some(target) => {
                filename = lisp_files_splice_dirname_file(&dir, &target);
            }
            None => return Ok(filename),
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn file_truename_lisp(
    filename: &crate::heap_types::LispString,
    default_dir: Option<&crate::heap_types::LispString>,
) -> Result<crate::heap_types::LispString, Flow> {
    let default_dir = default_dir
        .cloned()
        .unwrap_or_else(fallback_root_default_directory);
    let mut remaining_links = 100;
    let mut prev_dirs = HashMap::new();
    file_truename_lisp_inner(filename, &default_dir, &mut remaining_links, &mut prev_dirs)
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

/// Return the directory part of FILENAME, or None if there is no directory part.
/// Like Emacs `file-name-directory`: includes the trailing slash.
pub fn file_name_directory(filename: &str) -> Option<String> {
    // Emacs: if the filename ends with /, the whole thing is the directory part
    if filename
        .as_bytes()
        .last()
        .is_some_and(|&byte| file_name_directory_separator_byte(byte))
    {
        return if filename.is_empty() {
            None
        } else {
            Some(filename.to_string())
        };
    }
    // Find the last /
    filename
        .bytes()
        .rposition(file_name_directory_separator_byte)
        .map(|pos| filename[..=pos].to_string())
}

/// Return the non-directory part of FILENAME.
/// Like Emacs `file-name-nondirectory`.
pub fn file_name_nondirectory(filename: &str) -> String {
    // Emacs: if the filename ends with /, return ""
    if filename
        .as_bytes()
        .last()
        .is_some_and(|&byte| file_name_directory_separator_byte(byte))
    {
        return String::new();
    }
    match filename
        .bytes()
        .rposition(file_name_directory_separator_byte)
    {
        Some(pos) => filename[pos + 1..].to_string(),
        None => filename.to_string(),
    }
}

/// Return the extension of FILENAME.
/// When PERIOD is nil, returns extension without the leading dot, or nil if missing.
/// Return FILENAME as a directory name (must end in `/`).
/// Like Emacs `file-name-as-directory`.
pub fn file_name_as_directory(filename: &str) -> String {
    if filename.is_empty() {
        "./".to_string()
    } else if filename.ends_with('/') {
        filename.to_string()
    } else {
        format!("{filename}/")
    }
}

/// Return directory FILENAME in file-name form (without trailing slash).
/// Like Emacs `directory-file-name`.
pub fn directory_file_name(filename: &str) -> String {
    if filename.is_empty() {
        return String::new();
    }

    // Emacs keeps exactly two leading slashes as a distinct root marker.
    if filename.bytes().all(|b| b == b'/') {
        return if filename.len() == 2 {
            "//".to_string()
        } else {
            "/".to_string()
        };
    }

    filename.trim_end_matches('/').to_string()
}

/// Concatenate file name components with separator insertion between
/// non-empty components, skipping empty components.
/// Like Emacs `file-name-concat` after filtering nil/empty args.
pub fn file_name_concat(parts: &[&str]) -> String {
    let mut iter = parts.iter().copied().filter(|s| !s.is_empty());
    let Some(first) = iter.next() else {
        return String::new();
    };

    let mut out = first.to_string();
    for part in iter {
        if !out.ends_with('/') {
            out.push('/');
        }
        out.push_str(part);
    }
    out
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

fn file_name_absolute_bytes_p(filename: &[u8]) -> bool {
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

/// Return true if FILENAME is an absolute file name.
/// Mirrors GNU Emacs `file_name_absolute_p` for Unix path syntax.
pub fn file_name_absolute_p(filename: &str) -> bool {
    file_name_absolute_bytes_p(filename.as_bytes())
}

/// Return true if NAME is a directory name (ends with a directory separator).
pub fn directory_name_p(name: &str) -> bool {
    name.as_bytes()
        .last()
        .is_some_and(|&byte| file_name_directory_separator_byte(byte))
}

fn file_name_directory_separator_byte(byte: u8) -> bool {
    #[cfg(windows)]
    {
        byte == b'/' || byte == b'\\'
    }
    #[cfg(not(windows))]
    {
        byte == b'/'
    }
}

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);
static DEFAULT_FILE_MODE_MASK: AtomicU32 = AtomicU32::new(0o022);
static DEFAULT_FILE_MODE_MASK_INIT: Once = Once::new();

fn init_default_file_mode_mask() {
    DEFAULT_FILE_MODE_MASK_INIT.call_once(|| {
        #[cfg(unix)]
        unsafe {
            let old = libc::umask(0);
            libc::umask(old);
            DEFAULT_FILE_MODE_MASK.store(old as u32, Ordering::Relaxed);
        }
    });
}

fn env_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn embedded_absfilename_start(bytes: &[u8]) -> Option<usize> {
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

fn trim_embedded_absfilename_bytes(path: Vec<u8>) -> Vec<u8> {
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

pub(crate) fn substitute_in_file_name_lisp(
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
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

        crate::heap_types::LispString::from_unibyte(trim_embedded_absfilename_bytes(out))
    }

    #[cfg(not(unix))]
    {
        let substituted = substitute_in_file_name(&crate::emacs_core::emacs_char::to_utf8_lossy(
            filename.as_bytes(),
        ));
        crate::emacs_core::builtins::plain_str_to_lisp_string(&substituted, !substituted.is_ascii())
    }
}

// ===========================================================================
// File predicates (pure)
// ===========================================================================

/// Return true if FILENAME exists (file, directory, symlink, etc.).
pub fn file_exists_p(filename: &str) -> bool {
    Path::new(filename).exists()
}

/// Return true if FILENAME is readable.
pub fn file_readable_p(filename: &str) -> bool {
    file_readable_path(Path::new(filename))
}

/// Return true if FILENAME is writable.
pub fn file_writable_p(filename: &str) -> bool {
    file_writable_path(Path::new(filename))
}

/// Return true if FILENAME is an accessible directory.
pub fn file_accessible_directory_p(filename: &str) -> bool {
    let path = Path::new(filename);
    if !path.is_dir() {
        return false;
    }

    #[cfg(unix)]
    {
        let Ok(c_path) = CString::new(filename) else {
            return false;
        };
        let mode = libc::R_OK | libc::X_OK;
        unsafe { libc::access(c_path.as_ptr(), mode) == 0 }
    }

    #[cfg(not(unix))]
    {
        return fs::read_dir(path).is_ok();
    }
}

/// Return true if FILENAME is executable by the current process.
pub fn file_executable_p(filename: &str) -> bool {
    #[cfg(unix)]
    {
        let Ok(c_path) = CString::new(filename) else {
            return false;
        };
        unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
    }

    #[cfg(not(unix))]
    {
        return Path::new(filename).exists();
    }
}

/// Return true if FILENAME is currently locked by Emacs lockfiles.
///
/// NeoVM currently does not implement lockfile probing, so this returns nil.
pub fn file_locked_p(_filename: &crate::heap_types::LispString) -> bool {
    false
}

/// Return filesystem capacity information for PATH.
///
/// The tuple layout matches Emacs `file-system-info`:
/// `(TOTAL-BYTES FREE-BYTES AVAILABLE-BYTES)`.
fn file_system_info_path(path: &Path) -> std::io::Result<(i64, i64, i64)> {
    #[cfg(unix)]
    {
        fn saturating_i64(v: u128) -> i64 {
            if v > i64::MAX as u128 {
                i64::MAX
            } else {
                v as i64
            }
        }

        let c_path = path_to_cstring(path).map_err(|_| {
            std::io::Error::new(ErrorKind::InvalidInput, "embedded NUL in file name")
        })?;
        let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats as *mut libc::statvfs) } != 0 {
            return Err(std::io::Error::last_os_error());
        }

        let block_size = if stats.f_frsize > 0 {
            stats.f_frsize as u128
        } else {
            stats.f_bsize as u128
        };
        let total = (stats.f_blocks as u128) * block_size;
        let free = (stats.f_bfree as u128) * block_size;
        let available = (stats.f_bavail as u128) * block_size;
        Ok((
            saturating_i64(total),
            saturating_i64(free),
            saturating_i64(available),
        ))
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        Ok((0, 0, 0))
    }
}

/// Return true if FILENAME is a directory.
pub fn file_directory_p(filename: &str) -> bool {
    Path::new(filename).is_dir()
}

/// Return true if FILENAME is a regular file.
pub fn file_regular_p(filename: &str) -> bool {
    Path::new(filename).is_file()
}

/// Return true if FILENAME is a symbolic link.
/// Return true if FILENAME is a symbolic link. Used by predicates that
/// only need a yes/no answer (e.g. internal helpers); the public
/// `file-symlink-p` builtin returns the link target instead.
pub fn file_symlink_p(filename: &str) -> bool {
    match fs::symlink_metadata(filename) {
        Ok(meta) => meta.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// Return the symbolic link target of FILENAME as a String, mirroring
/// GNU Emacs `Ffile_symlink_p` (`fileio.c:3160`):
///
///   "Return non-nil if file FILENAME is the name of a symbolic link.
///    The value is the link target, as a string.
///    Return nil if FILENAME does not exist or is not a symbolic link,
///    or there was trouble determining whether the file is a symbolic link.
///    This function does not check whether the link target exists."
///
/// The returned target is whatever the OS `readlink` syscall produces;
/// it may be relative.  We do NOT canonicalize it (GNU's
/// `emacs_readlinkat` likewise does not).
pub fn file_symlink_target(filename: &str) -> Option<String> {
    let meta = fs::symlink_metadata(filename).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    fs::read_link(filename)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

pub fn file_symlink_target_lisp(
    filename: &crate::heap_types::LispString,
) -> Option<crate::heap_types::LispString> {
    let path = lisp_file_name_to_path_buf(filename);
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_symlink() {
        return None;
    }
    fs::read_link(&path)
        .ok()
        .map(|target| path_to_lisp_file_name(&target))
}

/// Return true if FILENAME is on a case-insensitive filesystem.
pub fn file_name_case_insensitive_p(filename: &str) -> bool {
    let mut probe = PathBuf::from(filename);
    while !probe.exists() {
        if !probe.pop() || probe.as_os_str().is_empty() {
            return false;
        }
    }
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Return true if FILE1 has a newer modification time than FILE2.
pub fn file_newer_than_file_p(file1: &str, file2: &str) -> bool {
    let meta1 = match fs::metadata(file1) {
        Ok(meta) => meta,
        Err(_) => return false,
    };
    let meta2 = match fs::metadata(file2) {
        Ok(meta) => meta,
        Err(_) => return true,
    };

    let mtime1 = match meta1.modified() {
        Ok(time) => time,
        Err(_) => return false,
    };
    let mtime2 = match meta2.modified() {
        Ok(time) => time,
        Err(_) => return true,
    };
    mtime1 > mtime2
}

fn file_newer_than_file_path(file1: &Path, file2: &Path) -> bool {
    let meta1 = match fs::metadata(file1) {
        Ok(meta) => meta,
        Err(_) => return false,
    };
    let meta2 = match fs::metadata(file2) {
        Ok(meta) => meta,
        Err(_) => return true,
    };

    let mtime1 = match meta1.modified() {
        Ok(time) => time,
        Err(_) => return false,
    };
    let mtime2 = match meta2.modified() {
        Ok(time) => time,
        Err(_) => return true,
    };
    mtime1 > mtime2
}

// ===========================================================================
// File I/O operations
// ===========================================================================

/// Read the contents of FILENAME as a UTF-8 string.
pub fn read_file_contents(filename: &str) -> std::io::Result<String> {
    fs::read_to_string(filename)
}

/// Write CONTENT to FILENAME, optionally appending.
pub fn write_string_to_file(content: &str, filename: &str, append: bool) -> std::io::Result<()> {
    let mode = if append {
        FileWriteMode::Append
    } else {
        FileWriteMode::Truncate
    };
    let file = write_bytes_to_file_with_mode(content.as_bytes(), Path::new(filename), mode)?;
    drop(file);
    Ok(())
}

#[derive(Clone, Copy)]
enum FileWriteMode {
    Truncate,
    Append,
    Seek(u64),
    /// `O_WRONLY | O_CREAT | O_EXCL` — used by `write-region` when MUSTBENEW is
    /// `excl`.  Fails with `ErrorKind::AlreadyExists` (EEXIST) if the file
    /// already exists, matching GNU's `open_flags |= O_EXCL`.
    Excl,
}

/// The three meanings GNU assigns to `write-region`'s VISIT argument.
///
/// GNU always retains a `visit_file` for locking and completion messages, but
/// only `t` and strings make the current buffer visit that file.  A different
/// non-nil value requests a quiet, non-visiting write.  Keeping those states
/// distinct prevents the reporting path from being accidentally coupled to
/// the buffer-visiting path.
enum WriteRegionVisit {
    ReportOnly(LispString),
    Visit(LispString),
    Quiet(LispString),
}

impl WriteRegionVisit {
    fn from_lisp(eval: &Context, argument: Value, output_file: &LispString) -> Self {
        if argument.is_t() {
            Self::Visit(output_file.clone())
        } else if let Some(visit_file) = argument.as_lisp_string() {
            Self::Visit(resolve_filename_lisp_for_eval(eval, visit_file))
        } else if argument.is_nil() {
            Self::ReportOnly(output_file.clone())
        } else {
            Self::Quiet(output_file.clone())
        }
    }

    fn file_name(&self) -> &LispString {
        match self {
            Self::ReportOnly(file) | Self::Visit(file) | Self::Quiet(file) => file,
        }
    }

    fn visited_file(&self) -> Option<&LispString> {
        match self {
            Self::Visit(file) => Some(file),
            Self::ReportOnly(_) | Self::Quiet(_) => None,
        }
    }

    fn reports_completion(&self) -> bool {
        match self {
            Self::ReportOnly(_) | Self::Visit(_) => true,
            Self::Quiet(_) => false,
        }
    }
}

#[derive(Clone, Copy)]
enum WriteRegionCompletion {
    Wrote,
    AddedTo,
    Updated,
}

impl WriteRegionCompletion {
    fn from_append_argument(append: Value) -> Self {
        if append.is_number() {
            Self::Updated
        } else if append.is_truthy() {
            Self::AddedTo
        } else {
            Self::Wrote
        }
    }

    fn message_format(self) -> &'static str {
        match self {
            Self::Wrote => "Wrote %s",
            Self::AddedTo => "Added to %s",
            Self::Updated => "Updated %s",
        }
    }
}

/// Write raw bytes to a file, returning the open `File` handle so the caller
/// can optionally `sync_all()` before the handle is dropped.
fn write_bytes_to_file_with_mode(
    content: &[u8],
    filename: &Path,
    mode: FileWriteMode,
) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true);
    match mode {
        FileWriteMode::Truncate => {
            options.create(true).truncate(true);
        }
        FileWriteMode::Append => {
            options.create(true).append(true);
        }
        FileWriteMode::Seek(_) => {
            options.create(true);
        }
        FileWriteMode::Excl => {
            // `create_new` maps to O_CREAT | O_EXCL.
            options.create_new(true);
        }
    }
    let mut file = options.open(filename)?;
    if let FileWriteMode::Seek(offset) = mode {
        file.seek(SeekFrom::Start(offset))?;
    }
    file.write_all(content)?;
    Ok(file)
}

// ===========================================================================
// Directory operations
// ===========================================================================

/// Return a list of file names in DIR.
/// If FULL is true, return absolute paths.
/// If MATCH_REGEX is Some, only include entries whose names match the regex.
/// If NOSORT is true, preserve filesystem enumeration order.
/// COUNT limits the number of accepted entries during enumeration.
fn read_directory_names_lisp(
    dir: &crate::heap_types::LispString,
) -> Result<Vec<crate::heap_types::LispString>, DirectoryFilesError> {
    let entries =
        fs::read_dir(lisp_file_name_to_path_buf(dir)).map_err(|e| DirectoryFilesError::Io {
            action: "Opening directory",
            err: e,
        })?;
    let mut names = vec![
        crate::heap_types::LispString::from_unibyte(b".".to_vec()),
        crate::heap_types::LispString::from_unibyte(b"..".to_vec()),
    ];
    for entry in entries {
        let entry = entry.map_err(|e| DirectoryFilesError::Io {
            action: "Reading directory entry",
            err: e,
        })?;
        // Keep the host entry bytes intact here.  Public directory primitives
        // apply GNU's DECODE_FILE step before matching or returning names.
        names.push(path_to_lisp_file_name(Path::new(&entry.file_name())));
    }
    Ok(names)
}

#[derive(Debug)]
enum DirectoryFilesError {
    Io {
        action: &'static str,
        err: std::io::Error,
    },
    InvalidRegexp(String),
}

#[cfg(test)]
fn directory_files(
    dir: &crate::heap_types::LispString,
    full: bool,
    match_regex: Option<&crate::heap_types::LispString>,
    nosort: bool,
    count: Option<usize>,
) -> Result<Vec<crate::heap_types::LispString>, DirectoryFilesError> {
    let eval = super::eval::Context::new();
    let syntax = super::builtins::search::FastStringMatchSyntax::for_current_buffer(&eval);
    directory_files_with_decoder(
        dir,
        full,
        match_regex,
        nosort,
        count,
        syntax,
        &eval.obarray,
        &eval.buffers,
        |bytes| crate::heap_types::LispString::from_unibyte(bytes.to_vec()),
    )
}

#[allow(clippy::too_many_arguments)] // match-time state stays explicit at the GNU-regexp boundary
fn directory_files_with_decoder(
    dir: &crate::heap_types::LispString,
    full: bool,
    match_regex: Option<&crate::heap_types::LispString>,
    nosort: bool,
    count: Option<usize>,
    syntax: super::builtins::search::FastStringMatchSyntax,
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    decode_name: impl Fn(&[u8]) -> crate::heap_types::LispString,
) -> Result<Vec<crate::heap_types::LispString>, DirectoryFilesError> {
    if count == Some(0) {
        return Ok(Vec::new());
    }

    let names = read_directory_names_lisp(dir)?;

    // Emacs builds this list via `cons` while scanning readdir output.
    // That makes NOSORT results reverse the traversal order and applies COUNT
    // before sort.
    let mut result = VecDeque::new();
    let mut remaining = count.unwrap_or(usize::MAX);
    let dir_with_slash = lisp_file_name_as_directory(dir);

    for raw_name in names {
        let name = decode_name(raw_name.as_bytes());
        if let Some(pattern) = match_regex {
            let matched = syntax
                .search(
                    obarray,
                    buffers,
                    pattern,
                    &name,
                    super::regex::SearchedString::Owned(name.clone()),
                    0,
                    false,
                )
                .map_err(|msg| {
                    DirectoryFilesError::InvalidRegexp(format!(
                        "Invalid regexp \"{}\": {}",
                        crate::emacs_core::emacs_char::to_utf8_lossy(pattern.as_bytes()),
                        msg
                    ))
                })?;
            if matched.is_none() {
                continue;
            }
        }

        if full {
            result.push_front(concat_file_name_lisp(&dir_with_slash, &name));
        } else {
            result.push_front(name);
        }

        if remaining != usize::MAX {
            remaining -= 1;
            if remaining == 0 {
                break;
            }
        }
    }

    let mut result: Vec<crate::heap_types::LispString> = result.into_iter().collect();
    if !nosort {
        result.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    }
    Ok(result)
}

/// Create directory DIR.  If PARENTS is true, create parent directories as needed.
pub fn make_directory(dir: &str, parents: bool) -> std::io::Result<()> {
    if parents {
        fs::create_dir_all(dir)
    } else {
        fs::create_dir(dir)
    }
}

// ===========================================================================
// File management
// ===========================================================================

/// Delete FILENAME.
pub fn delete_file(filename: &str) -> std::io::Result<()> {
    fs::remove_file(filename)
}

/// Rename file FROM to TO.
pub fn rename_file(from: &str, to: &str) -> std::io::Result<()> {
    rename_path_with_cross_device_fallback(Path::new(from), Path::new(to), true, |from, to| {
        fs::rename(from, to)
    })
}

fn is_cross_device_rename_error(err: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        err.raw_os_error() == Some(libc::EXDEV)
    }
    #[cfg(not(unix))]
    {
        let _ = err;
        false
    }
}

fn rename_path_with_cross_device_fallback<F>(
    from_path: &Path,
    to_path: &Path,
    ok_if_exists: bool,
    rename: F,
) -> std::io::Result<()>
where
    F: FnOnce(&Path, &Path) -> std::io::Result<()>,
{
    match rename(from_path, to_path) {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device_rename_error(&err) => {
            rename_regular_file_by_copy_delete(from_path, to_path, ok_if_exists)
        }
        Err(err) => Err(err),
    }
}

fn rename_regular_file_by_copy_delete(
    from_path: &Path,
    to_path: &Path,
    ok_if_exists: bool,
) -> std::io::Result<()> {
    let from_meta = fs::symlink_metadata(from_path)?;

    if fs::symlink_metadata(to_path).is_ok() && !ok_if_exists {
        return Err(std::io::Error::from(ErrorKind::AlreadyExists));
    }

    // GNU `Frename_file`'s EXDEV fallback (fileio.c) honors the source type:
    // a directory is `copy-directory`'d then `delete-directory`'d, a symlink is
    // recreated, a regular file is `copy-file`'d. Mirror that so a
    // cross-filesystem move — e.g. magit trashing an untracked directory to
    // ~/.local/share/Trash (#189) — succeeds instead of re-raising EXDEV.
    let from_type = from_meta.file_type();
    if from_type.is_symlink() {
        copy_symlink(from_path, to_path)?;
        fs::remove_file(from_path)
    } else if from_type.is_dir() {
        copy_dir_recursive(from_path, to_path)?;
        fs::remove_dir_all(from_path)
    } else {
        fs::copy(from_path, to_path)?;
        fs::set_permissions(to_path, from_meta.permissions())?;
        fs::remove_file(from_path)
    }
}

/// Recreate the symlink at `to_path` pointing at the same target as the symlink
/// at `from_path` (for the cross-device rename fallback).
fn copy_symlink(from_path: &Path, to_path: &Path) -> std::io::Result<()> {
    let target = fs::read_link(from_path)?;
    if fs::symlink_metadata(to_path).is_ok() {
        let _ = fs::remove_file(to_path).or_else(|_| fs::remove_dir_all(to_path));
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, to_path)
    }
    #[cfg(windows)]
    {
        if fs::metadata(from_path).map(|m| m.is_dir()).unwrap_or(false) {
            std::os::windows::fs::symlink_dir(&target, to_path)
        } else {
            std::os::windows::fs::symlink_file(&target, to_path)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = target;
        Err(std::io::Error::from(ErrorKind::Unsupported))
    }
}

/// Recursively copy directory `from` to `to`, preserving directory permissions,
/// recreating symlinks, and copying regular files — the cross-device rename
/// fallback's stand-in for GNU's `copy-directory`.
fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    if let Ok(meta) = fs::metadata(from) {
        let _ = fs::set_permissions(to, meta.permissions());
    }
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            copy_symlink(&src, &dst)?;
        } else if file_type.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Copy file FROM to TO.
pub fn copy_file(from: &str, to: &str) -> std::io::Result<()> {
    fs::copy(from, to).map(|_| ())
}

/// Create an additional name (hard link) from OLDNAME to NEWNAME.
pub fn add_name_to_file(oldname: &str, newname: &str) -> std::io::Result<()> {
    fs::hard_link(oldname, newname)
}

// ===========================================================================
// File attributes
// ===========================================================================

/// Metadata about a file.
#[derive(Debug, Clone)]
pub struct FileAttributes {
    pub size: u64,
    pub nlinks: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub modified: Option<f64>, // seconds since epoch
    pub modes: u32,
}

/// Return file attributes for FILENAME, or None if the file doesn't exist.
pub fn file_attributes(filename: &str) -> Option<FileAttributes> {
    let meta = fs::metadata(filename).ok()?;
    let symlink_meta = fs::symlink_metadata(filename).ok();

    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64());

    #[cfg(unix)]
    let modes = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    };
    #[cfg(not(unix))]
    let modes = if meta.permissions().readonly() {
        0o444
    } else {
        0o644
    };

    #[cfg(unix)]
    let nlinks = {
        use std::os::unix::fs::MetadataExt;
        meta.nlink()
    };
    #[cfg(not(unix))]
    let nlinks = 1;

    Some(FileAttributes {
        size: meta.len(),
        nlinks,
        is_dir: meta.is_dir(),
        is_symlink: symlink_meta.is_some_and(|m| m.file_type().is_symlink()),
        modified,
        modes,
    })
}

// ===========================================================================
// Builtin wrappers — pure (no evaluator needed)
// ===========================================================================

fn expect_lisp_string_strict(value: &Value) -> Result<crate::heap_types::LispString, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload")
            .clone()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
    }
}

fn expect_lisp_filename_string_strict(
    value: &Value,
) -> Result<crate::heap_types::LispString, Flow> {
    let string = expect_lisp_string_strict(value)?;
    if string.as_bytes().contains(&0) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("filenamep"), *value],
        ));
    }
    Ok(string)
}

fn file_name_lisp_from_bytes(bytes: Vec<u8>, multibyte: bool) -> crate::heap_types::LispString {
    if multibyte {
        crate::heap_types::LispString::from_emacs_bytes(bytes)
    } else {
        crate::heap_types::LispString::from_unibyte(bytes)
    }
}

fn empty_file_name_lisp_string() -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_unibyte(Vec::new())
}

fn fallback_root_default_directory() -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_utf8("/")
}

fn invalid_file_name_handler_result() -> Flow {
    signal(
        "error",
        vec![Value::string(
            "Invalid handler in ‘file-name-handler-alist’",
        )],
    )
}

fn file_name_handler_string_or_nil(result: Value) -> Value {
    if result.is_string() {
        result
    } else {
        Value::NIL
    }
}

fn file_name_handler_string_or_error(result: Value) -> EvalResult {
    if result.is_string() {
        Ok(result)
    } else {
        Err(invalid_file_name_handler_result())
    }
}

fn expand_file_name_result_multibyte(
    name: &crate::heap_types::LispString,
    default_directory: &crate::heap_types::LispString,
) -> bool {
    let mut multibyte = name.is_multibyte();
    let defdir_multibyte = default_directory.is_multibyte();
    if multibyte != defdir_multibyte {
        if multibyte {
            if name.is_ascii() || !default_directory.is_ascii() {
                multibyte = false;
            }
        } else if name.is_ascii() {
            multibyte = true;
        }
    }
    multibyte
}

fn file_name_concat_result_multibyte(parts: &[&crate::heap_types::LispString]) -> bool {
    let any_multibyte = parts.iter().any(|part| part.is_multibyte());
    let any_unibyte_non_ascii = parts
        .iter()
        .any(|part| !part.is_multibyte() && !part.is_ascii());
    any_multibyte && !any_unibyte_non_ascii
}

fn concat_file_name_lisp(
    left: &crate::heap_types::LispString,
    right: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    let mut bytes = Vec::with_capacity(left.as_bytes().len() + right.as_bytes().len());
    bytes.extend_from_slice(left.as_bytes());
    bytes.extend_from_slice(right.as_bytes());
    file_name_lisp_from_bytes(bytes, file_name_concat_result_multibyte(&[left, right]))
}

/// `file-name-concat` over Lisp file names, preserving raw file-name bytes
/// (skips empty components and inserts `/` separators, like the &str
/// `file_name_concat` but without the storage-String round-trip).
fn file_name_concat_lisp(
    parts: &[&crate::heap_types::LispString],
) -> crate::heap_types::LispString {
    let non_empty: Vec<&crate::heap_types::LispString> = parts
        .iter()
        .copied()
        .filter(|part| !part.as_bytes().is_empty())
        .collect();
    let Some((first, rest)) = non_empty.split_first() else {
        return empty_file_name_lisp_string();
    };
    let mut out = first.as_bytes().to_vec();
    for part in rest {
        if out.last() != Some(&b'/') {
            out.push(b'/');
        }
        out.extend_from_slice(part.as_bytes());
    }
    file_name_lisp_from_bytes(out, file_name_concat_result_multibyte(&non_empty))
}

fn lisp_file_name_directory(
    filename: &crate::heap_types::LispString,
) -> Option<crate::heap_types::LispString> {
    let bytes = filename.as_bytes();
    if bytes
        .last()
        .is_some_and(|&byte| file_name_directory_separator_byte(byte))
    {
        return (!bytes.is_empty())
            .then(|| lisp_file_name_normalize_directory_separators(filename));
    }
    if let Some(pos) = bytes
        .iter()
        .rposition(|&byte| file_name_directory_separator_byte(byte))
    {
        return Some(lisp_file_name_normalize_directory_separators(
            &file_name_lisp_from_bytes(bytes[..=pos].to_vec(), filename.is_multibyte()),
        ));
    }
    // GNU `file_name_directory` (fileio.c) treats the drive `:` as a directory
    // boundary under DOS_NT: the directory of a drive-relative name (`c:` or
    // `c:foo`, with no slash) is the drive `c:`, so it never returns nil for one.
    // Without this, walking such a path up to the bare drive -- e.g.
    // `org-persist--check-write-access`'s parent loop -- evaluates
    // `(directory-file-name (file-name-directory "c:"))` and signals
    // `wrong-type-argument stringp,nil`, which broke byte-compiling org on
    // Windows.  Mirrors the windows drive branch already in
    // `lisp_directory_file_name`.
    #[cfg(windows)]
    if matches!(bytes, [drive, b':', ..] if drive.is_ascii_alphabetic()) {
        return Some(file_name_lisp_from_bytes(
            bytes[..2].to_vec(),
            filename.is_multibyte(),
        ));
    }
    None
}

#[cfg(windows)]
fn lisp_file_name_normalize_directory_separators(
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    if !filename.as_bytes().contains(&b'\\') {
        return filename.clone();
    }
    let mut bytes = filename.as_bytes().to_vec();
    for byte in &mut bytes {
        if *byte == b'\\' {
            *byte = b'/';
        }
    }
    file_name_lisp_from_bytes(bytes, filename.is_multibyte())
}

#[cfg(not(windows))]
fn lisp_file_name_normalize_directory_separators(
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    filename.clone()
}

fn lisp_file_name_nondirectory(
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    let bytes = filename.as_bytes();
    if bytes
        .last()
        .is_some_and(|&byte| file_name_directory_separator_byte(byte))
    {
        return file_name_lisp_from_bytes(Vec::new(), filename.is_multibyte());
    }
    match bytes
        .iter()
        .rposition(|&byte| file_name_directory_separator_byte(byte))
    {
        Some(pos) => file_name_lisp_from_bytes(bytes[pos + 1..].to_vec(), filename.is_multibyte()),
        None => filename.clone(),
    }
}

fn lisp_file_name_as_directory(
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    if filename.as_bytes().is_empty() {
        return file_name_lisp_from_bytes(b"./".to_vec(), filename.is_multibyte());
    }
    if filename.as_bytes().ends_with(b"/") {
        return filename.clone();
    }
    let mut bytes = filename.as_bytes().to_vec();
    bytes.push(b'/');
    file_name_lisp_from_bytes(bytes, filename.is_multibyte())
}

fn lisp_directory_file_name(
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    let bytes = filename.as_bytes();
    if bytes.is_empty() {
        return filename.clone();
    }
    #[cfg(windows)]
    if matches!(bytes, [drive, b':', b'/', rest @ ..]
        if drive.is_ascii_alphabetic() && rest.iter().all(|&byte| byte == b'/'))
    {
        return file_name_lisp_from_bytes(bytes[..3].to_vec(), filename.is_multibyte());
    }
    if bytes.iter().all(|&byte| byte == b'/') {
        return if bytes.len() == 2 {
            filename.clone()
        } else {
            file_name_lisp_from_bytes(vec![b'/'], filename.is_multibyte())
        };
    }
    let trimmed_len = bytes
        .iter()
        .rposition(|&byte| byte != b'/')
        .map_or(0, |pos| pos + 1);
    file_name_lisp_from_bytes(bytes[..trimmed_len].to_vec(), filename.is_multibyte())
}

pub(crate) fn expand_file_name_lisp(
    name: &crate::heap_types::LispString,
    default_directory: Option<&crate::heap_types::LispString>,
) -> crate::heap_types::LispString {
    expand_file_name_lisp_with_home(name, default_directory, None)
}

/// Byte-faithful file-name expansion with an explicit HOME snapshot.
///
/// Process executable lookup uses this form because GNU `openp` expands every
/// search candidate through `Fexpand_file_name`, whose `~` resolution observes
/// the dynamically bound Lisp `process-environment`, not Rust's host
/// environment.  Keeping HOME explicit also prevents callers from growing
/// their own partial tilde-expansion rules.
pub(crate) fn expand_file_name_lisp_with_home(
    name: &crate::heap_types::LispString,
    default_directory: Option<&crate::heap_types::LispString>,
    home_directory: Option<&[u8]>,
) -> crate::heap_types::LispString {
    let default_directory = default_directory
        .cloned()
        .unwrap_or_else(fallback_root_default_directory);
    // Operate on the Emacs-internal-encoding bytes, like the rest of the
    // `_lisp` file-name family, so raw eight-bit file-name bytes round-trip
    // byte-exactly (no storage-String detour).
    let result = expand_file_name_bytes_with_home(
        name.as_bytes(),
        Some(default_directory.as_bytes()),
        home_directory,
        true,
    );
    file_name_lisp_from_bytes(
        result,
        expand_file_name_result_multibyte(name, &default_directory),
    )
}

/// HOME for `expand-file-name`, as Emacs-internal-encoding bytes to feed the
/// byte-native expansion core without a storage-String detour.
pub(crate) fn home_directory_for_expand_file_name(eval: &Context) -> Option<Vec<u8>> {
    // GNU `get_homedir`: the HOME environment variable, else the current user's
    // home directory (passwd entry on Unix, OS-derived on Windows). Without the
    // fallback, a session with HOME unset -- e.g. a minimal Windows build env,
    // where GNU relies on `init_environment` having set HOME -- leaves `~`
    // unexpanded, so `directory-files "~"` fails at startup.
    let environment = eval.visible_variable_value_or_nil("process-environment");
    if let super::environment::EnvironmentLookup::Value(value) =
        super::environment::lookup_environment_list(&LispString::from_utf8("HOME"), environment)
        && let Some(home) = eval.lisp_string(value)
    {
        return Some(home.as_bytes().to_vec());
    }
    current_user_home_bytes()
}

#[cfg(windows)]
fn lisp_file_name_absolute_system_p(filename: &crate::heap_types::LispString) -> bool {
    let bytes = filename.as_bytes();
    // A tilde-prefixed name is "absolute" per `file-name-absolute-p` but still
    // needs ~-expansion (cf. GNU `file_name_absolute_no_tilde_p`). If we treat it
    // as already-resolved, `resolve_filename_lisp_in_state` returns `~` verbatim,
    // so e.g. `directory-files "~"` (startup.el's init-file probe) opens a literal
    // "~" and fails with "Opening directory ~". Exclude tilde, matching the Unix
    // branch (which only treats a leading `/` as system-absolute).
    bytes.first() != Some(&b'~') && file_name_absolute_bytes_p(bytes)
}

#[cfg(not(windows))]
fn lisp_file_name_absolute_system_p(filename: &crate::heap_types::LispString) -> bool {
    filename.as_bytes().first() == Some(&b'/')
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lisp_string_runtime_eq(
    left: &crate::heap_types::LispString,
    right: &crate::heap_types::LispString,
) -> bool {
    left.as_bytes() == right.as_bytes()
}

fn lisp_file_name_is_ascii_text(filename: &crate::heap_types::LispString, text: &[u8]) -> bool {
    filename.as_bytes() == text
}

fn lisp_directory_name_p(filename: &crate::heap_types::LispString) -> bool {
    filename.as_bytes().last() == Some(&b'/')
}

fn lisp_string_strip_ascii_prefix(
    value: &crate::heap_types::LispString,
    prefix: &[u8],
) -> Option<crate::heap_types::LispString> {
    let rest = value.as_bytes().strip_prefix(prefix)?;
    Some(if value.is_multibyte() {
        crate::heap_types::LispString::from_emacs_bytes(rest.to_vec())
    } else {
        crate::heap_types::LispString::from_unibyte(rest.to_vec())
    })
}

fn wrap_ascii_around_lisp_string(
    value: &crate::heap_types::LispString,
    prefix: &[u8],
    suffix: &[u8],
) -> crate::heap_types::LispString {
    let mut bytes = Vec::with_capacity(prefix.len() + value.as_bytes().len() + suffix.len());
    bytes.extend_from_slice(prefix);
    bytes.extend_from_slice(value.as_bytes());
    bytes.extend_from_slice(suffix);
    file_name_lisp_from_bytes(bytes, value.is_multibyte())
}

fn expand_cp_target_lisp_for_eval(
    eval: &Context,
    file: &crate::heap_types::LispString,
    newname: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    if lisp_directory_name_p(newname) {
        let resolved_dir = resolve_filename_lisp_for_eval(eval, newname);
        expand_file_name_lisp(&lisp_file_name_nondirectory(file), Some(&resolved_dir))
    } else {
        resolve_filename_lisp_for_eval(eval, newname)
    }
}

fn expand_and_dir_to_file_lisp_for_eval(
    eval: &Context,
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    let absname = resolve_filename_lisp_for_eval(eval, filename);
    if absname.sbytes() > 1 && lisp_directory_name_p(&absname) {
        lisp_directory_file_name(&absname)
    } else {
        absname
    }
}

fn expand_file_name_lisp_for_file_predicate(
    eval: &mut Context,
    filename: &crate::heap_types::LispString,
) -> Result<crate::heap_types::LispString, Flow> {
    let expanded =
        builtin_expand_file_name(eval, vec![Value::heap_string(filename.clone()), Value::NIL])?;
    expect_lisp_filename_string_strict(&expanded)
}

fn expand_and_dir_to_file_lisp_for_file_predicate(
    eval: &mut Context,
    filename: &crate::heap_types::LispString,
) -> Result<crate::heap_types::LispString, Flow> {
    let absname = expand_file_name_lisp_for_file_predicate(eval, filename)?;
    if absname.sbytes() > 1 && lisp_directory_name_p(&absname) {
        let value = builtin_directory_file_name(eval, vec![Value::heap_string(absname)])?;
        expect_lisp_filename_string_strict(&value)
    } else {
        Ok(absname)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn lisp_files_splice_dirname_file(
    dirname: &crate::heap_types::LispString,
    file: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    if lisp_file_name_absolute_system_p(file) {
        file.clone()
    } else {
        concat_file_name_lisp(dirname, file)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_temp_prefix(value: &Value) -> Result<crate::heap_types::LispString, Flow> {
    match value.kind() {
        ValueKind::String => Ok(value
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload")
            .clone()),
        ValueKind::Nil | ValueKind::Cons | ValueKind::Veclike(VecLikeType::Vector) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *value],
        )),
    }
}

fn normalize_secs_nanos(mut secs: i64, mut nanos: i64) -> (i64, i64) {
    if nanos >= 1_000_000_000 {
        secs += nanos / 1_000_000_000;
        nanos %= 1_000_000_000;
    } else if nanos < 0 {
        let borrow = ((-nanos) + 999_999_999) / 1_000_000_000;
        secs -= borrow;
        nanos += borrow * 1_000_000_000;
    }
    (secs, nanos)
}

fn parse_timestamp_arg(value: &Value) -> Result<(i64, i64), Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok((n, 0)),
        ValueKind::Float => {
            let f = value.as_float().unwrap();
            let secs = f.floor() as i64;
            let nanos = ((f - f.floor()) * 1_000_000_000.0).round() as i64;
            Ok(normalize_secs_nanos(secs, nanos))
        }
        ValueKind::Cons => {
            let items = list_to_vec(value).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), *value],
                )
            })?;
            if items.len() < 2 {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), *value],
                ));
            }
            let high = items[0].as_int().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), items[0]],
                )
            })?;
            let low = items[1].as_int().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), items[1]],
                )
            })?;
            let usec = if items.len() > 2 {
                items[2].as_int().unwrap_or(0)
            } else {
                0
            };
            let secs = high * 65_536 + low;
            let nanos = usec * 1_000;
            Ok(normalize_secs_nanos(secs, nanos))
        }
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("numberp"), *value],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn validate_file_truename_counter(counter: &Value) -> Result<(), Flow> {
    if counter.is_nil() {
        return Ok(());
    }
    if !counter.is_list() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *counter],
        ));
    }
    if counter.is_cons() {
        let first = counter.cons_car();
        // Mirrors GNU `NUMBERP` which accepts bignums in addition
        // to fixnums and floats.
        if !(first.is_number() || first.as_char().is_some()) {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("number-or-marker-p"), first],
            ));
        }
    }
    Ok(())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn temporary_file_directory_for_eval(eval: &Context) -> Option<crate::heap_types::LispString> {
    let val = eval.obarray.symbol_value("temporary-file-directory")?;
    val.as_lisp_string().cloned()
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn make_temp_file_impl(
    eval: &super::eval::Context,
    temp_dir: &crate::heap_types::LispString,
    prefix: &crate::heap_types::LispString,
    dir_flag: bool,
    suffix: &crate::heap_types::LispString,
    text: Option<&[u8]>,
) -> Result<crate::heap_types::LispString, Flow> {
    let absolute_prefix = temp_file_absolute_prefix(temp_dir, prefix);
    make_temp_file_internal_impl(
        eval,
        &absolute_prefix,
        TempCreateKind::from_dir_flag(dir_flag),
        suffix,
        text,
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn temp_file_absolute_prefix(
    temp_dir: &crate::heap_types::LispString,
    prefix: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    // A non-absolute PREFIX is resolved against `temporary-file-directory`;
    // `lisp_file_name_to_path_buf().is_absolute()` mirrors the old
    // `Path::new(prefix).is_absolute()` check while keeping raw bytes intact.
    if lisp_file_name_to_path_buf(prefix).is_absolute() {
        prefix.clone()
    } else {
        lisp_file_name_as_directory(temp_dir).concat(prefix)
    }
}

#[derive(Clone, Copy)]
enum TempCreateKind {
    File,
    Directory,
    NoCreate,
}

impl TempCreateKind {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn from_dir_flag(dir_flag: bool) -> Self {
        if dir_flag {
            Self::Directory
        } else {
            Self::File
        }
    }

    fn error_action(self) -> &'static str {
        match self {
            Self::File => "Creating file with prefix",
            Self::Directory => "Creating directory with prefix",
            Self::NoCreate => "Creating file name with prefix",
        }
    }
}

/// Build the `OpenOptions` used to create a temporary file exclusively.
///
/// GNU's `make-temp-file-internal` (src/fileio.c) calls gnulib `gen_tempname`
/// (lib/tempname.c), whose `try_file` opens with
/// `open(..., O_RDWR | O_CREAT | O_EXCL, S_IRUSR | S_IWUSR)`, i.e. mode 0600
/// before umask.  The docstring guarantees the file is "created with access
/// mode bits that limit access to the current user."  On Unix we replicate the
/// explicit 0600 mode so the resulting file is private (subject to umask, just
/// like GNU); other platforms keep the libstd default.
fn private_temp_file_open_options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

/// Create a temporary directory with GNU's private mode bits.
///
/// gnulib `gen_tempname` `try_dir` calls `mkdir(..., S_IRWXU)`, i.e. mode 0700
/// before umask.  On Unix we mirror that explicit 0700 mode; other platforms
/// fall back to `fs::create_dir`.
fn create_private_temp_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir(path)
    }
}

/// GNU `decode_file_name` (`src/coding.c`): decode FNAME's file-name bytes
/// back to a multibyte string using `file-name-coding-system`, then
/// `default-file-name-coding-system`, then (when both are nil) identity.
/// Returns the decoded multibyte `LispString`.
pub(crate) fn decode_file_name_lisp(
    eval: &super::eval::Context,
    bytes: &[u8],
) -> crate::heap_types::LispString {
    let coding =
        coding_system_value_to_name(&eval.visible_variable_value_or_nil("file-name-coding-system"))
            .or_else(|| {
                coding_system_value_to_name(
                    &eval.visible_variable_value_or_nil("default-file-name-coding-system"),
                )
            });
    let decoded = match coding {
        Some(name) => {
            crate::encoding::decode_bytes_to_lisp_string(bytes, &name, eval.eol_conversion())
        }
        // Both variables nil: GNU returns the name unchanged (unibyte bytes).
        None => crate::heap_types::LispString::from_unibyte(bytes.to_vec()),
    };

    // GNU's `decode_file_name' uses `convert_string_nocopy': when decoding
    // yields a string equal to the original unibyte filename, it returns the
    // original object.  In particular, ASCII directory entries remain
    // unibyte under UTF-8 while a name containing decoded non-ASCII becomes
    // multibyte.  Preserve that representation distinction, not merely the
    // bytes, because Lisp observes it through `multibyte-string-p'.
    if decoded.schars() == bytes.len() && decoded.as_bytes() == bytes {
        crate::heap_types::LispString::from_unibyte(bytes.to_vec())
    } else {
        decoded
    }
}

fn make_temp_file_internal_impl(
    eval: &super::eval::Context,
    prefix: &crate::heap_types::LispString,
    kind: TempCreateKind,
    suffix: &crate::heap_types::LispString,
    text: Option<&[u8]>,
) -> Result<crate::heap_types::LispString, Flow> {
    const TEMP_FILE_ATTEMPTS: usize = 62 * 62 * 62;

    // GNU `Fmake_temp_file_internal` first ENCODE_FILEs PREFIX and SUFFIX, so
    // the on-disk name is built from file-name bytes.  Mirror that: encode both
    // to unibyte file-name bytes, build the candidate, and DECODE_FILE the
    // chosen name before returning so a non-ASCII PREFIX comes back multibyte.
    let file_name_coding =
        coding_system_value_to_name(&eval.visible_variable_value_or_nil("file-name-coding-system"))
            .or_else(|| {
                coding_system_value_to_name(
                    &eval.visible_variable_value_or_nil("default-file-name-coding-system"),
                )
            });
    let eol_conversion = eval.eol_conversion();
    let encode_file = |s: &crate::heap_types::LispString| -> crate::heap_types::LispString {
        match &file_name_coding {
            Some(name) => crate::heap_types::LispString::from_unibyte(
                crate::encoding::encode_lisp_string(s, name, eol_conversion),
            ),
            None => crate::heap_types::LispString::from_unibyte(s.as_bytes().to_vec()),
        }
    };
    let encoded_prefix = encode_file(prefix);
    let encoded_suffix = encode_file(suffix);

    for _ in 0..TEMP_FILE_ATTEMPTS {
        // GNU `Fmake_temp_file_internal` builds PREFIX + "XXXXXX" + SUFFIX as the
        // candidate name; keep the raw file-name bytes intact (the random nonce
        // is ASCII) instead of round-tripping through a UTF-8 String.
        let nonce =
            crate::heap_types::LispString::from_unibyte(make_temp_name_suffix().into_bytes());
        let candidate = encoded_prefix.concat(&nonce).concat(&encoded_suffix);
        let candidate_path = lisp_file_name_to_path_buf(&candidate);
        let candidate_display = crate::emacs_core::emacs_char::to_utf8_lossy(candidate.as_bytes());

        // GNU `val = DECODE_FILE (val)` (fileio.c:809): the returned name is the
        // *decoded* multibyte string, so a non-ASCII PREFIX round-trips back to
        // its char form rather than leaking raw file-name bytes.
        match kind {
            TempCreateKind::Directory => match create_private_temp_dir(&candidate_path) {
                Ok(()) => {
                    return Ok(decode_file_name_lisp(eval, candidate.as_bytes()));
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(signal_file_io_path(
                        err,
                        kind.error_action(),
                        &candidate_display,
                    ));
                }
            },
            TempCreateKind::File => match private_temp_file_open_options().open(&candidate_path) {
                Ok(mut file) => {
                    if let Some(contents) = text {
                        file.write_all(contents).map_err(|err| {
                            signal_file_io_path(err, "Writing to", &candidate_display)
                        })?;
                    }
                    return Ok(decode_file_name_lisp(eval, candidate.as_bytes()));
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(signal_file_io_path(
                        err,
                        kind.error_action(),
                        &candidate_display,
                    ));
                }
            },
            TempCreateKind::NoCreate => match fs::symlink_metadata(&candidate_path) {
                Ok(_) => continue,
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    return Ok(decode_file_name_lisp(eval, candidate.as_bytes()));
                }
                Err(err) => {
                    return Err(signal_file_io_path(
                        err,
                        kind.error_action(),
                        &candidate_display,
                    ));
                }
            },
        }
    }

    Err(signal(
        LispCondition::FileError,
        vec![Value::string("Cannot create temporary file")],
    ))
}

pub(crate) fn builtin_make_temp_file_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("make-temp-file-internal", &args, 4)?;
    let prefix = expect_lisp_string_strict(&args[0])?;
    let suffix = expect_lisp_string_strict(&args[2])?;
    let kind = if args[1].is_nil() {
        TempCreateKind::File
    } else if args[1].as_int() == Some(0) {
        TempCreateKind::NoCreate
    } else {
        TempCreateKind::Directory
    };
    let text = if args[3].is_string() {
        args[3].as_lisp_string().cloned()
    } else {
        None
    };
    let path = make_temp_file_internal_impl(
        eval,
        &prefix,
        kind,
        &suffix,
        text.as_ref().map(|t| t.as_bytes()),
    )?;
    Ok(Value::heap_string(path))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn split_nearby_temp_prefix(
    prefix: &crate::heap_types::LispString,
) -> Option<(crate::heap_types::LispString, crate::heap_types::LispString)> {
    let path = lisp_file_name_to_path_buf(prefix);
    if !path.is_absolute() {
        return None;
    }
    let file_name = path.file_name()?;
    if file_name.is_empty() {
        return None;
    }
    let parent = path.parent()?;
    if parent.as_os_str().is_empty() || parent == Path::new(".") {
        return None;
    }
    Some((
        path_to_lisp_file_name(parent),
        path_to_lisp_file_name(Path::new(file_name)),
    ))
}

fn make_temp_name_suffix() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let nonce = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut value = now ^ nonce.rotate_left(7);
    let mut out = [b'a'; 6];
    for slot in &mut out {
        let idx = (value % ALPHABET.len() as u64) as usize;
        *slot = ALPHABET[idx];
        value = value / ALPHABET.len() as u64 + 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `(expand-file-name NAME &optional DEFAULT-DIRECTORY)` — falls back
/// to dynamic `default-directory` when DEFAULT-DIRECTORY is omitted
/// or nil.
pub(crate) fn builtin_expand_file_name(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("expand-file-name", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("expand-file-name"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let name_lisp = expect_lisp_filename_string_strict(&args[0])?;
    let default_arg = args.get(1).copied().unwrap_or(Value::NIL);
    let operation = Value::symbol("expand-file-name");
    let name_handler = find_file_name_handler_lisp_for_eval(eval, &name_lisp, operation);
    if !name_handler.is_nil() {
        let result = eval.funcall_general(
            name_handler,
            vec![operation, Value::heap_string(name_lisp), default_arg],
        )?;
        return file_name_handler_string_or_error(result);
    }

    let default_dir_value = if let Some(arg) = args.get(1) {
        match arg.kind() {
            ValueKind::Nil => implicit_default_directory_value_for_expand_file_name(eval)?,
            ValueKind::String => *arg,
            _ => Value::heap_string(fallback_root_default_directory()),
        }
    } else {
        implicit_default_directory_value_for_expand_file_name(eval)?
    };
    let default_dir_lisp = expect_lisp_filename_string_strict(&default_dir_value)?;
    // GNU's `Fexpand_file_name` guards this recursion with object identity.
    // The allocator canonicalizes empty strings per storage kind just like GNU,
    // so the ordinary `eq` operation covers the empty-name case too.
    let default_dir_eq_name = eq_value(&default_dir_value, &args[0]);
    let default_handler = find_file_name_handler_lisp_for_eval(eval, &default_dir_lisp, operation);
    if !default_handler.is_nil() {
        let result = eval.funcall_general(
            default_handler,
            vec![operation, Value::heap_string(name_lisp), default_dir_value],
        )?;
        return file_name_handler_string_or_error(result);
    }

    let default_dir_lisp =
        if !lisp_file_name_absolute_system_p(&default_dir_lisp) && !default_dir_eq_name {
            let expanded = builtin_expand_file_name(eval, vec![default_dir_value, Value::NIL])?;
            expect_lisp_filename_string_strict(&expanded)?
        } else {
            default_dir_lisp
        };
    let default_handler = find_file_name_handler_lisp_for_eval(eval, &default_dir_lisp, operation);
    if !default_handler.is_nil() {
        let result = eval.funcall_general(
            default_handler,
            vec![
                operation,
                Value::heap_string(name_lisp),
                Value::heap_string(default_dir_lisp),
            ],
        )?;
        return file_name_handler_string_or_error(result);
    }

    let home_dir = home_directory_for_expand_file_name(eval);
    // Expand on Emacs-internal-encoding bytes so raw eight-bit file-name bytes
    // survive byte-exactly (no storage-String round-trip).
    let result = expand_file_name_bytes_with_home(
        name_lisp.as_bytes(),
        Some(default_dir_lisp.as_bytes()),
        home_dir.as_deref(),
        false,
    );
    let result_multibyte = expand_file_name_result_multibyte(&name_lisp, &default_dir_lisp);
    Ok(Value::heap_string(file_name_lisp_from_bytes(
        result,
        result_multibyte,
    )))
}

/// (make-temp-name PREFIX) -> string
pub(crate) fn builtin_make_temp_name(eval: &super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("make-temp-name", &args, 1)?;
    let prefix = expect_lisp_string_strict(&args[0])?;
    let path = make_temp_file_internal_impl(
        eval,
        &prefix,
        TempCreateKind::NoCreate,
        &empty_file_name_lisp_string(),
        None,
    )?;
    Ok(Value::heap_string(path))
}

/// (next-read-file-uses-dialog-p) -> nil
pub(crate) fn builtin_next_read_file_uses_dialog_p(args: Vec<Value>) -> EvalResult {
    expect_args("next-read-file-uses-dialog-p", &args, 0)?;
    Ok(Value::NIL)
}

/// (unhandled-file-name-directory FILENAME) -> directory string
pub(crate) fn builtin_unhandled_file_name_directory(args: Vec<Value>) -> EvalResult {
    expect_args("unhandled-file-name-directory", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    Ok(Value::heap_string(lisp_file_name_as_directory(&filename)))
}

/// (unhandled-file-name-directory FILENAME) -> directory string
pub(crate) fn builtin_unhandled_file_name_directory_eval(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("unhandled-file-name-directory", &args, 1)?;
    if let Some(result) = dispatch_file_handler(eval, "unhandled-file-name-directory", &args)? {
        return Ok(file_name_handler_string_or_nil(result));
    }
    builtin_unhandled_file_name_directory(args)
}

/// Context-aware variant of `make-temp-file` that honors dynamic
/// `temporary-file-directory`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_make_temp_file(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-temp-file", &args, 1)?;
    if args.len() > 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("make-temp-file"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let prefix = expect_temp_prefix(&args[0])?;
    let dir_flag = args.get(1).is_some_and(|value| value.is_truthy());
    let suffix = match args.get(2) {
        None => empty_file_name_lisp_string(),
        Some(v) if v.is_nil() => empty_file_name_lisp_string(),
        Some(value) => expect_lisp_string_strict(value)?,
    };
    let text = match args.get(3) {
        None => None,
        Some(v) if v.is_nil() => None,
        Some(v) if v.is_string() => v.as_lisp_string().cloned(),
        Some(_) => None,
    };
    let temp_dir = temporary_file_directory_for_eval(eval)
        .unwrap_or_else(|| path_to_lisp_file_name(&std::env::temp_dir()));

    let path = make_temp_file_impl(
        eval,
        &temp_dir,
        &prefix,
        dir_flag,
        &suffix,
        text.as_ref().map(|t| t.as_bytes()),
    )?;
    Ok(Value::heap_string(path))
}

/// Context-aware variant of `make-nearby-temp-file` that resolves relative
/// directory-containing prefixes against dynamic/default `default-directory`
/// and honors dynamic `temporary-file-directory` fallback.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_make_nearby_temp_file(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-nearby-temp-file", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("make-nearby-temp-file"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let prefix = expect_temp_prefix(&args[0])?;
    let dir_flag = args.get(1).is_some_and(|value| value.is_truthy());
    let suffix = match args.get(2) {
        None => empty_file_name_lisp_string(),
        Some(v) if v.is_nil() => empty_file_name_lisp_string(),
        Some(value) => expect_lisp_string_strict(value)?,
    };
    let fallback_temp_dir = temporary_file_directory_for_eval(eval)
        .unwrap_or_else(|| path_to_lisp_file_name(&std::env::temp_dir()));
    let (temp_dir, file_prefix) =
        split_nearby_temp_prefix(&prefix).unwrap_or_else(|| (fallback_temp_dir, prefix.clone()));

    let path = make_temp_file_impl(eval, &temp_dir, &file_prefix, dir_flag, &suffix, None)?;
    Ok(Value::heap_string(path))
}

/// `(file-truename FILENAME)` — resolves FILENAME against
/// dynamic/default `default-directory` and follows symlinks.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_file_truename(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "file-truename", &args)? {
        return Ok(result);
    }
    expect_min_args("file-truename", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("file-truename"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let filename = expect_lisp_string_strict(&args[0])?;
    if let Some(counter) = args.get(1) {
        validate_file_truename_counter(counter)?;
    }

    let default_dir = default_directory_lisp_for_eval(eval);
    Ok(Value::heap_string(file_truename_lisp(
        &filename,
        default_dir.as_ref(),
    )?))
}

/// (file-name-directory FILENAME) -> string or nil
pub(crate) fn builtin_file_name_directory(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "file-name-directory", &args)? {
        return Ok(file_name_handler_string_or_nil(result));
    }
    expect_args("file-name-directory", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    match lisp_file_name_directory(&filename) {
        Some(dir) => Ok(Value::heap_string(dir)),
        None => Ok(Value::NIL),
    }
}

/// (file-name-nondirectory FILENAME) -> string
pub(crate) fn builtin_file_name_nondirectory(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "file-name-nondirectory", &args)? {
        return file_name_handler_string_or_error(result);
    }
    expect_args("file-name-nondirectory", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    Ok(Value::heap_string(lisp_file_name_nondirectory(&filename)))
}

/// (file-name-as-directory FILENAME) -> string
pub(crate) fn builtin_file_name_as_directory(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "file-name-as-directory", &args)? {
        return file_name_handler_string_or_error(result);
    }
    expect_args("file-name-as-directory", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    Ok(Value::heap_string(lisp_file_name_as_directory(&filename)))
}

/// (directory-file-name FILENAME) -> string
pub(crate) fn builtin_directory_file_name(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "directory-file-name", &args)? {
        return file_name_handler_string_or_error(result);
    }
    expect_args("directory-file-name", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    Ok(Value::heap_string(lisp_directory_file_name(&filename)))
}

/// (file-name-concat DIRECTORY &rest COMPONENTS) -> string
pub(crate) fn builtin_file_name_concat(args: Vec<Value>) -> EvalResult {
    expect_min_args("file-name-concat", &args, 1)?;

    let mut parts: Vec<crate::heap_types::LispString> = Vec::new();
    for value in args {
        match value.kind() {
            ValueKind::Nil => {}
            ValueKind::String => {
                let s = value
                    .as_lisp_string()
                    .expect("ValueKind::String must carry LispString payload")
                    .clone();
                if !s.as_bytes().is_empty() {
                    parts.push(s);
                }
            }
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), value],
                ));
            }
        }
    }

    let refs: Vec<&crate::heap_types::LispString> = parts.iter().collect();
    Ok(Value::heap_string(file_name_concat_lisp(&refs)))
}

/// (file-name-absolute-p FILENAME) -> t or nil
pub(crate) fn builtin_file_name_absolute_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-absolute-p", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    Ok(Value::bool_val(file_name_absolute_bytes_p(
        filename.as_bytes(),
    )))
}

/// (directory-name-p NAME) -> t or nil
pub(crate) fn builtin_directory_name_p(args: Vec<Value>) -> EvalResult {
    expect_args("directory-name-p", &args, 1)?;
    let name = expect_lisp_string_strict(&args[0])?;
    Ok(Value::bool_val(name.as_bytes().last().is_some_and(
        |&byte| file_name_directory_separator_byte(byte),
    )))
}

/// (substitute-in-file-name FILENAME) -> string
pub(crate) fn builtin_substitute_in_file_name(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("substitute-in-file-name", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;

    if let Some(result) = dispatch_file_handler(eval, "substitute-in-file-name", &args)? {
        return file_name_handler_string_or_error(result);
    }

    if let Some(idx) = embedded_absfilename_start(filename.as_bytes()) {
        let suffix =
            file_name_lisp_from_bytes(filename.as_bytes()[idx..].to_vec(), filename.is_multibyte());
        return builtin_substitute_in_file_name(eval, vec![Value::heap_string(suffix)]);
    }

    let mut result = args[0];
    if eval
        .obarray()
        .symbol_function("substitute-env-in-file-name")
        .is_some()
    {
        result =
            eval.funcall_general(Value::symbol("substitute-env-in-file-name"), vec![args[0]])?;
        expect_lisp_string_strict(&result)?;
    }

    let result_string = result
        .as_lisp_string()
        .expect("substitute-in-file-name result was checked as a string");
    if let Some(idx) = embedded_absfilename_start(result_string.as_bytes()) {
        let trimmed = trim_embedded_absfilename_bytes(result_string.as_bytes()[idx..].to_vec());
        Ok(Value::heap_string(file_name_lisp_from_bytes(
            trimmed,
            result_string.is_multibyte(),
        )))
    } else {
        Ok(result)
    }
}

/// GNU's `BVAR (current_buffer, directory)` -- `default-directory` is
/// `DEFVAR_PER_BUFFER` (`src/buffer.c:5392`), so there is no global to fall
/// back to. The obarray fallback this used to end with could never fire: the
/// name is installed as a `LispFwdType::BufferObj` forwarder, whose buffer-less
/// `load()` is `None` by construction. Ledger 196.
pub(crate) fn default_directory_lisp_in_state(
    obarray: &Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
) -> Option<crate::heap_types::LispString> {
    obarray
        .value_in_buffer(buffers.current_buffer(), "default-directory")
        .and_then(|val| val.as_lisp_string().cloned())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn default_directory_lisp_for_eval(eval: &Context) -> Option<crate::heap_types::LispString> {
    default_directory_lisp_in_state(&eval.obarray, &[], &eval.buffers)
}

fn raw_default_directory_value_for_eval(eval: &Context) -> Option<Value> {
    eval.obarray
        .value_in_buffer(eval.buffers.current_buffer(), "default-directory")
}

fn invocation_directory_absolute_value_for_eval(eval: &Context) -> Option<Value> {
    let value = eval.obarray.symbol_value("invocation-directory").copied()?;
    let filename = value.as_lisp_string()?;
    if lisp_file_name_absolute_system_p(filename) {
        Some(value)
    } else {
        None
    }
}

fn implicit_default_directory_value_for_expand_file_name(eval: &mut Context) -> EvalResult {
    let Some(value) = raw_default_directory_value_for_eval(eval) else {
        return Ok(Value::heap_string(fallback_root_default_directory()));
    };
    if value.is_nil() {
        return Ok(Value::heap_string(fallback_root_default_directory()));
    }
    let filename = eval.lisp_string(value).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), value],
        )
    })?;
    if lisp_file_name_absolute_system_p(filename) {
        return Ok(value);
    }
    let absdir = invocation_directory_absolute_value_for_eval(eval)
        .unwrap_or_else(|| Value::heap_string(fallback_root_default_directory()));
    builtin_expand_file_name(eval, vec![value, absdir])
}

pub(crate) fn resolve_filename_lisp_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    if lisp_file_name_absolute_system_p(filename) {
        return filename.clone();
    }
    let default_dir = default_directory_lisp_in_state(obarray, dynamic, buffers)
        .unwrap_or_else(fallback_root_default_directory);
    expand_file_name_lisp(filename, Some(&default_dir))
}

pub(crate) fn resolve_filename_lisp_for_eval(
    eval: &Context,
    filename: &crate::heap_types::LispString,
) -> crate::heap_types::LispString {
    resolve_filename_lisp_in_state(&eval.obarray, &[], &eval.buffers, filename)
}

fn file_error_symbol(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound => "file-missing",
        ErrorKind::AlreadyExists => "file-already-exists",
        ErrorKind::PermissionDenied => "permission-denied",
        _ => "file-error",
    }
}

/// The bare `strerror` text for an errno, matching GNU's `emacs_strerror`
/// (e.g. ENOENT -> "No such file or directory").  Rust's
/// `io::Error::to_string()` appends "(os error N)", which GNU never emits, so
/// go through libc `strerror` directly.
#[cfg(unix)]
fn errno_strerror(errno: i32) -> String {
    // SAFETY: `strerror` returns a pointer to a static (per-thread) C string.
    unsafe {
        let ptr = libc::strerror(errno);
        if ptr.is_null() {
            String::new()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }
}

#[cfg(not(unix))]
fn errno_strerror(errno: i32) -> String {
    std::io::Error::from_raw_os_error(errno).to_string()
}

/// Best-effort errno for a `std::io::Error` lacking a raw OS code, used so the
/// errno-keyed branches of [`get_file_errno_data`] still classify correctly.
fn errno_for_kind(kind: ErrorKind) -> i32 {
    match kind {
        ErrorKind::NotFound => libc::ENOENT,
        ErrorKind::AlreadyExists => libc::EEXIST,
        ErrorKind::PermissionDenied => libc::EACCES,
        _ => libc::EIO,
    }
}

/// Faithful port of GNU `get_file_errno_data` + `report_file_errno`
/// (`src/fileio.c`).  Signals a `file-error`-family condition whose DATA
/// matches GNU exactly:
///
/// * `STRERROR` is the bare libc `strerror` text (no Rust "(os error N)").
/// * For `EEXIST` the DATA is `(file-already-exists STRERROR . NAME)` — the
///   ACTION string is *omitted* (GNU's `Fcons (Qfile_already_exists, errdata)`).
/// * Otherwise the DATA is `(SYMBOL ACTION STRERROR . NAME)` where SYMBOL is
///   `file-missing` (ENOENT), `permission-denied` (EACCES), else `file-error`.
///
/// `name_items` holds the flat NAME tail GNU would carry: one filename, or the
/// two-element `(file newname)` list.  Each element is appended verbatim.
fn get_file_errno_data(err: &std::io::Error, action: &str, name_items: Vec<Value>) -> Flow {
    let errno = err
        .raw_os_error()
        .unwrap_or_else(|| errno_for_kind(err.kind()));
    let strerror = errno_strerror(errno);
    if errno == libc::EEXIST {
        // `(file-already-exists STRERROR . NAME)` — no ACTION prefix.
        let mut data = vec![Value::string(strerror)];
        data.extend(name_items);
        signal(LispCondition::FileAlreadyExists, data)
    } else {
        let symbol = match errno {
            libc::ENOENT => "file-missing",
            libc::EACCES => "permission-denied",
            _ => "file-error",
        };
        let mut data = vec![Value::string(action), Value::string(strerror)];
        data.extend(name_items);
        signal(symbol, data)
    }
}

fn signal_file_io_error(err: std::io::Error, context: String) -> Flow {
    let symbol = file_error_symbol(err.kind());
    signal(symbol, vec![Value::string(format!("{context}: {err}"))])
}

fn signal_file_io_path(err: std::io::Error, action: &str, path: &str) -> Flow {
    signal_file_io_error(err, format!("{action} {path}"))
}

fn signal_directory_files_error(
    err: DirectoryFilesError,
    dir: &crate::heap_types::LispString,
) -> Flow {
    match err {
        DirectoryFilesError::Io { action, err } => signal_file_io_path(
            err,
            action,
            &crate::emacs_core::emacs_char::to_utf8_lossy(dir.as_bytes()),
        ),
        DirectoryFilesError::InvalidRegexp(msg) => {
            signal(LispCondition::InvalidRegexp, vec![Value::string(msg)])
        }
    }
}

pub(crate) fn signal_file_action_error_value(
    err: std::io::Error,
    action: &str,
    path: Value,
) -> Flow {
    get_file_errno_data(&err, action, vec![path])
}

fn signal_file_action_error_pair_values(
    err: std::io::Error,
    action: &str,
    left: Value,
    right: Value,
) -> Flow {
    get_file_errno_data(&err, action, vec![left, right])
}

fn signal_existing_path_value(path: &Path, value: Value) -> Flow {
    if fs::symlink_metadata(path)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        signal(
            LispCondition::FileError,
            vec![Value::string("File is a directory"), value],
        )
    } else {
        signal(
            LispCondition::FileAlreadyExists,
            vec![Value::string("File already exists"), value],
        )
    }
}

/// Faithful port of GNU `barf_or_query_if_file_exists` (`src/fileio.c`).
///
/// If FILENAME exists (or `known_to_exist` is set), either signal a
/// `file-already-exists` error or interactively ask the user whether to
/// proceed.  Mirrors the C control flow exactly:
///
/// * A directory at FILENAME always signals `file-error` with
///   "File is a directory".
/// * Non-interactive (`interactive == false`): signal `file-already-exists`
///   with the message "File already exists".
/// * Interactive: format the prompt
///   `"File <name> already exists; <querystring> anyway? "` and dispatch it
///   through `y-or-n-p` (when `quick`) or `yes-or-no-p`.  A negative answer
///   signals `file-already-exists` ("File already exists"); a positive answer
///   returns `Ok(())` so the caller proceeds to overwrite.
///
/// `absname` is the already-expanded filename (GNU calls this with the result
/// of `Fexpand_file_name`).  We keep it as a `LispString` so non-UTF-8
/// filenames are reproduced byte-for-byte in the prompt and the error data.
fn barf_or_query_if_file_exists(
    eval: &mut Context,
    absname: &LispString,
    known_to_exist: bool,
    querystring: &str,
    interactive: bool,
    quick: bool,
) -> Result<(), Flow> {
    let path = lisp_file_name_to_path_buf(absname);
    let absname_value = Value::heap_string(absname.clone());

    // `! known_to_exist && stat(...) == 0` in GNU: probe the file with a
    // non-symlink-following stat so a dangling symlink still counts as
    // existing, and so a directory is reported distinctly.
    let mut exists = known_to_exist;
    if !known_to_exist && let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.is_dir() {
            return Err(signal(
                LispCondition::FileError,
                vec![Value::string("File is a directory"), absname_value],
            ));
        }
        exists = true;
    }

    if !exists {
        return Ok(());
    }

    if !interactive {
        return Err(signal(
            LispCondition::FileAlreadyExists,
            vec![Value::string("File already exists"), absname_value],
        ));
    }

    // GNU: AUTO_STRING (format, "File %s already exists; %s anyway? ")
    //      tem = CALLN (Fformat, format, absname, build_string (querystring));
    // `y-or-n-p` / `yes-or-no-p` append the "(y or n) " / "(yes or no) "
    // suffix themselves, so we only build the leading sentence here.
    let mut prompt_bytes = b"File ".to_vec();
    prompt_bytes.extend_from_slice(absname.as_bytes());
    prompt_bytes.extend_from_slice(b" already exists; ");
    prompt_bytes.extend_from_slice(querystring.as_bytes());
    prompt_bytes.extend_from_slice(b" anyway? ");
    let prompt = if absname.is_multibyte() {
        Value::heap_string(LispString::from_emacs_bytes(prompt_bytes))
    } else {
        Value::heap_string(LispString::from_unibyte(prompt_bytes))
    };

    let answer = if quick {
        eval.apply(Value::symbol("y-or-n-p"), vec![prompt])?
    } else {
        eval.apply(Value::symbol("yes-or-no-p"), vec![prompt])?
    };

    if answer.is_nil() {
        return Err(signal(
            LispCondition::FileAlreadyExists,
            vec![Value::string("File already exists"), absname_value],
        ));
    }

    Ok(())
}

fn maybe_dispatch_resolved_file_handler(
    eval: &mut Context,
    operation_name: &str,
    first_lookup: Option<&crate::heap_types::LispString>,
    second_lookup: Option<&crate::heap_types::LispString>,
    mut call_args: Vec<Value>,
) -> Result<Option<Value>, Flow> {
    let operation_sym = Value::symbol(operation_name);
    if let Some(first) = first_lookup {
        let handler = find_file_name_handler_lisp_for_eval(eval, first, operation_sym);
        if !handler.is_nil() {
            call_args.insert(0, operation_sym);
            return Ok(Some(eval.funcall_general(handler, call_args)?));
        }
    }
    if let Some(second) = second_lookup {
        let handler = find_file_name_handler_lisp_for_eval(eval, second, operation_sym);
        if !handler.is_nil() {
            call_args.insert(0, operation_sym);
            return Ok(Some(eval.funcall_general(handler, call_args)?));
        }
    }
    Ok(None)
}

fn dispatch_expanded_file_handler(
    eval: &mut Context,
    operation_name: &str,
    filename: &crate::heap_types::LispString,
) -> Result<Option<Value>, Flow> {
    maybe_dispatch_resolved_file_handler(
        eval,
        operation_name,
        Some(filename),
        None,
        vec![Value::heap_string(filename.clone())],
    )
}

#[cfg(unix)]
fn path_to_cstring(path: &Path) -> Result<CString, std::ffi::NulError> {
    CString::new(path.as_os_str().as_bytes())
}

fn file_exists_path(path: &Path) -> bool {
    path.exists()
}

fn file_readable_path(path: &Path) -> bool {
    #[cfg(unix)]
    {
        let Ok(c_path) = path_to_cstring(path) else {
            return false;
        };
        unsafe { libc::access(c_path.as_ptr(), libc::R_OK) == 0 }
    }

    #[cfg(not(unix))]
    {
        // GNU's Windows faccessat(R_OK) checks file attributes, so directories
        // are readable even though opening them as regular files fails.
        fs::metadata(path).is_ok()
    }
}

fn file_writable_path(path: &Path) -> bool {
    #[cfg(unix)]
    {
        let Ok(c_path) = path_to_cstring(path) else {
            return false;
        };
        if unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 } {
            return true;
        }
        if std::io::Error::last_os_error().kind() != ErrorKind::NotFound {
            return false;
        }
        let Some(parent) = path.parent() else {
            return false;
        };
        let Ok(c_parent) = path_to_cstring(parent) else {
            return false;
        };
        let mode = libc::W_OK | libc::X_OK;
        unsafe { libc::access(c_parent.as_ptr(), mode) == 0 }
    }

    #[cfg(not(unix))]
    {
        match fs::metadata(path) {
            // GNU's Windows faccessat(W_OK) is an attribute query: content
            // writability is denied only by FILE_ATTRIBUTE_READONLY.  In
            // particular, a directory must not be opened as a regular file.
            Ok(metadata) => !metadata.permissions().readonly(),
            // If FILENAME does not exist, GNU asks whether its parent is a
            // directory.  The parent's read-only attribute does not control
            // whether entries can be created within a Windows directory.
            Err(err) if err.kind() == ErrorKind::NotFound => {
                path.parent().is_some_and(Path::is_dir)
            }
            Err(_) => false,
        }
    }
}

pub(crate) fn file_accessible_directory_path(path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }

    #[cfg(unix)]
    {
        let Ok(c_path) = path_to_cstring(path) else {
            return false;
        };
        let mode = libc::R_OK | libc::X_OK;
        unsafe { libc::access(c_path.as_ptr(), mode) == 0 }
    }

    #[cfg(not(unix))]
    {
        fs::read_dir(path).is_ok()
    }
}

fn file_executable_path(path: &Path) -> bool {
    #[cfg(unix)]
    {
        let Ok(c_path) = path_to_cstring(path) else {
            return false;
        };
        unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
    }

    #[cfg(not(unix))]
    {
        path.exists()
    }
}

fn access_file_path(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let c_path = path_to_cstring(path).map_err(|err| {
            std::io::Error::new(
                ErrorKind::InvalidInput,
                format!("embedded NUL in file name: {err}"),
            )
        })?;
        if unsafe { libc::access(c_path.as_ptr(), libc::R_OK) == 0 } {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    #[cfg(not(unix))]
    {
        fs::File::open(path).map(|_| ())
    }
}

fn file_directory_path(path: &Path) -> bool {
    path.is_dir()
}

fn file_regular_path(path: &Path) -> bool {
    path.is_file()
}

fn file_name_case_insensitive_path(path: &Path) -> bool {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        if !probe.pop() || probe.as_os_str().is_empty() {
            return false;
        }
    }
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn file_modes_path(path: &Path, nofollow: bool) -> Option<u32> {
    let meta = if nofollow {
        fs::symlink_metadata(path).ok()?
    } else {
        fs::metadata(path).ok()?
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Some(meta.permissions().mode() & 0o7777)
    }
    #[cfg(not(unix))]
    {
        Some(if meta.permissions().readonly() {
            0o444
        } else {
            0o644
        })
    }
}

fn set_file_modes_path(path: &Path, mode: i64, nofollow: bool) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if nofollow {
            let c_path = path_to_cstring(path).map_err(|err| {
                std::io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("embedded NUL in file name: {err}"),
                )
            })?;
            let result = unsafe {
                libc::fchmodat(
                    libc::AT_FDCWD,
                    c_path.as_ptr(),
                    (mode as libc::mode_t) & 0o7777,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if result == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        } else {
            fs::set_permissions(path, fs::Permissions::from_mode((mode as u32) & 0o7777))
        }
    }

    #[cfg(not(unix))]
    {
        let _ = nofollow;
        let mut perms = fs::metadata(path)?.permissions();
        let writable = (mode & 0o222) != 0;
        perms.set_readonly(!writable);
        fs::set_permissions(path, perms)
    }
}

fn build_file_times(timestamp: Option<(i64, i64)>) -> std::fs::FileTimes {
    let mut times = std::fs::FileTimes::new();
    let t = if let Some((secs, nanos)) = timestamp {
        std::time::UNIX_EPOCH + std::time::Duration::new(secs as u64, nanos as u32)
    } else {
        std::time::SystemTime::now()
    };
    times = times.set_accessed(t).set_modified(t);
    times
}

fn set_file_times_path(
    path: &Path,
    timestamp: Option<(i64, i64)>,
    nofollow: bool,
) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
        };

        // GNU's w32 utimensat replacement requests metadata write access,
        // not GENERIC_WRITE (src/w32.c:5998-6034).  That distinction lets
        // `set-file-times' work on a read-only file without racing another
        // observer by temporarily changing its attributes.  OpenOptionsExt is
        // a safe wrapper around the same CreateFileW contract.
        let mut flags = FILE_FLAG_BACKUP_SEMANTICS;
        if nofollow {
            flags |= FILE_FLAG_OPEN_REPARSE_POINT;
        }
        let file = fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(flags)
            .open(path)?;
        return file.set_times(build_file_times(timestamp));
    }

    #[cfg(not(windows))]
    if nofollow {
        #[cfg(unix)]
        {
            let c_path = path_to_cstring(path).map_err(|_| {
                std::io::Error::new(ErrorKind::InvalidInput, "embedded NUL in file name")
            })?;

            let mut ts = [
                libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
                libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
            ];
            if let Some((secs, nanos)) = timestamp {
                ts[0].tv_sec = secs as libc::time_t;
                ts[1].tv_sec = secs as libc::time_t;
                ts[0].tv_nsec = nanos as libc::c_long;
                ts[1].tv_nsec = nanos as libc::c_long;
            } else {
                ts[0].tv_nsec = libc::UTIME_NOW as libc::c_long;
                ts[1].tv_nsec = libc::UTIME_NOW as libc::c_long;
            }
            let result = unsafe {
                libc::utimensat(
                    libc::AT_FDCWD,
                    c_path.as_ptr(),
                    ts.as_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if result == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (path, timestamp);
            Err(std::io::Error::new(
                ErrorKind::Unsupported,
                "nofollow set-file-times is unsupported on this platform",
            ))
        }
    } else {
        let file = fs::OpenOptions::new().write(true).open(path)?;
        let times = build_file_times(timestamp);
        file.set_times(times)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn delete_file_compat(filename: &str) -> Result<(), Flow> {
    match delete_file(filename) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(signal_file_io_path(err, "Deleting", filename)),
    }
}

fn delete_file_compat_path(path: &Path, path_value: Value) -> Result<(), Flow> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(signal_file_action_error_value(err, "Deleting", path_value)),
    }
}

/// `(access-file FILENAME STRING)`
pub(crate) fn builtin_access_file(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("access-file", &args, 2)?;
    expect_lisp_string_strict(&args[0])?;
    let expanded = builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let resolved = expect_lisp_filename_string_strict(&expanded)?;
    // The STRING argument is a human-readable operation description (ASCII).
    let operation = expect_lisp_string_strict(&args[1])?;
    let operation = crate::emacs_core::emacs_char::to_utf8_lossy(operation.as_bytes());
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "access-file",
        Some(&resolved),
        None,
        vec![Value::heap_string(resolved.clone()), args[1]],
    )? {
        return Ok(result);
    }
    let path = lisp_file_name_to_path_buf(&resolved);
    match access_file_path(&path) {
        Ok(_) => Ok(Value::NIL),
        Err(err) => Err(signal_file_action_error_value(err, &operation, args[0])),
    }
}

/// Context-aware variant of `file-exists-p` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_file_exists_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-exists-p", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let filename = expand_file_name_lisp_for_file_predicate(eval, &filename)?;
    if let Some(result) = dispatch_expanded_file_handler(eval, "file-exists-p", &filename)? {
        return Ok(result);
    }
    Ok(Value::bool_val(file_exists_path(
        &lisp_file_name_to_path_buf(&filename),
    )))
}

/// `(file-readable-p FILENAME)` — resolves FILENAME against
/// dynamic/default `default-directory`.
pub(crate) fn builtin_file_readable_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-readable-p", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let filename = expand_file_name_lisp_for_file_predicate(eval, &filename)?;
    if let Some(result) = dispatch_expanded_file_handler(eval, "file-readable-p", &filename)? {
        return Ok(result);
    }
    Ok(Value::bool_val(file_readable_path(
        &lisp_file_name_to_path_buf(&filename),
    )))
}

/// `(file-writable-p FILENAME)`
pub(crate) fn builtin_file_writable_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-writable-p", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let filename = expand_file_name_lisp_for_file_predicate(eval, &filename)?;
    if let Some(result) = dispatch_expanded_file_handler(eval, "file-writable-p", &filename)? {
        return Ok(result);
    }
    Ok(Value::bool_val(file_writable_path(
        &lisp_file_name_to_path_buf(&filename),
    )))
}

/// `(file-accessible-directory-p FILENAME)`
pub(crate) fn builtin_file_accessible_directory_p(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("file-accessible-directory-p", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let filename = expand_file_name_lisp_for_file_predicate(eval, &filename)?;
    if let Some(result) =
        dispatch_expanded_file_handler(eval, "file-accessible-directory-p", &filename)?
    {
        return Ok(result);
    }
    Ok(Value::bool_val(file_accessible_directory_path(
        &lisp_file_name_to_path_buf(&filename),
    )))
}

/// `(file-executable-p FILENAME)`
pub(crate) fn builtin_file_executable_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-executable-p", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let filename = expand_file_name_lisp_for_file_predicate(eval, &filename)?;
    if let Some(result) = dispatch_expanded_file_handler(eval, "file-executable-p", &filename)? {
        return Ok(result);
    }
    Ok(Value::bool_val(file_executable_path(
        &lisp_file_name_to_path_buf(&filename),
    )))
}

/// `(file-acl FILENAME)` — return native ACL text, or nil when ACL support is
/// not compiled in.
///
/// GNU `src/fileio.c:Ffile_acl` puts filename expansion and file-name-handler
/// dispatch inside `#if USE_ACL`.  In a no-ACL build, the function only has the
/// DEFUN arity contract and then returns nil; it does not type-check FILENAME
/// and does not dispatch file-name handlers.  Neomacs currently has no native
/// ACL backend, so mirror that no-ACL build path exactly.
pub(crate) fn builtin_file_acl(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-acl", &args, 1)?;
    let _ = eval;
    let _ = &args[0];
    Ok(Value::NIL)
}

/// (set-file-acl FILENAME ACL) -> nil
pub(crate) fn builtin_set_file_acl(args: Vec<Value>) -> EvalResult {
    expect_args("set-file-acl", &args, 2)?;
    let _filename = &args[0];
    let _acl = &args[1];
    Ok(Value::NIL)
}

/// `(file-locked-p FILENAME)`
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_file_locked_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "file-locked-p", &args)? {
        return Ok(result);
    }
    expect_args("file-locked-p", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let filename = resolve_filename_lisp_for_eval(eval, &filename);
    Ok(Value::bool_val(file_locked_p(&filename)))
}

/// `(file-selinux-context FILENAME)` — stub returning a four-element
/// nil list, matching GNU's "no SELinux on this system" shape.
pub(crate) fn builtin_file_selinux_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "file-selinux-context", &args)? {
        return Ok(result);
    }
    expect_args("file-selinux-context", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let _filename = resolve_filename_lisp_for_eval(eval, &filename);
    Ok(Value::list(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]))
}

/// `(set-file-selinux-context FILENAME CONTEXT)` — set a file SELinux context,
/// or return nil when SELinux support is unavailable.
///
/// GNU `src/fileio.c:Fset_file_selinux_context` expands FILENAME and dispatches
/// file-name handlers before the `HAVE_LIBSELINUX` implementation block.  In a
/// no-SELinux build, native local files fall through to nil, but handlers still
/// observe the expanded filename and original context argument.
pub(crate) fn builtin_set_file_selinux_context(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("set-file-selinux-context", &args, 2)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let absname = builtin_expand_file_name(eval, vec![Value::heap_string(filename), Value::NIL])?;
    let absname = expect_lisp_filename_string_strict(&absname)?;
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "set-file-selinux-context",
        Some(&absname),
        None,
        vec![Value::heap_string(absname.clone()), args[1]],
    )? {
        return Ok(result);
    }
    Ok(Value::NIL)
}

/// `(file-system-info FILENAME)`
pub(crate) fn builtin_file_system_info(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "file-system-info", &args)? {
        return Ok(result);
    }
    expect_args("file-system-info", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let filename = resolve_filename_lisp_for_eval(eval, &filename);
    let (total, free, avail) = file_system_info_path(&lisp_file_name_to_path_buf(&filename))
        .map_err(|err| {
            signal_file_action_error_value(
                err,
                "Getting file system info",
                Value::heap_string(filename),
            )
        })?;
    Ok(Value::list(vec![
        Value::fixnum(total),
        Value::fixnum(free),
        Value::fixnum(avail),
    ]))
}

/// `(file-directory-p FILENAME)`
pub(crate) fn builtin_file_directory_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-directory-p", &args, 1)?;
    let filename = expand_and_dir_to_file_lisp_for_file_predicate(
        eval,
        &expect_lisp_string_strict(&args[0])?,
    )?;
    if let Some(result) = dispatch_expanded_file_handler(eval, "file-directory-p", &filename)? {
        return Ok(result);
    }
    Ok(Value::bool_val(file_directory_path(
        &lisp_file_name_to_path_buf(&filename),
    )))
}

/// Context-aware variant of `file-regular-p` that resolves relative paths
/// against dynamic/default `default-directory`.
/// `(file-regular-p FILENAME)`
pub(crate) fn builtin_file_regular_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-regular-p", &args, 1)?;
    let filename = expand_and_dir_to_file_lisp_for_file_predicate(
        eval,
        &expect_lisp_string_strict(&args[0])?,
    )?;
    if let Some(result) = dispatch_expanded_file_handler(eval, "file-regular-p", &filename)? {
        return Ok(result);
    }
    Ok(Value::bool_val(file_regular_path(
        &lisp_file_name_to_path_buf(&filename),
    )))
}

/// `(file-symlink-p FILENAME)`
///
/// Mirrors GNU `Ffile_symlink_p` (`src/fileio.c:3160`): returns the
/// link target as a string when FILENAME is a symbolic link, nil
/// otherwise. Previously this returned `Value::bool_val(...)` (audit
/// §10.3) which was a data-type bug — code that uses the result as a
/// path was always broken because it got `t` instead of a string.
pub(crate) fn builtin_file_symlink_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-symlink-p", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let filename = expand_file_name_lisp_for_file_predicate(eval, &filename)?;
    if let Some(result) = dispatch_expanded_file_handler(eval, "file-symlink-p", &filename)? {
        return Ok(result);
    }
    Ok(match file_symlink_target_lisp(&filename) {
        Some(target) => Value::heap_string(target),
        None => Value::NIL,
    })
}

/// `(file-name-case-insensitive-p FILENAME)`
pub(crate) fn builtin_file_name_case_insensitive_p(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("file-name-case-insensitive-p", &args, 1)?;
    expect_lisp_string_strict(&args[0])?;
    let expanded = builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let filename = expect_lisp_filename_string_strict(&expanded)?;
    if let Some(result) =
        dispatch_expanded_file_handler(eval, "file-name-case-insensitive-p", &filename)?
    {
        return Ok(result);
    }
    Ok(Value::bool_val(file_name_case_insensitive_path(
        &lisp_file_name_to_path_buf(&filename),
    )))
}

/// `(file-newer-than-file-p FILE1 FILE2)`
pub(crate) fn builtin_file_newer_than_file_p(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("file-newer-than-file-p", &args, 2)?;
    let file1 = expand_and_dir_to_file_lisp_for_eval(eval, &expect_lisp_string_strict(&args[0])?);
    let file2 = expand_and_dir_to_file_lisp_for_eval(eval, &expect_lisp_string_strict(&args[1])?);
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "file-newer-than-file-p",
        Some(&file1),
        Some(&file2),
        vec![
            Value::heap_string(file1.clone()),
            Value::heap_string(file2.clone()),
        ],
    )? {
        return Ok(result);
    }
    Ok(Value::bool_val(file_newer_than_file_path(
        &lisp_file_name_to_path_buf(&file1),
        &lisp_file_name_to_path_buf(&file2),
    )))
}

/// `(file-modes FILENAME &optional FLAG)` — returns the file's
/// mode bits as an integer, or nil if FILENAME is missing.
pub(crate) fn builtin_file_modes(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("file-modes", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("file-modes"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let filename = expect_lisp_string_strict(&args[0])?;
    let flag = args.get(1).copied().unwrap_or(Value::NIL);
    let nofollow = !flag.is_nil();
    let absname = expand_and_dir_to_file_lisp_for_file_predicate(eval, &filename)?;
    let operation = Value::symbol("file-modes");
    let handler = find_file_name_handler_lisp_for_eval(eval, &absname, operation);
    if !handler.is_nil() {
        return eval.funcall_general(handler, vec![operation, Value::heap_string(absname), flag]);
    }
    match file_modes_path(&lisp_file_name_to_path_buf(&absname), nofollow) {
        Some(mode) => Ok(Value::fixnum(mode as i64)),
        None => Ok(Value::NIL),
    }
}

/// `(set-file-modes FILENAME MODE &optional FLAG)`
pub(crate) fn builtin_set_file_modes(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("set-file-modes", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-file-modes"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let mode = expect_fixnum(&args[1])?;
    let flag = args.get(2).copied().unwrap_or(Value::NIL);
    let nofollow = !flag.is_nil();
    let expanded = builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let absname = expect_lisp_filename_string_strict(&expanded)?;
    let operation = Value::symbol("set-file-modes");
    let handler = find_file_name_handler_lisp_for_eval(eval, &absname, operation);
    if !handler.is_nil() {
        return eval.funcall_general(
            handler,
            vec![operation, Value::heap_string(absname), args[1], flag],
        );
    }
    set_file_modes_path(&lisp_file_name_to_path_buf(&absname), mode, nofollow).map_err(|err| {
        signal_file_action_error_value(err, "Doing chmod", Value::heap_string(absname))
    })?;
    Ok(Value::NIL)
}

/// `(set-file-times FILENAME &optional TIME FLAG)`
pub(crate) fn builtin_set_file_times(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("set-file-times", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("set-file-times"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let nofollow = args.get(2).is_some_and(|flag| !flag.is_nil());
    let timestamp_arg = args.get(1).copied().unwrap_or(Value::NIL);
    let flag_arg = args.get(2).copied().unwrap_or(Value::NIL);
    let timestamp = if !timestamp_arg.is_nil() {
        Some(parse_timestamp_arg(&timestamp_arg)?)
    } else {
        None
    };
    let filename = builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let filename = expect_lisp_filename_string_strict(&filename)?;
    let handler_args = vec![
        Value::heap_string(filename.clone()),
        timestamp_arg,
        flag_arg,
    ];
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "set-file-times",
        Some(&filename),
        None,
        handler_args,
    )? {
        return Ok(result);
    }
    set_file_times_path(&lisp_file_name_to_path_buf(&filename), timestamp, nofollow).map_err(
        |err| {
            signal_file_action_error_value(err, "Setting file times", Value::heap_string(filename))
        },
    )?;
    Ok(Value::T)
}

/// GNU `decode_buffer` (src/buffer.c): nil (or an omitted argument) means the
/// current buffer, anything else must be a live buffer.
///
/// This returns the decided [`crate::buffer::BufferId`] rather than merely
/// validating the argument: a validate-only signature lets a caller type-check
/// BUF and then silently operate on the current buffer instead, which is
/// exactly how `verify-visited-file-modtime` came to ignore its argument.
fn decode_buffer_arg_in_state(
    buffers: &crate::buffer::BufferManager,
    arg: Option<&Value>,
) -> Result<crate::buffer::BufferId, Flow> {
    let wrong_type = |bufferish: &Value| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), *bufferish],
        )
    };
    match arg {
        None => current_buffer_id_or_error(buffers),
        Some(bufferish) => match bufferish.kind() {
            ValueKind::Nil => current_buffer_id_or_error(buffers),
            ValueKind::Veclike(VecLikeType::Buffer) => bufferish
                .as_buffer_id()
                .filter(|id| buffers.get(*id).is_some())
                .ok_or_else(|| wrong_type(bufferish)),
            _ => Err(wrong_type(bufferish)),
        },
    }
}

fn validate_set_visited_file_modtime_arg(arg: &Value) -> Result<(), Flow> {
    match arg.kind() {
        // GNU Lisp uses `(set-visited-file-modtime 0)` via
        // `clear-visited-file-modtime` during `set-visited-file-name`
        // and `write-file`, so integer flags are valid inputs here.
        ValueKind::Fixnum(_) => Ok(()),
        ValueKind::String => Err(signal(
            "error",
            vec![Value::string("Invalid time specification")],
        )),
        ValueKind::Float | ValueKind::Cons => Ok(()),
        _ => Err(signal(
            "error",
            vec![Value::string("Invalid time specification")],
        )),
    }
}

/// `(visited-file-modtime)` — return the buffer's recorded modtime.
pub(crate) fn builtin_visited_file_modtime(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("visited-file-modtime", &args, 0)?;
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    // GNU `Fvisited_file_modtime` is exactly
    // `buffer_visited_file_modtime (current_buffer)` (`src/fileio.c:6165-6175`),
    // the same function `record_first_change` stores in its `(t . TIME)` undo
    // entry -- so the two must stay one implementation or `primitive-undo`
    // could never match them.
    Ok(buf.visited_file_modtime_value())
}

/// `(verify-visited-file-modtime &optional BUFFER)` — check if file
/// on disk matches the buffer's recorded modtime.
pub(crate) fn builtin_verify_visited_file_modtime(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("verify-visited-file-modtime", &args, 1)?;
    // GNU fileio.c:6129 `decode_buffer (buf)`: BUF, not the current buffer,
    // is the one whose recorded modtime is compared.  filelock.c:605 passes
    // the buffer that visits the file being locked, which need not be current.
    let buffer_id = decode_buffer_arg_in_state(&eval.buffers, args.first())?;
    let buf = eval
        .buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let recorded = buf.visited_file_modtime();
    // GNU fileio.c:6135: `if (!STRINGP (BVAR (b, filename))) return Qt;` — a
    // buffer that visits no file always matches.
    let Some(file_name) = buf.file_name_lisp_string() else {
        return Ok(Value::T);
    };
    // GNU fileio.c:6136: `if (b->modtime.tv_nsec == UNKNOWN_MODTIME_NSECS)
    // return Qt;` — a buffer with no recorded time never complains.
    if recorded == VisitedFileModtime::Unknown {
        return Ok(Value::T);
    }
    // GNU fileio.c:6138-6143: the handler is asked BEFORE the stat, on the
    // buffer's own `BVAR (b, filename)` rather than an expanded copy, and its
    // answer is returned verbatim.  That order is what makes the primitive
    // usable at all for a remote buffer: the visited name is not a path on this
    // filesystem, so the `emacs_fstatat` below can only fail, and the buffer
    // would report "changed" from the moment it was visited.  TRAMP's own
    // answer is `tramp-handle-verify-visited-file-modtime`
    // (lisp/net/tramp.el:5938-5967), which compares the recorded time against
    // the REMOTE attributes with a two-second tolerance.
    let operation = Value::symbol("verify-visited-file-modtime");
    let handler = find_file_name_handler_lisp_for_eval(eval, &file_name, operation);
    if !handler.is_nil() {
        return eval.funcall_general(handler, vec![operation, Value::make_buffer(buffer_id)]);
    }
    // Issue #131: encode the visited file name to its real OS path bytes via the
    // filesystem-boundary helper, not the PUA-sentinel storage string, so a
    // non-UTF-8 file name still stats the right file.
    let path = lisp_file_name_to_path_buf(file_name);
    // GNU fileio.c:6145-6148: the disk side is `get_stat_mtime` when the stat
    // succeeds and `time_error_value (errno)` when it does not — so a buffer
    // that recorded "this file does not exist" still matches while the file is
    // still missing, and stops matching the moment it appears.
    let (disk_modtime, disk_size) = match std::fs::metadata(&path) {
        Ok(meta) => {
            let modtime = match meta.modified() {
                Ok(mtime) => {
                    let dur = mtime
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default();
                    VisitedFileModtime::Known {
                        sec: dur.as_secs() as i64,
                        nsec: dur.subsec_nanos() as i32,
                    }
                }
                Err(_) => VisitedFileModtime::Unknown,
            };
            (modtime, Some(meta.len() as i64))
        }
        Err(err) => (VisitedFileModtime::from_open_error(&err), None),
    };
    // GNU fileio.c:6149-6153: `timespec_cmp (mtime, b->modtime) == 0
    // && (b->modtime_size < 0 || st.st_size == b->modtime_size)`.  An
    // unrecorded size means "do not check the size", not "the size is -1".
    let size_matches = match (buf.modtime_size, disk_size) {
        (None, _) => true,
        (Some(recorded_size), Some(size)) => recorded_size == size,
        (Some(_), None) => false,
    };
    Ok(Value::bool(disk_modtime == recorded && size_matches))
}

/// `(set-visited-file-modtime &optional TIME-LIST)` — set buffer's
/// modtime from the file on disk or from explicit timestamp.
pub(crate) fn builtin_set_visited_file_modtime(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("set-visited-file-modtime", &args, 1)?;

    if let Some(arg) = args.first()
        && !arg.is_nil()
    {
        // Explicit timestamp argument.
        validate_set_visited_file_modtime_arg(arg)?;
        // GNU `Fset_visited_file_modtime' (src/fileio.c:6188-6198): an integer
        // is a "flag", and the accepted flags are exactly the two values
        // `visited-file-modtime' can return that are not timestamps --
        // `check_integer_range (time_flag, -1, 0)` then
        // `make_timespec (0, UNKNOWN_MODTIME_NSECS - flag)`.  0 is the unknown
        // sentinel (this is how `clear-visited-file-modtime' works) and -1 is
        // "the visited file does not exist"; anything else is out of range.
        // Any other time form is decoded (including the 4-element
        // (HIGH LOW USEC PSEC) list) into one (sec, nsec) via
        // `lisp_time_argument', rather than taking elements 0 and 1 as raw
        // seconds and nanoseconds.
        let modtime = match arg.as_fixnum() {
            Some(flag) => VisitedFileModtime::from_lisp_flag(flag).ok_or_else(|| {
                signal(
                    LispCondition::ArgsOutOfRange,
                    vec![*arg, Value::fixnum(-1), Value::fixnum(0)],
                )
            })?,
            None => {
                let (sec, nsec) = crate::emacs_core::timefns::time_value_seconds_and_nanos(arg)?;
                VisitedFileModtime::Known {
                    sec,
                    nsec: nsec as i32,
                }
            }
        };
        let buf = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.set_visited_file_modtime(modtime);
        // GNU: `current_buffer->modtime_size = -1;` — an explicitly given time
        // says nothing about the file's size.
        buf.modtime_size = None;
        return Ok(Value::NIL);
    }

    // No arg — stat the visited file.  GNU (src/fileio.c:6202-6203) refuses
    // here rather than expanding a nil file name: an indirect buffer visits no
    // file of its own, and this error is half of the same change that made
    // `record_first_change' read the BASE buffer's modtime (Bug#56397).
    let current_buffer_is_indirect = eval
        .buffers
        .current_buffer()
        .is_some_and(|buf| buf.base_buffer.is_some());
    if current_buffer_is_indirect {
        return Err(signal(
            "error",
            vec![Value::string(
                "An indirect buffer does not have a visited file",
            )],
        ));
    }
    let file_name = eval
        .buffers
        .current_buffer()
        .and_then(|b| b.file_name_lisp_string());
    let Some(file_name) = file_name else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), Value::NIL],
        ));
    };
    // GNU fileio.c:6209-6216: expand the buffer's file name, then ask the
    // handler BEFORE stat'ing, and return its answer -- "the handler can find
    // the file name the same way we did".  This is the writing half of the same
    // rule `verify-visited-file-modtime` obeys above, and a remote buffer needs
    // both: only the handler can reach the file, so without this the buffer
    // keeps whatever `insert-file-contents` left it -- here the wall clock at
    // visit time.  The gap between that and the file's own mtime grows with
    // however long the connection took to open, and once it exceeds TRAMP's
    // two-second tolerance (lisp/net/tramp.el:5962) the buffer reports
    // "changed" and the first edit dies in
    // `ask-user-about-supersession-threat`.
    let expanded = builtin_expand_file_name(
        eval,
        vec![Value::heap_string(file_name.clone()), Value::NIL],
    )?;
    let expanded = expect_lisp_filename_string_strict(&expanded)?;
    let operation = Value::symbol("set-visited-file-modtime");
    let handler = find_file_name_handler_lisp_for_eval(eval, &expanded, operation);
    if !handler.is_nil() {
        return eval.funcall_general(handler, vec![operation, Value::NIL]);
    }
    let path = lisp_file_name_to_path_buf(&expanded);
    if let Ok(meta) = std::fs::metadata(&path) {
        let buf = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        if let Ok(mtime) = meta.modified() {
            let dur = mtime
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            buf.set_visited_file_modtime(VisitedFileModtime::Known {
                sec: dur.as_secs() as i64,
                nsec: dur.subsec_nanos() as i32,
            });
            buf.modtime_size = Some(meta.len() as i64);
        }
    }
    Ok(Value::NIL)
}

/// (set-default-file-modes MODE) -> nil
pub(crate) fn builtin_set_default_file_modes(args: Vec<Value>) -> EvalResult {
    expect_args("set-default-file-modes", &args, 1)?;
    init_default_file_mode_mask();
    let mode = expect_fixnum(&args[0])?;
    let new_mask = (!mode) & 0o777;
    #[cfg(unix)]
    unsafe {
        libc::umask(new_mask as libc::mode_t);
    }
    DEFAULT_FILE_MODE_MASK.store(new_mask as u32, Ordering::Relaxed);
    Ok(Value::NIL)
}

/// (default-file-modes) -> integer
pub(crate) fn builtin_default_file_modes(args: Vec<Value>) -> EvalResult {
    expect_args("default-file-modes", &args, 0)?;
    init_default_file_mode_mask();
    let mask = DEFAULT_FILE_MODE_MASK.load(Ordering::Relaxed) as i64;
    Ok(Value::fixnum((!mask) & 0o777))
}

/// Context-aware variant of `delete-file` that resolves relative paths
/// against dynamic/default `default-directory`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_delete_file(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("delete-file", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("delete-file"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    expect_lisp_string_strict(&args[0])?;
    let resolved = match expand_file_operation(eval, "delete-file", &args, 2)? {
        ExpandedFileOperation::Handled(result) => return Ok(result),
        ExpandedFileOperation::Local { expanded_filename } => {
            expect_lisp_filename_string_strict(&expanded_filename)?
        }
    };
    delete_file_compat_path(
        &lisp_file_name_to_path_buf(&resolved),
        Value::heap_string(resolved),
    )?;
    Ok(Value::NIL)
}

/// `(delete-file-internal FILENAME)` — internal primitive used by
/// the elisp `delete-file` wrapper.
pub(crate) fn builtin_delete_file_internal(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "delete-file", &args)? {
        return Ok(result);
    }
    expect_args("delete-file-internal", &args, 1)?;
    let filename = expect_lisp_string_strict(&args[0])?;
    let resolved = resolve_filename_lisp_for_eval(eval, &filename);
    delete_file_compat_path(
        &lisp_file_name_to_path_buf(&resolved),
        Value::heap_string(resolved),
    )?;
    Ok(Value::NIL)
}

/// `(delete-directory-internal DIRECTORY)`
pub(crate) fn builtin_delete_directory_internal(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("delete-directory-internal", &args, 1)?;
    let directory = expect_lisp_string_strict(&args[0])?;
    let resolved = lisp_directory_file_name(&resolve_filename_lisp_for_eval(eval, &directory));
    fs::remove_dir(lisp_file_name_to_path_buf(&resolved)).map_err(|err| {
        signal_file_action_error_value(err, "Removing directory", Value::heap_string(resolved))
    })?;
    Ok(Value::NIL)
}

/// Context-aware variant of `delete-directory` that resolves relative paths
/// against dynamic/default `default-directory`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_delete_directory(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "delete-directory", &args)? {
        return Ok(result);
    }
    expect_min_args("delete-directory", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("delete-directory"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let directory = expect_lisp_string_strict(&args[0])?;
    let directory = lisp_directory_file_name(&resolve_filename_lisp_for_eval(eval, &directory));
    let recursive = args.get(1).is_some_and(|value| value.is_truthy());
    let result = if recursive {
        fs::remove_dir_all(lisp_file_name_to_path_buf(&directory))
    } else {
        fs::remove_dir(lisp_file_name_to_path_buf(&directory))
    };
    result.map_err(|err| {
        signal_file_action_error_value(err, "Removing directory", Value::heap_string(directory))
    })?;
    Ok(Value::NIL)
}

/// `(make-symbolic-link TARGET LINKNAME &optional OK-IF-EXISTS)`
pub(crate) fn builtin_make_symbolic_link(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-symbolic-link", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("make-symbolic-link"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let mut target = expect_lisp_string_strict(&args[0])?;
    if matches!(
        args.get(2).map(|value| value.kind()),
        Some(ValueKind::Fixnum(_))
    ) {
        if lisp_file_name_is_ascii_text(&target, b"~") || target.as_bytes().starts_with(b"~/") {
            target = expand_file_name_lisp(&target, None);
        } else if let Some(stripped) = lisp_string_strip_ascii_prefix(&target, b"/:") {
            target = stripped;
        }
    }
    let linkname_arg = expect_lisp_string_strict(&args[1])?;
    let linkname = expand_cp_target_lisp_for_eval(eval, &target, &linkname_arg);
    let mut handler_args = Vec::with_capacity(args.len());
    handler_args.push(Value::heap_string(target.clone()));
    handler_args.push(Value::heap_string(linkname.clone()));
    if let Some(extra) = args.get(2) {
        handler_args.push(*extra);
    }
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "make-symbolic-link",
        None,
        Some(&linkname),
        handler_args,
    )? {
        return Ok(result);
    }
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    let link_path = lisp_file_name_to_path_buf(&linkname);

    #[cfg(unix)]
    {
        if fs::symlink_metadata(&link_path).is_ok() {
            if !ok_if_exists {
                return Err(signal_existing_path_value(
                    &link_path,
                    Value::heap_string(linkname.clone()),
                ));
            }
            fs::remove_file(&link_path).map_err(|err| {
                signal_file_action_error_value(
                    err,
                    "Removing old name",
                    Value::heap_string(linkname.clone()),
                )
            })?;
        }
        std::os::unix::fs::symlink(lisp_file_name_to_path_buf(&target), &link_path).map_err(
            |err| {
                signal_file_action_error_pair_values(
                    err,
                    "Making symbolic link",
                    Value::heap_string(target),
                    Value::heap_string(linkname),
                )
            },
        )?;
        Ok(Value::NIL)
    }

    #[cfg(not(unix))]
    {
        let _ = (target, linkname, ok_if_exists);
        Err(signal(
            LispCondition::FileError,
            vec![Value::string(
                "Symbolic links are unsupported on this platform",
            )],
        ))
    }
}

/// `(rename-file FROM TO &optional OK-IF-EXISTS)`
pub(crate) fn builtin_rename_file(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("rename-file", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("rename-file"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    // GNU `Frename_file` expands FROM, applies `directory-file-name` to it (both
    // dispatch their magic handlers), then expands NEWNAME, before the
    // rename-file handler.
    let from_expanded = builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let from = resolve_filename_lisp_for_eval(eval, &expect_lisp_string_strict(&from_expanded)?);
    let from_dfn = builtin_directory_file_name(eval, vec![Value::heap_string(from.clone())])?;
    let to_expanded = builtin_expand_file_name(eval, vec![args[1], Value::NIL])?;
    let to = expand_cp_target_lisp_for_eval(
        eval,
        &expect_lisp_string_strict(&from_dfn)?,
        &expect_lisp_string_strict(&to_expanded)?,
    );
    let mut handler_args = Vec::with_capacity(3);
    handler_args.push(Value::heap_string(from.clone()));
    handler_args.push(Value::heap_string(to.clone()));
    if let Some(extra) = args.get(2) {
        handler_args.push(*extra);
    }
    // rename-file arity 3 (FILE NEWNAME OK-IF-ALREADY-EXISTS).
    while handler_args.len() < 3 {
        handler_args.push(Value::NIL);
    }
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "rename-file",
        Some(&from),
        Some(&to),
        handler_args,
    )? {
        return Ok(result);
    }
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    let to_path = lisp_file_name_to_path_buf(&to);
    if fs::symlink_metadata(&to_path).is_ok() && !ok_if_exists {
        return Err(signal_existing_path_value(
            &to_path,
            Value::heap_string(to.clone()),
        ));
    }
    let from_path = lisp_file_name_to_path_buf(&from);
    rename_path_with_cross_device_fallback(&from_path, &to_path, ok_if_exists, |from, to| {
        fs::rename(from, to)
    })
    .map_err(|err| {
        signal_file_action_error_pair_values(
            err,
            "Renaming",
            Value::heap_string(from),
            Value::heap_string(to),
        )
    })?;
    Ok(Value::NIL)
}

/// `(copy-file FROM TO &optional OK-IF-EXISTS KEEP-TIME PRESERVE-UID-GID PRESERVE-PERMISSIONS)`
pub(crate) fn builtin_copy_file(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("copy-file", &args, 2)?;
    if args.len() > 6 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("copy-file"), Value::fixnum(args.len() as i64)],
        ));
    }
    // GNU `Fcopy_file` runs `Fexpand_file_name` on both FROM and TO first,
    // dispatching the expand-file-name magic handler before the copy-file one.
    let from_expanded = builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let to_expanded = builtin_expand_file_name(eval, vec![args[1], Value::NIL])?;
    let from = resolve_filename_lisp_for_eval(eval, &expect_lisp_string_strict(&from_expanded)?);
    let to = expand_cp_target_lisp_for_eval(eval, &from, &expect_lisp_string_strict(&to_expanded)?);
    let mut handler_args = Vec::with_capacity(6);
    handler_args.push(Value::heap_string(from.clone()));
    handler_args.push(Value::heap_string(to.clone()));
    handler_args.extend_from_slice(&args[2..]);
    // copy-file arity 6 (FROM TO OK-IF-EXISTS KEEP-TIME PRESERVE-UID-GID PRESERVE-PERMISSIONS).
    while handler_args.len() < 6 {
        handler_args.push(Value::NIL);
    }
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "copy-file",
        Some(&from),
        Some(&to),
        handler_args,
    )? {
        return Ok(result);
    }
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    let from_path = lisp_file_name_to_path_buf(&from);
    let to_path = lisp_file_name_to_path_buf(&to);

    // GNU opens the input first; a failure here is reported as
    // `report_file_error ("Opening input file", file)` — only the source
    // filename, before the destination is even considered (fileio.c:2346).
    let from_meta = fs::metadata(&from_path).map_err(|err| {
        signal_file_action_error_value(err, "Opening input file", Value::heap_string(from.clone()))
    })?;

    let dest_exists = fs::symlink_metadata(&to_path).is_ok();
    if dest_exists && !ok_if_exists {
        return Err(signal_existing_path_value(
            &to_path,
            Value::heap_string(to.clone()),
        ));
    }

    // GNU's `already_exists` path: after reopening the destination it compares
    // the input/output `st_dev`+`st_ino` and, when equal, signals
    // `report_file_errno ("Input and output files are the same",
    //  list2 (file, newname), 0)` — errno 0, so strerror is "Success"
    // (fileio.c:2401).
    #[cfg(unix)]
    if dest_exists {
        use std::os::unix::fs::MetadataExt;
        if let Ok(to_meta) = fs::metadata(&to_path)
            && from_meta.dev() == to_meta.dev()
            && from_meta.ino() == to_meta.ino()
        {
            return Err(get_file_errno_data(
                &std::io::Error::from_raw_os_error(0),
                "Input and output files are the same",
                vec![
                    Value::heap_string(from.clone()),
                    Value::heap_string(to.clone()),
                ],
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = &from_meta;

    fs::copy(&from_path, &to_path).map_err(|err| {
        signal_file_action_error_pair_values(
            err,
            "Copying",
            Value::heap_string(from),
            Value::heap_string(to),
        )
    })?;
    Ok(Value::NIL)
}

/// `(add-name-to-file OLDNAME NEWNAME &optional OK-IF-EXISTS)`
pub(crate) fn builtin_add_name_to_file(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("add-name-to-file", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("add-name-to-file"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let oldname = resolve_filename_lisp_for_eval(eval, &expect_lisp_string_strict(&args[0])?);
    let newname =
        expand_cp_target_lisp_for_eval(eval, &oldname, &expect_lisp_string_strict(&args[1])?);
    let mut handler_args = Vec::with_capacity(args.len());
    handler_args.push(Value::heap_string(oldname.clone()));
    handler_args.push(Value::heap_string(newname.clone()));
    if let Some(extra) = args.get(2) {
        handler_args.push(*extra);
    }
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "add-name-to-file",
        Some(&oldname),
        Some(&newname),
        handler_args,
    )? {
        return Ok(result);
    }
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    let newname_path = lisp_file_name_to_path_buf(&newname);
    if fs::symlink_metadata(&newname_path).is_ok() {
        if !ok_if_exists {
            return Err(signal_existing_path_value(
                &newname_path,
                Value::heap_string(newname.clone()),
            ));
        }
        fs::remove_file(&newname_path).map_err(|err| {
            signal_file_action_error_value(
                err,
                "Removing old name",
                Value::heap_string(newname.clone()),
            )
        })?;
    }
    fs::hard_link(lisp_file_name_to_path_buf(&oldname), &newname_path).map_err(|err| {
        signal_file_action_error_pair_values(
            err,
            "Adding new name",
            Value::heap_string(oldname),
            Value::heap_string(newname),
        )
    })?;
    Ok(Value::NIL)
}

/// `(make-directory-internal DIRECTORY)` — internal primitive for the
/// elisp `make-directory` wrapper. GNU dispatches the handler at the
/// `make-directory` level via Qmake_directory; we mirror that so
/// callers that go through the internal entry point still see the
/// handler.
pub(crate) fn builtin_make_directory_internal(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = dispatch_file_handler(eval, "make-directory", &args)? {
        return Ok(result);
    }
    expect_args("make-directory-internal", &args, 1)?;
    let dir = expect_lisp_string_strict(&args[0])?;
    let resolved = resolve_filename_lisp_for_eval(eval, &dir);
    fs::create_dir(lisp_file_name_to_path_buf(&resolved)).map_err(|e| {
        signal_file_action_error_value(e, "Creating directory", Value::heap_string(resolved))
    })?;
    Ok(Value::NIL)
}

/// `(find-file-name-handler FILENAME OPERATION)` — public elisp
/// surface for the [`find_file_name_handler`] dispatch helper.
pub(crate) fn builtin_find_file_name_handler(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("find-file-name-handler", &args, 2)?;
    let filename = match args[0].kind() {
        ValueKind::String => args[0]
            .as_lisp_string()
            .expect("ValueKind::String must carry LispString payload"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };
    let operation = args[1];
    Ok(find_file_name_handler_lisp_for_eval(
        eval, filename, operation,
    ))
}

/// Walk `file-name-handler-alist` looking for a handler matching FILENAME
/// for OPERATION. Mirrors GNU `Ffind_file_name_handler`
/// (`src/fileio.c:371`).
///
/// The alist is a list of `(REGEXP . HANDLER)` cons cells. For each
/// matching entry the highest match position wins (using `>` not `>=`,
/// so the *first* match at any given position is preferred). When
/// `OPERATION` equals `inhibit-file-name-operation`, handlers listed
/// in `inhibit-file-name-handlers` are skipped — that is how a handler
/// can call standard primitives without recursing into itself.
///
/// If a handler symbol carries a non-nil `'operations` property, the
/// handler is only used when `OPERATION` is in that list. This lets
/// handlers declare a restricted operation set without writing
/// trampolines for everything else.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn dynamic_or_global_symbol_value(eval: &Context, name: &str) -> Option<Value> {
    eval.eval_symbol_by_id(intern(name)).ok()
}

pub(crate) fn find_file_name_handler_lisp_for_eval(
    eval: &Context,
    filename: &crate::heap_types::LispString,
    operation: Value,
) -> Value {
    find_file_name_handler_lisp_with_values(
        &eval.obarray,
        &eval.buffers,
        super::builtins::search::FastStringMatchSyntax::for_current_buffer(eval),
        filename,
        operation,
        dynamic_or_global_symbol_value(eval, "file-name-handler-alist"),
        dynamic_or_global_symbol_value(eval, "inhibit-file-name-operation"),
        dynamic_or_global_symbol_value(eval, "inhibit-file-name-handlers"),
    )
}

#[allow(clippy::too_many_arguments)] // match-time state stays explicit at the GNU-regexp boundary
fn find_file_name_handler_lisp_with_values(
    obarray: &Obarray,
    buffers: &crate::buffer::BufferManager,
    syntax: super::builtins::search::FastStringMatchSyntax,
    filename: &crate::heap_types::LispString,
    operation: Value,
    handler_alist: Option<Value>,
    inhibit_operation: Option<Value>,
    inhibit_handlers: Option<Value>,
) -> Value {
    // Read the alist. If unbound or non-list, no handlers apply.
    let alist = match handler_alist {
        Some(v) if v.is_cons() => v,
        _ => return Value::NIL,
    };
    // Compute the inhibit list lazily — only consulted when operation
    // matches inhibit-file-name-operation.
    let mut inhibited: Option<Value> = None;
    if let Some(inh_op) = inhibit_operation
        && !inh_op.is_nil()
        && super::value::eq_value(&inh_op, &operation)
    {
        inhibited = inhibit_handlers;
    }

    // Walk the alist exactly like GNU's loop, picking the entry with
    // the strictly-greatest match position.
    let mut best: Value = Value::NIL;
    let mut best_pos: i64 = -1;
    let mut cursor = alist;
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if !entry.is_cons() {
            continue;
        }
        let regexp_val = entry.cons_car();
        let handler = entry.cons_cdr();
        let Some(regexp) = regexp_val.as_lisp_string() else {
            continue;
        };

        // If the handler is a symbol with a non-nil `operations`
        // property, restrict to listed operations. Mirrors GNU's
        // `Fget (handler, Qoperations)` check at fileio.c:409.
        if let Some(handler_sym) = handler.as_symbol_id() {
            let ops_sym = super::intern::intern("operations");
            if let Some(ops) = obarray
                .get_property_id(handler_sym, ops_sym)
                .filter(|v| !v.is_nil())
            {
                let mut op_cursor = ops;
                let mut found = false;
                while op_cursor.is_cons() {
                    if super::value::eq_value(&op_cursor.cons_car(), &operation) {
                        found = true;
                        break;
                    }
                    op_cursor = op_cursor.cons_cdr();
                }
                if !found {
                    continue;
                }
            }
        }

        // Match the regexp against the filename.
        let match_pos = match syntax.search(
            obarray,
            buffers,
            regexp,
            filename,
            crate::emacs_core::regex::SearchedString::Owned(filename.clone()),
            0,
            false,
        ) {
            Ok(Some(success)) => success.into_parts().0.get() as i64,
            _ => continue,
        };

        if match_pos > best_pos {
            // Skip if this handler is inhibited for the current operation.
            if let Some(inh) = inhibited {
                let mut inh_cursor = inh;
                let mut skip = false;
                while inh_cursor.is_cons() {
                    if super::value::eq_value(&inh_cursor.cons_car(), &handler) {
                        skip = true;
                        break;
                    }
                    inh_cursor = inh_cursor.cons_cdr();
                }
                if skip {
                    continue;
                }
            }
            best = handler;
            best_pos = match_pos;
        }
    }
    best
}

/// Convenience for builtins that have an `eval` context. Looks up a
/// handler for `(filename, operation)` and, if one is installed,
/// invokes it as `(funcall handler operation arg1 arg2 ...)` and
/// returns the result wrapped in `Some`. Returns `None` if no handler
/// matched, in which case the caller should fall back to its native
/// implementation.
///
/// `operation_name` is the symbol the handler will receive as its
/// first argument (e.g. `"file-exists-p"`). It must match the GNU
/// operation symbol exactly.
pub(crate) fn dispatch_file_handler(
    eval: &mut Context,
    operation_name: &str,
    args: &[Value],
) -> Result<Option<Value>, super::error::Flow> {
    // Every operation we wire up takes the filename in args[0]. Two-
    // argument file ops (copy-file, rename-file, add-name-to-file,
    // make-symbolic-link) need to consult the handler for *both*
    // names; those have a separate helper below.
    let Some(first) = args.first() else {
        return Ok(None);
    };
    let Some(filename) = eval.lisp_string(*first) else {
        return Ok(None);
    };
    let operation_sym = Value::symbol(operation_name);
    let handler = find_file_name_handler_lisp_for_eval(eval, filename, operation_sym);
    if handler.is_nil() {
        return Ok(None);
    }
    // Build (operation arg1 arg2 ...) and funcall the handler.
    let mut call_args = Vec::with_capacity(args.len() + 1);
    call_args.push(operation_sym);
    call_args.extend_from_slice(args);
    let result = eval.funcall_general(handler, call_args)?;
    Ok(Some(result))
}

/// Result of GNU's expand-then-dispatch preamble for a single-filename file
/// operation.  The local arm owns the expanded filename so a caller cannot
/// accidentally resume with its original, unresolved argument after handler
/// lookup says no.
pub(crate) enum ExpandedFileOperation {
    Handled(Value),
    Local { expanded_filename: Value },
}

/// Mirror GNU's per-operation preamble: run `Fexpand_file_name` first (which
/// can dispatch the expand-file-name magic handler), then invoke the operation
/// handler with the expanded name and its full DEFUN arglist padded to `arity`.
pub(crate) fn expand_file_operation(
    eval: &mut Context,
    operation_name: &str,
    args: &[Value],
    arity: usize,
) -> Result<ExpandedFileOperation, super::error::Flow> {
    let first = args
        .first()
        .expect("file operation arity must be validated before expansion");
    debug_assert!(first.as_lisp_string().is_some());
    let expanded = builtin_expand_file_name(eval, vec![*first, Value::NIL])?;
    let mut call = Vec::with_capacity(arity.max(args.len()));
    call.push(expanded);
    call.extend_from_slice(&args[1..]);
    while call.len() < arity {
        call.push(Value::NIL);
    }
    Ok(
        if let Some(result) = dispatch_file_handler(eval, operation_name, &call)? {
            ExpandedFileOperation::Handled(result)
        } else {
            ExpandedFileOperation::Local {
                expanded_filename: expanded,
            }
        },
    )
}

/// `(directory-files DIRECTORY &optional FULL MATCH NOSORT COUNT)`
pub(crate) fn builtin_directory_files(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("directory-files", &args, 1)?;
    if args.len() > 5 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("directory-files"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    expect_lisp_string_strict(&args[0])?;
    let dir = match expand_file_operation(eval, "directory-files", &args, 5)? {
        ExpandedFileOperation::Handled(result) => return Ok(result),
        ExpandedFileOperation::Local { expanded_filename } => {
            expect_lisp_filename_string_strict(&expanded_filename)?
        }
    };
    let full = args.get(1).is_some_and(|v| v.is_truthy());
    let match_pattern = if let Some(val) = args.get(2) {
        if val.is_truthy() {
            Some(expect_lisp_string_strict(val)?)
        } else {
            None
        }
    } else {
        None
    };
    let nosort = args.get(3).is_some_and(|v| v.is_truthy());
    let count = if let Some(val) = args.get(4) {
        match val.kind() {
            ValueKind::Fixnum(n) if n >= 0 => Some(n as usize),
            _other => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("wholenump"), *val],
                ));
            }
        }
    } else {
        None
    };

    let syntax = super::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
    let files = directory_files_with_decoder(
        &dir,
        full,
        match_pattern.as_ref(),
        nosort,
        count,
        syntax,
        &eval.obarray,
        &eval.buffers,
        |bytes| decode_file_name_lisp(eval, bytes),
    )
    .map_err(|e| signal_directory_files_error(e, &dir))?;
    Ok(Value::list(
        files.into_iter().map(Value::heap_string).collect(),
    ))
}

// ===========================================================================
// Context-dependent builtins
// ===========================================================================

fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

fn expect_file_offset(value: &Value) -> Result<i64, Flow> {
    let offset = expect_int(value)?;
    if offset < 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("file-offset"), *value],
        ));
    }
    Ok(offset)
}

fn current_buffer_id_or_error(
    buffers: &crate::buffer::BufferManager,
) -> Result<crate::buffer::BufferId, Flow> {
    buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))
}

fn emacs_char_boundary(bytes: &[u8], byte_pos: usize, multibyte: bool) -> bool {
    if byte_pos > bytes.len() {
        return false;
    }
    if !multibyte || byte_pos == 0 || byte_pos == bytes.len() {
        return true;
    }
    crate::emacs_core::emacs_char::char_head_p(bytes[byte_pos])
}

fn previous_emacs_char_boundary(bytes: &[u8], mut byte_pos: usize, multibyte: bool) -> usize {
    byte_pos = byte_pos.min(bytes.len());
    while byte_pos > 0 && !emacs_char_boundary(bytes, byte_pos, multibyte) {
        byte_pos -= 1;
    }
    byte_pos
}

fn common_replacement_prefix_bytes(
    old_bytes: &[u8],
    old_multibyte: bool,
    new_bytes: &[u8],
    new_multibyte: bool,
) -> usize {
    let mut prefix = 0usize;
    let limit = old_bytes.len().min(new_bytes.len());
    while prefix < limit && old_bytes[prefix] == new_bytes[prefix] {
        prefix += 1;
    }
    let old_prefix = previous_emacs_char_boundary(old_bytes, prefix, old_multibyte);
    let new_prefix = previous_emacs_char_boundary(new_bytes, prefix, new_multibyte);
    old_prefix.min(new_prefix)
}

fn common_replacement_suffix_bytes(
    old_bytes: &[u8],
    old_multibyte: bool,
    new_bytes: &[u8],
    new_multibyte: bool,
    prefix: usize,
) -> usize {
    let old_tail_limit = old_bytes.len().saturating_sub(prefix);
    let new_tail_limit = new_bytes.len().saturating_sub(prefix);
    let mut suffix = 0usize;
    let limit = old_tail_limit.min(new_tail_limit);
    while suffix < limit
        && old_bytes[old_bytes.len() - suffix - 1] == new_bytes[new_bytes.len() - suffix - 1]
    {
        suffix += 1;
    }
    while suffix > 0
        && (!emacs_char_boundary(old_bytes, old_bytes.len() - suffix, old_multibyte)
            || !emacs_char_boundary(new_bytes, new_bytes.len() - suffix, new_multibyte))
    {
        suffix -= 1;
    }
    suffix
}

fn char_count_for_lisp_string_byte_prefix(text: &LispString, byte_pos: usize) -> usize {
    if text.is_multibyte() {
        crate::emacs_core::emacs_char::byte_to_char_pos(text.as_bytes(), byte_pos)
    } else {
        byte_pos
    }
}

fn restore_point_after_file_replace(
    buffers: &mut crate::buffer::BufferManager,
    current_id: crate::buffer::BufferId,
    saved_point_char: usize,
    same_at_start_char: usize,
    same_at_end_char: usize,
    inserted_chars: usize,
) -> Result<(), Flow> {
    let new_point_char = if saved_point_char <= same_at_start_char {
        saved_point_char
    } else if saved_point_char <= same_at_end_char {
        let old_size = same_at_end_char.saturating_sub(same_at_start_char);
        if old_size == 0 {
            same_at_start_char
        } else {
            same_at_start_char
                + ((inserted_chars as f64 / old_size as f64)
                    * (saved_point_char - same_at_start_char) as f64) as usize
        }
    } else {
        saved_point_char
            .saturating_add(inserted_chars)
            .saturating_sub(same_at_end_char.saturating_sub(same_at_start_char))
    };
    let point = {
        let buf = buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let point_char = CharPos0::new(new_point_char).min(buf.total_char_end_pos());
        TextPositionAnchor::new(
            point_char,
            buf.char_pos_to_emacs_byte_pos_clamped(point_char),
        )
    };
    buffers
        .set_buffer_point_anchor(current_id, point)
        .map(|_| ())
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))
}

fn signal_and_delete_file_replace_region(
    eval: &mut Context,
    current_id: crate::buffer::BufferId,
    byte_range: EmacsByteRange,
    signal_hooks: bool,
) -> Result<(), Flow> {
    if byte_range.is_empty() {
        return Ok(());
    }
    let change =
        super::editfns::text_change_for_deletion_in_manager(&eval.buffers, current_id, byte_range)?;
    if signal_hooks {
        super::editfns::signal_before_text_change(eval, change)?;
    }
    eval.buffers
        .delete_buffer_measured_region(current_id, change.old_range())
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if signal_hooks {
        super::editfns::signal_after_text_change(eval, change)?;
    }
    Ok(())
}

fn signal_and_insert_file_replace_text(
    eval: &mut Context,
    current_id: crate::buffer::BufferId,
    byte_pos: EmacsBytePos,
    text: &LispString,
    signal_hooks: bool,
) -> Result<(), Flow> {
    if text.is_empty() {
        return Ok(());
    }
    eval.buffers
        .goto_buffer_emacs_byte_pos(current_id, byte_pos)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let change = super::editfns::text_change_for_lisp_string_replacement_in_manager(
        &eval.buffers,
        current_id,
        EmacsByteRange::from_start_len(byte_pos, EmacsByteLen::ZERO),
        text,
    )?;
    if signal_hooks {
        super::editfns::signal_before_text_change(eval, change)?;
    }
    eval.buffers
        // GNU's `insert-file-contents' REPLACE path performs a deletion and
        // then an ordinary insertion (`insert_from_buffer`), not one atomic
        // `adjust_markers_for_replace` edit.  Markers collapsed to the insert
        // position by the deletion must therefore honor their insertion type.
        .insert_lisp_string_into_buffer(current_id, text)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if signal_hooks {
        super::editfns::signal_after_text_change(eval, change)?;
    }
    Ok(())
}

/// Returns the *net* number of characters inserted after eliding the
/// unchanged head/tail affixes, matching GNU's final `inserted = PT - temp`
/// in the REPLACE branch of `Finsert_file_contents` (fileio.c).  When the new
/// text is byte-identical to the accessible buffer text this is 0.
fn replace_accessible_portion_for_insert_file_contents(
    eval: &mut Context,
    current_id: crate::buffer::BufferId,
    text: &LispString,
    signal_hooks: bool,
    hide_visited_file_name_during_replace: bool,
) -> Result<i64, Flow> {
    let (
        accessible_range,
        accessible_start_char,
        accessible_end_char,
        old_point_char,
        old_multibyte,
        old_text,
    ) = {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let accessible = buf.accessible_emacs_byte_region();
        let range = accessible.range();
        let edit_range = buf.edit_range_for_emacs_byte_range(range);
        (
            range,
            edit_range.char_start().get(),
            edit_range.char_end().get(),
            buf.point_char_pos().get(),
            buf.get_multibyte(),
            buf.buffer_substring_lisp_string_range(range),
        )
    };
    let old_bytes = old_text.as_bytes();
    let new_bytes = text.as_bytes();
    let prefix =
        common_replacement_prefix_bytes(old_bytes, old_multibyte, new_bytes, text.is_multibyte());
    let suffix = common_replacement_suffix_bytes(
        old_bytes,
        old_multibyte,
        new_bytes,
        text.is_multibyte(),
        prefix,
    );

    if prefix == old_bytes.len() && prefix == new_bytes.len() {
        restore_point_after_file_replace(
            &mut eval.buffers,
            current_id,
            old_point_char,
            accessible_start_char + char_count_for_lisp_string_byte_prefix(&old_text, prefix),
            accessible_end_char - char_count_for_lisp_string_byte_prefix(&old_text, suffix),
            text.schars(),
        )?;
        // Byte-identical content: GNU elides both affixes entirely, so the
        // net inserted count reported by `(FILE INSERTED)` is 0.
        return Ok(0);
    }

    let delete_range = EmacsByteRange::new(
        accessible_range.start().add_len(EmacsByteLen::new(prefix)),
        accessible_range
            .end()
            .saturating_sub_len(EmacsByteLen::new(suffix)),
    );
    let insert_start = prefix;
    let insert_end = new_bytes.len() - suffix;
    let insert_text = text.slice(insert_start, insert_end).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Invalid replacement text slice")],
        )
    })?;
    let same_at_start_char =
        accessible_start_char + char_count_for_lisp_string_byte_prefix(&old_text, prefix);
    let same_at_end_char =
        accessible_end_char - char_count_for_lisp_string_byte_prefix(&old_text, suffix);
    let inserted_chars = insert_text.schars();

    // GNU `Finsert_file_contents' temporarily binds `buffer-file-name' to nil
    // while a VISIT+REPLACE mutates an unnarrowed buffer.  Change preparation
    // otherwise treats the stale visited modtime as a user edit and asks about
    // a supersession threat, even though this mutation is making the buffer
    // match the file.  Keep the binding scoped to the actual replacement:
    // byte-identical reads above remain true no-ops, including for variable
    // watchers.
    let specpdl_count = eval.specpdl.len();
    if hide_visited_file_name_during_replace {
        eval.try_specbind_or_unwind_to(specpdl_count, intern("buffer-file-name"), Value::NIL)?;
    }
    let replace_result = (|| -> Result<i64, Flow> {
        signal_and_delete_file_replace_region(eval, current_id, delete_range, signal_hooks)?;
        signal_and_insert_file_replace_text(
            eval,
            current_id,
            delete_range.start(),
            &insert_text,
            signal_hooks,
        )?;
        restore_point_after_file_replace(
            &mut eval.buffers,
            current_id,
            old_point_char,
            same_at_start_char,
            same_at_end_char,
            inserted_chars,
        )?;
        Ok(inserted_chars as i64)
    })();
    finish_inserted_count_scope(eval, specpdl_count, replace_result)
}

fn finish_inserted_count_scope(
    eval: &mut Context,
    specpdl_count: usize,
    result: Result<i64, Flow>,
) -> Result<i64, Flow> {
    match result {
        Ok(inserted) => eval
            .unbind_to_with_result(specpdl_count, Ok(Value::NIL))
            .map(|_| inserted),
        Err(flow) => match eval.unbind_to_with_result(specpdl_count, Err(flow)) {
            Err(flow) => Err(flow),
            Ok(_) => unreachable!("unwinding an error cannot produce a value"),
        },
    }
}

/// Inserts CONTENTS into the current buffer.  For a REPLACE request this
/// returns `Some(net)` — the affix-elided net inserted char count GNU reports
/// in `(FILE INSERTED)` — and `None` for a plain insert, where the caller keeps
/// the full decoded char count.
fn insert_file_contents_into_current_buffer_in_state(
    eval: &mut Context,
    current_id: crate::buffer::BufferId,
    contents: &LispString,
    replace_requested: bool,
    signal_hooks: bool,
    hide_visited_file_name_during_replace: bool,
) -> Result<Option<i64>, Flow> {
    if replace_requested {
        replace_accessible_portion_for_insert_file_contents(
            eval,
            current_id,
            contents,
            signal_hooks,
            hide_visited_file_name_during_replace,
        )
        .map(Some)
    } else {
        // GNU Emacs: insert-file-contents inserts text at point but does NOT
        // advance point past the inserted text (unlike regular `insert`).
        // It calls TEMP_SET_PT_BOTH(BEG, BEG_BYTE) to keep point at the
        // beginning of the inserted region.
        let pt_before = eval
            .buffers
            .get(current_id)
            .map(|b| b.point_anchor())
            .unwrap_or_else(|| TextPositionAnchor::new(CharPos0::ZERO, EmacsBytePos::ZERO));
        let change = super::editfns::text_change_for_lisp_string_replacement_in_manager(
            &eval.buffers,
            current_id,
            EmacsByteRange::from_start_len(pt_before.emacs_byte_pos(), EmacsByteLen::ZERO),
            contents,
        )?;
        if signal_hooks && !contents.is_empty() {
            super::editfns::signal_before_text_change(eval, change)?;
        }
        eval.buffers
            .insert_lisp_string_into_buffer(current_id, contents)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        if signal_hooks && !contents.is_empty() {
            super::editfns::signal_after_text_change(eval, change)?;
        }
        // Restore point to before the insertion (matching GNU).
        let _ = eval.buffers.set_buffer_point_anchor(current_id, pt_before);
        Ok(None)
    }
}

fn expect_inserted_char_count(value: &Value) -> Result<i64, Flow> {
    let inserted = expect_int(value)?;
    if inserted < 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        ));
    }
    Ok(inserted)
}

fn run_after_insert_file_pipeline(
    eval: &mut Context,
    current_id: crate::buffer::BufferId,
    visit: bool,
    replace_requested: bool,
    inserted_chars: i64,
) -> Result<i64, Flow> {
    let visit_value = if visit { Value::T } else { Value::NIL };
    let mut inserted = inserted_chars;

    if eval.obarray.fboundp("after-insert-file-set-coding") {
        let result = eval.funcall_general(
            Value::symbol("after-insert-file-set-coding"),
            vec![Value::fixnum(inserted), visit_value],
        )?;
        if !result.is_nil() {
            inserted = expect_inserted_char_count(&result)?;
        }
    }

    if inserted <= 0 || !eval.obarray.fboundp("format-decode") {
        return Ok(inserted);
    }

    let (saved_point, accessible_start, chars_modiff_before) = eval
        .buffers
        .get(current_id)
        .map(|buf| {
            let accessible = buf.accessible_emacs_byte_region();
            (
                buf.point_anchor(),
                accessible.start(),
                buf.chars_modified_tick(),
            )
        })
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let specpdl_count = eval.specpdl.len();
    eval.try_specbind_or_unwind_to(
        specpdl_count,
        intern("inhibit-point-motion-hooks"),
        Value::T,
    )?;
    eval.try_specbind_or_unwind_to(
        specpdl_count,
        intern("inhibit-modification-hooks"),
        Value::T,
    )?;
    eval.try_specbind_or_unwind_to(specpdl_count, intern("buffer-undo-list"), Value::T)?;

    let pipeline_result = (|| -> Result<i64, Flow> {
        if replace_requested {
            eval.buffers
                .goto_buffer_emacs_byte_pos(current_id, accessible_start)
                .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        }

        let format_result = eval.funcall_general(
            Value::symbol("format-decode"),
            vec![Value::NIL, Value::fixnum(inserted), visit_value],
        )?;
        if !format_result.is_nil() {
            inserted = expect_inserted_char_count(&format_result)?;
        }

        let hook_sym = intern("after-insert-file-functions");
        let hook_value = eval.visible_variable_value_or_nil("after-insert-file-functions");
        let hook_functions = crate::emacs_core::hook_runtime::collect_hook_functions_in_state(
            eval, hook_sym, hook_value, true,
        );
        if !hook_functions.is_empty() {
            let gc_roots = eval.save_specpdl_roots();
            for func in &hook_functions {
                eval.push_specpdl_root(*func);
            }
            eval.push_specpdl_root(Value::fixnum(inserted));
            let hook_result = (|| -> Result<i64, Flow> {
                let mut inserted_now = inserted;
                for function in &hook_functions {
                    let result = eval.apply(*function, vec![Value::fixnum(inserted_now)])?;
                    if !result.is_nil() {
                        inserted_now = expect_inserted_char_count(&result)?;
                    }
                }
                Ok(inserted_now)
            })();
            eval.restore_specpdl_roots(gc_roots);
            inserted = hook_result?;
        }

        Ok(inserted)
    })();

    eval.restore_current_buffer_if_live(current_id);
    let chars_modiff_after = eval
        .buffers
        .get(current_id)
        .map(|buf| buf.chars_modified_tick())
        .unwrap_or(chars_modiff_before + 1);
    if replace_requested && chars_modiff_after == chars_modiff_before {
        let _ = eval
            .buffers
            .set_buffer_point_anchor(current_id, saved_point);
    }
    finish_inserted_count_scope(eval, specpdl_count, pipeline_result)
}

/// A position in the character stream consumed by GNU's `a_write`.
///
/// Keeping this distinct from byte offsets prevents multibyte annotations from
/// being placed using storage coordinates.  GNU annotation positions are
/// 1-based buffer character positions, including the region end boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct WriteAnnotationPosition(LispCharPos1);

/// The callback result that contributed an annotation.  GNU merges a newer
/// callback's whole list before an older list when their positions tie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct WriteAnnotationBatch(usize);

/// Stable order inside one callback result.  This is intentionally a distinct
/// type from `WriteAnnotationBatch`: reversing batches must never reverse the
/// annotations within a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct WriteAnnotationBatchIndex(usize);

#[derive(Clone)]
enum WriteAnnotationPayload {
    Text(LispString),
    /// GNU consumes a non-string annotation at its position but emits no text
    /// (`a_write`, src/fileio.c:6013-6021).
    NoText,
}

#[derive(Clone)]
struct WriteAnnotation {
    position: WriteAnnotationPosition,
    payload: WriteAnnotationPayload,
    batch: WriteAnnotationBatch,
    batch_index: WriteAnnotationBatchIndex,
    original_pair: Value,
}

#[derive(Default)]
struct WriteAnnotationStream {
    entries: Vec<WriteAnnotation>,
    next_batch: usize,
}

impl WriteAnnotationStream {
    fn clear(&mut self) {
        self.entries.clear();
    }

    fn as_lisp_list(&self) -> Value {
        let pairs: Vec<Value> = self
            .entries
            .iter()
            .map(|annotation| annotation.original_pair)
            .collect();
        Value::list(pairs)
    }

    fn merge_callback_result(&mut self, result: Value) -> Result<(), Flow> {
        let pairs = list_to_vec(&result).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), result],
            )
        })?;

        let batch = WriteAnnotationBatch(self.next_batch);
        self.next_batch += 1;
        for (batch_index, pair) in pairs.into_iter().enumerate() {
            if !pair.is_cons() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("consp"), pair],
                ));
            }
            let position_value = pair.cons_car();
            let position = position_value.as_fixnum().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), position_value],
                )
            })?;
            let payload_value = pair.cons_cdr();
            let payload = payload_value
                .as_lisp_string()
                .cloned()
                .map(WriteAnnotationPayload::Text)
                .unwrap_or(WriteAnnotationPayload::NoText);
            self.entries.push(WriteAnnotation {
                position: WriteAnnotationPosition(LispCharPos1::new(position)),
                payload,
                batch,
                batch_index: WriteAnnotationBatchIndex(batch_index),
                original_pair: pair,
            });
        }

        self.entries.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| right.batch.cmp(&left.batch))
                .then_with(|| left.batch_index.cmp(&right.batch_index))
        });
        Ok(())
    }

    fn intersperse(
        &self,
        source: &LispString,
        source_start: LispCharPos1,
        source_end: LispCharPos1,
    ) -> LispString {
        let mut output = source
            .slice(0, 0)
            .expect("an empty Lisp string prefix is always in bounds");
        let mut source_char_cursor = 0usize;
        let source_start = source_start.as_i64();
        let source_end = source_end.as_i64();

        for annotation in &self.entries {
            let position = annotation.position.0.as_i64();
            if position < source_start || position > source_end {
                continue;
            }
            let annotation_char_offset = usize::try_from(position - source_start)
                .expect("an in-range annotation offset is nonnegative")
                .min(source.schars());
            let byte_start = source.char_to_byte_pos(source_char_cursor);
            let byte_end = source.char_to_byte_pos(annotation_char_offset);
            let source_piece = source
                .slice(byte_start, byte_end)
                .expect("character-derived Lisp string slice is in bounds");
            output = output.concat(&source_piece);
            if let WriteAnnotationPayload::Text(text) = &annotation.payload {
                output = output.concat(text);
            }
            source_char_cursor = annotation_char_offset;
        }

        let byte_start = source.char_to_byte_pos(source_char_cursor);
        let source_tail = source
            .slice(byte_start, source.sbytes())
            .expect("Lisp string tail is in bounds");
        output.concat(&source_tail)
    }
}

/// The mutually exclusive sources accepted by `write-region`.
///
/// GNU intentionally skips annotation callbacks when START is a string.  This
/// enum makes that rule exhaustive: only `BufferRegion` can enter annotation
/// collection, while `Literal` carries its already-complete content.
enum WriteRegionSource {
    Literal {
        content: LispString,
        coding_buffer: BufferId,
    },
    BufferRegion {
        buffer: BufferId,
        start: LispCharPos1,
        end: LispCharPos1,
    },
}

impl WriteRegionSource {
    fn buffer_region(
        buffers: &crate::buffer::BufferManager,
        buffer: BufferId,
        start: Value,
        end: Value,
    ) -> Result<Self, Flow> {
        let buf = buffers
            .get(buffer)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let byte_range = if start.is_nil() {
            buf.accessible_emacs_byte_range()
        } else {
            super::position::LispRegionArgs::from_values(buffers, start, end)?
                .accessible_byte_range(buf)?
        };
        Ok(Self::BufferRegion {
            buffer,
            start: buf.emacs_byte_pos_to_lisp_char_pos(byte_range.start()),
            end: buf.emacs_byte_pos_to_lisp_char_pos(byte_range.end()),
        })
    }

    fn whole_accessible_buffer(
        buffers: &crate::buffer::BufferManager,
        buffer: BufferId,
    ) -> Result<Self, Flow> {
        let buf = buffers
            .get(buffer)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        Ok(Self::BufferRegion {
            buffer,
            start: buf.point_min_lisp_char_pos(),
            end: buf.point_max_lisp_char_pos(),
        })
    }

    fn content(&self, buffers: &crate::buffer::BufferManager) -> Result<LispString, Flow> {
        match self {
            Self::Literal { content, .. } => Ok(content.clone()),
            Self::BufferRegion { buffer, start, end } => {
                let buf = buffers
                    .get(*buffer)
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                Ok(buf.buffer_substring_lisp_string_range(EmacsByteRange::new(
                    buf.lisp_pos_to_accessible_emacs_byte_pos(*start),
                    buf.lisp_pos_to_accessible_emacs_byte_pos(*end),
                )))
            }
        }
    }

    fn coding_buffer(&self) -> BufferId {
        match self {
            Self::Literal { coding_buffer, .. } => *coding_buffer,
            Self::BufferRegion { buffer, .. } => *buffer,
        }
    }

    fn coding_bounds(&self) -> (Value, Value) {
        match self {
            Self::Literal { content, .. } => (Value::heap_string(content.clone()), Value::NIL),
            Self::BufferRegion { start, end, .. } => {
                (Value::fixnum(start.as_i64()), Value::fixnum(end.as_i64()))
            }
        }
    }

    fn apply_annotations(
        &self,
        buffers: &crate::buffer::BufferManager,
        annotations: &WriteAnnotationStream,
    ) -> Result<LispString, Flow> {
        let content = self.content(buffers)?;
        match self {
            Self::Literal { .. } => Ok(content),
            Self::BufferRegion { start, end, .. } => {
                Ok(annotations.intersperse(&content, *start, *end))
            }
        }
    }
}

struct PreparedWriteRegion {
    content: LispString,
    source: WriteRegionSource,
    original_buffer: BufferId,
    annotation_buffers: Vec<BufferId>,
}

impl PreparedWriteRegion {
    fn run_post_annotation_functions(&self, eval: &mut super::eval::Context) -> Result<(), Flow> {
        let result = (|| {
            // GNU conses each newly selected annotation buffer onto the front,
            // so cleanup visits the most recent buffer first and the original
            // buffer last (`src/fileio.c:5804-5819`).
            for buffer in self.annotation_buffers.iter().rev().copied() {
                if eval.buffers.get(buffer).is_none() {
                    continue;
                }
                eval.switch_current_buffer(buffer)?;
                let function =
                    eval.visible_variable_value_or_nil("write-region-post-annotation-function");
                if super::builtins::value_is_function(eval, function) {
                    let _ = eval.apply(function, vec![])?;
                }
            }
            Ok(())
        })();
        eval.restore_current_buffer_if_live(self.original_buffer);
        result
    }
}

fn prepare_write_region(
    eval: &mut super::eval::Context,
    original_buffer: BufferId,
    start: Value,
    end: Value,
) -> Result<PreparedWriteRegion, Flow> {
    let mut annotation_buffers = vec![original_buffer];
    if let Some(content) = start.as_lisp_string().cloned() {
        return Ok(PreparedWriteRegion {
            content: content.clone(),
            source: WriteRegionSource::Literal {
                content,
                coding_buffer: original_buffer,
            },
            original_buffer,
            annotation_buffers,
        });
    }

    let mut source = WriteRegionSource::buffer_region(&eval.buffers, original_buffer, start, end)?;
    let mut callback_start = start;
    let mut callback_end = end;
    let hook_sym = intern("write-region-annotate-functions");
    let hook_value = eval.visible_variable_value_or_nil("write-region-annotate-functions");
    let functions = crate::emacs_core::hook_runtime::collect_hook_functions_in_state(
        eval, hook_sym, hook_value, true,
    );
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(Value::list(functions.clone()));
    let mut annotations = WriteAnnotationStream::default();

    let collection_result = (|| -> Result<(), Flow> {
        for function in functions {
            eval.set_variable(
                "write-region-annotations-so-far",
                annotations.as_lisp_list(),
            );
            let buffer_before = current_buffer_id_or_error(&eval.buffers)?;
            let result = eval.apply(function, vec![callback_start, callback_end])?;
            eval.push_specpdl_root(result);
            let buffer_after = current_buffer_id_or_error(&eval.buffers)?;
            if buffer_after != buffer_before {
                annotation_buffers.push(buffer_after);
                source = WriteRegionSource::whole_accessible_buffer(&eval.buffers, buffer_after)?;
                let buf = eval
                    .buffers
                    .get(buffer_after)
                    .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
                callback_start = Value::fixnum(buf.point_min_lisp_char_pos().as_i64());
                callback_end = Value::fixnum(buf.point_max_lisp_char_pos().as_i64());
                annotations.clear();
            }
            annotations.merge_callback_result(result)?;
        }
        Ok(())
    })();
    if let Err(error) = collection_result {
        eval.restore_specpdl_roots(root_scope);
        eval.restore_current_buffer_if_live(original_buffer);
        return Err(error);
    }
    let content = source.apply_annotations(&eval.buffers, &annotations)?;
    eval.restore_specpdl_roots(root_scope);

    Ok(PreparedWriteRegion {
        content,
        source,
        original_buffer,
        annotation_buffers,
    })
}

struct DecodedFileContents {
    text: LispString,
    coding: String,
}

impl DecodedFileContents {
    fn from_lisp_string(text: LispString, coding: String) -> Self {
        Self { text, coding }
    }

    fn text(&self) -> &LispString {
        &self.text
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn text_properties(&self) -> Option<&TextPropertyTable> {
        let table = self.text().intervals();
        if table.is_empty() { None } else { Some(table) }
    }

    fn char_count(&self) -> i64 {
        self.text().schars() as i64
    }
}

fn has_utf8_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xEF, 0xBB, 0xBF])
}

fn decode_insert_file_contents(
    eval: &mut crate::emacs_core::eval::Context,
    bytes: &[u8],
    multibyte: bool,
    coding_system_for_read: Option<&str>,
) -> Result<DecodedFileContents, Flow> {
    if !multibyte {
        return Ok(DecodedFileContents::from_lisp_string(
            LispString::from_unibyte(bytes.to_vec()),
            "no-conversion".to_string(),
        ));
    }

    // No end-of-line detection here.  GNU does it once, in `decode_eol`
    // (src/coding.c:6783-6806), inside the decoder every consumer shares -- and
    // it does it on the DECODED text with a whole-text bitmask, not on a
    // three-terminator prefix of the raw bytes.  A second, prefix-based
    // detection in this function answered a different question than the decoder
    // would: measured under GNU 31.0.90, a file holding
    // `a\r\nb\r\nc\r\nd\r\ne\nf` reads back with every CR intact and
    // `buffer-file-coding-system' `undecided-unix', because the fifth, bare LF
    // makes the text mixed; the prefix scan stopped at the third CR LF and
    // called the file DOS.
    let coding =
        match coding_system_for_read.filter(|coding| !coding.is_empty() && *coding != "nil") {
            Some(coding) => coding.to_string(),
            // A bare test context does not load the Lisp BOM recognizer used by
            // `set-auto-coding-function`; keep the same observable fallback.
            None if has_utf8_signature(bytes) => "utf-8-with-signature".to_string(),
            // GNU uses the ordinary undecided category engine here, including
            // while `load-with-code-conversion` binds set-auto-coding-for-load.
            None => "undecided".to_string(),
        };

    let decoded = crate::encoding::decode_file_bytes_in_context(eval, bytes, &coding)?;
    Ok(DecodedFileContents::from_lisp_string(
        decoded.text,
        resolve_sym(decoded.coding_system).to_owned(),
    ))
}

/// Which source decided the coding system `insert-file-contents` decodes with.
///
/// GNU consults exactly these, in exactly this order, and stops at the first
/// that answers: `coding-system-for-read` (src/fileio.c:4317-4318), then
/// `set-auto-coding-function` (src/fileio.c:4401-4402 for a non-empty buffer,
/// :5051-5055 for the empty-buffer path), then `file-coding-system-alist` via
/// `find-operation-coding-system` (src/fileio.c:4411-4420, :5057-5066), and
/// finally plain `undecided` (src/fileio.c:4423-4424).
///
/// The ladder is a value rather than a chain of `Option::or` calls so that
/// every rung is named: a new source cannot be spliced in without choosing
/// where it sits, and a dropped rung is a missing match arm rather than a
/// silently shorter chain.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ReadCodingDecision {
    /// `coding-system-for-read` was non-nil and wins outright.
    ForRead(String),
    /// `set-auto-coding-function` found a `coding:` cookie, an
    /// `auto-coding-alist` entry or an `auto-coding-functions` hit.
    AutoCoding(String),
    /// `file-coding-system-alist` matched the file name.
    FileNameAlist(String),
    /// Nothing decided; the undecided category engine detects.
    Undecided,
}

impl ReadCodingDecision {
    /// The coding-system name to decode with, or `None` for `undecided`.
    fn coding_system(&self) -> Option<&str> {
        match self {
            Self::ForRead(name) | Self::AutoCoding(name) | Self::FileNameAlist(name) => Some(name),
            Self::Undecided => None,
        }
    }
}

/// Ask `file-coding-system-alist` (through `find-operation-coding-system`)
/// which coding system this file name asks for, mirroring GNU's
/// `CALLN (Ffind_operation_coding_system, Qinsert_file_contents, ...)`
/// (src/fileio.c:4415-4419, :5061-5065) including its `XCAR` of a cons answer.
fn decide_file_name_coding_for_insert_file_contents(
    eval: &mut super::eval::Context,
    filename: crate::heap_types::LispString,
    visit: Value,
    beg: Value,
    end: Value,
    replace: Value,
) -> Result<Option<String>, Flow> {
    let answer = super::builtins::builtin_find_operation_coding_system(
        eval,
        vec![
            Value::symbol("insert-file-contents"),
            Value::heap_string(filename),
            visit,
            beg,
            end,
            replace,
        ],
    )?;
    let decoding = if answer.is_cons() {
        answer.cons_car()
    } else {
        answer
    };
    auto_coding_system_name(eval, decoding)
}

fn decide_auto_coding_for_insert_file_contents(
    eval: &mut super::eval::Context,
    filename: crate::heap_types::LispString,
    bytes: &[u8],
) -> Result<Option<String>, Flow> {
    let function = eval.visible_variable_value_or_nil("set-auto-coding-function");
    if function.is_nil() || bytes.is_empty() {
        return Ok(None);
    }

    let saved_current = eval.buffers.current_buffer_id();
    let work_buffer = eval
        .buffers
        .find_buffer_by_name(" *code-converting-work*")
        .unwrap_or_else(|| eval.buffers.create_buffer(" *code-converting-work*"));
    let restore_and_finish = |eval: &mut super::eval::Context, result: EvalResult| {
        if let Some(saved) = saved_current {
            eval.restore_current_buffer_if_live(saved);
        }
        result
    };

    eval.set_current_buffer_unrecorded(work_buffer)?;
    let old_range = eval
        .buffers
        .get(work_buffer)
        .map(|buf| buf.full_emacs_byte_range())
        .unwrap_or(EmacsByteRange::EMPTY);
    if !old_range.is_empty() {
        eval.buffers
            .delete_buffer_emacs_byte_range(work_buffer, old_range)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    }
    eval.buffers
        .set_buffer_multibyte_flag(work_buffer, false)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let raw = crate::heap_types::LispString::from_unibyte(bytes.to_vec());
    eval.buffers
        .insert_lisp_string_into_buffer(work_buffer, &raw)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(work_buffer, EmacsBytePos::new(0));

    let result = eval.apply(
        function,
        vec![
            Value::heap_string(filename),
            Value::fixnum(bytes.len() as i64),
        ],
    );
    let value = restore_and_finish(eval, result)?;
    auto_coding_system_name(eval, value)
}

fn auto_coding_system_name(
    eval: &super::eval::Context,
    value: Value,
) -> Result<Option<String>, Flow> {
    // `nil` is GNU's "I did not decide" answer (`NILP (coding_system)`,
    // src/fileio.c:4411, :5057), not a coding-system name.  `nil` is a symbol
    // here, so without this guard it would come back as the name "nil" and
    // read as a decision.
    if value.is_nil() {
        return Ok(None);
    }
    let Some(name) = value.as_symbol_name().map(str::to_owned).or_else(|| {
        value
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    }) else {
        return Ok(None);
    };
    if eval.coding_systems.is_known_or_derived(&name) {
        Ok(Some(name))
    } else {
        Ok(None)
    }
}

fn restore_empty_buffer_after_auto_coding_probe(
    eval: &mut super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    saved_multibyte: bool,
    saved_undo_list: Value,
) {
    if eval.buffers.get(buffer_id).is_none() {
        return;
    }
    let _ = eval.set_current_buffer_unrecorded(buffer_id);
    if let Some(buf) = eval.buffers.get_mut(buffer_id) {
        buf.set_undo_list(Value::T);
    }
    let delete_range = eval
        .buffers
        .get(buffer_id)
        .map(|buf| buf.full_emacs_byte_range())
        .unwrap_or(EmacsByteRange::EMPTY);
    if !delete_range.is_empty() {
        let _ = eval
            .buffers
            .delete_buffer_emacs_byte_range(buffer_id, delete_range);
    }
    let _ = eval
        .buffers
        .set_buffer_multibyte_flag(buffer_id, saved_multibyte);
    if let Some(buf) = eval.buffers.get_mut(buffer_id) {
        buf.set_undo_list(saved_undo_list);
    }
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, EmacsBytePos::new(0));
}

fn decide_auto_coding_for_empty_insert_file_contents(
    eval: &mut super::eval::Context,
    filename: crate::heap_types::LispString,
    bytes: &[u8],
    current_id: crate::buffer::BufferId,
) -> Result<Option<String>, Flow> {
    let function = eval.visible_variable_value_or_nil("set-auto-coding-function");
    if function.is_nil() || bytes.is_empty() {
        return Ok(None);
    }

    let (saved_multibyte, saved_undo_list) = {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        (buf.get_multibyte(), buf.get_undo_list())
    };

    eval.set_current_buffer_unrecorded(current_id)?;
    eval.buffers
        .set_buffer_multibyte_flag(current_id, false)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if let Some(buf) = eval.buffers.get_mut(current_id) {
        buf.set_undo_list(Value::T);
    }
    let raw = crate::heap_types::LispString::from_unibyte(bytes.to_vec());
    eval.buffers
        .insert_lisp_string_into_buffer(current_id, &raw)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let _ = eval
        .buffers
        .goto_buffer_emacs_byte_pos(current_id, EmacsBytePos::new(0));

    let result = eval.apply(
        function,
        vec![
            Value::heap_string(filename),
            Value::fixnum(bytes.len() as i64),
        ],
    );
    let value = match result {
        Ok(value) => {
            restore_empty_buffer_after_auto_coding_probe(
                eval,
                current_id,
                saved_multibyte,
                saved_undo_list,
            );
            value
        }
        Err(flow) => {
            restore_empty_buffer_after_auto_coding_probe(
                eval,
                current_id,
                saved_multibyte,
                saved_undo_list,
            );
            return Err(flow);
        }
    };
    auto_coding_system_name(eval, value)
}

/// `(insert-file-contents FILENAME &optional VISIT BEG END REPLACE)`
///
/// Read file FILENAME and insert its contents into the current buffer
/// at point. Returns a list of `(FILENAME LENGTH)`. Mirrors GNU's
/// `Finsert_file_contents` (`src/fileio.c`).
pub(crate) fn builtin_insert_file_contents(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("insert-file-contents", &args, 1)?;
    expect_max_args("insert-file-contents", &args, 5)?;

    let coding_val = eval.visible_variable_value_or_nil("coding-system-for-read");
    let coding_system_for_read: Option<String> = match coding_val.kind() {
        ValueKind::Nil => None,
        ValueKind::Symbol(id) => Some(resolve_sym(id).to_owned()),
        // Coding-system names are ASCII protocol identifiers; decode lossily.
        ValueKind::String => coding_val
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes())),
        _ => None,
    };
    // GNU `Finsert_file_contents` runs `Fexpand_file_name` on the filename
    // first (dispatching the expand-file-name magic handler) before looking up
    // the insert-file-contents handler.
    let expanded = builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let resolved = resolve_filename_lisp_for_eval(eval, &expect_lisp_string_strict(&expanded)?);
    let mut handler_args = Vec::with_capacity(5);
    handler_args.push(Value::heap_string(resolved.clone()));
    handler_args.extend_from_slice(&args[1..]);
    // GNU calls the handler with the operation's full arglist (optional args
    // default to nil): insert-file-contents has arity 5.
    while handler_args.len() < 5 {
        handler_args.push(Value::NIL);
    }
    if let Some(result) = maybe_dispatch_resolved_file_handler(
        eval,
        "insert-file-contents",
        Some(&resolved),
        None,
        handler_args,
    )? {
        return Ok(result);
    }
    let visit = args.get(1).is_some_and(|v| v.is_truthy());
    let replace_requested = args.get(4).is_some_and(|v| !v.is_nil());

    let empty_undo_list_p = eval
        .buffers
        .current_buffer()
        .is_some_and(|buf| visit && buf.get_undo_list().is_nil() && buf.is_text_empty());

    let current_id = current_buffer_id_or_error(&eval.buffers)?;
    {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        if visit
            && (!args.get(2).is_none_or(|v| v.is_nil()) || !args.get(3).is_none_or(|v| v.is_nil()))
        {
            return Err(signal(
                "error",
                vec![Value::string("Attempt to visit less than an entire file")],
            ));
        }
        if visit && buf.base_buffer.is_some() {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Cannot do file visiting in an indirect buffer",
                )],
            ));
        }
        if visit && !replace_requested && !buf.is_text_empty() {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Cannot do file visiting in a non-empty buffer",
                )],
            ));
        }
        if crate::emacs_core::editfns::buffer_read_only_active_in_state(&eval.obarray, &[], buf) {
            return Err(signal(
                LispCondition::BufferReadOnly,
                vec![Value::make_buffer(current_id)],
            ));
        }
    }

    let contents_bytes = match fs::read(lisp_file_name_to_path_buf(&resolved)) {
        Ok(contents) => contents,
        Err(err) => {
            if visit {
                let _ = eval
                    .buffers
                    .set_buffer_file_name(current_id, Value::heap_string(resolved.clone()));
                let _ = eval.buffers.set_buffer_modified_flag(current_id, false);
                // GNU src/fileio.c:4194-4208: a file it was told to VISIT but
                // could not open still records a modtime --
                // `mtime = time_error_value (save_errno)` -- and only signals
                // after storing it (src/fileio.c:5307-5313).  That is why
                // `find-file' of a NEW file leaves `visited-file-modtime' at
                // -1 rather than 0, and why the first change to that buffer
                // records `(t . -1)'.
                if let Some(buf) = eval.buffers.get_mut(current_id) {
                    buf.set_visited_file_modtime(VisitedFileModtime::from_open_error(&err));
                    buf.modtime_size = None;
                }
                if empty_undo_list_p {
                    let _ = eval
                        .buffers
                        .configure_buffer_undo_list(current_id, Value::NIL);
                }
            }
            return Err(signal_file_action_error_value(
                err,
                "Opening input file",
                Value::heap_string(resolved.clone()),
            ));
        }
    };
    let file_len = contents_bytes.len() as i64;

    let begin = if args.get(2).is_some_and(|v| !v.is_nil()) {
        expect_file_offset(args.get(2).expect("checked above"))?
    } else {
        0
    };
    let mut end_off = if args.get(3).is_some_and(|v| !v.is_nil()) {
        expect_file_offset(args.get(3).expect("checked above"))?
    } else {
        file_len
    };

    if begin > file_len {
        return Err(signal(
            LispCondition::FileError,
            vec![
                Value::string("Read error"),
                Value::string("Bad address"),
                Value::heap_string(resolved.clone()),
            ],
        ));
    }
    if end_off > file_len {
        end_off = file_len;
    }
    if end_off < begin {
        end_off = begin;
    }

    let slice = &contents_bytes[begin as usize..end_off as usize];
    let multibyte = eval
        .buffers
        .get(current_id)
        .map(|buffer| buffer.get_multibyte())
        .unwrap_or(true);
    let current_buffer_was_empty = eval
        .buffers
        .get(current_id)
        .is_some_and(|buffer| buffer.is_text_empty());
    // GNU's decision ladder, in GNU's order (src/fileio.c:4317-4424 for a
    // non-empty buffer, :5023-5075 for the empty-buffer path).
    let decision = match coding_system_for_read.clone() {
        Some(name) => ReadCodingDecision::ForRead(name),
        None if !multibyte => ReadCodingDecision::Undecided,
        None => {
            let auto = if current_buffer_was_empty {
                decide_auto_coding_for_empty_insert_file_contents(
                    eval,
                    resolved.clone(),
                    slice,
                    current_id,
                )?
            } else {
                decide_auto_coding_for_insert_file_contents(eval, resolved.clone(), slice)?
            };
            match auto {
                Some(name) => ReadCodingDecision::AutoCoding(name),
                None => {
                    // GNU passes REPLACE through on the non-empty-buffer path
                    // and nil on the empty-buffer one (src/fileio.c:4417 vs
                    // :5063).
                    let replace_arg = if current_buffer_was_empty {
                        Value::NIL
                    } else {
                        args.get(4).copied().unwrap_or(Value::NIL)
                    };
                    match decide_file_name_coding_for_insert_file_contents(
                        eval,
                        resolved.clone(),
                        args.get(1).copied().unwrap_or(Value::NIL),
                        args.get(2).copied().unwrap_or(Value::NIL),
                        args.get(3).copied().unwrap_or(Value::NIL),
                        replace_arg,
                    )? {
                        Some(name) => ReadCodingDecision::FileNameAlist(name),
                        None => ReadCodingDecision::Undecided,
                    }
                }
            }
        }
    };
    let contents = decode_insert_file_contents(eval, slice, multibyte, decision.coding_system())?;
    let decoded_char_count = contents.char_count();

    let signal_change_hooks = !visit || replace_requested;
    let hide_visited_file_name_during_replace = visit
        && replace_requested
        && eval.buffers.get(current_id).is_some_and(|buffer| {
            buffer.full_emacs_byte_range() == buffer.accessible_emacs_byte_range()
        });
    // GNU's REPLACE branch reports only the *net* inserted chars (the unchanged
    // head/tail affixes are elided); a plain insert reports the full decoded
    // char count.  Re-reading byte-identical content under REPLACE therefore
    // yields `(FILE 0)`, not `(FILE FULL-COUNT)`.
    let net_inserted = insert_file_contents_into_current_buffer_in_state(
        eval,
        current_id,
        contents.text(),
        replace_requested,
        signal_change_hooks,
        hide_visited_file_name_during_replace,
    )?;
    let base_inserted_count = net_inserted.unwrap_or(decoded_char_count);

    // GNU `insert-file-contents' sets `last-coding-system-used' before
    // `after-insert-file-set-coding' derives `buffer-file-coding-system'.
    eval.set_variable("last-coding-system-used", Value::symbol(&contents.coding));

    let inserted_char_count = run_after_insert_file_pipeline(
        eval,
        current_id,
        visit,
        replace_requested,
        base_inserted_count,
    )?;

    if visit {
        let _ = eval
            .buffers
            .set_buffer_file_name(current_id, Value::heap_string(resolved.clone()));
        let _ = eval.buffers.set_buffer_modified_flag(current_id, false);
        // Store file modification time (GNU: insert-file-contents stores
        // current_buffer->modtime = mtime; current_buffer->modtime_size = st_size).
        if let Ok(meta) = std::fs::metadata(lisp_file_name_to_path_buf(&resolved))
            && let Ok(mtime) = meta.modified()
        {
            let dur = mtime
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            if let Some(buf) = eval.buffers.get_mut(current_id) {
                buf.set_visited_file_modtime(VisitedFileModtime::Known {
                    sec: dur.as_secs() as i64,
                    nsec: dur.subsec_nanos() as i32,
                });
                buf.modtime_size = Some(meta.len() as i64);
            }
        }
        if empty_undo_list_p {
            let _ = eval
                .buffers
                .configure_buffer_undo_list(current_id, Value::NIL);
        }
    }

    let value = Value::list(vec![
        Value::heap_string(resolved),
        Value::fixnum(inserted_char_count),
    ]);

    Ok(value)
}

/// Typed fallback for operations that share GNU's write-coding precedence but
/// differ when neither a dynamic nor a buffer coding system is selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WriteCodingFallback {
    Utf8,
    RawText,
}

/// Resolve the coding system to use for writing as an interned protocol value.
///
/// Priority:
/// 1. `coding-system-for-write` (if bound and non-nil)
/// 2. the buffer's visible `buffer-file-coding-system`
/// 3. the caller's operation-specific fallback
pub(crate) fn resolve_write_coding_system(
    eval: &super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    fallback: WriteCodingFallback,
) -> crate::encoding::RuntimeCodingSystem {
    crate::encoding::RuntimeCodingSystem::from_symbol(resolve_write_coding_system_symbol(
        eval, buffer_id, fallback,
    ))
}

fn resolve_write_coding_system_symbol(
    eval: &super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    fallback: WriteCodingFallback,
) -> super::intern::SymId {
    // 1. Check coding-system-for-write
    let coding_system_for_write = eval.visible_variable_value_or_nil("coding-system-for-write");
    if let Some(name) = coding_system_value_to_name(&coding_system_for_write) {
        return intern(&name);
    }

    // 2. Read the visible per-buffer slot, not only an explicitly local
    // override.  GNU's BVAR accessor sees the inherited slot value too.
    if let Some(buf) = eval.buffers.get(buffer_id)
        && let Some(val) = buf.buffer_local_value("buffer-file-coding-system")
        && let Some(name) = coding_system_value_to_name(&val)
    {
        return intern(&name);
    }

    match fallback {
        WriteCodingFallback::Utf8 => intern("utf-8"),
        WriteCodingFallback::RawText => intern("raw-text"),
    }
}

fn select_write_region_coding_system(
    eval: &mut super::eval::Context,
    buffer_id: crate::buffer::BufferId,
    from: Value,
    to: Value,
    filename: Value,
) -> Result<crate::encoding::RuntimeCodingSystem, Flow> {
    let fallback = resolve_write_coding_system_symbol(eval, buffer_id, WriteCodingFallback::Utf8);
    let selector = eval.visible_variable_value_or_nil("select-safe-coding-system-function");
    let Some(selector_id) = selector.as_symbol_id() else {
        return Ok(crate::encoding::RuntimeCodingSystem::from_symbol(fallback));
    };
    if !eval.obarray().fboundp_id(selector_id) {
        return Ok(crate::encoding::RuntimeCodingSystem::from_symbol(fallback));
    }

    // GNU `choose_write_coding_system` delegates the final choice to
    // `select-safe-coding-system-function`.  Among other safety checks, the
    // Lisp selector calls `find-auto-coding` on the exact region, so a
    // `coding:` declaration in generated Lisp takes precedence over the
    // inherited platform default.
    let selected = eval.funcall_general(
        selector,
        vec![from, to, Value::symbol(fallback), Value::NIL, filename],
    )?;
    let selected_id = selected
        .as_symbol_id()
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![selected]))?;
    let selected_name = resolve_sym(selected_id);
    if !eval.coding_systems.is_known_or_derived(selected_name) {
        return Err(signal(LispCondition::CodingSystemError, vec![selected]));
    }
    Ok(crate::encoding::RuntimeCodingSystem::from_symbol(
        selected_id,
    ))
}

/// Extract a coding system name from a `Value` (symbol or string).
/// Returns `None` for nil / unrecognized types.
fn coding_system_value_to_name(val: &Value) -> Option<String> {
    match val.kind() {
        ValueKind::Nil => None,
        ValueKind::Symbol(id) => {
            let name = resolve_sym(id).to_owned();
            if name == "nil" { None } else { Some(name) }
        }
        ValueKind::String => {
            // Coding-system names are ASCII protocol identifiers; decode lossily.
            let name = val
                .as_lisp_string()
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                .unwrap_or_default();
            if name.is_empty() || name == "nil" {
                None
            } else {
                Some(name)
            }
        }
        _ => None,
    }
}

/// `(write-region START END FILENAME &optional APPEND VISIT LOCKNAME MUSTBENEW)`
///
/// Write the region between START and END to FILENAME. If START is
/// nil, writes the entire buffer. Mirrors GNU `Fwrite_region`
/// (`src/fileio.c`).
pub(crate) fn builtin_write_region(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("write-region", &args, 3)?;
    expect_max_args("write-region", &args, 7)?;
    // GNU `Fwrite_region` runs `Fexpand_file_name` on FILENAME first, dispatching
    // the expand-file-name magic handler before the write-region handler.
    let expanded = builtin_expand_file_name(eval, vec![args[2], Value::NIL])?;
    let resolved = resolve_filename_lisp_for_eval(eval, &expect_lisp_string_strict(&expanded)?);
    let visit_argument = args.get(4).copied().unwrap_or(Value::NIL);
    let visit = WriteRegionVisit::from_lisp(eval, visit_argument, &resolved);
    let default_lock_name = visit.file_name().clone();

    // GNU `Fwrite_region`, immediately after `Fexpand_file_name` and before the
    // file-name-handler dispatch:
    //
    //   if (!NILP (mustbenew) && !EQ (mustbenew, Qexcl))
    //     barf_or_query_if_file_exists (filename, false, "overwrite", true, true);
    //
    // `excl` is handled later via the `O_EXCL` open flag (it produces an
    // EEXIST -> `file-already-exists` error at write time); every other
    // non-nil MUSTBENEW asks for interactive confirmation here.
    let mustbenew = args.get(6).copied().unwrap_or(Value::NIL);
    let mustbenew_is_excl = mustbenew
        .as_symbol_name()
        .is_some_and(|name| name == "excl");
    if !mustbenew.is_nil() && !mustbenew_is_excl {
        barf_or_query_if_file_exists(eval, &resolved, false, "overwrite", true, true)?;
    }

    let op = Value::symbol("write-region");
    let handler = find_file_name_handler_lisp_for_eval(eval, &resolved, op);
    if !handler.is_nil() {
        // GNU calls the handler with the full arglist
        // (START END FILENAME APPEND VISIT LOCKNAME MUSTBENEW), defaulting
        // LOCKNAME to FILENAME.
        let mut call_args = Vec::with_capacity(8);
        call_args.push(op);
        call_args.extend_from_slice(&args);
        while call_args.len() < 8 {
            call_args.push(Value::NIL);
        }
        call_args[3] = Value::heap_string(resolved.clone());
        if call_args[6].is_nil() {
            call_args[6] = Value::heap_string(default_lock_name.clone());
        }
        return eval.funcall_general(handler, call_args);
    }
    if handler.is_nil()
        && let Some(visit_arg) = args.get(4).and_then(|value| eval.lisp_string(*value))
    {
        let visit_handler = find_file_name_handler_lisp_for_eval(eval, visit_arg, op);
        if !visit_handler.is_nil() {
            let mut call_args = Vec::with_capacity(args.len() + 1);
            call_args.push(op);
            call_args.extend_from_slice(&args);
            while call_args.len() < 8 {
                call_args.push(Value::NIL);
            }
            call_args[3] = Value::heap_string(resolved.clone());
            if call_args[6].is_nil() {
                call_args[6] = Value::heap_string(default_lock_name.clone());
            }
            return eval.funcall_general(visit_handler, call_args);
        }
    }

    let resolved_path = lisp_file_name_to_path_buf(&resolved);
    // GNU `Fwrite_region`:
    //   open_flags |= EQ (mustbenew, Qexcl) ? O_EXCL
    //               : !NILP (append) ? 0 : O_TRUNC;
    // `excl` forces an exclusive create (O_EXCL) that takes precedence over
    // APPEND; a pre-existing file then yields EEXIST -> `file-already-exists`.
    let append_mode = if mustbenew_is_excl {
        FileWriteMode::Excl
    } else {
        match args.get(3) {
            Some(value) if value.is_fixnum() || value.is_char() => {
                FileWriteMode::Seek(expect_file_offset(value)? as u64)
            }
            Some(value) if value.is_truthy() => FileWriteMode::Append,
            _ => FileWriteMode::Truncate,
        }
    };
    let current_id = current_buffer_id_or_error(&eval.buffers)?;

    if visit.visited_file().is_some() {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        if buf.base_buffer.is_some() {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Cannot do file visiting in an indirect buffer",
                )],
            ));
        }
    }

    // GNU builds annotations before coding-system selection and before opening
    // the destination (`src/fileio.c:5596-5627`).  The resulting plan owns the
    // exact source choice and already-interspersed character stream.
    let prepared = prepare_write_region(
        eval,
        current_id,
        args[0],
        args.get(1).copied().unwrap_or(Value::NIL),
    )?;
    let content = prepared.content.clone();

    // GNU `Fwrite_region` runs the chosen coding system through
    // `Fcheck_coding_system`; an explicit but unknown `coding-system-for-write`
    // (e.g. `utf-8-sig`) signals `coding-system-error` before anything is
    // written.  Validate it here so we don't silently produce an empty file.
    let coding_for_write = eval.visible_variable_value_or_nil("coding-system-for-write");
    if !coding_for_write.is_nil()
        && let Some(name) = coding_for_write.as_symbol_name()
        && name != "nil"
        && !eval.coding_systems.is_known_or_derived(name)
    {
        return Err(signal(
            LispCondition::CodingSystemError,
            vec![coding_for_write],
        ));
    }

    let (coding_from, coding_to) = prepared.source.coding_bounds();

    // --- Encode using GNU's complete write-coding selection protocol. ---
    // The Lisp selector detects content declarations such as the
    // `utf-8-emacs-unix` trailer emitted for generated Lisp files.
    let coding_system = select_write_region_coding_system(
        eval,
        prepared.source.coding_buffer(),
        coding_from,
        coding_to,
        Value::heap_string(resolved.clone()),
    )?;
    let encoded = crate::encoding::encode_file_region_in_context(eval, content, coding_system)?;

    // GNU `write_region' locks LOCKNAME after coding-system selection and
    // keeps that lock scoped to the native open/write/close operation.  The
    // buffer may already own the lock from its first modification; re-locking
    // our own file is intentionally idempotent.
    let lock_name = match args.get(5) {
        Some(value) if !value.is_nil() => {
            resolve_filename_lisp_for_eval(eval, &expect_lisp_string_strict(value)?)
        }
        _ => default_lock_name,
    };
    super::filelock::lock_file(eval, &lock_name)?;

    // --- Write encoded bytes and handle fsync ---
    let write_result = (|| {
        let file = write_bytes_to_file_with_mode(&encoded.bytes, &resolved_path, append_mode)
            .map_err(|err| {
                // GNU `Fwrite_region` reports *any* open failure via
                // `report_file_errno ("Opening output file", filename, open_errno)`
                // (fileio.c:5656) — the action is always "Opening output file",
                // never "Writing to" (which GNU reserves for `a_write` errors).
                // `get_file_errno_data` then handles errno specially: a MUSTBENEW
                // =`excl` collision is EEXIST -> `(file-already-exists "File exists"
                // FILENAME)` (action omitted), ENOENT -> `file-missing`, etc.
                signal_file_action_error_value(
                    err,
                    "Opening output file",
                    Value::heap_string(resolved.clone()),
                )
            })?;

        // fsync after write unless write-region-inhibit-fsync is non-nil.
        let inhibit_fsync = eval
            .visible_variable_value_or_nil("write-region-inhibit-fsync")
            .is_truthy();
        if !inhibit_fsync {
            file.sync_all().map_err(|err| {
                signal_file_action_error_value(
                    err,
                    "Writing to",
                    Value::heap_string(resolved.clone()),
                )
            })?;
        }
        drop(file);

        let visiting_modtime = if visit.visited_file().is_some() {
            let meta = std::fs::metadata(&resolved_path).map_err(|err| {
                signal_file_action_error_value(
                    err,
                    "Writing to",
                    Value::heap_string(resolved.clone()),
                )
            })?;
            let mtime = meta.modified().map_err(|err| {
                signal_file_action_error_value(
                    err,
                    "Writing to",
                    Value::heap_string(resolved.clone()),
                )
            })?;
            let dur = mtime
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            Some((
                dur.as_secs() as i64,
                dur.subsec_nanos() as i32,
                meta.len() as i64,
            ))
        } else {
            None
        };
        Ok::<_, Flow>(visiting_modtime)
    })();

    // GNU runs the post-annotation callback after the native write and before
    // unwinding back to the original buffer (`src/fileio.c:5804-5823`).  An
    // open error occurs before that phase and therefore skips the callback.
    let post_annotation_result = if write_result.is_ok() {
        prepared.run_post_annotation_functions(eval)
    } else {
        eval.restore_current_buffer_if_live(current_id);
        Ok(())
    };

    // Always attempt the matching unlock, including write and annotation
    // cleanup errors. Preserve the primary write error first, then the post
    // annotation error, if cleanup also fails.
    let unlock_result = super::filelock::unlock_file(eval, &lock_name);
    let visiting_modtime = match (write_result, post_annotation_result, unlock_result) {
        (Err(write_error), _, _) => return Err(write_error),
        (Ok(_), Err(annotation_error), _) => return Err(annotation_error),
        (Ok(_), Ok(_), Err(unlock_error)) => return Err(unlock_error),
        (Ok(visiting_modtime), Ok(_), Ok(_)) => visiting_modtime,
    };

    if let Some(visit_path) = visit.visited_file().cloned() {
        let _ = eval
            .buffers
            .set_buffer_file_name(current_id, Value::heap_string(visit_path));
        let _ = eval.buffers.set_buffer_modified_flag(current_id, false);
        if let Some((sec, nsec, size)) = visiting_modtime
            && let Some(buf) = eval.buffers.get_mut(current_id)
        {
            buf.set_visited_file_modtime(VisitedFileModtime::Known { sec, nsec });
            buf.modtime_size = Some(size);
        }
    }

    eval.set_variable(
        "last-coding-system-used",
        Value::symbol(encoded.coding_system),
    );
    if visit.reports_completion() && !eval.noninteractive() {
        let completion =
            WriteRegionCompletion::from_append_argument(args.get(3).copied().unwrap_or(Value::NIL));
        crate::emacs_core::builtins::builtin_message(
            eval,
            vec![
                Value::string(completion.message_format()),
                Value::heap_string(visit.file_name().clone()),
            ],
        )?;
    }
    Ok(Value::NIL)
}

/// (find-file-noselect FILENAME &optional NOWARN RAWFILE) -> buffer
///
/// Read file FILENAME into a buffer and return the buffer.
/// If a buffer visiting FILENAME already exists, return it.
/// Does not select the buffer.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_find_file_noselect(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("find-file-noselect", &args, 1)?;
    expect_max_args("find-file-noselect", &args, 4)?;
    let abs_path = resolve_filename_lisp_for_eval(eval, &expect_lisp_string_strict(&args[0])?);
    let abs_path_buf = lisp_file_name_to_path_buf(&abs_path);
    let rawfile = args.get(2).is_some_and(|value| !value.is_nil());

    // Check if there's already a buffer visiting this file
    for buf_id in eval.buffers.buffer_list() {
        if let Some(buf) = eval.buffers.get(buf_id)
            && buf
                .file_name_value()
                .as_lisp_string()
                .is_some_and(|name| name == &abs_path)
        {
            return Ok(Value::make_buffer(buf_id));
        }
    }

    // Derive buffer name from file name.  Buffer names are display strings, so
    // a lossy decode of the file-name bytes is acceptable here.
    let buf_name = crate::emacs_core::emacs_char::to_utf8_lossy(
        lisp_file_name_nondirectory(&abs_path).as_bytes(),
    );
    let unique_name = eval.buffers.generate_new_buffer_name(&buf_name);
    let buf_id = eval.buffers.create_buffer(&unique_name);

    let saved_current = eval.buffers.current_buffer_id();
    let open_result = (|| -> EvalResult {
        eval.switch_current_buffer(buf_id)?;

        let visit_error = if file_exists_path(&abs_path_buf) {
            builtin_insert_file_contents(
                eval,
                vec![Value::heap_string(abs_path.clone()), Value::T],
            )?;
            let _ = eval
                .buffers
                .goto_buffer_emacs_byte_pos(buf_id, EmacsBytePos::new(0));
            Value::NIL
        } else {
            Value::T
        };

        let _ = eval
            .buffers
            .set_buffer_file_name(buf_id, Value::heap_string(abs_path.clone()));
        let truename = file_truename_lisp(&abs_path, None)?;
        let _ = eval
            .buffers
            .set_buffer_file_truename(buf_id, Value::heap_string(truename));
        if let Some(default_directory) = lisp_file_name_directory(&abs_path) {
            let _ = eval.buffers.set_buffer_local_property(
                buf_id,
                "default-directory",
                Value::heap_string(default_directory),
            );
        }
        let _ = eval.buffers.set_buffer_modified_flag(buf_id, false);

        // GNU `find-file-noselect-1` routes normal visits through
        // `after-find-file`, which applies major-mode detection,
        // file-local variables, and `find-file-hook`. Plain
        // `Context::new()` test evaluators don't load that Lisp yet,
        // so only call it when the runtime has it installed.
        if !rawfile && eval.obarray().symbol_function("after-find-file").is_some() {
            let warn = Value::bool_val(args.get(1).is_none_or(|value| value.is_nil()));
            let _ =
                eval.funcall_general(Value::symbol("after-find-file"), vec![visit_error, warn])?;
        }

        Ok(Value::make_buffer(buf_id))
    })();

    if let Some(prev_id) = saved_current {
        eval.restore_current_buffer_if_live(prev_id);
    }

    open_result
}

// ===========================================================================
// Auto-save support
// ===========================================================================

/// Compute the auto-save file name for a buffer.
///
/// For visited files: `#filename#` in the same directory.
/// For non-visited buffers: `#*buffername*#` in the auto-save-list-file-prefix
/// directory (or temporary-file-directory as fallback).
fn make_auto_save_file_name_for_buffer(
    obarray: &Obarray,
    buf: &crate::buffer::Buffer,
) -> crate::heap_types::LispString {
    if let Some(file_name) = buf.file_name_lisp_string() {
        // Visited file: #dir/filename# -> dir/#filename#
        let dir = lisp_file_name_directory(file_name)
            .unwrap_or_else(|| file_name_lisp_from_bytes(Vec::new(), file_name.is_multibyte()));
        let base =
            wrap_ascii_around_lisp_string(&lisp_file_name_nondirectory(file_name), b"#", b"#");
        concat_file_name_lisp(&dir, &base)
    } else {
        // Non-visited buffer: #*buffername*# in prefix dir or temp dir
        let dir = obarray
            .symbol_value("auto-save-list-file-prefix")
            .and_then(|value| value.as_lisp_string().cloned())
            .and_then(|value| {
                if value.as_bytes().is_empty() {
                    None
                } else {
                    lisp_file_name_directory(&value)
                }
            })
            .or_else(|| {
                obarray
                    .symbol_value("temporary-file-directory")
                    .and_then(|value| value.as_lisp_string().cloned())
            })
            .unwrap_or_else(|| crate::heap_types::LispString::from_utf8("/tmp/"));
        let name_value = buf.name_value();
        let name = name_value
            .as_lisp_string()
            .expect("buffer name must be a Lisp string");
        let mut safe_name_bytes = name.as_bytes().to_vec();
        for byte in &mut safe_name_bytes {
            if *byte == b'/' {
                *byte = b'!';
            }
        }
        let safe_name = file_name_lisp_from_bytes(safe_name_bytes, name.is_multibyte());
        let base = wrap_ascii_around_lisp_string(&safe_name, b"#*", b"*#");
        concat_file_name_lisp(&lisp_file_name_as_directory(&dir), &base)
    }
}

// `make-auto-save-file-name' is not here.  GNU has no C version: it is
// `(defun make-auto-save-file-name () ...)' at lisp/files.el:7699, over
// `auto-save-file-name-transforms', and it only RETURNS a name -- setting
// `buffer-auto-save-file-name' is `auto-save-mode's job (lisp/files.el), and
// C only reads the field (`BVAR (b, auto_save_file_name)', src/fileio.c:6406).
// The Rust subr wrote the field itself (DIVERGENCES.md 152).
// `make_auto_save_file_name_for_buffer' below is still reached from
// `builtin_do_auto_save', which computes a name when the buffer has none.

/// `(do-auto-save &optional NO-MESSAGE CURRENT)` -> nil
///
/// Auto-save all buffers that need it.
/// If NO-MESSAGE is non-nil, suppress the "Auto-saving..." message.
/// If CURRENT is non-nil, only auto-save the current buffer.
pub(crate) fn builtin_do_auto_save(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("do-auto-save", &args, 0)?;
    expect_max_args("do-auto-save", &args, 2)?;

    let _no_message = args.first().is_some_and(|v| v.is_truthy());
    let current_only = args.get(1).is_some_and(|v| v.is_truthy());

    // GNU `Fdo_auto_save` runs this before inspecting or snapshotting any
    // buffer, so edits made by the hook belong to this auto-save pass.  Use
    // the same safe named-hook boundary as the command loop: one broken hook
    // is reported and removed without suppressing the remaining functions.
    eval.safe_run_hook_if_bound("auto-save-hook")?;

    let auto_save_visited = eval
        .obarray
        .symbol_value("auto-save-visited-file-name")
        .is_some_and(|v| v.is_truthy());

    // Collect buffer ids to process
    let buffer_ids: Vec<crate::buffer::BufferId> = if current_only {
        eval.buffers.current_buffer_id().into_iter().collect()
    } else {
        eval.buffers.buffer_list()
    };

    for buf_id in buffer_ids {
        // Gather info from the buffer (immutable borrow)
        let (auto_save_name, file_name, content_bytes, content_len) = {
            let Some(buf) = eval.buffers.get(buf_id) else {
                continue;
            };

            // Skip internal buffers (name starts with space)
            if buf.name_starts_with_space() {
                continue;
            }

            // Skip indirect buffers
            if buf.base_buffer.is_some() {
                continue;
            }

            // Check buffer-saved-size: if negative, auto-save is disabled for
            // this buffer
            if let Some(saved_size_val) = buf.get_buffer_local("buffer-saved-size")
                && let Some(n) = saved_size_val.as_fixnum()
                && n < 0
            {
                continue;
            }

            // Buffer must be modified since last auto-save
            // (modified_tick > autosave_modified_tick means unsaved changes)
            if buf.autosave_modified_tick >= buf.modified_tick() {
                continue;
            }

            // Buffer must actually be modified
            if !buf.is_modified() {
                continue;
            }

            // Determine the auto-save target
            let auto_name = buf.auto_save_file_name_lisp_string().cloned();
            let visit_name = buf.file_name_lisp_string().cloned();
            let mut bytes = Vec::new();
            buf.copy_emacs_byte_range_to(buf.full_emacs_byte_range(), &mut bytes);

            (
                auto_name,
                visit_name,
                bytes,
                buf.total_emacs_byte_len().get() as i64,
            )
        };

        // Determine which file to write to
        let target = if auto_save_visited {
            // Save to visited file if auto-save-visited-file-name is set
            file_name.clone()
        } else {
            auto_save_name.clone()
        };

        let Some(target_path) = target else {
            // No auto-save file name and no visited file -- generate one
            let auto_name = {
                let buf = eval.buffers.get(buf_id).unwrap();
                make_auto_save_file_name_for_buffer(&eval.obarray, buf)
            };
            // Set the auto-save name on the buffer
            let auto_name_value = Value::heap_string(auto_name.clone());
            if let Some(buf) = eval.buffers.get_mut(buf_id) {
                buf.set_buffer_local("buffer-auto-save-file-name", auto_name_value);
                buf.set_auto_save_file_name_value(auto_name_value);
            }
            let _ = write_bytes_to_file_with_mode(
                &content_bytes,
                &lisp_file_name_to_path_buf(&auto_name),
                FileWriteMode::Truncate,
            );
            let _ = eval.buffers.set_buffer_auto_saved(buf_id);
            // Update buffer-saved-size
            if let Some(buf) = eval.buffers.get_mut(buf_id) {
                buf.set_buffer_local("buffer-saved-size", Value::fixnum(content_len));
            }
            continue;
        };

        // Write the buffer content to the target file
        if write_bytes_to_file_with_mode(
            &content_bytes,
            &lisp_file_name_to_path_buf(&target_path),
            FileWriteMode::Truncate,
        )
        .is_ok()
        {
            // Mark the buffer as auto-saved
            let _ = eval.buffers.set_buffer_auto_saved(buf_id);
            // Update buffer-saved-size
            if let Some(buf) = eval.buffers.get_mut(buf_id) {
                buf.set_buffer_local("buffer-saved-size", Value::fixnum(content_len));
            }
        }
    }

    // GNU `Fdo_auto_save` ends with `record_auto_save`, regardless of whether
    // any buffer needed writing. Keeping the record at the primitive boundary
    // also makes direct Lisp calls and idle-triggered calls obey the same
    // "new input is required before another automatic pass" invariant.
    eval.command_loop.last_auto_save_input_events = eval.num_nonmacro_input_events();

    Ok(Value::NIL)
}

// ===========================================================================
// Bootstrap variables
// ===========================================================================

pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    let temporary_file_directory = std::env::temp_dir().to_string_lossy().to_string();
    obarray.set_symbol_value("file-name-coding-system", Value::NIL);
    obarray.make_special("file-name-coding-system");
    obarray.set_symbol_value("default-file-name-coding-system", Value::NIL);
    obarray.make_special("default-file-name-coding-system");
    obarray.set_symbol_value("set-auto-coding-for-load", Value::NIL);
    obarray.set_symbol_value("file-name-handler-alist", Value::NIL);
    obarray.make_special("file-name-handler-alist");
    // fileio.c:6856-6944 DEFVAR_LISP cluster, all initialized to nil.
    obarray.define_special_variable("set-auto-coding-function", Value::NIL);
    obarray.define_c_hook_variable("after-insert-file-functions");
    obarray.define_c_hook_variable("write-region-annotate-functions");
    obarray.define_special_variable("write-region-post-annotation-function", Value::NIL);
    obarray.define_special_variable("write-region-annotations-so-far", Value::NIL);
    obarray.set_symbol_value("inhibit-file-name-handlers", Value::NIL);
    obarray.make_special("inhibit-file-name-handlers");
    obarray.set_symbol_value("inhibit-file-name-operation", Value::NIL);
    obarray.make_special("inhibit-file-name-operation");
    obarray.set_symbol_value("directory-abbrev-alist", Value::NIL);
    obarray.set_symbol_value("auto-save-list-file-name", Value::NIL);
    obarray.make_special("auto-save-list-file-name");
    obarray.set_symbol_value("auto-save-list-file-prefix", Value::NIL);
    obarray.set_symbol_value("auto-save-visited-file-name", Value::NIL);
    // fileio.c:6944 DEFVAR_LISP, init nil.
    obarray.define_special_variable("auto-save-include-big-deletions", Value::NIL);
    obarray.set_symbol_value("small-temporary-file-directory", Value::NIL);
    obarray.set_symbol_value("auto-save-file-name-transforms", Value::NIL);
    obarray.set_symbol_value(
        "temporary-file-directory",
        Value::string(temporary_file_directory),
    );
    obarray.make_special("temporary-file-directory");

    // Backup-related variables
    obarray.set_symbol_value("make-backup-files", Value::T);
    obarray.set_symbol_value("backup-inhibited", Value::NIL);
    obarray.set_symbol_value("version-control", Value::NIL);
    obarray.set_symbol_value("backup-directory-alist", Value::NIL);
    // files.el: defvar for vc-hooks.el and locate-dominating-file
    obarray.set_symbol_value(
        "locate-dominating-stop-dir-regexp",
        Value::string(r"\`\(?:[\\/][\\/][^\\/]+[\\/]\|/\(?:net\|afs\|\.\.\.\)/\)\'"),
    );
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/fix8.rs"]
mod fix8_tests;
