//! Directory and file attribute builtins for the Elisp interpreter.
//!
//! Provides dired-related primitives:
//! - `directory-files`, `directory-files-and-attributes`
//! - `file-name-completion`, `file-name-all-completions`
//! - `file-attributes`, `file-attributes-lessp`
//! - `system-users`, `system-groups`

use super::error::{signal, EvalResult, Flow};
use super::eval::Evaluator;
use super::value::*;
use std::collections::{HashMap, VecDeque};
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

/// Convert UNIX seconds + nanoseconds to Emacs (HIGH LOW USEC PSEC) format.
fn time_to_emacs_tuple(secs: i64, nanos: i64) -> Value {
    let mut s = secs;
    let mut ns = nanos;
    if ns < 0 {
        let borrow = ((-ns) + 999_999_999) / 1_000_000_000;
        s -= borrow;
        ns += borrow * 1_000_000_000;
    } else if ns >= 1_000_000_000 {
        s += ns / 1_000_000_000;
        ns %= 1_000_000_000;
    }

    let high = s >> 16;
    let low = s & 0xFFFF;
    let usec = ns / 1_000;
    let psec = (ns % 1_000) * 1_000;

    Value::list(vec![
        Value::Int(high),
        Value::Int(low),
        Value::Int(usec),
        Value::Int(psec),
    ])
}

/// Get UNIX seconds + nanoseconds from SystemTime.
fn system_time_to_secs_nanos(time: std::time::SystemTime) -> Option<(i64, i64)> {
    let d = time.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some((d.as_secs() as i64, d.subsec_nanos() as i64))
}

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
            (
                Value::string(uid_to_name(uid).unwrap_or_else(|| uid.to_string())),
                Value::string(gid_to_name(gid).unwrap_or_else(|| gid.to_string())),
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
        time_to_emacs_tuple(sym_meta.atime(), sym_meta.atime_nsec())
    };
    #[cfg(not(unix))]
    let atime = meta
        .accessed()
        .ok()
        .and_then(system_time_to_secs_nanos)
        .map(|(secs, nanos)| time_to_emacs_tuple(secs, nanos))
        .unwrap_or(Value::Nil);

    // Modification time.
    #[cfg(unix)]
    let mtime = {
        use std::os::unix::fs::MetadataExt;
        time_to_emacs_tuple(meta.mtime(), meta.mtime_nsec())
    };
    #[cfg(not(unix))]
    let mtime = meta
        .modified()
        .ok()
        .and_then(system_time_to_secs_nanos)
        .map(|(secs, nanos)| time_to_emacs_tuple(secs, nanos))
        .unwrap_or(Value::Nil);

    // Status change time (ctime on Unix, creation time on other platforms).
    #[cfg(unix)]
    let ctime = {
        use std::os::unix::fs::MetadataExt;
        time_to_emacs_tuple(sym_meta.ctime(), sym_meta.ctime_nsec())
    };
    #[cfg(not(unix))]
    let ctime = meta
        .created()
        .ok()
        .and_then(system_time_to_secs_nanos)
        .map(|(secs, nanos)| time_to_emacs_tuple(secs, nanos))
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

    // GID-CHANGEP: Emacs commonly reports t on Unix filesystems.
    #[cfg(unix)]
    let gid_changep = Value::True;
    #[cfg(not(unix))]
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
/// This pure-dispatch variant supports symbol predicates.
pub(crate) fn builtin_file_name_completion(args: Vec<Value>) -> EvalResult {
    expect_range_args("file-name-completion", &args, 2, 3)?;

    let file = expect_string("file-name-completion", &args[0])?;
    let directory = expect_string("file-name-completion", &args[1])?;
    let predicate = args.get(2);
    if file.contains('/') {
        return Ok(Value::Nil);
    }
    let completions = collect_file_name_completions(&file, &directory)?;
    let completions = filter_completions_by_symbol_predicate(predicate, &directory, completions)?;
    Ok(resolve_file_name_completion(&file, completions))
}

/// Evaluator-backed variant of `file-name-completion`.
/// This supports arbitrary callable predicates and matches Emacs behavior of
/// binding `default-directory` to DIRECTORY while predicate is invoked.
pub(crate) fn builtin_file_name_completion_eval(
    eval: &mut Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_range_args("file-name-completion", &args, 2, 3)?;

    let file = expect_string("file-name-completion", &args[0])?;
    let directory = expect_string("file-name-completion", &args[1])?;
    let predicate = args.get(2);
    if file.contains('/') {
        return Ok(Value::Nil);
    }
    let completions = collect_file_name_completions(&file, &directory)?;
    let completions =
        filter_completions_by_eval_predicate(eval, predicate, &directory, completions)?;
    Ok(resolve_file_name_completion(&file, completions))
}

/// (file-name-all-completions FILE DIRECTORY)
///
/// Return a list of all completions of FILE in DIRECTORY.
/// Each entry that is a directory has a trailing '/'.
pub(crate) fn builtin_file_name_all_completions(args: Vec<Value>) -> EvalResult {
    expect_range_args("file-name-all-completions", &args, 2, 2)?;

    let file = expect_string("file-name-all-completions", &args[0])?;
    let directory = expect_string("file-name-all-completions", &args[1])?;
    if file.contains('/') {
        return Ok(Value::Nil);
    }
    let completions = collect_file_name_completions(&file, &directory)?;
    Ok(Value::list(
        completions.into_iter().map(Value::string).collect(),
    ))
}

fn collect_file_name_completions(file: &str, directory: &str) -> Result<Vec<String>, Flow> {
    let names = read_directory_names(directory)?;
    let mut completions = VecDeque::new();

    for name in names {
        if !name.starts_with(file) {
            continue;
        }

        let full_path = std::path::Path::new(directory).join(&name);
        if full_path.is_dir() {
            completions.push_front(format!("{}/", name));
        } else {
            completions.push_front(name);
        }
    }

    Ok(completions.into_iter().collect())
}

fn resolve_file_name_completion(file: &str, completions: Vec<String>) -> Value {
    if completions.is_empty() {
        return Value::Nil;
    }

    let filtered = filter_completion_candidates(file, completions);
    if filtered.is_empty() {
        return Value::string(file);
    }

    // If there is exactly one completion and it matches FILE exactly, return t.
    // For directory candidates ending in '/', Emacs returns the completion
    // string when FILE lacks the trailing slash (e.g. ".." -> "../").
    if filtered.len() == 1 {
        let comp = &filtered[0];
        if comp == file {
            return Value::True;
        }
        return Value::string(comp.clone());
    }

    // Find the longest common prefix among completions.
    let mut prefix = filtered[0].clone();
    for comp in &filtered[1..] {
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
    Value::string(prefix)
}

fn filter_completion_candidates(file: &str, completions: Vec<String>) -> Vec<String> {
    completions
        .into_iter()
        .filter(|c| c != "./")
        .filter(|c| file.starts_with("..") || c != "../")
        .collect()
}

fn filter_completions_by_symbol_predicate(
    predicate: Option<&Value>,
    directory: &str,
    completions: Vec<String>,
) -> Result<Vec<String>, Flow> {
    let Some(predicate) = predicate else {
        return Ok(completions);
    };
    if predicate.is_nil() {
        return Ok(completions);
    }
    let Some(symbol) = predicate_callable_name(predicate) else {
        // Pure dispatch cannot evaluate lambda/object predicates.
        return Ok(completions);
    };

    let mut filtered = Vec::new();
    for candidate in completions {
        if symbol_predicate_matches_candidate(symbol, directory, &candidate)? {
            filtered.push(candidate);
        }
    }
    Ok(filtered)
}

fn symbol_predicate_matches_candidate(
    symbol: &str,
    directory: &str,
    candidate: &str,
) -> Result<bool, Flow> {
    if let Some(result) =
        super::builtins::dispatch_builtin_pure(symbol, vec![Value::string(candidate)])
    {
        let result = result?;
        if result.is_truthy() || !is_builtin_path_predicate(symbol) {
            return Ok(result.is_truthy());
        }

        let absolute = std::path::Path::new(directory).join(candidate);
        let absolute = absolute.to_string_lossy().into_owned();
        if let Some(result) =
            super::builtins::dispatch_builtin_pure(symbol, vec![Value::string(absolute)])
        {
            return Ok(result?.is_truthy());
        }
        return Ok(false);
    }

    // Fallback for pure mode: try absolute path to make path predicates useful
    // without evaluator-backed dynamic binding.
    let absolute = std::path::Path::new(directory).join(candidate);
    let absolute = absolute.to_string_lossy().into_owned();
    if let Some(result) =
        super::builtins::dispatch_builtin_pure(symbol, vec![Value::string(absolute)])
    {
        return Ok(result?.is_truthy());
    }

    // Preserve current behavior for unknown/non-callable predicates in pure mode.
    Ok(true)
}

fn filter_completions_by_eval_predicate(
    eval: &mut Evaluator,
    predicate: Option<&Value>,
    directory: &str,
    completions: Vec<String>,
) -> Result<Vec<String>, Flow> {
    let Some(predicate) = predicate else {
        return Ok(completions);
    };
    if predicate.is_nil() {
        return Ok(completions);
    }

    let mut filtered = Vec::new();
    for candidate in completions {
        let predicate_arg = predicate_argument_for_eval(eval, predicate, directory, &candidate);
        let keep = with_default_directory_binding(eval, directory, |eval| {
            eval.apply(predicate.clone(), vec![predicate_arg])
        })?
        .is_truthy();
        if keep {
            filtered.push(candidate);
        }
    }
    Ok(filtered)
}

fn with_default_directory_binding<T>(
    eval: &mut Evaluator,
    directory: &str,
    f: impl FnOnce(&mut Evaluator) -> Result<T, Flow>,
) -> Result<T, Flow> {
    let mut frame = HashMap::new();
    frame.insert("default-directory".to_string(), Value::string(directory));
    eval.dynamic.push(frame);
    let result = f(eval);
    eval.dynamic.pop();
    result
}

fn predicate_argument_for_eval(
    eval: &Evaluator,
    predicate: &Value,
    directory: &str,
    candidate: &str,
) -> Value {
    let Some(symbol) = predicate_callable_name(predicate) else {
        return Value::string(candidate);
    };

    // NeoVM path predicates currently resolve relative paths against process CWD.
    // For builtin path predicates, pass an absolute candidate to match Emacs behavior
    // where relative paths are interpreted under bound `default-directory`.
    if eval.obarray.symbol_function(symbol).is_none() && is_builtin_path_predicate(symbol) {
        let absolute = std::path::Path::new(directory).join(candidate);
        return Value::string(absolute.to_string_lossy().into_owned());
    }

    Value::string(candidate)
}

fn is_builtin_path_predicate(name: &str) -> bool {
    matches!(
        name,
        "file-directory-p"
            | "file-exists-p"
            | "file-readable-p"
            | "file-writable-p"
            | "file-regular-p"
            | "file-symlink-p"
            | "file-executable-p"
    )
}

fn predicate_callable_name(predicate: &Value) -> Option<&str> {
    match predicate {
        Value::Symbol(name) | Value::Subr(name) => Some(name.as_str()),
        _ => None,
    }
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
    let id_format_string = args.get(1).is_some_and(|v| v.is_truthy());

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

    #[test]
    fn test_file_name_completion_dot_and_slash_behavior() {
        let (dir, dir_str) = make_test_dir("fnc_dot_slash");
        create_file(&dir, ".hidden", "");
        fs::create_dir(dir.join("subdir")).unwrap();

        let dot = builtin_file_name_completion(vec![Value::string("."), Value::string(&dir_str)])
            .unwrap();
        assert_eq!(dot.as_str(), Some(".hidden"));

        let dotdot =
            builtin_file_name_completion(vec![Value::string(".."), Value::string(&dir_str)])
                .unwrap();
        assert_eq!(dotdot.as_str(), Some("../"));

        let slash =
            builtin_file_name_completion(vec![Value::string("./"), Value::string(&dir_str)])
                .unwrap();
        assert!(slash.is_nil());

        let subdir_prefix =
            builtin_file_name_completion(vec![Value::string("sub"), Value::string(&dir_str)])
                .unwrap();
        assert_eq!(subdir_prefix.as_str(), Some("subdir/"));

        let subdir_with_slash =
            builtin_file_name_completion(vec![Value::string("subdir/"), Value::string(&dir_str)])
                .unwrap();
        assert!(subdir_with_slash.is_nil());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_file_name_completion_predicate_with_eval() {
        let (dir, dir_str) = make_test_dir("fnc_predicate");
        create_file(&dir, "alpha.txt", "");
        fs::create_dir(dir.join("subdir")).unwrap();

        let mut eval = Evaluator::new();
        eval.obarray
            .set_symbol_value("default-directory", Value::string("/tmp/"));

        let dirs_only_none = builtin_file_name_completion_eval(
            &mut eval,
            vec![
                Value::string("a"),
                Value::string(&dir_str),
                Value::symbol("file-directory-p"),
            ],
        )
        .unwrap();
        assert!(dirs_only_none.is_nil());

        let dirs_only_match = builtin_file_name_completion_eval(
            &mut eval,
            vec![
                Value::string("s"),
                Value::string(&dir_str),
                Value::symbol("file-directory-p"),
            ],
        )
        .unwrap();
        assert_eq!(dirs_only_match.as_str(), Some("subdir/"));

        let bad_pred = builtin_file_name_completion_eval(
            &mut eval,
            vec![Value::string("a"), Value::string(&dir_str), Value::Int(123)],
        );
        assert!(bad_pred.is_err());

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

    #[test]
    fn test_file_name_all_completions_special_entries() {
        let (dir, dir_str) = make_test_dir("fnac_special");
        create_file(&dir, ".hidden", "");
        fs::create_dir(dir.join("subdir")).unwrap();

        let dot =
            builtin_file_name_all_completions(vec![Value::string("."), Value::string(&dir_str)])
                .unwrap();
        let dot_items = list_to_vec(&dot).unwrap();
        let dot_names: Vec<&str> = dot_items.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(dot_names.contains(&"./"));
        assert!(dot_names.contains(&"../"));
        assert!(dot_names.contains(&".hidden"));

        let dotdot =
            builtin_file_name_all_completions(vec![Value::string(".."), Value::string(&dir_str)])
                .unwrap();
        let dotdot_items = list_to_vec(&dotdot).unwrap();
        assert_eq!(dotdot_items.len(), 1);
        assert_eq!(dotdot_items[0].as_str(), Some("../"));

        let slash =
            builtin_file_name_all_completions(vec![Value::string("./"), Value::string(&dir_str)])
                .unwrap();
        assert!(slash.is_nil());

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
    fn test_file_attributes_time_tuple_shape_and_gid_changep() {
        let (dir, _) = make_test_dir("fa_time");
        let path = dir.join("time.txt");
        let path_str = path.to_string_lossy().to_string();
        create_file(&dir, "time.txt", "hello");

        let result = builtin_file_attributes(vec![Value::string(&path_str)]).unwrap();
        let items = list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 12);

        for idx in [4usize, 5usize, 6usize] {
            let tm = list_to_vec(&items[idx]).expect("time tuple must be a list");
            assert_eq!(tm.len(), 4);
            assert!(tm.iter().all(|v| matches!(v, Value::Int(_))));
        }

        // Emacs commonly reports non-nil here on Unix.
        #[cfg(unix)]
        assert!(items[9].is_truthy());

        let _ = fs::remove_dir_all(&dir);
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
    fn test_time_to_emacs_tuple() {
        let val = time_to_emacs_tuple(1_234_567_890, 123_456_789);
        let items = list_to_vec(&val).unwrap();
        assert_eq!(items.len(), 4);
        let high = items[0].as_int().unwrap();
        let low = items[1].as_int().unwrap();
        assert_eq!((high << 16) | low, 1_234_567_890);
        assert_eq!(items[2].as_int(), Some(123_456));
        assert_eq!(items[3].as_int(), Some(789_000));
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
