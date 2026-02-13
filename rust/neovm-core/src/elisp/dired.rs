//! Directory and file attribute builtins for the Elisp interpreter.
//!
//! Provides dired-related primitives:
//! - `directory-files`, `directory-files-and-attributes`
//! - `file-name-completion`, `file-name-all-completions`
//! - `file-attributes`, `file-attributes-lessp`
//! - `system-users`, `system-groups`

use super::error::{signal, EvalResult, Flow};
use super::value::*;
use std::collections::VecDeque;
#[cfg(unix)]
use std::ffi::{CStr, CString};
use std::fs;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_range_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_string(name: &str, value: &Value) -> Result<String, Flow> {
    match value {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

/// Ensure a directory path ends with '/'.
fn ensure_trailing_slash(dir: &str) -> String {
    if dir.ends_with('/') {
        dir.to_string()
    } else {
        format!("{}/", dir)
    }
}

#[cfg(unix)]
fn read_directory_names(dir: &str) -> Result<Vec<String>, Flow> {
    let dir_cstr = CString::new(dir).map_err(|_| {
        signal(
            "file-error",
            vec![
                Value::string("Opening directory"),
                Value::string("path contains interior NUL"),
                Value::string(dir),
            ],
        )
    })?;
    let dirp = unsafe { libc::opendir(dir_cstr.as_ptr()) };
    if dirp.is_null() {
        return Err(signal(
            "file-error",
            vec![
                Value::string("Opening directory"),
                Value::string(std::io::Error::last_os_error().to_string()),
                Value::string(dir),
            ],
        ));
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
fn read_directory_names(dir: &str) -> Result<Vec<String>, Flow> {
    let entries = fs::read_dir(dir).map_err(|e| {
        signal(
            "file-error",
            vec![
                Value::string("Opening directory"),
                Value::string(e.to_string()),
                Value::string(dir),
            ],
        )
    })?;
    let mut names = vec![".".to_string(), "..".to_string()];
    for entry in entries {
        let entry = entry.map_err(|e| {
            signal(
                "file-error",
                vec![Value::string(format!("Reading directory entry: {}", e))],
            )
        })?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

fn parse_wholenump_count(arg: Option<&Value>) -> Result<Option<usize>, Flow> {
    match arg {
        Some(Value::Int(n)) if *n >= 0 => Ok(Some(*n as usize)),
        Some(v @ Value::Int(_)) => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("wholenump"), v.clone()],
        )),
        Some(v) if v.is_truthy() => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("wholenump"), v.clone()],
        )),
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Time helpers
// ---------------------------------------------------------------------------

/// Convert seconds since epoch to Emacs (HIGH LOW) time format.
/// HIGH is the upper 16 bits, LOW is the lower 16 bits of the seconds count.
fn time_to_high_low(secs: f64) -> Value {
    let total = secs as u64;
    let high = (total >> 16) as i64;
    let low = (total & 0xFFFF) as i64;
    Value::list(vec![Value::Int(high), Value::Int(low)])
}

/// Get time from SystemTime as seconds since epoch.
fn system_time_to_secs(time: std::time::SystemTime) -> Option<f64> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64())
}

// ---------------------------------------------------------------------------
// file-attributes core
// ---------------------------------------------------------------------------

/// Build the Emacs-compatible file-attributes list for a path.
///
/// Returns:
///   (TYPE NLINKS UID GID ATIME MTIME CTIME SIZE MODE GID-CHANGEP INODE DEVICE)
///
/// TYPE is:
///   t        for a directory
///   nil      for a regular file
///   string   for a symlink (the link target)
///
/// Times are in Emacs (HIGH LOW) format.
/// If ID-FORMAT is 'string, UID/GID are returned as strings; otherwise integers.
fn build_file_attributes(filename: &str, id_format_string: bool) -> Option<Value> {
    // Use symlink_metadata first to detect symlinks.
    let sym_meta = fs::symlink_metadata(filename).ok()?;

    // Determine file type.
    let file_type = if sym_meta.file_type().is_symlink() {
        // Read the symlink target.
        match fs::read_link(filename) {
            Ok(target) => Value::string(target.to_string_lossy().into_owned()),
            Err(_) => Value::string(""),
        }
    } else if sym_meta.is_dir() {
        Value::True
    } else {
        Value::Nil
    };

    // For symlinks, get the target metadata for size etc; fall back to symlink meta.
    let meta = if sym_meta.file_type().is_symlink() {
        fs::metadata(filename).unwrap_or_else(|_| sym_meta.clone())
    } else {
        sym_meta.clone()
    };

    // Number of hard links.
    #[cfg(unix)]
    let nlinks = {
        use std::os::unix::fs::MetadataExt;
        Value::Int(sym_meta.nlink() as i64)
    };
    #[cfg(not(unix))]
    let nlinks = Value::Int(1);

    // UID / GID.
    #[cfg(unix)]
    let (uid_val, gid_val) = {
        use std::os::unix::fs::MetadataExt;
        let uid = sym_meta.uid();
        let gid = sym_meta.gid();
        if id_format_string {
            // Attempt to resolve names; fall back to numeric string.
            (
                Value::string(uid.to_string()),
                Value::string(gid.to_string()),
            )
        } else {
            (Value::Int(uid as i64), Value::Int(gid as i64))
        }
    };
    #[cfg(not(unix))]
    let (uid_val, gid_val) = if id_format_string {
        (Value::string("0"), Value::string("0"))
    } else {
        (Value::Int(0), Value::Int(0))
    };

    // Access time.
    #[cfg(unix)]
    let atime = {
        use std::os::unix::fs::MetadataExt;
        time_to_high_low(sym_meta.atime() as f64)
    };
    #[cfg(not(unix))]
    let atime = meta
        .accessed()
        .ok()
        .and_then(|t| system_time_to_secs(t))
        .map(time_to_high_low)
        .unwrap_or(Value::Nil);

    // Modification time.
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| system_time_to_secs(t))
        .map(time_to_high_low)
        .unwrap_or(Value::Nil);

    // Status change time (ctime on Unix, creation time on other platforms).
    #[cfg(unix)]
    let ctime = {
        use std::os::unix::fs::MetadataExt;
        time_to_high_low(sym_meta.ctime() as f64)
    };
    #[cfg(not(unix))]
    let ctime = meta
        .created()
        .ok()
        .and_then(|t| system_time_to_secs(t))
        .map(time_to_high_low)
        .unwrap_or(Value::Nil);

    // Size.
    let size = Value::Int(meta.len() as i64);

    // Mode string (like "drwxr-xr-x").
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        let mode_bits = sym_meta.permissions().mode();
        Value::string(format_mode_string(mode_bits, &sym_meta))
    };
    #[cfg(not(unix))]
    let mode = Value::string(if meta.is_dir() {
        "drwxr-xr-x"
    } else {
        "-rw-r--r--"
    });

    // GID-CHANGEP: whether the file's GID would change on recreation.
    // Stub: always nil.
    let gid_changep = Value::Nil;

    // Inode.
    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        Value::Int(sym_meta.ino() as i64)
    };
    #[cfg(not(unix))]
    let inode = Value::Int(0);

    // Device.
    #[cfg(unix)]
    let device = {
        use std::os::unix::fs::MetadataExt;
        Value::Int(sym_meta.dev() as i64)
    };
    #[cfg(not(unix))]
    let device = Value::Int(0);

    Some(Value::list(vec![
        file_type,
        nlinks,
        uid_val,
        gid_val,
        atime,
        mtime,
        ctime,
        size,
        mode,
        gid_changep,
        inode,
        device,
    ]))
}

/// Format a Unix file mode string like "drwxr-xr-x" or "-rw-r--r--".
#[cfg(unix)]
fn format_mode_string(mode: u32, meta: &fs::Metadata) -> String {
    let mut s = String::with_capacity(10);

    // File type character.
    if meta.file_type().is_symlink() {
        s.push('l');
    } else if meta.is_dir() {
        s.push('d');
    } else {
        s.push('-');
    }

    // Owner permissions.
    s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o4000 != 0 {
        if mode & 0o100 != 0 {
            's'
        } else {
            'S'
        }
    } else if mode & 0o100 != 0 {
        'x'
    } else {
        '-'
    });

    // Group permissions.
    s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o2000 != 0 {
        if mode & 0o010 != 0 {
            's'
        } else {
            'S'
        }
    } else if mode & 0o010 != 0 {
        'x'
    } else {
        '-'
    });

    // Other permissions.
    s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o1000 != 0 {
        if mode & 0o001 != 0 {
            't'
        } else {
            'T'
        }
    } else if mode & 0o001 != 0 {
        'x'
    } else {
        '-'
    });

    s
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (directory-files DIRECTORY &optional FULL-NAME MATCH-REGEXP NOSORT COUNT)
///
/// Return a list of names of files in DIRECTORY.
/// If FULL-NAME is non-nil, return absolute file names.
/// If MATCH-REGEXP is non-nil, only return names matching the regexp.
/// If NOSORT is nil, sort the result alphabetically.
/// COUNT limits the number of results returned.
pub(crate) fn builtin_directory_files(args: Vec<Value>) -> EvalResult {
    expect_range_args("directory-files", &args, 1, 5)?;

    let dir = expect_string("directory-files", &args[0])?;
    let full_name = args.get(1).is_some_and(|v| v.is_truthy());
    let match_regexp = match args.get(2) {
        Some(v) if v.is_truthy() => Some(expect_string("directory-files", v)?),
        _ => None,
    };
    let nosort = args.get(3).is_some_and(|v| v.is_truthy());
    let count = parse_wholenump_count(args.get(4))?;
    if count == Some(0) {
        return Ok(Value::Nil);
    }

    // Compile regex if provided.
    let re = match &match_regexp {
        Some(pattern) => {
            let compiled = regex::Regex::new(pattern).map_err(|e| {
                signal(
                    "invalid-regexp",
                    vec![Value::string(format!(
                        "Invalid regexp \"{}\": {}",
                        pattern, e
                    ))],
                )
            })?;
            Some(compiled)
        }
        None => None,
    };

    let names = read_directory_names(&dir)?;

    let dir_with_slash = ensure_trailing_slash(&dir);
    let mut result: VecDeque<String> = VecDeque::new();
    let mut remaining = count.unwrap_or(usize::MAX);
    for name in names {

        // Apply regex filter.
        if let Some(ref re) = re {
            if !re.is_match(&name) {
                continue;
            }
        }

        if full_name {
            result.push_front(format!("{}{}", dir_with_slash, name));
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
    // Sort unless NOSORT is non-nil.
    if !nosort {
        result.sort();
    }

    Ok(Value::list(result.into_iter().map(Value::string).collect()))
}

/// (directory-files-and-attributes DIRECTORY &optional FULL-NAME MATCH-REGEXP NOSORT ID-FORMAT COUNT)
///
/// Like `directory-files` but each element is (NAME . ATTRIBUTES) where
/// ATTRIBUTES is the result of `file-attributes`.
pub(crate) fn builtin_directory_files_and_attributes(args: Vec<Value>) -> EvalResult {
    expect_range_args("directory-files-and-attributes", &args, 1, 6)?;

    let dir = expect_string("directory-files-and-attributes", &args[0])?;
    let full_name = args.get(1).is_some_and(|v| v.is_truthy());
    let match_regexp = match args.get(2) {
        Some(v) if v.is_truthy() => Some(expect_string("directory-files-and-attributes", v)?),
        _ => None,
    };
    let nosort = args.get(3).is_some_and(|v| v.is_truthy());
    let id_format_string = args.get(4).is_some_and(|v| v.is_truthy());
    let count = parse_wholenump_count(args.get(5))?;
    if count == Some(0) {
        return Ok(Value::Nil);
    }

    // Compile regex if provided.
    let re = match &match_regexp {
        Some(pattern) => {
            let compiled = regex::Regex::new(pattern).map_err(|e| {
                signal(
                    "invalid-regexp",
                    vec![Value::string(format!(
                        "Invalid regexp \"{}\": {}",
                        pattern, e
                    ))],
                )
            })?;
            Some(compiled)
        }
        None => None,
    };

    let names = read_directory_names(&dir)?;

    let dir_with_slash = ensure_trailing_slash(&dir);
    let mut items: VecDeque<(String, String)> = VecDeque::new();
    let mut remaining = count.unwrap_or(usize::MAX);
    for name in names {

        // Apply regex filter.
        if let Some(ref re) = re {
            if !re.is_match(&name) {
                continue;
            }
        }

        let full_path = format!("{}{}", dir_with_slash, name);
        let display_name = if full_name { full_path.clone() } else { name };
        items.push_front((display_name, full_path));

        if remaining != usize::MAX {
            remaining -= 1;
            if remaining == 0 {
                break;
            }
        }
    }

    let mut items: Vec<(String, String)> = items.into_iter().collect();
    // Sort unless NOSORT is non-nil.
    if !nosort {
        items.sort_by(|a, b| a.0.cmp(&b.0));
    }

    // Build result list of (NAME . ATTRIBUTES) cons cells.
    let result: Vec<Value> = items
        .into_iter()
        .map(|(display_name, full_path)| {
            let attrs = build_file_attributes(&full_path, id_format_string).unwrap_or(Value::Nil);
            Value::cons(Value::string(display_name), attrs)
        })
        .collect();

    Ok(Value::list(result))
}

/// (file-name-completion FILE DIRECTORY &optional PREDICATE)
///
/// Complete file name FILE in DIRECTORY.
/// Returns the longest common completion prefix, or t if FILE is an exact
/// and unique match, or nil if no completions exist.
/// PREDICATE is accepted but currently ignored.
pub(crate) fn builtin_file_name_completion(args: Vec<Value>) -> EvalResult {
    expect_range_args("file-name-completion", &args, 2, 3)?;

    let file = expect_string("file-name-completion", &args[0])?;
    let directory = expect_string("file-name-completion", &args[1])?;

    let entries = fs::read_dir(&directory).map_err(|e| {
        signal(
            "file-error",
            vec![
                Value::string("Opening directory"),
                Value::string(e.to_string()),
                Value::string(&directory),
            ],
        )
    })?;

    let mut completions: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            signal(
                "file-error",
                vec![Value::string(format!("Reading directory entry: {}", e))],
            )
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&file) {
            // Append '/' for directories.
            let full_path = std::path::Path::new(&directory).join(&name);
            if full_path.is_dir() {
                completions.push(format!("{}/", name));
            } else {
                completions.push(name);
            }
        }
    }

    if completions.is_empty() {
        return Ok(Value::Nil);
    }

    // If there is exactly one completion and it matches FILE exactly
    // (possibly with trailing slash for directories), return t.
    if completions.len() == 1 {
        let comp = &completions[0];
        let base = comp.strip_suffix('/').unwrap_or(comp);
        if base == file {
            return Ok(Value::True);
        }
        return Ok(Value::string(comp.clone()));
    }

    // Find the longest common prefix among completions.
    let mut prefix = completions[0].clone();
    for comp in &completions[1..] {
        let common_len = prefix
            .chars()
            .zip(comp.chars())
            .take_while(|(a, b)| a == b)
            .count();
        prefix.truncate(
            prefix
                .char_indices()
                .nth(common_len)
                .map(|(i, _)| i)
                .unwrap_or(prefix.len()),
        );
    }

    // If the prefix equals the input exactly and there are multiple matches,
    // return the prefix (Emacs returns what was typed if ambiguous but valid prefix).
    Ok(Value::string(prefix))
}

/// (file-name-all-completions FILE DIRECTORY)
///
/// Return a list of all completions of FILE in DIRECTORY.
/// Each entry that is a directory has a trailing '/'.
pub(crate) fn builtin_file_name_all_completions(args: Vec<Value>) -> EvalResult {
    expect_range_args("file-name-all-completions", &args, 2, 2)?;

    let file = expect_string("file-name-all-completions", &args[0])?;
    let directory = expect_string("file-name-all-completions", &args[1])?;

    let entries = fs::read_dir(&directory).map_err(|e| {
        signal(
            "file-error",
            vec![
                Value::string("Opening directory"),
                Value::string(e.to_string()),
                Value::string(&directory),
            ],
        )
    })?;

    let mut completions: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            signal(
                "file-error",
                vec![Value::string(format!("Reading directory entry: {}", e))],
            )
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&file) {
            // Append '/' for directories.
            let full_path = std::path::Path::new(&directory).join(&name);
            if full_path.is_dir() {
                completions.push(format!("{}/", name));
            } else {
                completions.push(name);
            }
        }
    }

    completions.sort();

    Ok(Value::list(
        completions.into_iter().map(Value::string).collect(),
    ))
}

/// (file-attributes FILENAME &optional ID-FORMAT)
///
/// Return a list of attributes of file FILENAME.
/// The list elements are:
///   0. TYPE (t=dir, nil=regular, string=symlink target)
///   1. Number of hard links
///   2. UID (integer or string if ID-FORMAT is 'string)
///   3. GID (integer or string if ID-FORMAT is 'string)
///   4. Last access time (HIGH LOW)
///   5. Last modification time (HIGH LOW)
///   6. Status change time (HIGH LOW)
///   7. Size in bytes
///   8. File modes as string (like "drwxr-xr-x")
///   9. GID-CHANGEP (always nil)
///  10. Inode number
///  11. Device number
pub(crate) fn builtin_file_attributes(args: Vec<Value>) -> EvalResult {
    expect_range_args("file-attributes", &args, 1, 2)?;

    let filename = expect_string("file-attributes", &args[0])?;
    let id_format_string = match args.get(1) {
        Some(Value::Symbol(s)) if s == "string" => true,
        _ => false,
    };

    match build_file_attributes(&filename, id_format_string) {
        Some(attrs) => Ok(attrs),
        None => Ok(Value::Nil),
    }
}

/// (file-attributes-lessp F1 F2)
///
/// Return t if the first element (filename) of F1 is less than that of F2.
/// F1 and F2 are each (NAME . ATTRIBUTES) cons cells as returned by
/// `directory-files-and-attributes`.
pub(crate) fn builtin_file_attributes_lessp(args: Vec<Value>) -> EvalResult {
    expect_range_args("file-attributes-lessp", &args, 2, 2)?;

    let name1 = extract_car_string("file-attributes-lessp", &args[0])?;
    let name2 = extract_car_string("file-attributes-lessp", &args[1])?;

    Ok(Value::bool(name1 < name2))
}

/// Extract the car of a cons cell as a string.
fn extract_car_string(name: &str, val: &Value) -> Result<String, Flow> {
    match val {
        Value::Cons(cell) => {
            let pair = cell.lock().expect("poisoned");
            match &pair.car {
                Value::Str(s) => Ok((**s).clone()),
                other => Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("stringp"), other.clone()],
                )),
            }
        }
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("consp"), other.clone()],
        )),
    }
}

/// (system-users)
///
/// Return a list of user names on the system.
/// Stub implementation: returns a list containing the current user from
/// the USER environment variable, or "unknown" if not set.
pub(crate) fn builtin_system_users(args: Vec<Value>) -> EvalResult {
    expect_range_args("system-users", &args, 0, 0)?;

    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string());

    Ok(Value::list(vec![Value::string(user)]))
}

/// (system-groups)
///
/// Return a list of group names on the system.
/// Stub: returns nil.
pub(crate) fn builtin_system_groups(args: Vec<Value>) -> EvalResult {
    expect_range_args("system-groups", &args, 0, 0)?;
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_test_dir(name: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("neovm_dired_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        (dir, dir_str)
    }

    fn create_file(dir: &std::path::Path, name: &str, content: &str) {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    // -----------------------------------------------------------------------
    // directory-files
    // -----------------------------------------------------------------------

    #[test]
    fn test_directory_files_basic() {
        let (dir, dir_str) = make_test_dir("df_basic");
        create_file(&dir, "alpha.txt", "a");
        create_file(&dir, "beta.txt", "b");
        create_file(&dir, "gamma.el", "g");

        let result = builtin_directory_files(vec![Value::string(&dir_str)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        let names: Vec<String> = items
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

        assert!(names.contains(&"alpha.txt".to_string()));
        assert!(names.contains(&"beta.txt".to_string()));
        assert!(names.contains(&"gamma.el".to_string()));
        // Should be sorted.
        assert!(names.windows(2).all(|w| w[0] <= w[1]));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_directory_files_full_name() {
        let (dir, dir_str) = make_test_dir("df_full");
        create_file(&dir, "file.txt", "data");

        let result = builtin_directory_files(vec![Value::string(&dir_str), Value::True]).unwrap();
        let items = list_to_vec(&result).unwrap();
        for item in &items {
            let s = item.as_str().unwrap();
            assert!(s.starts_with(&dir_str), "Expected full path, got: {}", s);
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_directory_files_match_regexp() {
        let (dir, dir_str) = make_test_dir("df_re");
        create_file(&dir, "foo.el", "");
        create_file(&dir, "bar.el", "");
        create_file(&dir, "baz.txt", "");

        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::string("\\.el$"),
        ])
        .unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        for item in &items {
            assert!(item.as_str().unwrap().ends_with(".el"));
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_directory_files_nosort() {
        let (dir, dir_str) = make_test_dir("df_nosort");
        create_file(&dir, "zzz.txt", "");
        create_file(&dir, "aaa.txt", "");

        // NOSORT = t: we just check it does not error.
        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::True,
        ]);
        assert!(result.is_ok());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_directory_files_count() {
        let (dir, dir_str) = make_test_dir("df_count");
        create_file(&dir, "a.txt", "");
        create_file(&dir, "b.txt", "");
        create_file(&dir, "c.txt", "");

        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Int(2),
        ])
        .unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_directory_files_nonexistent() {
        let result = builtin_directory_files(vec![Value::string("/nonexistent_dir_xyz_12345")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_directory_files_invalid_regexp() {
        let (dir, dir_str) = make_test_dir("df_badre");
        create_file(&dir, "a.txt", "");

        let result = builtin_directory_files(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::string("[invalid"),
        ]);
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // directory-files-and-attributes
    // -----------------------------------------------------------------------

    #[test]
    fn test_directory_files_and_attributes_basic() {
        let (dir, dir_str) = make_test_dir("dfa_basic");
        create_file(&dir, "test.txt", "hello");

        let result = builtin_directory_files_and_attributes(vec![Value::string(&dir_str)]).unwrap();
        let items = list_to_vec(&result).unwrap();

        // Find our file.
        let mut found = false;
        for item in &items {
            if let Value::Cons(cell) = item {
                let pair = cell.lock().unwrap();
                if pair.car.as_str() == Some("test.txt") {
                    found = true;
                    // cdr should be a list (the attributes).
                    assert!(pair.cdr.is_cons() || pair.cdr.is_nil());
                }
            }
        }
        assert!(found, "test.txt not found in results");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_directory_files_and_attributes_order_and_count() {
        let (dir, dir_str) = make_test_dir("dfa_count");
        create_file(&dir, "alpha.txt", "");
        create_file(&dir, "beta.txt", "");
        create_file(&dir, "z.txt", "");

        let unsorted = builtin_directory_files_and_attributes(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::True,
        ])
        .unwrap();
        let unsorted_items = list_to_vec(&unsorted).unwrap();
        let unsorted_names: Vec<String> = unsorted_items
            .iter()
            .map(|pair| {
                if let Value::Cons(cell) = pair {
                    cell.lock().unwrap().car.as_str().unwrap().to_string()
                } else {
                    panic!("expected cons pair");
                }
            })
            .collect();
        assert!(unsorted_names.contains(&".".to_string()));
        assert!(unsorted_names.contains(&"..".to_string()));

        let unsorted_limited = builtin_directory_files_and_attributes(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::True,
            Value::Nil,
            Value::Int(2),
        ])
        .unwrap();
        let unsorted_limited_names: Vec<String> = list_to_vec(&unsorted_limited)
            .unwrap()
            .iter()
            .map(|pair| {
                if let Value::Cons(cell) = pair {
                    cell.lock().unwrap().car.as_str().unwrap().to_string()
                } else {
                    panic!("expected cons pair");
                }
            })
            .collect();
        let tail = &unsorted_names[unsorted_names.len() - 2..];
        assert_eq!(unsorted_limited_names.as_slice(), tail);

        let sorted_limited = builtin_directory_files_and_attributes(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Int(2),
        ])
        .unwrap();
        let sorted_limited_names: Vec<String> = list_to_vec(&sorted_limited)
            .unwrap()
            .iter()
            .map(|pair| {
                if let Value::Cons(cell) = pair {
                    cell.lock().unwrap().car.as_str().unwrap().to_string()
                } else {
                    panic!("expected cons pair");
                }
            })
            .collect();
        let mut sorted_from_unsorted = unsorted_limited_names.clone();
        sorted_from_unsorted.sort();
        assert_eq!(sorted_limited_names, sorted_from_unsorted);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_directory_files_and_attributes_count_and_id_format() {
        let (dir, dir_str) = make_test_dir("dfa_types");
        create_file(&dir, "alpha.txt", "");

        let result = builtin_directory_files_and_attributes(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Int(-1),
        ]);
        assert!(result.is_err());

        let result = builtin_directory_files_and_attributes(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::string("x"),
        ]);
        assert!(result.is_err());

        let result = builtin_directory_files_and_attributes(vec![
            Value::string(&dir_str),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::True,
            Value::Int(1),
        ])
        .unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 1);
        let attrs = if let Value::Cons(cell) = &items[0] {
            cell.lock().unwrap().cdr.clone()
        } else {
            panic!("expected cons pair");
        };
        let attrs_items = list_to_vec(&attrs).unwrap();
        assert!(attrs_items[2].is_string());
        assert!(attrs_items[3].is_string());

        let _ = fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // file-name-completion
    // -----------------------------------------------------------------------

    #[test]
    fn test_file_name_completion_basic() {
        let (dir, dir_str) = make_test_dir("fnc_basic");
        create_file(&dir, "foobar.txt", "");
        create_file(&dir, "foobaz.txt", "");

        // "foo" should complete to "fooba" (longest common prefix).
        let result =
            builtin_file_name_completion(vec![Value::string("foo"), Value::string(&dir_str)])
                .unwrap();
        assert_eq!(result.as_str(), Some("fooba"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_name_completion_exact() {
        let (dir, dir_str) = make_test_dir("fnc_exact");
        create_file(&dir, "unique.txt", "");

        let result = builtin_file_name_completion(vec![
            Value::string("unique.txt"),
            Value::string(&dir_str),
        ])
        .unwrap();
        // Exact and unique match returns t.
        assert!(result.is_truthy());
        // In Emacs, exact unique match returns t.
        match result {
            Value::True => {} // correct
            _ => panic!("Expected t for exact match, got {:?}", result),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_name_completion_no_match() {
        let (dir, dir_str) = make_test_dir("fnc_none");
        create_file(&dir, "hello.txt", "");

        let result =
            builtin_file_name_completion(vec![Value::string("xyz"), Value::string(&dir_str)])
                .unwrap();
        assert!(result.is_nil());

        let _ = fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // file-name-all-completions
    // -----------------------------------------------------------------------

    #[test]
    fn test_file_name_all_completions() {
        let (dir, dir_str) = make_test_dir("fnac");
        create_file(&dir, "abc.txt", "");
        create_file(&dir, "abd.txt", "");
        create_file(&dir, "xyz.txt", "");

        let result =
            builtin_file_name_all_completions(vec![Value::string("ab"), Value::string(&dir_str)])
                .unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
        let names: Vec<&str> = items.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(names.contains(&"abc.txt"));
        assert!(names.contains(&"abd.txt"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_name_all_completions_empty() {
        let (dir, dir_str) = make_test_dir("fnac_empty");
        create_file(&dir, "hello.txt", "");

        let result =
            builtin_file_name_all_completions(vec![Value::string("zzz"), Value::string(&dir_str)])
                .unwrap();
        assert!(result.is_nil());

        let _ = fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // file-attributes
    // -----------------------------------------------------------------------

    #[test]
    fn test_file_attributes_regular_file() {
        let (dir, dir_str) = make_test_dir("fa_reg");
        let path = dir.join("test.txt");
        let path_str = path.to_string_lossy().to_string();
        create_file(&dir, "test.txt", "hello");

        let result = builtin_file_attributes(vec![Value::string(&path_str)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 12);

        // TYPE should be nil for regular file.
        assert!(items[0].is_nil());
        // SIZE should be 5.
        assert_eq!(items[7].as_int(), Some(5));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_attributes_directory() {
        let (dir, _dir_str) = make_test_dir("fa_dir");
        let sub = dir.join("subdir");
        fs::create_dir_all(&sub).unwrap();
        let sub_str = sub.to_string_lossy().to_string();

        let result = builtin_file_attributes(vec![Value::string(&sub_str)]).unwrap();
        let items = list_to_vec(&result).unwrap();

        // TYPE should be t for directory.
        assert!(matches!(items[0], Value::True));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_attributes_nonexistent() {
        let result =
            builtin_file_attributes(vec![Value::string("/nonexistent_file_xyz_99999")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn test_file_attributes_id_format_string() {
        let (dir, _) = make_test_dir("fa_idfmt");
        let path = dir.join("idtest.txt");
        let path_str = path.to_string_lossy().to_string();
        create_file(&dir, "idtest.txt", "x");

        let result =
            builtin_file_attributes(vec![Value::string(&path_str), Value::symbol("string")])
                .unwrap();
        let items = list_to_vec(&result).unwrap();
        // UID (index 2) should be a string.
        assert!(items[2].is_string());
        // GID (index 3) should be a string.
        assert!(items[3].is_string());

        let _ = fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // file-attributes-lessp
    // -----------------------------------------------------------------------

    #[test]
    fn test_file_attributes_lessp() {
        let f1 = Value::cons(Value::string("alpha.txt"), Value::Nil);
        let f2 = Value::cons(Value::string("beta.txt"), Value::Nil);

        let result = builtin_file_attributes_lessp(vec![f1.clone(), f2.clone()]).unwrap();
        assert!(result.is_truthy());

        let result = builtin_file_attributes_lessp(vec![f2, f1]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn test_file_attributes_lessp_equal() {
        let f1 = Value::cons(Value::string("same.txt"), Value::Nil);
        let f2 = Value::cons(Value::string("same.txt"), Value::Nil);

        let result = builtin_file_attributes_lessp(vec![f1, f2]).unwrap();
        assert!(result.is_nil()); // not less than
    }

    // -----------------------------------------------------------------------
    // system-users / system-groups
    // -----------------------------------------------------------------------

    #[test]
    fn test_system_users() {
        let result = builtin_system_users(vec![]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert!(!items.is_empty());
        assert!(items[0].is_string());
    }

    #[test]
    fn test_system_groups() {
        let result = builtin_system_groups(vec![]).unwrap();
        assert!(result.is_nil());
    }

    // -----------------------------------------------------------------------
    // Argument validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_directory_files_wrong_args() {
        // No args.
        assert!(builtin_directory_files(vec![]).is_err());
        // Too many args.
        assert!(builtin_directory_files(vec![
            Value::string("/tmp"),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil, // 6th arg
        ])
        .is_err());
    }

    #[test]
    fn test_file_attributes_wrong_args() {
        assert!(builtin_file_attributes(vec![]).is_err());
        assert!(
            builtin_file_attributes(vec![Value::string("/tmp"), Value::Nil, Value::Nil,]).is_err()
        );
    }

    #[test]
    fn test_system_users_wrong_args() {
        assert!(builtin_system_users(vec![Value::Nil]).is_err());
    }

    #[test]
    fn test_system_groups_wrong_args() {
        assert!(builtin_system_groups(vec![Value::Nil]).is_err());
    }

    // -----------------------------------------------------------------------
    // Time helper
    // -----------------------------------------------------------------------

    #[test]
    fn test_time_to_high_low() {
        let val = time_to_high_low(1234567890.0);
        let items = list_to_vec(&val).unwrap();
        assert_eq!(items.len(), 2);
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        assert_eq!((high << 16) | low, 1234567890);
    }

    #[cfg(unix)]
    #[test]
    fn test_format_mode_string() {
        // Regular file with 0o644.
        let dir = std::env::temp_dir().join("neovm_mode_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("modefile.txt");
        {
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(b"test").unwrap();
        }
        let meta = fs::symlink_metadata(&path).unwrap();
        let mode_str = format_mode_string(0o100644, &meta);
        assert_eq!(&mode_str[0..1], "-");
        assert_eq!(&mode_str[1..4], "rw-");
        assert_eq!(&mode_str[4..7], "r--");
        assert_eq!(&mode_str[7..10], "r--");

        let _ = fs::remove_dir_all(&dir);
    }
}
