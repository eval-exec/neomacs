//! Pure file-name expansion helpers (stage-4 K2 oracleb batch 4).
//!
//! The string-native `expand-file-name` chain and its HOME/`~` resolution
//! were moved here verbatim from `system/fileio` so the evaluator files that
//! need pure path expansion (treesit's inherited-directory predicate) do not
//! name the core system module. The byte-native lexical chain and visible
//! default-directory resolver also live here; system reuses these definitions
//! while retaining its handler-aware Lisp entry and filesystem operations.

use crate::emacs_core::eval::{Context, VariableView};
use neovm_value::system_vocab::file_names::file_name_absolute_bytes_p;
use neovm_value::value::OrderedRuntimeBindingMap;
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

// Byte-faithful lexical expansion and resolver, moved from system/fileio.

/// Byte-native `expand-file-name` core.
///
/// Operates on Emacs internal-encoding bytes so raw eight-bit file-name bytes
/// survive byte-exactly (GNU keeps file names as Lisp strings; on Unix
/// ENCODE_FILE / DECODE_FILE are byte-identity).  Path manipulation is
/// ASCII-structural (`/`, `~`, `.`); non-ASCII byte8 multibyte sequences are
/// non-special and pass through untouched.  This mirrors
/// `expand_file_name_with_home_inner` but never round-trips through a
/// storage-`&str`.
#[doc(hidden)]
pub fn expand_file_name_bytes_with_home(
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
    // One byte of room for the NUL the result's Lisp string appends.
    let mut out = Vec::with_capacity(bytes.len() + 1);
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

#[doc(hidden)]
pub fn file_name_lisp_from_bytes(
    bytes: Vec<u8>,
    multibyte: bool,
) -> neovm_value::heap_types::LispString {
    if multibyte {
        neovm_value::heap_types::LispString::from_emacs_bytes(bytes)
    } else {
        neovm_value::heap_types::LispString::from_unibyte(bytes)
    }
}

#[doc(hidden)]
pub fn fallback_root_default_directory() -> neovm_value::heap_types::LispString {
    neovm_value::heap_types::LispString::from_utf8("/")
}

#[doc(hidden)]
pub fn expand_file_name_result_multibyte(
    name: &neovm_value::heap_types::LispString,
    default_directory: &neovm_value::heap_types::LispString,
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

#[doc(hidden)]
pub fn expand_file_name_lisp(
    name: &neovm_value::heap_types::LispString,
    default_directory: Option<&neovm_value::heap_types::LispString>,
) -> neovm_value::heap_types::LispString {
    expand_file_name_lisp_with_home(name, default_directory, None)
}

/// Byte-faithful file-name expansion with an explicit HOME snapshot.
///
/// Process executable lookup uses this form because GNU `openp` expands every
/// search candidate through `Fexpand_file_name`, whose `~` resolution observes
/// the dynamically bound Lisp `process-environment`, not Rust's host
/// environment.  Keeping HOME explicit also prevents callers from growing
/// their own partial tilde-expansion rules.
#[doc(hidden)]
pub fn expand_file_name_lisp_with_home(
    name: &neovm_value::heap_types::LispString,
    default_directory: Option<&neovm_value::heap_types::LispString>,
    home_directory: Option<&[u8]>,
) -> neovm_value::heap_types::LispString {
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

#[cfg(windows)]
#[doc(hidden)]
pub fn lisp_file_name_absolute_system_p(filename: &neovm_value::heap_types::LispString) -> bool {
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
#[doc(hidden)]
pub fn lisp_file_name_absolute_system_p(filename: &neovm_value::heap_types::LispString) -> bool {
    filename.as_bytes().first() == Some(&b'/')
}

/// GNU's `BVAR (current_buffer, directory)` -- `default-directory` is
/// `DEFVAR_PER_BUFFER` (`src/buffer.c:5392`), so there is no global to fall
/// back to. The obarray fallback this used to end with could never fire: the
/// name is installed as a `LispFwdType::BufferObj` forwarder, whose buffer-less
/// `load()` is `None` by construction. Ledger 196.
#[doc(hidden)]
pub fn default_directory_lisp_in_state(
    vars: VariableView<'_>,
    _dynamic: &[OrderedRuntimeBindingMap],
    buffers: &neovm_edit::buffer::BufferManager,
) -> Option<neovm_value::heap_types::LispString> {
    vars.value_in_buffer_by_name(buffers.current_buffer(), "default-directory")
        .and_then(|val| val.as_lisp_string().cloned())
}

#[doc(hidden)]
pub fn resolve_filename_lisp_in_state(
    vars: VariableView<'_>,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &neovm_edit::buffer::BufferManager,
    filename: &neovm_value::heap_types::LispString,
) -> neovm_value::heap_types::LispString {
    if lisp_file_name_absolute_system_p(filename) {
        return filename.clone();
    }
    let default_dir = default_directory_lisp_in_state(vars, dynamic, buffers)
        .unwrap_or_else(fallback_root_default_directory);
    expand_file_name_lisp(filename, Some(&default_dir))
}

#[doc(hidden)]
pub fn resolve_filename_lisp_for_eval(
    eval: &Context,
    filename: &neovm_value::heap_types::LispString,
) -> neovm_value::heap_types::LispString {
    resolve_filename_lisp_in_state(eval.variables(), &[], &eval.edit.buffers, filename)
}

#[cfg(test)]
mod lexical_resolver_tests {
    use super::*;
    use crate::emacs_core::eval::Context;
    use neovm_edit::emacs_core::edit_host::EditHost;
    use neovm_value::heap_types::LispString;
    use neovm_value::value::Value;

    fn base_directory() -> &'static str {
        if cfg!(windows) {
            "C:/resolve-lexical/"
        } else {
            "/resolve-lexical/"
        }
    }

    #[test]
    fn eval_unit_resolves_relative_name_using_visible_default_directory() {
        let mut context = Context::new(crate::test_utils::unit_runtime());
        context.set_variable("default-directory", Value::unibyte_string(base_directory()));
        let request = LispString::from_utf8("part/../leaf");
        let resolved = crate::emacs_core::eval::lifecycle::resolve_filename(&context, &request);
        let expected = format!("{}leaf", base_directory());
        assert_eq!(resolved.as_bytes(), expected.as_bytes());
        assert!(!resolved.is_multibyte());
    }

    #[test]
    fn lexical_resolver_preserves_absolute_spelling_and_raw_file_name_bytes() {
        let mut context = Context::new(crate::test_utils::unit_runtime());
        context.set_variable("default-directory", Value::unibyte_string(base_directory()));
        // Internal resolver fast path is intentionally preserved by this move:
        // already system-absolute names are returned byte-for-byte.
        let absolute = if cfg!(windows) {
            b"C:/unchanged/../leaf/".to_vec()
        } else {
            b"/unchanged/../leaf/".to_vec()
        };
        let absolute = LispString::from_unibyte(absolute);
        let resolved = EditHost::resolve_filename(&context, &absolute);
        assert_eq!(resolved.as_bytes(), absolute.as_bytes());
        assert!(!resolved.is_multibyte());

        let raw = LispString::from_unibyte(vec![0xff]);
        let resolved = EditHost::resolve_filename(&context, &raw);
        let mut expected = base_directory().as_bytes().to_vec();
        expected.push(0xff);
        assert_eq!(resolved.as_bytes(), expected.as_slice());
        assert!(!resolved.is_multibyte());

        let unicode = LispString::from_utf8("λ");
        let resolved = EditHost::resolve_filename(&context, &unicode);
        let expected = format!("{}λ", base_directory());
        assert_eq!(resolved.as_bytes(), expected.as_bytes());
        assert!(resolved.is_multibyte());
    }

    #[test]
    fn legacy_lifecycle_resolver_alias_is_the_canonical_lower_definition() {
        type Resolver = fn(&Context, &LispString) -> LispString;
        let legacy: Resolver = crate::emacs_core::eval::lifecycle::resolve_filename;
        let canonical: Resolver = resolve_filename_lisp_for_eval;
        assert!(std::ptr::fn_addr_eq(legacy, canonical));
    }
}
