//! File I/O primitives for the Elisp VM.
//!
//! Provides path manipulation, file predicates, read/write operations,
//! directory operations, and file attribute queries.

use std::collections::VecDeque;
#[cfg(unix)]
use std::ffi::{CStr, CString};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

use super::error::{signal, EvalResult, Flow};
use super::eval::Evaluator;
use super::value::{list_to_vec, Value};

// ===========================================================================
// Path operations (pure, no evaluator needed)
// ===========================================================================

/// Expand FILE relative to DEFAULT_DIR (or the current working directory).
/// Handles `~` expansion and absolute path detection.
pub fn expand_file_name(name: &str, default_dir: Option<&str>) -> String {
    // Handle ~ expansion
    let expanded = if name.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let home_str = home.to_string_lossy();
            format!("{}{}", home_str, &name[1..])
        } else {
            name.to_string()
        }
    } else if name == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            home.to_string_lossy().into_owned()
        } else {
            name.to_string()
        }
    } else {
        name.to_string()
    };

    let path = Path::new(&expanded);
    let preserve_trailing_slash = expanded.ends_with('/');

    // If already absolute, just clean it up
    if path.is_absolute() {
        let mut cleaned = clean_path(&PathBuf::from(&expanded));
        if preserve_trailing_slash && !cleaned.ends_with('/') {
            cleaned.push('/');
        }
        return cleaned;
    }

    // Resolve relative to default_dir or cwd
    let base = if let Some(dir) = default_dir {
        // Recursively expand the default dir too (handles ~ in dir)
        let expanded_dir = expand_file_name(dir, None);
        PathBuf::from(expanded_dir)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
    };

    let joined = base.join(&expanded);
    let mut cleaned = clean_path(&joined);
    if preserve_trailing_slash && !cleaned.ends_with('/') {
        cleaned.push('/');
    }
    cleaned
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

/// Clean up a path by resolving `.` and `..` components without touching the
/// filesystem (no symlink resolution).
fn clean_path(path: &Path) -> String {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {} // skip "."
            std::path::Component::ParentDir => {
                // Pop the last component if possible
                if !components.is_empty() {
                    components.pop();
                }
            }
            other => components.push(other),
        }
    }
    let result: PathBuf = components.iter().collect();
    result.to_string_lossy().into_owned()
}

/// Return the directory part of FILENAME, or None if there is no directory part.
/// Like Emacs `file-name-directory`: includes the trailing slash.
pub fn file_name_directory(filename: &str) -> Option<String> {
    // Emacs: if the filename ends with /, the whole thing is the directory part
    if filename.ends_with('/') {
        return if filename.is_empty() {
            None
        } else {
            Some(filename.to_string())
        };
    }
    // Find the last /
    match filename.rfind('/') {
        Some(pos) => Some(filename[..=pos].to_string()),
        None => None,
    }
}

/// Return the non-directory part of FILENAME.
/// Like Emacs `file-name-nondirectory`.
pub fn file_name_nondirectory(filename: &str) -> String {
    // Emacs: if the filename ends with /, return ""
    if filename.ends_with('/') {
        return String::new();
    }
    match filename.rfind('/') {
        Some(pos) => filename[pos + 1..].to_string(),
        None => filename.to_string(),
    }
}

/// Return the extension of FILENAME.
/// When PERIOD is nil, returns extension without the leading dot, or nil if missing.
/// When PERIOD is non-nil, returns extension with the leading dot, or an empty string if missing.
pub fn file_name_extension(filename: &str, period: bool) -> Option<String> {
    if filename.ends_with('/') {
        return if period { Some(String::new()) } else { None };
    }
    let path = Path::new(filename);
    let extension = path.extension().map(|e| e.to_string_lossy().into_owned());
    if period {
        Some(match extension {
            Some(ext) => format!(".{ext}"),
            None => String::new(),
        })
    } else {
        extension
    }
}

/// Return FILENAME without its extension.
pub fn file_name_sans_extension(filename: &str) -> String {
    // Emacs keeps directory-path forms unchanged.
    if filename.ends_with('/') {
        return filename.to_string();
    }
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            let parent_str = parent.to_string_lossy();
            format!("{}/{}", parent_str, stem)
        }
        _ => stem,
    }
}

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

/// Return true if FILENAME is an absolute file name.
/// On Unix this means it starts with `/` or `~`.
pub fn file_name_absolute_p(filename: &str) -> bool {
    filename.starts_with('/') || filename.starts_with('~')
}

/// Return true if NAME is a directory name (ends with a directory separator).
pub fn directory_name_p(name: &str) -> bool {
    name.ends_with('/')
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

fn trim_embedded_absfilename(path: String) -> String {
    let mut current = path;
    loop {
        let bytes = current.as_bytes();
        let mut cut_at = None;
        let mut i = 1usize;
        while i < bytes.len() {
            if bytes[i - 1] == b'/' && (bytes[i] == b'/' || bytes[i] == b'~') {
                cut_at = Some(i);
                break;
            }
            i += 1;
        }
        if let Some(idx) = cut_at {
            current = current[idx..].to_string();
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

// ===========================================================================
// File predicates (pure)
// ===========================================================================

/// Return true if FILENAME exists (file, directory, symlink, etc.).
pub fn file_exists_p(filename: &str) -> bool {
    Path::new(filename).exists()
}

/// Return true if FILENAME is readable.
pub fn file_readable_p(filename: &str) -> bool {
    // A file is "readable" if we can open it for reading.
    fs::File::open(filename).is_ok()
}

/// Return true if FILENAME is writable.
/// Checks if the file can be opened for writing, or if it doesn't exist,
/// whether the parent directory is writable.
pub fn file_writable_p(filename: &str) -> bool {
    let path = Path::new(filename);
    if path.exists() {
        fs::OpenOptions::new().write(true).open(filename).is_ok()
    } else {
        // File doesn't exist; check if parent directory is writable
        match path.parent() {
            Some(parent) if parent.exists() => {
                // Try to check write permission on the parent directory
                // by attempting to create a temp file
                let test_path = parent.join(".neovm_write_test");
                match fs::File::create(&test_path) {
                    Ok(_) => {
                        let _ = fs::remove_file(&test_path);
                        true
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        }
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
pub fn file_symlink_p(filename: &str) -> bool {
    match fs::symlink_metadata(filename) {
        Ok(meta) => meta.file_type().is_symlink(),
        Err(_) => false,
    }
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

/// Return true if FILE1 and FILE2 name the same existing file.
///
/// Missing paths return false. Symlinks are followed (matching Emacs behavior).
pub fn file_equal_p(file1: &str, file2: &str) -> bool {
    let meta1 = match fs::metadata(file1) {
        Ok(meta) => meta,
        Err(_) => return false,
    };
    let meta2 = match fs::metadata(file2) {
        Ok(meta) => meta,
        Err(_) => return false,
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return meta1.dev() == meta2.dev() && meta1.ino() == meta2.ino();
    }

    #[cfg(windows)]
    {
        let path1 = match fs::canonicalize(file1) {
            Ok(path) => path,
            Err(_) => return false,
        };
        let path2 = match fs::canonicalize(file2) {
            Ok(path) => path,
            Err(_) => return false,
        };
        return path1.to_string_lossy().to_lowercase() == path2.to_string_lossy().to_lowercase();
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (meta1, meta2);
        let path1 = match fs::canonicalize(file1) {
            Ok(path) => path,
            Err(_) => return false,
        };
        let path2 = match fs::canonicalize(file2) {
            Ok(path) => path,
            Err(_) => return false,
        };
        return path1 == path2;
    }
}

/// Return true if FILE is in DIR, using true-name semantics.
pub fn file_in_directory_p(file: &str, dir: &str, default_dir: Option<&str>) -> bool {
    let file_true = file_truename(file, default_dir);
    let dir_true = file_truename(dir, default_dir);

    let file_norm = directory_file_name(&file_true);
    let dir_norm = directory_file_name(&dir_true);
    if file_norm == dir_norm {
        return true;
    }

    let mut prefix = dir_norm.clone();
    if !prefix.ends_with('/') {
        prefix.push('/');
    }
    file_norm.starts_with(&prefix)
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
    use std::io::Write;
    let mut file = if append {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(filename)?
    } else {
        fs::File::create(filename)?
    };
    file.write_all(content.as_bytes())
}

// ===========================================================================
// Directory operations
// ===========================================================================

/// Return a list of file names in DIR.
/// If FULL is true, return absolute paths.
/// If MATCH_REGEX is Some, only include entries whose names match the regex.
/// If NOSORT is true, preserve filesystem enumeration order.
/// COUNT limits the number of accepted entries during enumeration.
#[cfg(unix)]
fn read_directory_names(dir: &str) -> Result<Vec<String>, DirectoryFilesError> {
    let dir_cstr = CString::new(dir).map_err(|_| DirectoryFilesError::Io {
        action: "Opening directory",
        err: std::io::Error::new(ErrorKind::InvalidInput, "path contains interior NUL"),
    })?;
    let dirp = unsafe { libc::opendir(dir_cstr.as_ptr()) };
    if dirp.is_null() {
        return Err(DirectoryFilesError::Io {
            action: "Opening directory",
            err: std::io::Error::last_os_error(),
        });
    }

    let mut names = Vec::new();
    loop {
        let entry = unsafe { libc::readdir(dirp) };
        if entry.is_null() {
            break;
        }
        let raw_name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        names.push(raw_name.to_string_lossy().into_owned());
    }

    let _ = unsafe { libc::closedir(dirp) };
    Ok(names)
}

#[cfg(not(unix))]
fn read_directory_names(dir: &str) -> Result<Vec<String>, DirectoryFilesError> {
    let entries = fs::read_dir(dir).map_err(|e| DirectoryFilesError::Io {
        action: "Opening directory",
        err: e,
    })?;
    let mut names = vec![".".to_string(), "..".to_string()];
    for entry in entries {
        let entry = entry.map_err(|e| DirectoryFilesError::Io {
            action: "Reading directory entry",
            err: e,
        })?;
        names.push(entry.file_name().to_string_lossy().into_owned());
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

fn directory_files(
    dir: &str,
    full: bool,
    match_regex: Option<&str>,
    nosort: bool,
    count: Option<usize>,
) -> Result<Vec<String>, DirectoryFilesError> {
    if count == Some(0) {
        return Ok(Vec::new());
    }

    let re = match match_regex {
        Some(pattern) => Some(Regex::new(pattern).map_err(|e| {
            DirectoryFilesError::InvalidRegexp(format!("Invalid regexp \"{}\": {}", pattern, e))
        })?),
        None => None,
    };

    let names = read_directory_names(dir)?;

    // Emacs builds this list via `cons` while scanning readdir output.
    // That makes NOSORT results reverse the traversal order and applies COUNT
    // before sort.
    let mut result = VecDeque::new();
    let mut remaining = count.unwrap_or(usize::MAX);
    let dir_with_slash = if dir.ends_with('/') {
        dir.to_string()
    } else {
        format!("{dir}/")
    };

    for name in names {
        if let Some(re) = re.as_ref() {
            if !re.is_match(&name) {
                continue;
            }
        }

        if full {
            result.push_front(format!("{dir_with_slash}{name}"));
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

    let mut result: Vec<String> = result.into_iter().collect();
    if !nosort {
        result.sort();
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
    fs::rename(from, to)
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

    Some(FileAttributes {
        size: meta.len(),
        is_dir: meta.is_dir(),
        is_symlink: symlink_meta.map_or(false, |m| m.file_type().is_symlink()),
        modified,
        modes,
    })
}

fn file_modes(filename: &str) -> Option<u32> {
    let meta = fs::symlink_metadata(filename).ok()?;
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

// ===========================================================================
// Builtin wrappers — pure (no evaluator needed)
// ===========================================================================

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_string(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        Value::Symbol(s) => Ok(s.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

fn expect_string_strict(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

fn expect_temp_prefix(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        Value::Nil | Value::Cons(_) | Value::Vector(_) => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), value.clone()],
        )),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("sequencep"), other.clone()],
        )),
    }
}

fn expect_fixnum(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("fixnump"), other.clone()],
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
    match value {
        Value::Int(n) => Ok((*n, 0)),
        Value::Float(f) => {
            let secs = f.floor() as i64;
            let nanos = ((f - f.floor()) * 1_000_000_000.0).round() as i64;
            Ok(normalize_secs_nanos(secs, nanos))
        }
        Value::Cons(_) => {
            let items = list_to_vec(value).ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), value.clone()],
                )
            })?;
            if items.len() < 2 {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), value.clone()],
                ));
            }
            let high = items[0].as_int().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), items[0].clone()],
                )
            })?;
            let low = items[1].as_int().ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), items[1].clone()],
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
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("numberp"), other.clone()],
        )),
    }
}

fn validate_file_truename_counter(counter: &Value) -> Result<(), Flow> {
    if counter.is_nil() {
        return Ok(());
    }
    if !counter.is_list() {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("listp"), counter.clone()],
        ));
    }
    if let Value::Cons(cell) = counter {
        let first = cell.lock().unwrap().car.clone();
        if !matches!(first, Value::Int(_) | Value::Float(_) | Value::Char(_)) {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("number-or-marker-p"), first],
            ));
        }
    }
    Ok(())
}

fn temporary_file_directory_for_eval(eval: &Evaluator) -> Option<String> {
    for frame in eval.dynamic.iter().rev() {
        if let Some(value) = frame.get("temporary-file-directory") {
            if let Value::Str(s) = value {
                return Some((**s).clone());
            }
        }
    }
    match eval.obarray.symbol_value("temporary-file-directory") {
        Some(Value::Str(s)) => Some((**s).clone()),
        _ => None,
    }
}

fn make_temp_file_impl(
    temp_dir: &str,
    prefix: &str,
    dir_flag: bool,
    suffix: &str,
    text: Option<&str>,
) -> Result<String, Flow> {
    let base = PathBuf::from(temp_dir);

    for _ in 0..256 {
        let nonce = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let candidate = base.join(format!("{prefix}{now:x}{nonce:x}{suffix}"));
        let candidate_str = candidate.to_string_lossy().into_owned();

        if dir_flag {
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    if let Some(contents) = text {
                        let mut file = fs::OpenOptions::new()
                            .write(true)
                            .open(&candidate)
                            .map_err(|err| {
                                signal_file_io_path(err, "Writing to", &candidate_str)
                            })?;
                        file.write_all(contents.as_bytes()).map_err(|err| {
                            signal_file_io_path(err, "Writing to", &candidate_str)
                        })?;
                    }
                    return Ok(candidate_str);
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(signal_file_io_path(
                        err,
                        "Creating directory",
                        &candidate_str,
                    ))
                }
            }
        } else {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(mut file) => {
                    if let Some(contents) = text {
                        file.write_all(contents.as_bytes()).map_err(|err| {
                            signal_file_io_path(err, "Writing to", &candidate_str)
                        })?;
                    }
                    return Ok(candidate_str);
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(signal_file_io_path(err, "Creating file", &candidate_str)),
            }
        }
    }

    Err(signal(
        "file-error",
        vec![Value::string("Cannot create temporary file")],
    ))
}

fn split_nearby_temp_prefix(prefix: &str) -> Option<(String, String)> {
    let path = Path::new(prefix);
    if !path.is_absolute() {
        return None;
    }
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    if file_name.is_empty() {
        return None;
    }
    let parent = path.parent()?;
    if parent.as_os_str().is_empty() || parent == Path::new(".") {
        return None;
    }
    Some((parent.to_string_lossy().into_owned(), file_name))
}

/// (expand-file-name NAME &optional DEFAULT-DIRECTORY) -> string
pub(crate) fn builtin_expand_file_name(args: Vec<Value>) -> EvalResult {
    expect_min_args("expand-file-name", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("expand-file-name"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let name = expect_string_strict(&args[0])?;
    let default_dir = if let Some(arg) = args.get(1) {
        match arg {
            Value::Nil => None,
            Value::Str(s) => Some((**s).clone()),
            // Emacs treats non-string DEFAULT-DIRECTORY as root.
            _ => Some("/".to_string()),
        }
    } else {
        None
    };
    Ok(Value::string(expand_file_name(
        &name,
        default_dir.as_deref(),
    )))
}

/// Evaluator-aware variant of `expand-file-name` that falls back to dynamic
/// `default-directory` when DEFAULT-DIRECTORY is omitted or nil.
pub(crate) fn builtin_expand_file_name_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("expand-file-name", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("expand-file-name"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let name = expect_string_strict(&args[0])?;
    let default_dir = if let Some(arg) = args.get(1) {
        match arg {
            Value::Nil => default_directory_for_eval(eval),
            Value::Str(s) => Some((**s).clone()),
            // Emacs treats non-string DEFAULT-DIRECTORY as root.
            _ => Some("/".to_string()),
        }
    } else {
        default_directory_for_eval(eval)
    };

    Ok(Value::string(expand_file_name(
        &name,
        default_dir.as_deref(),
    )))
}

/// (make-temp-file PREFIX &optional DIR-FLAG SUFFIX TEXT) -> string
pub(crate) fn builtin_make_temp_file(args: Vec<Value>) -> EvalResult {
    expect_min_args("make-temp-file", &args, 1)?;
    if args.len() > 4 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-temp-file"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let prefix = expect_temp_prefix(&args[0])?;
    let dir_flag = args.get(1).is_some_and(|value| value.is_truthy());
    let suffix = match args.get(2) {
        None | Some(Value::Nil) => String::new(),
        Some(value) => expect_string_strict(value)?,
    };
    let text = match args.get(3) {
        None | Some(Value::Nil) => None,
        Some(Value::Str(s)) => Some((**s).clone()),
        Some(_) => None,
    };
    let temp_dir = std::env::temp_dir().to_string_lossy().into_owned();

    let path = make_temp_file_impl(&temp_dir, &prefix, dir_flag, &suffix, text.as_deref())?;
    Ok(Value::string(path))
}

/// Evaluator-aware variant of `make-temp-file` that honors dynamic
/// `temporary-file-directory`.
pub(crate) fn builtin_make_temp_file_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-temp-file", &args, 1)?;
    if args.len() > 4 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-temp-file"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let prefix = expect_temp_prefix(&args[0])?;
    let dir_flag = args.get(1).is_some_and(|value| value.is_truthy());
    let suffix = match args.get(2) {
        None | Some(Value::Nil) => String::new(),
        Some(value) => expect_string_strict(value)?,
    };
    let text = match args.get(3) {
        None | Some(Value::Nil) => None,
        Some(Value::Str(s)) => Some((**s).clone()),
        Some(_) => None,
    };
    let temp_dir = temporary_file_directory_for_eval(eval)
        .unwrap_or_else(|| std::env::temp_dir().to_string_lossy().into_owned());

    let path = make_temp_file_impl(&temp_dir, &prefix, dir_flag, &suffix, text.as_deref())?;
    Ok(Value::string(path))
}

/// (make-nearby-temp-file PREFIX &optional DIR-FLAG SUFFIX) -> string
pub(crate) fn builtin_make_nearby_temp_file(args: Vec<Value>) -> EvalResult {
    expect_min_args("make-nearby-temp-file", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-nearby-temp-file"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let prefix = expect_temp_prefix(&args[0])?;
    let dir_flag = args.get(1).is_some_and(|value| value.is_truthy());
    let suffix = match args.get(2) {
        None | Some(Value::Nil) => String::new(),
        Some(value) => expect_string_strict(value)?,
    };
    let fallback_temp_dir = std::env::temp_dir().to_string_lossy().into_owned();
    let (temp_dir, file_prefix) =
        split_nearby_temp_prefix(&prefix).unwrap_or_else(|| (fallback_temp_dir, prefix.clone()));

    let path = make_temp_file_impl(&temp_dir, &file_prefix, dir_flag, &suffix, None)?;
    Ok(Value::string(path))
}

/// Evaluator-aware variant of `make-nearby-temp-file` that resolves relative
/// directory-containing prefixes against dynamic/default `default-directory`
/// and honors dynamic `temporary-file-directory` fallback.
pub(crate) fn builtin_make_nearby_temp_file_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-nearby-temp-file", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-nearby-temp-file"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let prefix = expect_temp_prefix(&args[0])?;
    let dir_flag = args.get(1).is_some_and(|value| value.is_truthy());
    let suffix = match args.get(2) {
        None | Some(Value::Nil) => String::new(),
        Some(value) => expect_string_strict(value)?,
    };
    let fallback_temp_dir = temporary_file_directory_for_eval(eval)
        .unwrap_or_else(|| std::env::temp_dir().to_string_lossy().into_owned());
    let (temp_dir, file_prefix) =
        split_nearby_temp_prefix(&prefix).unwrap_or_else(|| (fallback_temp_dir, prefix.clone()));

    let path = make_temp_file_impl(&temp_dir, &file_prefix, dir_flag, &suffix, None)?;
    Ok(Value::string(path))
}

/// (file-truename FILENAME &optional COUNTER PREV-DIRS) -> string
pub(crate) fn builtin_file_truename(args: Vec<Value>) -> EvalResult {
    expect_min_args("file-truename", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("file-truename"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let filename = expect_string_strict(&args[0])?;
    if let Some(counter) = args.get(1) {
        validate_file_truename_counter(counter)?;
    }

    Ok(Value::string(file_truename(&filename, None)))
}

/// Evaluator-aware variant of `file-truename` that resolves relative
/// filenames against dynamic/default `default-directory`.
pub(crate) fn builtin_file_truename_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("file-truename", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("file-truename"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let filename = expect_string_strict(&args[0])?;
    if let Some(counter) = args.get(1) {
        validate_file_truename_counter(counter)?;
    }

    Ok(Value::string(file_truename(
        &filename,
        default_directory_for_eval(eval).as_deref(),
    )))
}

/// (file-name-directory FILENAME) -> string or nil
pub(crate) fn builtin_file_name_directory(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-directory", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    match file_name_directory(&filename) {
        Some(dir) => Ok(Value::string(dir)),
        None => Ok(Value::Nil),
    }
}

/// (file-name-nondirectory FILENAME) -> string
pub(crate) fn builtin_file_name_nondirectory(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-nondirectory", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::string(file_name_nondirectory(&filename)))
}

/// (file-name-extension FILENAME &optional PERIOD) -> string or nil
pub(crate) fn builtin_file_name_extension(args: Vec<Value>) -> EvalResult {
    expect_min_args("file-name-extension", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("file-name-extension"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    let period = args.get(1).is_some_and(|v| v.is_truthy());
    match file_name_extension(&filename, period) {
        Some(ext) => Ok(Value::string(ext)),
        None => Ok(Value::Nil),
    }
}

/// (file-name-sans-extension FILENAME) -> string
pub(crate) fn builtin_file_name_sans_extension(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-sans-extension", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::string(file_name_sans_extension(&filename)))
}

/// (file-name-as-directory FILENAME) -> string
pub(crate) fn builtin_file_name_as_directory(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-as-directory", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::string(file_name_as_directory(&filename)))
}

/// (directory-file-name FILENAME) -> string
pub(crate) fn builtin_directory_file_name(args: Vec<Value>) -> EvalResult {
    expect_args("directory-file-name", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::string(directory_file_name(&filename)))
}

/// (file-name-concat DIRECTORY &rest COMPONENTS) -> string
pub(crate) fn builtin_file_name_concat(args: Vec<Value>) -> EvalResult {
    expect_min_args("file-name-concat", &args, 1)?;

    let mut parts = Vec::new();
    for value in args {
        match value {
            Value::Nil => {}
            Value::Str(s) => {
                if !s.is_empty() {
                    parts.push((*s).clone());
                }
            }
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other],
                ));
            }
        }
    }

    let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
    Ok(Value::string(file_name_concat(&refs)))
}

/// (file-name-absolute-p FILENAME) -> t or nil
pub(crate) fn builtin_file_name_absolute_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-absolute-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::bool(file_name_absolute_p(&filename)))
}

/// (directory-name-p NAME) -> t or nil
pub(crate) fn builtin_directory_name_p(args: Vec<Value>) -> EvalResult {
    expect_args("directory-name-p", &args, 1)?;
    let name = expect_string_strict(&args[0])?;
    Ok(Value::bool(directory_name_p(&name)))
}

/// (substitute-in-file-name FILENAME) -> string
pub(crate) fn builtin_substitute_in_file_name(args: Vec<Value>) -> EvalResult {
    expect_args("substitute-in-file-name", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::string(substitute_in_file_name(&filename)))
}

fn default_directory_for_eval(eval: &Evaluator) -> Option<String> {
    for frame in eval.dynamic.iter().rev() {
        if let Some(value) = frame.get("default-directory") {
            return match value {
                Value::Str(s) => Some((**s).clone()),
                _ => None,
            };
        }
    }
    match eval.obarray.symbol_value("default-directory") {
        Some(Value::Str(s)) => Some((**s).clone()),
        _ => None,
    }
}

pub(crate) fn resolve_filename_for_eval(eval: &Evaluator, filename: &str) -> String {
    if filename.is_empty() || Path::new(filename).is_absolute() {
        return filename.to_string();
    }
    let default_dir = default_directory_for_eval(eval);
    expand_file_name(filename, default_dir.as_deref())
}

fn file_error_symbol(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound => "file-missing",
        ErrorKind::AlreadyExists => "file-already-exists",
        ErrorKind::PermissionDenied => "permission-denied",
        _ => "file-error",
    }
}

fn signal_file_io_error(err: std::io::Error, context: String) -> Flow {
    let symbol = file_error_symbol(err.kind());
    signal(symbol, vec![Value::string(format!("{context}: {err}"))])
}

fn signal_file_io_path(err: std::io::Error, action: &str, path: &str) -> Flow {
    signal_file_io_error(err, format!("{action} {path}"))
}

fn signal_file_io_paths(err: std::io::Error, action: &str, from: &str, to: &str) -> Flow {
    signal_file_io_error(err, format!("{action} {from} to {to}"))
}

fn signal_directory_files_error(err: DirectoryFilesError, dir: &str) -> Flow {
    match err {
        DirectoryFilesError::Io { action, err } => signal_file_io_path(err, action, dir),
        DirectoryFilesError::InvalidRegexp(msg) => {
            signal("invalid-regexp", vec![Value::string(msg)])
        }
    }
}

fn signal_file_action_error(err: std::io::Error, action: &str, path: &str) -> Flow {
    signal(
        file_error_symbol(err.kind()),
        vec![
            Value::string(action),
            Value::string(err.to_string()),
            Value::string(path),
        ],
    )
}

fn set_file_times_compat(
    filename: &str,
    timestamp: Option<(i64, i64)>,
    nofollow: bool,
) -> Result<(), Flow> {
    #[cfg(unix)]
    {
        let c_path = CString::new(filename.as_bytes()).map_err(|_| {
            signal(
                "file-error",
                vec![
                    Value::string("Setting file times"),
                    Value::string("embedded NUL in file name"),
                    Value::string(filename),
                ],
            )
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
        let flags = if nofollow { libc::AT_SYMLINK_NOFOLLOW } else { 0 };
        let result = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), ts.as_ptr(), flags) };
        if result != 0 {
            return Err(signal_file_action_error(
                std::io::Error::last_os_error(),
                "Setting file times",
                filename,
            ));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (timestamp, nofollow);
        Err(signal(
            "file-error",
            vec![
                Value::string("Setting file times"),
                Value::string("set-file-times is unsupported on this platform"),
                Value::string(filename),
            ],
        ))
    }
}

fn delete_file_compat(filename: &str) -> Result<(), Flow> {
    match delete_file(filename) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(signal_file_io_path(err, "Deleting", filename)),
    }
}

/// (file-exists-p FILENAME) -> t or nil
pub(crate) fn builtin_file_exists_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-exists-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::bool(file_exists_p(&filename)))
}

/// Evaluator-aware variant of `file-exists-p` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_file_exists_p_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("file-exists-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    Ok(Value::bool(file_exists_p(&filename)))
}

/// (file-readable-p FILENAME) -> t or nil
pub(crate) fn builtin_file_readable_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-readable-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::bool(file_readable_p(&filename)))
}

/// Evaluator-aware variant of `file-readable-p` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_file_readable_p_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("file-readable-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    Ok(Value::bool(file_readable_p(&filename)))
}

/// (file-writable-p FILENAME) -> t or nil
pub(crate) fn builtin_file_writable_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-writable-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::bool(file_writable_p(&filename)))
}

/// Evaluator-aware variant of `file-writable-p` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_file_writable_p_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("file-writable-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    Ok(Value::bool(file_writable_p(&filename)))
}

/// (file-directory-p FILENAME) -> t or nil
pub(crate) fn builtin_file_directory_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-directory-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::bool(file_directory_p(&filename)))
}

/// Evaluator-aware variant of `file-directory-p` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_file_directory_p_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("file-directory-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    Ok(Value::bool(file_directory_p(&filename)))
}

/// (file-regular-p FILENAME) -> t or nil
pub(crate) fn builtin_file_regular_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-regular-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::bool(file_regular_p(&filename)))
}

/// Evaluator-aware variant of `file-regular-p` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_file_regular_p_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("file-regular-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    Ok(Value::bool(file_regular_p(&filename)))
}

/// (file-symlink-p FILENAME) -> t or nil
pub(crate) fn builtin_file_symlink_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-symlink-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    Ok(Value::bool(file_symlink_p(&filename)))
}

/// Evaluator-aware variant of `file-symlink-p` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_file_symlink_p_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("file-symlink-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    Ok(Value::bool(file_symlink_p(&filename)))
}

/// (file-name-case-insensitive-p FILENAME) -> t or nil
pub(crate) fn builtin_file_name_case_insensitive_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-case-insensitive-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    let filename = expand_file_name(&filename, None);
    Ok(Value::bool(file_name_case_insensitive_p(&filename)))
}

/// Evaluator-aware variant of `file-name-case-insensitive-p` that resolves
/// relative paths against dynamic/default `default-directory`.
pub(crate) fn builtin_file_name_case_insensitive_p_eval(
    eval: &Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("file-name-case-insensitive-p", &args, 1)?;
    let filename = expect_string_strict(&args[0])?;
    let default_dir = default_directory_for_eval(eval);
    let filename = expand_file_name(&filename, default_dir.as_deref());
    Ok(Value::bool(file_name_case_insensitive_p(&filename)))
}

/// (file-newer-than-file-p FILE1 FILE2) -> t or nil
pub(crate) fn builtin_file_newer_than_file_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-newer-than-file-p", &args, 2)?;
    let file1 = expect_string_strict(&args[0])?;
    let file2 = expect_string_strict(&args[1])?;
    let file1 = expand_file_name(&file1, None);
    let file2 = expand_file_name(&file2, None);
    Ok(Value::bool(file_newer_than_file_p(&file1, &file2)))
}

/// Evaluator-aware variant of `file-newer-than-file-p` that resolves
/// relative paths against dynamic/default `default-directory`.
pub(crate) fn builtin_file_newer_than_file_p_eval(
    eval: &Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("file-newer-than-file-p", &args, 2)?;
    let file1 = expect_string_strict(&args[0])?;
    let file2 = expect_string_strict(&args[1])?;
    let default_dir = default_directory_for_eval(eval);
    let file1 = expand_file_name(&file1, default_dir.as_deref());
    let file2 = expand_file_name(&file2, default_dir.as_deref());
    Ok(Value::bool(file_newer_than_file_p(&file1, &file2)))
}

/// (file-equal-p FILE1 FILE2) -> t or nil
pub(crate) fn builtin_file_equal_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-equal-p", &args, 2)?;
    let file1 = expect_string_strict(&args[0])?;
    let file2 = expect_string_strict(&args[1])?;
    let file1 = expand_file_name(&file1, None);
    let file2 = expand_file_name(&file2, None);
    Ok(Value::bool(file_equal_p(&file1, &file2)))
}

/// Evaluator-aware variant of `file-equal-p` that resolves relative
/// paths against dynamic/default `default-directory`.
pub(crate) fn builtin_file_equal_p_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("file-equal-p", &args, 2)?;
    let file1 = expect_string_strict(&args[0])?;
    let file2 = expect_string_strict(&args[1])?;
    let default_dir = default_directory_for_eval(eval);
    let file1 = expand_file_name(&file1, default_dir.as_deref());
    let file2 = expand_file_name(&file2, default_dir.as_deref());
    Ok(Value::bool(file_equal_p(&file1, &file2)))
}

/// (file-in-directory-p FILE DIR) -> t or nil
pub(crate) fn builtin_file_in_directory_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-in-directory-p", &args, 2)?;
    let file = expect_string_strict(&args[0])?;
    let dir = expect_string_strict(&args[1])?;
    Ok(Value::bool(file_in_directory_p(&file, &dir, None)))
}

/// Evaluator-aware variant of `file-in-directory-p` that resolves relative
/// paths against dynamic/default `default-directory`.
pub(crate) fn builtin_file_in_directory_p_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("file-in-directory-p", &args, 2)?;
    let file = expect_string_strict(&args[0])?;
    let dir = expect_string_strict(&args[1])?;
    let default_dir = default_directory_for_eval(eval);
    Ok(Value::bool(file_in_directory_p(
        &file,
        &dir,
        default_dir.as_deref(),
    )))
}

/// (file-modes FILENAME &optional FLAG) -> integer or nil
pub(crate) fn builtin_file_modes(args: Vec<Value>) -> EvalResult {
    expect_min_args("file-modes", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("file-modes"), Value::Int(args.len() as i64)],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    match file_modes(&filename) {
        Some(mode) => Ok(Value::Int(mode as i64)),
        None => Ok(Value::Nil),
    }
}

/// Evaluator-aware variant of `file-modes` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_file_modes_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("file-modes", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("file-modes"), Value::Int(args.len() as i64)],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    match file_modes(&filename) {
        Some(mode) => Ok(Value::Int(mode as i64)),
        None => Ok(Value::Nil),
    }
}

/// (set-file-modes FILENAME MODE &optional FLAG) -> nil
pub(crate) fn builtin_set_file_modes(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-file-modes", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("set-file-modes"), Value::Int(args.len() as i64)],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    let mode = expect_fixnum(&args[1])?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(mode as u32);
        fs::set_permissions(&filename, perms)
            .map_err(|err| signal_file_action_error(err, "Doing chmod", &filename))?;
    }
    #[cfg(not(unix))]
    {
        let mut perms = fs::metadata(&filename)
            .map_err(|err| signal_file_action_error(err, "Doing chmod", &filename))?
            .permissions();
        let writable = (mode & 0o222) != 0;
        perms.set_readonly(!writable);
        fs::set_permissions(&filename, perms)
            .map_err(|err| signal_file_action_error(err, "Doing chmod", &filename))?;
    }
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `set-file-modes` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_set_file_modes_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("set-file-modes", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("set-file-modes"), Value::Int(args.len() as i64)],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    let mode = expect_fixnum(&args[1])?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(mode as u32);
        fs::set_permissions(&filename, perms)
            .map_err(|err| signal_file_action_error(err, "Doing chmod", &filename))?;
    }
    #[cfg(not(unix))]
    {
        let mut perms = fs::metadata(&filename)
            .map_err(|err| signal_file_action_error(err, "Doing chmod", &filename))?
            .permissions();
        let writable = (mode & 0o222) != 0;
        perms.set_readonly(!writable);
        fs::set_permissions(&filename, perms)
            .map_err(|err| signal_file_action_error(err, "Doing chmod", &filename))?;
    }
    Ok(Value::Nil)
}

/// (set-file-times FILENAME &optional TIMESTAMP FLAG) -> t
pub(crate) fn builtin_set_file_times(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-file-times", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("set-file-times"), Value::Int(args.len() as i64)],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    let filename = expand_file_name(&filename, None);
    let timestamp = if args.len() > 1 && !args[1].is_nil() {
        Some(parse_timestamp_arg(&args[1])?)
    } else {
        None
    };
    // Emacs currently treats all non-nil values like `nofollow`.
    let nofollow = args.get(2).is_some_and(|flag| !flag.is_nil());
    set_file_times_compat(&filename, timestamp, nofollow)?;
    Ok(Value::True)
}

/// Evaluator-aware variant of `set-file-times` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_set_file_times_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("set-file-times", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("set-file-times"), Value::Int(args.len() as i64)],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    let default_dir = default_directory_for_eval(eval);
    let filename = expand_file_name(&filename, default_dir.as_deref());
    let timestamp = if args.len() > 1 && !args[1].is_nil() {
        Some(parse_timestamp_arg(&args[1])?)
    } else {
        None
    };
    // Emacs currently treats all non-nil values like `nofollow`.
    let nofollow = args.get(2).is_some_and(|flag| !flag.is_nil());
    set_file_times_compat(&filename, timestamp, nofollow)?;
    Ok(Value::True)
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
    Ok(Value::Nil)
}

/// (default-file-modes) -> integer
pub(crate) fn builtin_default_file_modes(args: Vec<Value>) -> EvalResult {
    expect_args("default-file-modes", &args, 0)?;
    init_default_file_mode_mask();
    let mask = DEFAULT_FILE_MODE_MASK.load(Ordering::Relaxed) as i64;
    Ok(Value::Int((!mask) & 0o777))
}

/// (delete-file FILENAME &optional TRASH) -> nil
pub(crate) fn builtin_delete_file(args: Vec<Value>) -> EvalResult {
    expect_min_args("delete-file", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("delete-file"), Value::Int(args.len() as i64)],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    delete_file_compat(&filename)?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `delete-file` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_delete_file_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("delete-file", &args, 1)?;
    if args.len() > 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("delete-file"), Value::Int(args.len() as i64)],
        ));
    }
    let filename = expect_string_strict(&args[0])?;
    let filename = resolve_filename_for_eval(eval, &filename);
    delete_file_compat(&filename)?;
    Ok(Value::Nil)
}

/// (delete-directory DIRECTORY &optional RECURSIVE TRASH) -> nil
pub(crate) fn builtin_delete_directory(args: Vec<Value>) -> EvalResult {
    expect_min_args("delete-directory", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("delete-directory"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let directory = expect_string_strict(&args[0])?;
    let recursive = args.get(1).is_some_and(|value| value.is_truthy());
    let result = if recursive {
        fs::remove_dir_all(&directory)
    } else {
        fs::remove_dir(&directory)
    };
    result.map_err(|err| signal_file_io_path(err, "Removing directory", &directory))?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `delete-directory` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_delete_directory_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("delete-directory", &args, 1)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("delete-directory"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let directory = expect_string_strict(&args[0])?;
    let directory = resolve_filename_for_eval(eval, &directory);
    let recursive = args.get(1).is_some_and(|value| value.is_truthy());
    let result = if recursive {
        fs::remove_dir_all(&directory)
    } else {
        fs::remove_dir(&directory)
    };
    result.map_err(|err| signal_file_io_path(err, "Removing directory", &directory))?;
    Ok(Value::Nil)
}

/// (make-symbolic-link TARGET LINKNAME &optional OK-IF-ALREADY-EXISTS) -> nil
pub(crate) fn builtin_make_symbolic_link(args: Vec<Value>) -> EvalResult {
    expect_min_args("make-symbolic-link", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-symbolic-link"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let target = expect_string_strict(&args[0])?;
    let linkname = expect_string_strict(&args[1])?;
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());

    #[cfg(unix)]
    {
        if ok_if_exists {
            if fs::symlink_metadata(&linkname).is_ok() {
                fs::remove_file(&linkname)
                    .map_err(|err| signal_file_io_path(err, "Removing old name", &linkname))?;
            }
        }
        std::os::unix::fs::symlink(&target, &linkname)
            .map_err(|err| signal_file_io_path(err, "Making symbolic link", &linkname))?;
        Ok(Value::Nil)
    }

    #[cfg(not(unix))]
    {
        let _ = (target, linkname, ok_if_exists);
        Err(signal(
            "file-error",
            vec![Value::string(
                "Symbolic links are unsupported on this platform",
            )],
        ))
    }
}

/// Evaluator-aware variant of `make-symbolic-link` that resolves relative
/// target/link paths against dynamic/default `default-directory`.
pub(crate) fn builtin_make_symbolic_link_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-symbolic-link", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-symbolic-link"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let target = resolve_filename_for_eval(eval, &expect_string_strict(&args[0])?);
    let linkname = resolve_filename_for_eval(eval, &expect_string_strict(&args[1])?);
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());

    #[cfg(unix)]
    {
        if ok_if_exists {
            if fs::symlink_metadata(&linkname).is_ok() {
                fs::remove_file(&linkname)
                    .map_err(|err| signal_file_io_path(err, "Removing old name", &linkname))?;
            }
        }
        std::os::unix::fs::symlink(&target, &linkname)
            .map_err(|err| signal_file_io_path(err, "Making symbolic link", &linkname))?;
        Ok(Value::Nil)
    }

    #[cfg(not(unix))]
    {
        let _ = (target, linkname, ok_if_exists);
        Err(signal(
            "file-error",
            vec![Value::string(
                "Symbolic links are unsupported on this platform",
            )],
        ))
    }
}

/// (rename-file FROM TO) -> nil
pub(crate) fn builtin_rename_file(args: Vec<Value>) -> EvalResult {
    expect_min_args("rename-file", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("rename-file"), Value::Int(args.len() as i64)],
        ));
    }
    let from = expect_string_strict(&args[0])?;
    let to = expect_string_strict(&args[1])?;
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    if fs::symlink_metadata(&to).is_ok() {
        if ok_if_exists {
            fs::remove_file(&to).map_err(|e| signal_file_io_path(e, "Removing old name", &to))?;
        } else {
            return Err(signal(
                "file-already-exists",
                vec![
                    Value::string("Renaming"),
                    Value::string(format!("File exists: {to}")),
                    Value::string(&from),
                    Value::string(&to),
                ],
            ));
        }
    }
    rename_file(&from, &to).map_err(|e| signal_file_io_paths(e, "Renaming", &from, &to))?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `rename-file` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_rename_file_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("rename-file", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("rename-file"), Value::Int(args.len() as i64)],
        ));
    }
    let from = resolve_filename_for_eval(eval, &expect_string_strict(&args[0])?);
    let to = resolve_filename_for_eval(eval, &expect_string_strict(&args[1])?);
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    if fs::symlink_metadata(&to).is_ok() {
        if ok_if_exists {
            fs::remove_file(&to).map_err(|e| signal_file_io_path(e, "Removing old name", &to))?;
        } else {
            return Err(signal(
                "file-already-exists",
                vec![
                    Value::string("Renaming"),
                    Value::string(format!("File exists: {to}")),
                    Value::string(&from),
                    Value::string(&to),
                ],
            ));
        }
    }
    rename_file(&from, &to).map_err(|e| signal_file_io_paths(e, "Renaming", &from, &to))?;
    Ok(Value::Nil)
}

/// (copy-file FROM TO) -> nil
pub(crate) fn builtin_copy_file(args: Vec<Value>) -> EvalResult {
    expect_min_args("copy-file", &args, 2)?;
    if args.len() > 6 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("copy-file"), Value::Int(args.len() as i64)],
        ));
    }
    let from = expect_string_strict(&args[0])?;
    let to = expect_string_strict(&args[1])?;
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    if fs::symlink_metadata(&to).is_ok() && !ok_if_exists {
        return Err(signal(
            "file-already-exists",
            vec![
                Value::string("Copying"),
                Value::string(format!("File exists: {to}")),
                Value::string(&from),
                Value::string(&to),
            ],
        ));
    }
    copy_file(&from, &to).map_err(|e| signal_file_io_paths(e, "Copying", &from, &to))?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `copy-file` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_copy_file_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("copy-file", &args, 2)?;
    if args.len() > 6 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("copy-file"), Value::Int(args.len() as i64)],
        ));
    }
    let from = resolve_filename_for_eval(eval, &expect_string_strict(&args[0])?);
    let to = resolve_filename_for_eval(eval, &expect_string_strict(&args[1])?);
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    if fs::symlink_metadata(&to).is_ok() && !ok_if_exists {
        return Err(signal(
            "file-already-exists",
            vec![
                Value::string("Copying"),
                Value::string(format!("File exists: {to}")),
                Value::string(&from),
                Value::string(&to),
            ],
        ));
    }
    copy_file(&from, &to).map_err(|e| signal_file_io_paths(e, "Copying", &from, &to))?;
    Ok(Value::Nil)
}

/// (add-name-to-file OLDNAME NEWNAME &optional OK-IF-ALREADY-EXISTS) -> nil
pub(crate) fn builtin_add_name_to_file(args: Vec<Value>) -> EvalResult {
    expect_min_args("add-name-to-file", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("add-name-to-file"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let oldname = expect_string_strict(&args[0])?;
    let newname = expect_string_strict(&args[1])?;
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    if ok_if_exists && fs::symlink_metadata(&newname).is_ok() {
        fs::remove_file(&newname).map_err(|err| signal_file_io_path(err, "Removing old name", &newname))?;
    }
    add_name_to_file(&oldname, &newname)
        .map_err(|err| signal_file_io_paths(err, "Adding new name", &oldname, &newname))?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `add-name-to-file` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_add_name_to_file_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("add-name-to-file", &args, 2)?;
    if args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("add-name-to-file"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let oldname = resolve_filename_for_eval(eval, &expect_string_strict(&args[0])?);
    let newname = resolve_filename_for_eval(eval, &expect_string_strict(&args[1])?);
    let ok_if_exists = args.get(2).is_some_and(|value| value.is_truthy());
    if ok_if_exists && fs::symlink_metadata(&newname).is_ok() {
        fs::remove_file(&newname).map_err(|err| signal_file_io_path(err, "Removing old name", &newname))?;
    }
    add_name_to_file(&oldname, &newname)
        .map_err(|err| signal_file_io_paths(err, "Adding new name", &oldname, &newname))?;
    Ok(Value::Nil)
}

/// (make-directory DIR &optional PARENTS) -> nil
pub(crate) fn builtin_make_directory(args: Vec<Value>) -> EvalResult {
    expect_min_args("make-directory", &args, 1)?;
    let dir = expect_string_strict(&args[0])?;
    let parents = args.get(1).is_some_and(|v| v.is_truthy());
    make_directory(&dir, parents)
        .map_err(|e| signal_file_io_path(e, "Creating directory", &dir))?;
    Ok(Value::Nil)
}

/// Evaluator-aware variant of `make-directory` that resolves relative paths
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_make_directory_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("make-directory", &args, 1)?;
    let dir = resolve_filename_for_eval(eval, &expect_string_strict(&args[0])?);
    let parents = args.get(1).is_some_and(|v| v.is_truthy());
    make_directory(&dir, parents)
        .map_err(|e| signal_file_io_path(e, "Creating directory", &dir))?;
    Ok(Value::Nil)
}

/// (directory-files DIR &optional FULL MATCH NOSORT COUNT) -> list of strings
pub(crate) fn builtin_directory_files(args: Vec<Value>) -> EvalResult {
    expect_min_args("directory-files", &args, 1)?;
    if args.len() > 5 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("directory-files"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let dir = expect_string_strict(&args[0])?;
    let full = args.get(1).is_some_and(|v| v.is_truthy());
    let match_pattern = if let Some(val) = args.get(2) {
        if val.is_truthy() {
            Some(expect_string_strict(val)?)
        } else {
            None
        }
    } else {
        None
    };
    let nosort = args.get(3).is_some_and(|v| v.is_truthy());
    let count = if let Some(val) = args.get(4) {
        match val {
            Value::Int(n) if *n >= 0 => Some(*n as usize),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("natnump"), other.clone()],
                ));
            }
        }
    } else {
        None
    };

    let files = directory_files(&dir, full, match_pattern.as_deref(), nosort, count)
        .map_err(|e| signal_directory_files_error(e, &dir))?;
    Ok(Value::list(files.into_iter().map(Value::string).collect()))
}

/// Evaluator-aware variant of `directory-files` that resolves relative DIR
/// against dynamic/default `default-directory`.
pub(crate) fn builtin_directory_files_eval(eval: &Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("directory-files", &args, 1)?;
    if args.len() > 5 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("directory-files"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    let dir = resolve_filename_for_eval(eval, &expect_string_strict(&args[0])?);
    let full = args.get(1).is_some_and(|v| v.is_truthy());
    let match_pattern = if let Some(val) = args.get(2) {
        if val.is_truthy() {
            Some(expect_string_strict(val)?)
        } else {
            None
        }
    } else {
        None
    };
    let nosort = args.get(3).is_some_and(|v| v.is_truthy());
    let count = if let Some(val) = args.get(4) {
        match val {
            Value::Int(n) if *n >= 0 => Some(*n as usize),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("natnump"), other.clone()],
                ));
            }
        }
    } else {
        None
    };

    let files = directory_files(&dir, full, match_pattern.as_deref(), nosort, count)
        .map_err(|e| signal_directory_files_error(e, &dir))?;
    Ok(Value::list(files.into_iter().map(Value::string).collect()))
}

/// (file-attributes FILENAME) -> list or nil
pub(crate) fn builtin_file_attributes(args: Vec<Value>) -> EvalResult {
    expect_args("file-attributes", &args, 1)?;
    let filename = expect_string(&args[0])?;
    match file_attributes(&filename) {
        Some(attrs) => {
            // Return a list: (size is-dir is-symlink modified modes)
            let modified_val = match attrs.modified {
                Some(t) => Value::Float(t),
                None => Value::Nil,
            };
            Ok(Value::list(vec![
                Value::Int(attrs.size as i64),
                Value::bool(attrs.is_dir),
                Value::bool(attrs.is_symlink),
                modified_val,
                Value::Int(attrs.modes as i64),
            ]))
        }
        None => Ok(Value::Nil),
    }
}

// ===========================================================================
// Evaluator-dependent builtins
// ===========================================================================

fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integerp"), other.clone()],
        )),
    }
}

/// (insert-file-contents FILENAME &optional VISIT BEG END REPLACE) -> (FILENAME LENGTH)
///
/// Read file FILENAME and insert its contents into the current buffer at point.
/// Returns a list of the absolute filename and the number of characters inserted.
pub(crate) fn builtin_insert_file_contents(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("insert-file-contents", &args, 1)?;
    let filename = expect_string(&args[0])?;
    let resolved = resolve_filename_for_eval(eval, &filename);
    let visit = args.get(1).is_some_and(|v| v.is_truthy());

    // Read file contents
    let contents = read_file_contents(&resolved)
        .map_err(|e| signal_file_io_path(e, "Opening input file", &resolved))?;

    let char_count = contents.chars().count() as i64;

    // Insert into current buffer
    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.insert(&contents);

    if visit {
        buf.file_name = Some(resolved.clone());
        buf.set_modified(false);
    }

    Ok(Value::list(vec![
        Value::string(resolved),
        Value::Int(char_count),
    ]))
}

/// (write-region START END FILENAME &optional APPEND VISIT) -> nil
///
/// Write the region between START and END to FILENAME.
/// If START is nil, writes the entire buffer.
pub(crate) fn builtin_write_region(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("write-region", &args, 3)?;
    let filename = expect_string(&args[2])?;
    let resolved = resolve_filename_for_eval(eval, &filename);
    let append = args.get(3).is_some_and(|v| v.is_truthy());
    let visit = args.get(4).is_some_and(|v| v.is_truthy());

    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    // Extract the text region
    let content = if args[0].is_nil() && args[1].is_nil() {
        // Write entire buffer
        buf.buffer_string()
    } else {
        let start = expect_int(&args[0])? as usize;
        let end = expect_int(&args[1])? as usize;
        // Convert 1-based Emacs positions to 0-based
        let char_start = if start > 0 { start - 1 } else { 0 };
        let char_end = if end > 0 { end - 1 } else { 0 };
        let byte_start = buf.text.char_to_byte(char_start.min(buf.text.char_count()));
        let byte_end = buf.text.char_to_byte(char_end.min(buf.text.char_count()));
        buf.buffer_substring(byte_start, byte_end)
    };

    write_string_to_file(&content, &resolved, append)
        .map_err(|e| signal_file_io_path(e, "Writing to", &resolved))?;

    if visit {
        // Need mutable access to set file_name and modified flag
        let buf_mut = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf_mut.file_name = Some(resolved);
        buf_mut.set_modified(false);
    }

    Ok(Value::Nil)
}

/// (find-file-noselect FILENAME &optional NOWARN RAWFILE) -> buffer
///
/// Read file FILENAME into a buffer and return the buffer.
/// If a buffer visiting FILENAME already exists, return it.
/// Does not select the buffer.
pub(crate) fn builtin_find_file_noselect(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("find-file-noselect", &args, 1)?;
    let filename = expect_string(&args[0])?;
    let abs_path = resolve_filename_for_eval(eval, &filename);

    // Check if there's already a buffer visiting this file
    for buf_id in eval.buffers.buffer_list() {
        if let Some(buf) = eval.buffers.get(buf_id) {
            if buf.file_name.as_deref() == Some(&abs_path) {
                return Ok(Value::Buffer(buf_id));
            }
        }
    }

    // Derive buffer name from file name
    let buf_name = file_name_nondirectory(&abs_path);
    let unique_name = eval.buffers.generate_new_buffer_name(&buf_name);
    let buf_id = eval.buffers.create_buffer(&unique_name);

    // If the file exists, read its contents into the new buffer
    if file_exists_p(&abs_path) {
        let contents = read_file_contents(&abs_path)
            .map_err(|e| signal_file_io_path(e, "Opening input file", &abs_path))?;

        // Save and restore current buffer around the insert
        let saved_current = eval
            .buffers
            .buffer_list()
            .into_iter()
            .find(|&id| eval.buffers.current_buffer().map_or(false, |b| b.id == id));

        eval.buffers.set_current(buf_id);
        if let Some(buf) = eval.buffers.get_mut(buf_id) {
            buf.insert(&contents);
            // Move point to the beginning
            buf.goto_char(0);
            buf.file_name = Some(abs_path);
            buf.set_modified(false);
        }

        // Restore the previous current buffer
        if let Some(prev_id) = saved_current {
            eval.buffers.set_current(prev_id);
        }
    } else {
        // File doesn't exist — create an empty buffer with the file name set
        if let Some(buf) = eval.buffers.get_mut(buf_id) {
            buf.file_name = Some(abs_path);
        }
    }

    Ok(Value::Buffer(buf_id))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::value::list_to_vec;
    use std::io::Write;

    // -----------------------------------------------------------------------
    // Path operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_expand_file_name_absolute() {
        let result = expand_file_name("/usr/bin/ls", None);
        assert_eq!(result, "/usr/bin/ls");
    }

    #[test]
    fn test_expand_file_name_relative() {
        let result = expand_file_name("foo.txt", Some("/home/user"));
        assert_eq!(result, "/home/user/foo.txt");
    }

    #[test]
    fn test_expand_file_name_tilde() {
        if std::env::var("HOME").is_ok() {
            let result = expand_file_name("~/test.txt", None);
            assert!(result.ends_with("/test.txt"));
            assert!(!result.starts_with("~"));
        }
    }

    #[test]
    fn test_expand_file_name_dotdot() {
        let result = expand_file_name("../bar.txt", Some("/home/user/dir"));
        assert_eq!(result, "/home/user/bar.txt");
    }

    #[test]
    fn test_expand_file_name_dot() {
        let result = expand_file_name("./foo.txt", Some("/home/user"));
        assert_eq!(result, "/home/user/foo.txt");
    }

    #[test]
    fn test_expand_file_name_preserves_directory_marker() {
        assert_eq!(
            expand_file_name("fixtures/", Some("/tmp")),
            "/tmp/fixtures/"
        );
        assert_eq!(expand_file_name("", Some("/tmp")), "/tmp");
    }

    #[test]
    fn test_file_truename_missing_file_and_trailing_slash() {
        assert_eq!(
            file_truename("/tmp/neovm-file-truename-missing", None),
            "/tmp/neovm-file-truename-missing"
        );
        assert_eq!(file_truename("/tmp/../tmp/", None), "/tmp/");
    }

    #[test]
    fn test_file_truename_resolves_relative_default_directory() {
        let dir = std::env::temp_dir().join("neovm-file-truename-rel");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("alpha.txt");
        fs::write(&file, b"alpha").unwrap();

        let resolved = file_truename("alpha.txt", Some(&dir.to_string_lossy()));
        assert_eq!(resolved, file.to_string_lossy());

        let _ = fs::remove_file(file);
        let _ = fs::remove_dir(dir);
    }

    #[test]
    fn test_file_name_directory() {
        assert_eq!(
            file_name_directory("/home/user/test.txt"),
            Some("/home/user/".to_string())
        );
        assert_eq!(file_name_directory("test.txt"), None);
        assert_eq!(
            file_name_directory("/home/user/dir/"),
            Some("/home/user/dir/".to_string())
        );
    }

    #[test]
    fn test_file_name_nondirectory() {
        assert_eq!(file_name_nondirectory("/home/user/test.txt"), "test.txt");
        assert_eq!(file_name_nondirectory("test.txt"), "test.txt");
        assert_eq!(file_name_nondirectory("/home/user/"), "");
    }

    #[test]
    fn test_file_name_extension() {
        assert_eq!(
            file_name_extension("test.txt", false),
            Some("txt".to_string())
        );
        assert_eq!(
            file_name_extension("test.txt", true),
            Some(".txt".to_string())
        );
        assert_eq!(
            file_name_extension("/home/user/file.el", false),
            Some("el".to_string())
        );
        assert_eq!(
            file_name_extension("/home/user/file.el", true),
            Some(".el".to_string())
        );
        assert_eq!(file_name_extension("no_ext", false), None);
        assert_eq!(file_name_extension("no_ext", true), Some("".to_string()));
        assert_eq!(file_name_extension(".bashrc", false), None);
        assert_eq!(file_name_extension(".bashrc", true), Some("".to_string()));
        assert_eq!(file_name_extension("..x", false), Some("x".to_string()));
        assert_eq!(file_name_extension("..x", true), Some(".x".to_string()));
        assert_eq!(file_name_extension("a.", false), Some("".to_string()));
        assert_eq!(file_name_extension("a.", true), Some(".".to_string()));
        assert_eq!(file_name_extension("foo.bar/", false), None);
        assert_eq!(file_name_extension("foo.bar/", true), Some("".to_string()));
        assert_eq!(
            file_name_extension("archive.tar.gz", false),
            Some("gz".to_string())
        );
        assert_eq!(
            file_name_extension("archive.tar.gz", true),
            Some(".gz".to_string())
        );
    }

    #[test]
    fn test_file_name_sans_extension() {
        assert_eq!(file_name_sans_extension("test.txt"), "test");
        assert_eq!(
            file_name_sans_extension("/home/user/file.el"),
            "/home/user/file"
        );
        assert_eq!(file_name_sans_extension("no_ext"), "no_ext");
        assert_eq!(file_name_sans_extension("archive.tar.gz"), "archive.tar");
        assert_eq!(file_name_sans_extension("foo.bar/"), "foo.bar/");
        assert_eq!(file_name_sans_extension("foo/"), "foo/");
        assert_eq!(file_name_sans_extension("/tmp/foo.bar/"), "/tmp/foo.bar/");
    }

    #[test]
    fn test_file_name_as_directory() {
        assert_eq!(file_name_as_directory("/tmp"), "/tmp/");
        assert_eq!(file_name_as_directory("/tmp/"), "/tmp/");
        assert_eq!(file_name_as_directory(""), "./");
        assert_eq!(file_name_as_directory("foo"), "foo/");
        assert_eq!(file_name_as_directory("foo/"), "foo/");
        assert_eq!(file_name_as_directory("~"), "~/");
        assert_eq!(file_name_as_directory("~/"), "~/");
    }

    #[test]
    fn test_directory_file_name() {
        assert_eq!(directory_file_name("/tmp/"), "/tmp");
        assert_eq!(directory_file_name("/tmp"), "/tmp");
        assert_eq!(directory_file_name("/"), "/");
        assert_eq!(directory_file_name("//"), "//");
        assert_eq!(directory_file_name("///"), "/");
        assert_eq!(directory_file_name("foo/"), "foo");
        assert_eq!(directory_file_name("foo"), "foo");
        assert_eq!(directory_file_name("a//"), "a");
        assert_eq!(directory_file_name("~/"), "~");
        assert_eq!(directory_file_name("~"), "~");
        assert_eq!(directory_file_name(""), "");
    }

    #[test]
    fn test_file_name_concat() {
        assert_eq!(file_name_concat(&["foo", "bar"]), "foo/bar");
        assert_eq!(file_name_concat(&["foo", "bar", "zot"]), "foo/bar/zot");
        assert_eq!(file_name_concat(&["foo/", "bar"]), "foo/bar");
        assert_eq!(file_name_concat(&["foo/", "bar/", "zot"]), "foo/bar/zot");
        assert_eq!(file_name_concat(&["foo", "/bar"]), "foo//bar");
        assert_eq!(file_name_concat(&["foo"]), "foo");
        assert_eq!(file_name_concat(&["foo/"]), "foo/");
        assert_eq!(file_name_concat(&["foo", "", "", ""]), "foo");
        assert_eq!(file_name_concat(&[""]), "");
        assert_eq!(file_name_concat(&["", "bar"]), "bar");
        assert_eq!(file_name_concat(&[]), "");
    }

    #[test]
    fn test_file_name_absolute_p() {
        assert!(file_name_absolute_p("/tmp"));
        assert!(file_name_absolute_p("~/tmp"));
        assert!(file_name_absolute_p("~"));
        assert!(file_name_absolute_p("~root"));
        assert!(!file_name_absolute_p("tmp"));
        assert!(!file_name_absolute_p("./tmp"));
    }

    #[test]
    fn test_directory_name_p() {
        assert!(directory_name_p("/tmp/"));
        assert!(directory_name_p("foo/"));
        assert!(!directory_name_p("/tmp"));
        assert!(!directory_name_p("foo"));
        assert!(!directory_name_p(""));
    }

    #[test]
    fn test_substitute_in_file_name() {
        let home = std::env::var("HOME").unwrap_or_default();

        assert_eq!(substitute_in_file_name("$HOME/foo"), format!("{home}/foo"));
        assert_eq!(
            substitute_in_file_name("${HOME}/foo"),
            format!("{home}/foo")
        );
        assert_eq!(substitute_in_file_name("$UNDEF/foo"), "$UNDEF/foo");
        assert_eq!(substitute_in_file_name("$$HOME"), "$HOME");
        assert_eq!(substitute_in_file_name("${}"), "${}");
        assert_eq!(substitute_in_file_name("$"), "$");
        assert_eq!(substitute_in_file_name("${HOME"), "${HOME");
        assert_eq!(substitute_in_file_name("bar/~/foo"), "~/foo");
        assert_eq!(
            substitute_in_file_name("/usr/local/$HOME/foo"),
            format!("{home}/foo")
        );
        assert_eq!(substitute_in_file_name("a//b"), "/b");
        assert_eq!(substitute_in_file_name("a///b"), "/b");
    }

    // -----------------------------------------------------------------------
    // File predicates
    // -----------------------------------------------------------------------

    #[test]
    fn test_file_exists_p() {
        assert!(file_exists_p("/tmp"));
        assert!(!file_exists_p("/nonexistent_path_12345"));
    }

    #[test]
    fn test_file_directory_p() {
        assert!(file_directory_p("/tmp"));
        assert!(!file_directory_p("/nonexistent_path_12345"));
    }

    #[test]
    fn test_file_regular_p() {
        // /tmp is a directory, not a regular file
        assert!(!file_regular_p("/tmp"));
        assert!(!file_regular_p("/nonexistent_path_12345"));
    }

    #[test]
    fn test_file_symlink_p() {
        // /tmp itself typically isn't a symlink
        assert!(!file_symlink_p("/nonexistent_path_12345"));
    }

    // -----------------------------------------------------------------------
    // File read/write
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_write_file() {
        let dir = std::env::temp_dir().join("neovm_fileio_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("test_rw.txt");
        let path_str = path.to_string_lossy().to_string();

        // Write
        write_string_to_file("hello, world\n", &path_str, false).unwrap();

        // Read back
        let contents = read_file_contents(&path_str).unwrap();
        assert_eq!(contents, "hello, world\n");

        // Append
        write_string_to_file("second line\n", &path_str, true).unwrap();
        let contents = read_file_contents(&path_str).unwrap();
        assert_eq!(contents, "hello, world\nsecond line\n");

        // Overwrite
        write_string_to_file("replaced\n", &path_str, false).unwrap();
        let contents = read_file_contents(&path_str).unwrap();
        assert_eq!(contents, "replaced\n");

        // Predicates on the file we just wrote
        assert!(file_exists_p(&path_str));
        assert!(file_regular_p(&path_str));
        assert!(file_readable_p(&path_str));
        assert!(!file_directory_p(&path_str));

        // Clean up
        delete_file(&path_str).unwrap();
        assert!(!file_exists_p(&path_str));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_error_symbol_mapping() {
        assert_eq!(file_error_symbol(ErrorKind::NotFound), "file-missing");
        assert_eq!(
            file_error_symbol(ErrorKind::AlreadyExists),
            "file-already-exists"
        );
        assert_eq!(
            file_error_symbol(ErrorKind::PermissionDenied),
            "permission-denied"
        );
        assert_eq!(file_error_symbol(ErrorKind::InvalidInput), "file-error");
    }

    #[test]
    fn test_signal_file_io_error_uses_specific_condition() {
        let flow = signal_file_io_error(
            std::io::Error::from(ErrorKind::PermissionDenied),
            "Writing to /tmp/neovm-probe".to_string(),
        );
        match flow {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "permission-denied");
                assert_eq!(sig.data.len(), 1);
                let Some(message) = sig.data[0].as_str() else {
                    panic!("expected string error payload");
                };
                assert!(message.contains("Writing to /tmp/neovm-probe"));
            }
            other => panic!("expected signal, got {:?}", other),
        }
    }

    #[test]
    fn test_delete_file_compat_missing_is_noop() {
        let path = std::env::temp_dir().join("neovm_delete_missing_noop.tmp");
        let path_str = path.to_string_lossy().to_string();
        let _ = fs::remove_file(&path);
        assert!(delete_file_compat(&path_str).is_ok());
    }

    #[test]
    fn test_builtin_delete_file_accepts_optional_trash_arg() {
        let path = std::env::temp_dir().join("neovm_delete_file_trash_arg.tmp");
        let path_str = path.to_string_lossy().to_string();
        let _ = fs::remove_file(&path);
        fs::write(&path, b"x").unwrap();

        let result = builtin_delete_file(vec![Value::string(&path_str), Value::True]).unwrap();
        assert_eq!(result, Value::Nil);
        assert!(!path.exists());

        let err = builtin_delete_file(vec![Value::string(&path_str), Value::Nil, Value::Nil])
            .unwrap_err();
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-number-of-arguments");
                assert_eq!(sig.data, vec![Value::symbol("delete-file"), Value::Int(3)]);
            }
            other => panic!("expected signal, got {:?}", other),
        }
    }

    #[test]
    fn test_builtin_delete_directory_basic_and_recursive() {
        let root = std::env::temp_dir().join("neovm_delete_directory_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let root_str = root.to_string_lossy().to_string();

        // Non-recursive removal succeeds for empty directories.
        assert_eq!(
            builtin_delete_directory(vec![Value::string(&root_str)]).unwrap(),
            Value::Nil
        );
        assert!(!root.exists());

        // Non-recursive removal fails for non-empty directories.
        fs::create_dir_all(&root).unwrap();
        let nested = root.join("child.txt");
        fs::write(&nested, b"x").unwrap();
        let err = builtin_delete_directory(vec![Value::string(&root_str)]).unwrap_err();
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "file-error");
            }
            other => panic!("expected signal, got {:?}", other),
        }

        // Recursive removal succeeds.
        assert_eq!(
            builtin_delete_directory(vec![Value::string(&root_str), Value::True]).unwrap(),
            Value::Nil
        );
        assert!(!root.exists());
    }

    #[test]
    fn test_builtin_delete_directory_eval_resolves_default_directory() {
        let base = std::env::temp_dir().join("neovm-delete-dir-eval");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", base.to_string_lossy())),
        );

        let child = base.join("child");
        fs::create_dir_all(&child).unwrap();
        builtin_delete_directory_eval(&eval, vec![Value::string("child")]).unwrap();
        assert!(!child.exists());

        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn test_builtin_make_symbolic_link_core_semantics() {
        let base = std::env::temp_dir().join("neovm-symlink-test");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let target = base.join("target.txt");
        let link = base.join("link.txt");
        fs::write(&target, b"x").unwrap();
        let target_str = target.to_string_lossy().to_string();
        let link_str = link.to_string_lossy().to_string();

        assert_eq!(
            builtin_make_symbolic_link(vec![Value::string(&target_str), Value::string(&link_str)])
                .unwrap(),
            Value::Nil
        );
        assert!(file_symlink_p(&link_str));

        let err =
            builtin_make_symbolic_link(vec![Value::string(&target_str), Value::string(&link_str)])
                .unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "file-already-exists"),
            other => panic!("expected signal, got {:?}", other),
        }

        assert_eq!(
            builtin_make_symbolic_link(vec![
                Value::string(&target_str),
                Value::string(&link_str),
                Value::True,
            ])
            .unwrap(),
            Value::Nil
        );

        delete_file(&link_str).unwrap();
        delete_file(&target_str).unwrap();
        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn test_builtin_make_symbolic_link_eval_uses_default_directory() {
        let base = std::env::temp_dir().join("neovm-symlink-eval");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", base.to_string_lossy())),
        );

        fs::write(base.join("target.txt"), b"x").unwrap();
        builtin_make_symbolic_link_eval(
            &eval,
            vec![Value::string("target.txt"), Value::string("link.txt")],
        )
        .unwrap();
        assert!(file_symlink_p(&base.join("link.txt").to_string_lossy()));

        delete_file(&base.join("link.txt").to_string_lossy()).unwrap();
        delete_file(&base.join("target.txt").to_string_lossy()).unwrap();
        let _ = fs::remove_dir_all(base);
    }

    // -----------------------------------------------------------------------
    // Directory operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_make_directory_and_directory_files() {
        let base = std::env::temp_dir().join("neovm_dirtest");
        let _ = fs::remove_dir_all(&base);
        let base_str = base.to_string_lossy().to_string();

        // Create with parents
        let nested = base.join("a/b/c");
        let nested_str = nested.to_string_lossy().to_string();
        make_directory(&nested_str, true).unwrap();
        assert!(file_directory_p(&nested_str));

        // Create files in the base directory
        for name in &["foo.txt", "bar.txt", "baz.el"] {
            let p = base.join(name);
            let mut f = fs::File::create(&p).unwrap();
            f.write_all(b"data").unwrap();
        }

        // List files
        let files = directory_files(&base_str, false, None, false, None).unwrap();
        assert!(files.contains(&".".to_string()));
        assert!(files.contains(&"..".to_string()));
        assert!(files.contains(&"foo.txt".to_string()));
        assert!(files.contains(&"bar.txt".to_string()));
        assert!(files.contains(&"baz.el".to_string()));

        // List with filter
        let filtered = directory_files(&base_str, false, Some("\\.el$"), false, None).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], "baz.el");

        // List with full paths
        let full = directory_files(&base_str, true, None, false, None).unwrap();
        for entry in &full {
            assert!(entry.starts_with(&base_str));
        }

        // Clean up
        let _ = fs::remove_dir_all(&base);
    }

    // -----------------------------------------------------------------------
    // File management: rename, copy
    // -----------------------------------------------------------------------

    #[test]
    fn test_rename_and_copy_file() {
        let dir = std::env::temp_dir().join("neovm_rename_copy_test");
        let _ = fs::create_dir_all(&dir);

        let src = dir.join("source.txt");
        let dst_rename = dir.join("renamed.txt");
        let dst_copy = dir.join("copied.txt");

        let src_str = src.to_string_lossy().to_string();
        let dst_rename_str = dst_rename.to_string_lossy().to_string();
        let dst_copy_str = dst_copy.to_string_lossy().to_string();

        // Create source
        write_string_to_file("original content", &src_str, false).unwrap();

        // Copy
        copy_file(&src_str, &dst_copy_str).unwrap();
        assert!(file_exists_p(&src_str));
        assert!(file_exists_p(&dst_copy_str));
        assert_eq!(
            read_file_contents(&dst_copy_str).unwrap(),
            "original content"
        );

        // Rename
        rename_file(&src_str, &dst_rename_str).unwrap();
        assert!(!file_exists_p(&src_str));
        assert!(file_exists_p(&dst_rename_str));
        assert_eq!(
            read_file_contents(&dst_rename_str).unwrap(),
            "original content"
        );

        // Clean up
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builtin_rename_file_overwrite_semantics() {
        let dir = std::env::temp_dir().join("neovm_builtin_rename_overwrite");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let src = dir.join("src.txt");
        let dst = dir.join("dst.txt");
        fs::write(&src, b"x").unwrap();
        fs::write(&dst, b"y").unwrap();
        let src_s = src.to_string_lossy().to_string();
        let dst_s = dst.to_string_lossy().to_string();

        let err = builtin_rename_file(vec![Value::string(&src_s), Value::string(&dst_s)]).unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "file-already-exists"),
            other => panic!("expected signal, got {:?}", other),
        }

        assert_eq!(
            builtin_rename_file(vec![Value::string(&src_s), Value::string(&dst_s), Value::True])
                .unwrap(),
            Value::Nil
        );
        assert!(!src.exists());
        assert!(dst.exists());

        let err = builtin_rename_file(vec![
            Value::string("a"),
            Value::string("b"),
            Value::Nil,
            Value::Nil,
        ])
        .unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "wrong-number-of-arguments"),
            other => panic!("expected wrong-number-of-arguments, got {:?}", other),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builtin_copy_file_optional_arg_semantics() {
        let dir = std::env::temp_dir().join("neovm_builtin_copy_optional");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let src = dir.join("src.txt");
        let dst = dir.join("dst.txt");
        fs::write(&src, b"src").unwrap();
        fs::write(&dst, b"dst").unwrap();
        let src_s = src.to_string_lossy().to_string();
        let dst_s = dst.to_string_lossy().to_string();

        let err = builtin_copy_file(vec![Value::string(&src_s), Value::string(&dst_s)]).unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "file-already-exists"),
            other => panic!("expected signal, got {:?}", other),
        }

        assert_eq!(
            builtin_copy_file(vec![Value::string(&src_s), Value::string(&dst_s), Value::True])
                .unwrap(),
            Value::Nil
        );

        assert_eq!(
            builtin_copy_file(vec![
                Value::string(&src_s),
                Value::string(&dst_s),
                Value::True,
                Value::True,
                Value::True,
                Value::True,
            ])
            .unwrap(),
            Value::Nil
        );

        let err = builtin_copy_file(vec![
            Value::string("a"),
            Value::string("b"),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
        ])
        .unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "wrong-number-of-arguments"),
            other => panic!("expected wrong-number-of-arguments, got {:?}", other),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builtin_add_name_to_file_semantics() {
        let dir = std::env::temp_dir().join("neovm_add_name_to_file_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let src = dir.join("source.txt");
        let dst = dir.join("alias.txt");
        fs::write(&src, b"x").unwrap();

        let src_str = src.to_string_lossy().to_string();
        let dst_str = dst.to_string_lossy().to_string();

        assert_eq!(
            builtin_add_name_to_file(vec![Value::string(&src_str), Value::string(&dst_str)]).unwrap(),
            Value::Nil
        );
        assert!(file_exists_p(&dst_str));
        assert!(file_equal_p(&src_str, &dst_str));

        let err = builtin_add_name_to_file(vec![Value::string(&src_str), Value::string(&dst_str)])
            .unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "file-already-exists"),
            other => panic!("expected signal, got {:?}", other),
        }

        assert_eq!(
            builtin_add_name_to_file(vec![
                Value::string(&src_str),
                Value::string(&dst_str),
                Value::True,
            ])
            .unwrap(),
            Value::Nil
        );
        assert!(file_equal_p(&src_str, &dst_str));

        let missing = dir.join("missing.txt").to_string_lossy().to_string();
        let dst2 = dir.join("alias2.txt").to_string_lossy().to_string();
        let err = builtin_add_name_to_file(vec![Value::string(&missing), Value::string(&dst2)])
            .unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "file-missing"),
            other => panic!("expected signal, got {:?}", other),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // File attributes
    // -----------------------------------------------------------------------

    #[test]
    fn test_file_attributes() {
        let dir = std::env::temp_dir().join("neovm_attrs_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("attrs.txt");
        let path_str = path.to_string_lossy().to_string();

        write_string_to_file("content", &path_str, false).unwrap();

        let attrs = file_attributes(&path_str).unwrap();
        assert_eq!(attrs.size, 7); // "content" is 7 bytes
        assert!(!attrs.is_dir);
        assert!(!attrs.is_symlink);
        assert!(attrs.modified.is_some());

        // Directory attributes
        let dir_str = dir.to_string_lossy().to_string();
        let dir_attrs = file_attributes(&dir_str).unwrap();
        assert!(dir_attrs.is_dir);

        // Non-existent file
        assert!(file_attributes("/nonexistent_path_12345").is_none());

        // Clean up
        let _ = fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // Builtin wrappers (Value-level)
    // -----------------------------------------------------------------------

    #[test]
    fn test_builtin_expand_file_name() {
        let result = builtin_expand_file_name(vec![Value::string("/usr/local/bin/emacs")]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), Some("/usr/local/bin/emacs"));

        // Emacs treats non-string DEFAULT-DIRECTORY as root.
        let result = builtin_expand_file_name(vec![Value::string("a"), Value::symbol("x")]);
        assert_eq!(result.unwrap().as_str(), Some("/a"));

        let result = builtin_expand_file_name(vec![Value::string("a"), Value::Nil, Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn test_builtin_expand_file_name_eval_uses_default_directory() {
        let mut eval = Evaluator::new();
        eval.obarray
            .set_symbol_value("default-directory", Value::string("/tmp/neovm-expand/"));

        let with_implicit = builtin_expand_file_name_eval(&eval, vec![Value::string("alpha.txt")]);
        assert_eq!(
            with_implicit.unwrap().as_str(),
            Some("/tmp/neovm-expand/alpha.txt")
        );

        let with_nil =
            builtin_expand_file_name_eval(&eval, vec![Value::string("beta.txt"), Value::Nil]);
        assert_eq!(
            with_nil.unwrap().as_str(),
            Some("/tmp/neovm-expand/beta.txt")
        );
    }

    #[test]
    fn test_builtin_file_truename_counter_validation() {
        let value =
            builtin_file_truename(vec![Value::string("/tmp"), Value::list(vec![])]).unwrap();
        assert_eq!(value.as_str(), Some("/tmp"));

        let err = builtin_file_truename(vec![Value::string("/tmp"), Value::Int(1)]).unwrap_err();
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("listp"), Value::Int(1)]);
            }
            other => panic!("expected signal, got {:?}", other),
        }

        let err = builtin_file_truename(vec![
            Value::string("/tmp"),
            Value::list(vec![Value::symbol("visited")]),
        ])
        .unwrap_err();
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![
                        Value::symbol("number-or-marker-p"),
                        Value::symbol("visited")
                    ]
                );
            }
            other => panic!("expected signal, got {:?}", other),
        }
    }

    #[test]
    fn test_builtin_file_truename_eval_uses_default_directory() {
        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string("/tmp/neovm-file-truename/"),
        );

        let value = builtin_file_truename_eval(&eval, vec![Value::string("alpha.txt")]).unwrap();
        assert_eq!(value.as_str(), Some("/tmp/neovm-file-truename/alpha.txt"));
    }

    #[test]
    fn test_builtin_make_temp_file_core_paths() {
        let file = builtin_make_temp_file(vec![Value::string("neovm-mtf-")]).unwrap();
        let file_path = file.as_str().unwrap().to_string();
        assert!(file_exists_p(&file_path));
        delete_file(&file_path).unwrap();

        let dir =
            builtin_make_temp_file(vec![Value::string("neovm-mtf-dir-"), Value::True]).unwrap();
        let dir_path = dir.as_str().unwrap().to_string();
        assert!(file_directory_p(&dir_path));
        fs::remove_dir(&dir_path).unwrap();

        let with_text = builtin_make_temp_file(vec![
            Value::string("neovm-mtf-text-"),
            Value::Nil,
            Value::string(".txt"),
            Value::string("abc"),
        ])
        .unwrap();
        let text_path = with_text.as_str().unwrap().to_string();
        assert_eq!(read_file_contents(&text_path).unwrap(), "abc");
        delete_file(&text_path).unwrap();
    }

    #[test]
    fn test_builtin_make_temp_file_validation() {
        let err = builtin_make_temp_file(vec![Value::Int(1)]).unwrap_err();
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("sequencep"), Value::Int(1)]);
            }
            other => panic!("expected signal, got {:?}", other),
        }

        let err = builtin_make_temp_file(vec![Value::string("neo"), Value::Nil, Value::Int(1)])
            .unwrap_err();
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::Int(1)]);
            }
            other => panic!("expected signal, got {:?}", other),
        }
    }

    #[test]
    fn test_builtin_make_temp_file_eval_honors_temp_directory() {
        let mut eval = Evaluator::new();
        let dir = std::env::temp_dir().join("neovm-mtf-eval");
        let _ = fs::create_dir_all(&dir);
        eval.obarray.set_symbol_value(
            "temporary-file-directory",
            Value::string(format!("{}/", dir.to_string_lossy())),
        );

        let value = builtin_make_temp_file_eval(&eval, vec![Value::string("eval-neo-")]).unwrap();
        let path = value.as_str().unwrap().to_string();
        assert!(path.starts_with(&dir.to_string_lossy().to_string()));
        assert!(file_exists_p(&path));
        delete_file(&path).unwrap();
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn test_builtin_make_nearby_temp_file_core_semantics() {
        let path = builtin_make_nearby_temp_file(vec![Value::string("neovm-nearby-")]).unwrap();
        let path_str = path.as_str().unwrap().to_string();
        assert!(file_exists_p(&path_str));
        delete_file(&path_str).unwrap();

        let dir =
            builtin_make_nearby_temp_file(vec![Value::string("neovm-nearby-dir-"), Value::True])
                .unwrap();
        let dir_str = dir.as_str().unwrap().to_string();
        assert!(file_directory_p(&dir_str));
        fs::remove_dir(&dir_str).unwrap();

        let base = std::env::temp_dir().join("neovm-nearby-parent");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let prefix = base.join("child-").to_string_lossy().to_string();
        let nearby = builtin_make_nearby_temp_file(vec![Value::string(&prefix)]).unwrap();
        let nearby_str = nearby.as_str().unwrap().to_string();
        assert_eq!(
            file_name_directory(&nearby_str),
            file_name_directory(&prefix),
        );
        assert!(file_exists_p(&nearby_str));
        delete_file(&nearby_str).unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn test_builtin_make_nearby_temp_file_eval_relative_prefix_uses_temp_dir() {
        let base = std::env::temp_dir().join("neovm-nearby-eval");
        let sub = base.join("sub");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&sub).unwrap();
        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", base.to_string_lossy())),
        );

        let err = builtin_make_nearby_temp_file_eval(&eval, vec![Value::string("sub/child-")])
            .unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "file-missing"),
            other => panic!("expected signal, got {:?}", other),
        }
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn test_builtin_file_predicates() {
        let result = builtin_file_exists_p(vec![Value::string("/tmp")]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_truthy());

        let result = builtin_file_directory_p(vec![Value::string("/tmp")]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_truthy());

        let result = builtin_file_exists_p(vec![Value::string("/no_such_file_xyz")]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn test_builtin_file_modes_semantics() {
        assert_eq!(builtin_file_modes(vec![Value::string("/tmp/neovm-file-modes-missing")]).unwrap(), Value::Nil);

        let path = builtin_make_temp_file(vec![Value::string("neovm-file-modes-")]).unwrap();
        let path_str = path.as_str().unwrap().to_string();
        let mode = builtin_file_modes(vec![Value::string(&path_str)]).unwrap();
        assert!(matches!(mode, Value::Int(_)));
        let with_flag = builtin_file_modes(vec![Value::string(&path_str), Value::True]).unwrap();
        assert!(matches!(with_flag, Value::Int(_)));
        delete_file(&path_str).unwrap();
    }

    #[test]
    fn test_builtin_file_modes_eval_respects_default_directory() {
        let base = std::env::temp_dir().join("neovm-file-modes-eval");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let file = base.join("alpha.txt");
        fs::write(&file, b"x").unwrap();

        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", base.to_string_lossy())),
        );
        let mode = builtin_file_modes_eval(&eval, vec![Value::string("alpha.txt")]).unwrap();
        assert!(matches!(mode, Value::Int(_)));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_set_file_modes_semantics() {
        let path = builtin_make_temp_file(vec![Value::string("neovm-set-file-modes-")]).unwrap();
        let path_str = path.as_str().unwrap().to_string();

        assert_eq!(
            builtin_set_file_modes(vec![Value::string(&path_str), Value::Int(0o600)]).unwrap(),
            Value::Nil
        );
        assert_eq!(
            builtin_set_file_modes(vec![Value::string(&path_str), Value::Int(0o640), Value::True])
                .unwrap(),
            Value::Nil
        );
        assert_eq!(
            builtin_file_modes(vec![Value::string(&path_str)]).unwrap().as_int(),
            Some(0o640)
        );

        delete_file(&path_str).unwrap();
    }

    #[test]
    fn test_builtin_set_file_modes_eval_respects_default_directory() {
        let base = std::env::temp_dir().join("neovm-set-file-modes-eval");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let file = base.join("alpha.txt");
        fs::write(&file, b"x").unwrap();

        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", base.to_string_lossy())),
        );
        builtin_set_file_modes_eval(
            &eval,
            vec![Value::string("alpha.txt"), Value::Int(0o600)],
        )
        .unwrap();
        assert_eq!(
            builtin_file_modes(vec![Value::string(file.to_string_lossy().to_string())])
                .unwrap()
                .as_int(),
            Some(0o600)
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_directory_files_args() {
        let dir = std::env::temp_dir().join("neovm_dirfiles_builtin");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        let file = dir.join("beta.el");
        fs::write(&file, "").unwrap();
        fs::write(dir.join("alpha.txt"), "").unwrap();
        fs::write(dir.join(".hidden"), "").unwrap();

        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::string("\\.el$"),
            Value::Nil,
            Value::Int(1),
        ])
        .unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str(), Some("beta.el"));

        let unsorted = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::True,
        ])
        .unwrap();
        let unsorted_items = list_to_vec(&unsorted).unwrap();

        let unsorted_limited = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::True,
            Value::Int(2),
        ])
        .unwrap();
        let unsorted_limited_items = list_to_vec(&unsorted_limited).unwrap();
        let tail = &unsorted_items[unsorted_items.len() - 2..];
        assert_eq!(unsorted_limited_items.as_slice(), tail);

        let sorted_limited = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Int(2),
        ])
        .unwrap();
        let mut sorted_from_unsorted = unsorted_limited_items.clone();
        sorted_from_unsorted.sort_by(|a, b| {
            let a = a.as_str().unwrap_or_default();
            let b = b.as_str().unwrap_or_default();
            a.cmp(b)
        });
        assert_eq!(list_to_vec(&sorted_limited).unwrap(), sorted_from_unsorted);

        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::True,
            Value::Int(0),
        ])
        .unwrap();
        assert!(list_to_vec(&result).unwrap().is_empty());

        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Int(-1),
        ]);
        assert!(result.is_err());

        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Int(0),
            Value::Nil,
        ]);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builtin_directory_files_eval_respects_default_directory() {
        let base = std::env::temp_dir().join("neovm_dirfiles_eval_builtin");
        let fixture = base.join("fixtures");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("alpha.txt"), "").unwrap();
        fs::write(fixture.join("beta.el"), "").unwrap();

        let mut eval = Evaluator::new();
        let base_str = format!("{}/", base.to_string_lossy());
        eval.obarray
            .set_symbol_value("default-directory", Value::string(&base_str));

        let result = builtin_directory_files_eval(
            &eval,
            vec![
                Value::string("fixtures"),
                Value::Nil,
                Value::string("\\.el$"),
            ],
        )
        .unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].as_str(), Some("beta.el"));

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_directory_files_nonexistent_signals_file_missing() {
        let result = builtin_directory_files(vec![Value::string("/nonexistent_dir_xyz_12345")]);
        match result {
            Err(Flow::Signal(sig)) => assert_eq!(sig.symbol, "file-missing"),
            other => panic!("expected file-missing signal, got {:?}", other),
        }
    }

    #[test]
    fn test_builtin_directory_files_invalid_regexp_signals_invalid_regexp() {
        let dir = std::env::temp_dir().join("neovm_dirfiles_invalid_regexp");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_string_lossy().to_string();

        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::string("[invalid"),
        ]);
        match result {
            Err(Flow::Signal(sig)) => assert_eq!(sig.symbol, "invalid-regexp"),
            other => panic!("expected invalid-regexp signal, got {:?}", other),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builtin_file_ops_eval_respects_default_directory() {
        let base = std::env::temp_dir().join("neovm_fileops_eval_builtin");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("alpha.txt"), "x").unwrap();

        let mut eval = Evaluator::new();
        let base_str = format!("{}/", base.to_string_lossy());
        eval.obarray
            .set_symbol_value("default-directory", Value::string(&base_str));

        builtin_copy_file_eval(
            &eval,
            vec![Value::string("alpha.txt"), Value::string("beta.txt")],
        )
        .unwrap();
        assert!(base.join("beta.txt").exists());

        builtin_rename_file_eval(
            &eval,
            vec![Value::string("beta.txt"), Value::string("gamma.txt")],
        )
        .unwrap();
        assert!(!base.join("beta.txt").exists());
        assert!(base.join("gamma.txt").exists());

        builtin_delete_file_eval(&eval, vec![Value::string("gamma.txt")]).unwrap();
        assert!(!base.join("gamma.txt").exists());

        builtin_add_name_to_file_eval(
            &eval,
            vec![Value::string("alpha.txt"), Value::string("delta.txt")],
        )
        .unwrap();
        assert!(base.join("delta.txt").exists());
        assert!(file_equal_p(
            &base.join("alpha.txt").to_string_lossy(),
            &base.join("delta.txt").to_string_lossy(),
        ));
        builtin_delete_file_eval(&eval, vec![Value::string("delta.txt")]).unwrap();

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_rename_file_eval_overwrite_semantics() {
        let base = std::env::temp_dir().join("neovm_rename_eval_overwrite");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("src.txt"), "x").unwrap();
        fs::write(base.join("dst.txt"), "y").unwrap();

        let mut eval = Evaluator::new();
        let base_str = format!("{}/", base.to_string_lossy());
        eval.obarray
            .set_symbol_value("default-directory", Value::string(&base_str));

        let err = builtin_rename_file_eval(
            &eval,
            vec![Value::string("src.txt"), Value::string("dst.txt")],
        )
        .unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "file-already-exists"),
            other => panic!("expected signal, got {:?}", other),
        }

        assert_eq!(
            builtin_rename_file_eval(
                &eval,
                vec![Value::string("src.txt"), Value::string("dst.txt"), Value::True],
            )
            .unwrap(),
            Value::Nil
        );
        assert!(!base.join("src.txt").exists());
        assert!(base.join("dst.txt").exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_copy_file_eval_optional_arg_semantics() {
        let base = std::env::temp_dir().join("neovm_copy_eval_optional");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("src.txt"), "src").unwrap();
        fs::write(base.join("dst.txt"), "dst").unwrap();

        let mut eval = Evaluator::new();
        let base_str = format!("{}/", base.to_string_lossy());
        eval.obarray
            .set_symbol_value("default-directory", Value::string(&base_str));

        let err = builtin_copy_file_eval(
            &eval,
            vec![Value::string("src.txt"), Value::string("dst.txt")],
        )
        .unwrap_err();
        match err {
            Flow::Signal(sig) => assert_eq!(sig.symbol, "file-already-exists"),
            other => panic!("expected signal, got {:?}", other),
        }

        assert_eq!(
            builtin_copy_file_eval(
                &eval,
                vec![Value::string("src.txt"), Value::string("dst.txt"), Value::True],
            )
            .unwrap(),
            Value::Nil
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_make_directory_eval_respects_default_directory() {
        let base = std::env::temp_dir().join("neovm_mkdir_eval_builtin");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let mut eval = Evaluator::new();
        let base_str = format!("{}/", base.to_string_lossy());
        eval.obarray
            .set_symbol_value("default-directory", Value::string(&base_str));

        builtin_make_directory_eval(&eval, vec![Value::string("child")]).unwrap();
        assert!(base.join("child").is_dir());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_file_name_ops() {
        let result = builtin_file_name_directory(vec![Value::string("/home/user/test.el")]);
        assert_eq!(result.unwrap().as_str(), Some("/home/user/"));

        let result = builtin_file_name_nondirectory(vec![Value::string("/home/user/test.el")]);
        assert_eq!(result.unwrap().as_str(), Some("test.el"));

        let result = builtin_file_name_extension(vec![Value::string("/home/user/test.el")]);
        assert_eq!(result.unwrap().as_str(), Some("el"));

        let result =
            builtin_file_name_extension(vec![Value::string("/home/user/test.el"), Value::True]);
        assert_eq!(result.unwrap().as_str(), Some(".el"));

        let result = builtin_file_name_extension(vec![Value::string("no_ext"), Value::True]);
        assert_eq!(result.unwrap().as_str(), Some(""));

        let result = builtin_file_name_sans_extension(vec![Value::string("/home/user/test.el")]);
        assert_eq!(result.unwrap().as_str(), Some("/home/user/test"));

        let result = builtin_file_name_as_directory(vec![Value::string("/home/user")]);
        assert_eq!(result.unwrap().as_str(), Some("/home/user/"));

        let result = builtin_directory_file_name(vec![Value::string("/home/user/")]);
        assert_eq!(result.unwrap().as_str(), Some("/home/user"));

        let result = builtin_file_name_concat(vec![
            Value::string("foo"),
            Value::string(""),
            Value::Nil,
            Value::string("bar"),
        ]);
        assert_eq!(result.unwrap().as_str(), Some("foo/bar"));
    }

    #[test]
    fn test_builtin_file_name_ops_strict_types() {
        assert!(builtin_file_name_directory(vec![Value::symbol("x")]).is_err());
        assert!(builtin_file_name_nondirectory(vec![Value::symbol("x")]).is_err());
        assert!(builtin_file_name_extension(vec![Value::symbol("x")]).is_err());
        assert!(
            builtin_file_name_extension(vec![Value::string("x"), Value::Nil, Value::Nil]).is_err()
        );
        assert!(builtin_file_name_sans_extension(vec![Value::symbol("x")]).is_err());
        assert!(builtin_file_name_as_directory(vec![Value::symbol("x")]).is_err());
        assert!(builtin_directory_file_name(vec![Value::symbol("x")]).is_err());
    }

    #[test]
    fn test_builtin_file_name_concat_strict_types() {
        let result = builtin_file_name_concat(vec![Value::Nil, Value::string("bar")]);
        assert_eq!(result.unwrap().as_str(), Some("bar"));

        let result = builtin_file_name_concat(vec![Value::symbol("foo"), Value::string("bar")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_builtin_path_predicates() {
        let result = builtin_file_name_absolute_p(vec![Value::string("/tmp")]);
        assert_eq!(result.unwrap(), Value::True);

        let result = builtin_file_name_absolute_p(vec![Value::string("tmp")]);
        assert_eq!(result.unwrap(), Value::Nil);

        let result = builtin_directory_name_p(vec![Value::string("foo/")]);
        assert_eq!(result.unwrap(), Value::True);

        let result = builtin_directory_name_p(vec![Value::string("foo")]);
        assert_eq!(result.unwrap(), Value::Nil);
    }

    #[test]
    fn test_builtin_path_predicates_strict_types() {
        let result = builtin_file_name_absolute_p(vec![Value::symbol("foo")]);
        assert!(result.is_err());

        let result = builtin_directory_name_p(vec![Value::Nil]);
        assert!(result.is_err());
    }

    #[test]
    fn test_builtin_file_predicates_strict_types() {
        assert!(builtin_file_exists_p(vec![Value::Nil]).is_err());
        assert!(builtin_file_readable_p(vec![Value::Nil]).is_err());
        assert!(builtin_file_writable_p(vec![Value::Nil]).is_err());
        assert!(builtin_file_directory_p(vec![Value::Nil]).is_err());
        assert!(builtin_file_regular_p(vec![Value::Nil]).is_err());
        assert!(builtin_file_symlink_p(vec![Value::Nil]).is_err());
        assert!(builtin_file_name_case_insensitive_p(vec![Value::Nil]).is_err());
        assert!(builtin_file_newer_than_file_p(vec![Value::Nil, Value::string("/tmp")]).is_err());
        assert!(builtin_file_newer_than_file_p(vec![Value::string("/tmp"), Value::Nil]).is_err());
        assert!(builtin_file_equal_p(vec![Value::Nil, Value::string("/tmp")]).is_err());
        assert!(builtin_file_equal_p(vec![Value::string("/tmp"), Value::Nil]).is_err());
        assert!(builtin_file_in_directory_p(vec![Value::Nil, Value::string("/tmp")]).is_err());
        assert!(builtin_file_in_directory_p(vec![Value::string("/tmp"), Value::Nil]).is_err());
    }

    #[test]
    fn test_eval_file_predicates_respect_default_directory() {
        use super::super::eval::Evaluator;

        let dir = std::env::temp_dir().join("neovm_fileio_eval_default_dir");
        let subdir = dir.join("subdir");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&subdir).expect("create test subdir");

        let mut eval = Evaluator::new();
        eval.obarray
            .set_symbol_value("default-directory", Value::string(dir.to_string_lossy()));

        let is_dir = builtin_file_directory_p_eval(&eval, vec![Value::string("subdir")])
            .expect("file-directory-p eval");
        assert!(is_dir.is_truthy());

        let exists = builtin_file_exists_p_eval(&eval, vec![Value::string("subdir")])
            .expect("file-exists-p eval");
        assert!(exists.is_truthy());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_name_case_insensitive_eval_respects_default_directory() {
        use super::super::eval::Evaluator;

        let dir = std::env::temp_dir().join("neovm_fileio_case_insensitive_eval");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");
        let file = dir.join("alpha.txt");
        fs::write(&file, b"x").expect("create test file");

        let absolute =
            builtin_file_name_case_insensitive_p(vec![Value::string(file.to_string_lossy())])
                .expect("absolute case-insensitive query");

        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", dir.to_string_lossy())),
        );
        let relative = builtin_file_name_case_insensitive_p_eval(
            &eval,
            vec![Value::string("alpha.txt")],
        )
        .expect("relative case-insensitive query");
        assert_eq!(relative, absolute);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builtin_file_newer_than_file_p_semantics() {
        let dir = std::env::temp_dir().join("neovm-file-newer-than-file-p");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");

        let old = dir.join("old.txt");
        let new = dir.join("new.txt");
        let missing = dir.join("missing.txt");

        fs::write(&old, b"old").expect("write old file");
        std::thread::sleep(std::time::Duration::from_millis(1200));
        fs::write(&new, b"new").expect("write new file");

        assert_eq!(
            builtin_file_newer_than_file_p(vec![
                Value::string(new.to_string_lossy()),
                Value::string(old.to_string_lossy()),
            ])
            .expect("newer"),
            Value::True
        );
        assert_eq!(
            builtin_file_newer_than_file_p(vec![
                Value::string(old.to_string_lossy()),
                Value::string(new.to_string_lossy()),
            ])
            .expect("older"),
            Value::Nil
        );
        assert_eq!(
            builtin_file_newer_than_file_p(vec![
                Value::string(missing.to_string_lossy()),
                Value::string(old.to_string_lossy()),
            ])
            .expect("missing first"),
            Value::Nil
        );
        assert_eq!(
            builtin_file_newer_than_file_p(vec![
                Value::string(old.to_string_lossy()),
                Value::string(missing.to_string_lossy()),
            ])
            .expect("missing second"),
            Value::True
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_newer_than_file_p_eval_respects_default_directory() {
        use super::super::eval::Evaluator;

        let dir = std::env::temp_dir().join("neovm-file-newer-than-file-p-eval");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");

        let old = dir.join("old.txt");
        let new = dir.join("new.txt");
        fs::write(&old, b"old").expect("write old file");
        std::thread::sleep(std::time::Duration::from_millis(1200));
        fs::write(&new, b"new").expect("write new file");

        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", dir.to_string_lossy())),
        );

        let result = builtin_file_newer_than_file_p_eval(
            &eval,
            vec![Value::string("new.txt"), Value::string("old.txt")],
        )
        .expect("relative newer check");
        assert_eq!(result, Value::True);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_builtin_file_equal_p_semantics() {
        let base = std::env::temp_dir().join("neovm-file-equal-p");
        let dir = base.join("dir");
        let a = dir.join("a.txt");
        let b = dir.join("b.txt");
        let missing = dir.join("missing.txt");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(&a, b"a").expect("write a");
        fs::write(&b, b"b").expect("write b");

        assert_eq!(
            builtin_file_equal_p(vec![
                Value::string(a.to_string_lossy()),
                Value::string(a.to_string_lossy()),
            ])
            .expect("same path"),
            Value::True
        );
        assert_eq!(
            builtin_file_equal_p(vec![
                Value::string(a.to_string_lossy()),
                Value::string(b.to_string_lossy()),
            ])
            .expect("different files"),
            Value::Nil
        );
        assert_eq!(
            builtin_file_equal_p(vec![
                Value::string(a.to_string_lossy()),
                Value::string(missing.to_string_lossy()),
            ])
            .expect("missing file"),
            Value::Nil
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = dir.join("link.txt");
            symlink(&a, &link).expect("symlink");
            assert_eq!(
                builtin_file_equal_p(vec![
                    Value::string(a.to_string_lossy()),
                    Value::string(link.to_string_lossy()),
                ])
                .expect("symlink equals target"),
                Value::True
            );
            let _ = fs::remove_file(&link);
        }

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_file_equal_p_eval_respects_default_directory() {
        use super::super::eval::Evaluator;

        let base = std::env::temp_dir().join("neovm-file-equal-p-eval");
        let dir = base.join("dir");
        let a = dir.join("a.txt");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(&a, b"a").expect("write a");

        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", base.to_string_lossy())),
        );

        assert_eq!(
            builtin_file_equal_p_eval(
                &eval,
                vec![Value::string("dir/a.txt"), Value::string("dir/./a.txt")],
            )
            .expect("relative equality in default-directory"),
            Value::True
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_file_in_directory_p_semantics() {
        let base = std::env::temp_dir().join("neovm-file-in-directory-p");
        let dir = base.join("dir");
        let out = base.join("out");
        let inside = dir.join("inside.txt");
        let outside = out.join("outside.txt");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&dir).expect("create dir");
        fs::create_dir_all(&out).expect("create out");
        fs::write(&inside, b"inside").expect("write inside");
        fs::write(&outside, b"outside").expect("write outside");

        assert_eq!(
            builtin_file_in_directory_p(vec![
                Value::string(inside.to_string_lossy()),
                Value::string(dir.to_string_lossy()),
            ])
            .expect("inside check"),
            Value::True
        );
        assert_eq!(
            builtin_file_in_directory_p(vec![
                Value::string(outside.to_string_lossy()),
                Value::string(dir.to_string_lossy()),
            ])
            .expect("outside check"),
            Value::Nil
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let link = dir.join("link");
            symlink(&out, &link).expect("symlink");
            let link_child = link.join("child.txt");
            assert_eq!(
                builtin_file_in_directory_p(vec![
                    Value::string(link_child.to_string_lossy()),
                    Value::string(dir.to_string_lossy()),
                ])
                .expect("symlink path against dir"),
                Value::Nil
            );
            assert_eq!(
                builtin_file_in_directory_p(vec![
                    Value::string(link_child.to_string_lossy()),
                    Value::string(out.to_string_lossy()),
                ])
                .expect("symlink path against target dir"),
                Value::True
            );
        }

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_file_in_directory_p_eval_respects_default_directory() {
        use super::super::eval::Evaluator;

        let base = std::env::temp_dir().join("neovm-file-in-directory-p-eval");
        let dir = base.join("dir");
        let file = dir.join("inside.txt");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&dir).expect("create dir");
        fs::write(&file, b"inside").expect("write file");

        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", base.to_string_lossy())),
        );

        assert_eq!(
            builtin_file_in_directory_p_eval(
                &eval,
                vec![Value::string("dir/inside.txt"), Value::string("dir")],
            )
            .expect("eval in-directory check"),
            Value::True
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_builtin_set_file_times_semantics() {
        let dir = std::env::temp_dir().join("neovm-set-file-times");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");

        let older = dir.join("older.txt");
        let newer = dir.join("newer.txt");
        fs::write(&older, b"older").expect("write older");
        fs::write(&newer, b"newer").expect("write newer");

        assert_eq!(
            builtin_set_file_times(vec![
                Value::string(older.to_string_lossy()),
                Value::Int(0),
            ])
            .expect("set-file-times"),
            Value::True
        );
        assert_eq!(
            builtin_set_file_times(vec![
                Value::string(newer.to_string_lossy()),
                Value::Nil,
                Value::True,
            ])
            .expect("set-file-times with flag"),
            Value::True
        );
        assert_eq!(
            builtin_file_newer_than_file_p(vec![
                Value::string(newer.to_string_lossy()),
                Value::string(older.to_string_lossy()),
            ])
            .expect("newer-than"),
            Value::True
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_set_file_times_eval_respects_default_directory() {
        use super::super::eval::Evaluator;

        let dir = std::env::temp_dir().join("neovm-set-file-times-eval");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create test dir");
        let file = dir.join("alpha.txt");
        fs::write(&file, b"alpha").expect("write file");

        let mut eval = Evaluator::new();
        eval.obarray.set_symbol_value(
            "default-directory",
            Value::string(format!("{}/", dir.to_string_lossy())),
        );

        assert_eq!(
            builtin_set_file_times_eval(
                &eval,
                vec![Value::string("alpha.txt"), Value::Int(0)],
            )
            .expect("eval set-file-times"),
            Value::True
        );
        let mtime = fs::metadata(&file)
            .expect("metadata")
            .modified()
            .expect("modified")
            .duration_since(UNIX_EPOCH)
            .expect("epoch")
            .as_secs();
        assert_eq!(mtime, 0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_default_file_modes_round_trip() {
        let original = builtin_default_file_modes(vec![])
            .expect("default-file-modes")
            .as_int()
            .expect("default-file-modes int");
        assert_eq!(
            builtin_set_default_file_modes(vec![Value::Int(0o700)]).expect("set-default-file-modes"),
            Value::Nil
        );
        assert_eq!(
            builtin_default_file_modes(vec![])
                .expect("default-file-modes after set")
                .as_int(),
            Some(0o700)
        );
        let _ = builtin_set_default_file_modes(vec![Value::Int(original)]);
    }

    #[test]
    fn test_default_file_modes_argument_errors() {
        assert!(builtin_set_default_file_modes(vec![]).is_err());
        assert!(builtin_default_file_modes(vec![Value::Int(1)]).is_err());
        assert!(builtin_set_default_file_modes(vec![Value::Nil]).is_err());
    }

    #[test]
    fn test_builtin_substitute_in_file_name() {
        let home = std::env::var("HOME").unwrap_or_default();
        let result = builtin_substitute_in_file_name(vec![Value::string("$HOME/foo")]).unwrap();
        assert_eq!(result.as_str(), Some(format!("{home}/foo").as_str()));
    }

    #[test]
    fn test_builtin_substitute_in_file_name_strict_type() {
        let result = builtin_substitute_in_file_name(vec![Value::symbol("foo")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_builtin_wrong_arg_count() {
        // expand-file-name needs at least 1 arg
        let result = builtin_expand_file_name(vec![]);
        assert!(result.is_err());

        // file-exists-p needs exactly 1 arg
        let result = builtin_file_exists_p(vec![]);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Evaluator-dependent builtins
    // -----------------------------------------------------------------------

    #[test]
    fn test_insert_file_contents_and_write_region() {
        use super::super::eval::Evaluator;

        let dir = std::env::temp_dir().join("neovm_eval_fileio_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("eval_test.txt");
        let path_str = path.to_string_lossy().to_string();

        // Write a test file to disk
        write_string_to_file("hello from file", &path_str, false).unwrap();

        let mut eval = Evaluator::new();

        // insert-file-contents
        let result = builtin_insert_file_contents(&mut eval, vec![Value::string(&path_str)]);
        assert!(result.is_ok());

        // Check that the buffer now contains the text
        let buf = eval.buffers.current_buffer().unwrap();
        assert_eq!(buf.buffer_string(), "hello from file");

        // write-region: write entire buffer to a new file
        let out_path = dir.join("output.txt");
        let out_str = out_path.to_string_lossy().to_string();
        let result = builtin_write_region(
            &mut eval,
            vec![Value::Nil, Value::Nil, Value::string(&out_str)],
        );
        assert!(result.is_ok());

        let written = read_file_contents(&out_str).unwrap();
        assert_eq!(written, "hello from file");

        // Clean up
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_eval_fileio_relative_paths_respect_default_directory() {
        use super::super::eval::Evaluator;

        let dir = std::env::temp_dir().join("neovm_eval_fileio_relative");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let alpha_path = dir.join("alpha.txt");
        fs::write(&alpha_path, "alpha\n").unwrap();
        let alpha_str = alpha_path.to_string_lossy().to_string();
        let out_path = dir.join("out.txt");
        let out_str = out_path.to_string_lossy().to_string();
        let default_dir = format!("{}/", dir.to_string_lossy());

        let mut eval_insert = Evaluator::new();
        eval_insert
            .obarray
            .set_symbol_value("default-directory", Value::string(&default_dir));
        let inserted =
            builtin_insert_file_contents(&mut eval_insert, vec![Value::string("alpha.txt")])
                .unwrap();
        let inserted_parts = list_to_vec(&inserted).unwrap();
        assert_eq!(inserted_parts[0].as_str(), Some(alpha_str.as_str()));
        let ibuf = eval_insert.buffers.current_buffer().unwrap();
        assert_eq!(ibuf.buffer_string(), "alpha\n");

        let mut eval_write = Evaluator::new();
        eval_write
            .obarray
            .set_symbol_value("default-directory", Value::string(&default_dir));
        eval_write
            .buffers
            .current_buffer_mut()
            .unwrap()
            .insert("neo");
        builtin_write_region(
            &mut eval_write,
            vec![Value::Nil, Value::Nil, Value::string("out.txt")],
        )
        .unwrap();
        assert_eq!(read_file_contents(&out_str).unwrap(), "neo");

        let mut eval_find = Evaluator::new();
        eval_find
            .obarray
            .set_symbol_value("default-directory", Value::string(&default_dir));
        let found =
            builtin_find_file_noselect(&mut eval_find, vec![Value::string("alpha.txt")]).unwrap();
        let Value::Buffer(buf_id) = found else {
            panic!("expected Buffer");
        };
        let fbuf = eval_find.buffers.get(buf_id).unwrap();
        assert_eq!(fbuf.buffer_string(), "alpha\n");
        assert_eq!(fbuf.file_name.as_deref(), Some(alpha_str.as_str()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_file_noselect() {
        use super::super::eval::Evaluator;

        let dir = std::env::temp_dir().join("neovm_findfile_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("findme.txt");
        let path_str = path.to_string_lossy().to_string();

        write_string_to_file("file content here", &path_str, false).unwrap();

        let mut eval = Evaluator::new();

        // find-file-noselect
        let result = builtin_find_file_noselect(&mut eval, vec![Value::string(&path_str)]);
        assert!(result.is_ok());
        let buf_val = result.unwrap();
        match &buf_val {
            Value::Buffer(id) => {
                let buf = eval.buffers.get(*id).unwrap();
                assert_eq!(buf.buffer_string(), "file content here");
                assert!(buf.file_name.is_some());
                assert!(!buf.is_modified());
            }
            other => panic!("Expected Buffer, got {:?}", other),
        }

        // Calling again with the same file should return the same buffer
        let result2 = builtin_find_file_noselect(&mut eval, vec![Value::string(&path_str)]);
        assert!(result2.is_ok());
        match (&buf_val, &result2.unwrap()) {
            (Value::Buffer(a), Value::Buffer(b)) => assert_eq!(a, b),
            _ => panic!("Expected matching Buffer values"),
        }

        // Clean up
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_file_noselect_nonexistent() {
        use super::super::eval::Evaluator;

        let mut eval = Evaluator::new();
        let result = builtin_find_file_noselect(
            &mut eval,
            vec![Value::string("/tmp/neovm_nonexistent_file_xyz.txt")],
        );
        assert!(result.is_ok());
        match result.unwrap() {
            Value::Buffer(id) => {
                let buf = eval.buffers.get(id).unwrap();
                // Buffer should be empty for a nonexistent file
                assert_eq!(buf.buffer_string(), "");
                assert!(buf.file_name.is_some());
            }
            other => panic!("Expected Buffer, got {:?}", other),
        }
    }
}
