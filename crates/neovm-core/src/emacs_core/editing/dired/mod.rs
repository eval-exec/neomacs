//! Directory and file attribute builtins for the Elisp interpreter.
//!
//! Provides dired-related primitives:
//! - `directory-files-and-attributes`
//! - `file-name-completion`, `file-name-all-completions`
//! - `file-attributes`, `file-attributes-lessp`
//! - `system-users`, `system-groups`

use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::intern::{intern, resolve_sym};
use super::timefns::{LispTimeOutput, make_lisp_time};
use super::value::*;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::expect_args_range;
use crate::heap_types::LispString;
use std::collections::VecDeque;
#[cfg(unix)]
use std::ffi::CStr;
use std::fs;
use std::io::ErrorKind;

mod file_identity;

#[derive(Clone, Copy, Debug, Eq, PartialEq, strum::EnumString, strum::IntoStaticStr)]
enum FileIdFormat {
    #[strum(serialize = "integer")]
    Integer,
    #[strum(serialize = "string")]
    String,
}

impl FileIdFormat {
    fn from_id_format_arg(arg: Option<&Value>) -> Self {
        let Some(value) = arg else {
            return Self::Integer;
        };
        if value.is_nil() {
            return Self::Integer;
        }
        value
            .as_symbol_name()
            .and_then(|name| name.parse::<Self>().ok())
            .unwrap_or(Self::String)
    }

    fn ids_as_strings(self) -> bool {
        matches!(self, Self::String)
    }
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_lisp_string(_name: &str, value: &Value) -> Result<LispString, Flow> {
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

/// Build a file-name `LispString` from raw bytes, preserving raw file-name
/// bytes exactly. ASCII-only names stay unibyte; anything else is treated as
/// Emacs-internal multibyte so eight-bit file-name bytes round-trip faithfully.
fn file_name_lisp_from_bytes(bytes: Vec<u8>) -> LispString {
    if bytes.is_ascii() {
        LispString::from_unibyte(bytes)
    } else {
        LispString::from_emacs_bytes(bytes)
    }
}

/// Wrap a file-name `LispString` as a `Value` without any storage round-trip.
fn file_name_value(name: LispString) -> Value {
    Value::heap_string(name)
}

/// Ensure a directory file-name `LispString` ends with '/', preserving raw
/// file-name bytes (GNU `file-name-as-directory`).
fn ensure_trailing_slash_lisp(dir: &LispString) -> LispString {
    let bytes = dir.as_bytes();
    if bytes.ends_with(b"/") {
        return dir.clone();
    }
    let mut out = bytes.to_vec();
    out.push(b'/');
    file_name_lisp_from_bytes(out)
}

/// Test-only convenience: ensure a directory path `&str` ends with '/'.
#[cfg(test)]
fn ensure_trailing_slash(dir: &str) -> String {
    if dir.ends_with('/') {
        dir.to_string()
    } else {
        format!("{}/", dir)
    }
}

/// Concatenate a directory file-name (already slash-terminated) with an entry
/// name, preserving raw file-name bytes.
fn concat_dir_entry_lisp(dir_with_slash: &LispString, name: &LispString) -> LispString {
    let mut out = Vec::with_capacity(dir_with_slash.as_bytes().len() + name.as_bytes().len());
    out.extend_from_slice(dir_with_slash.as_bytes());
    out.extend_from_slice(name.as_bytes());
    file_name_lisp_from_bytes(out)
}

fn file_error_symbol(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NotFound => "file-missing",
        ErrorKind::AlreadyExists => "file-already-exists",
        ErrorKind::PermissionDenied => "permission-denied",
        _ => "file-error",
    }
}

fn signal_file_io(action: &str, path: &str, err: std::io::Error) -> Flow {
    signal(
        file_error_symbol(err.kind()),
        vec![
            Value::string(action),
            Value::string(err.to_string()),
            Value::string(path),
        ],
    )
}

/// Read directory entry names byte-faithfully.  Public directory primitives
/// decode these host bytes using GNU's DECODE_FILE rules before inspecting
/// them. Returns the entries as file-name `LispString`s plus "." and "..".
fn read_directory_names(dir: &LispString) -> Result<Vec<LispString>, Flow> {
    let path = super::fileio::lisp_file_name_to_path_buf(dir);
    let entries = fs::read_dir(&path).map_err(|e| {
        signal_file_io(
            "Opening directory",
            &super::emacs_char::to_utf8_lossy(dir.as_bytes()),
            e,
        )
    })?;
    let mut names = vec![
        LispString::from_unibyte(b".".to_vec()),
        LispString::from_unibyte(b"..".to_vec()),
    ];
    for entry in entries {
        let entry = entry.map_err(|e| {
            signal_file_io(
                "Reading directory entry",
                &super::emacs_char::to_utf8_lossy(dir.as_bytes()),
                e,
            )
        })?;
        names.push(super::fileio::path_to_lisp_file_name(std::path::Path::new(
            &entry.file_name(),
        )));
    }
    Ok(names)
}

fn parse_wholenump_count(arg: Option<&Value>) -> Result<Option<usize>, Flow> {
    match arg {
        Some(v) if v.is_fixnum() && v.as_fixnum().unwrap() >= 0 => {
            Ok(Some(v.as_fixnum().unwrap() as usize))
        }
        Some(v) if v.is_fixnum() => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), *v],
        )),
        Some(v) if v.is_truthy() => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("wholenump"), *v],
        )),
        _ => Ok(None),
    }
}

/// Get UNIX seconds + nanoseconds from SystemTime.
#[cfg(not(unix))]
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
/// Times use the representation selected by `time_output`.
/// If ID-FORMAT is non-nil and not 'integer, UID/GID are returned as strings.
fn build_file_attributes(
    filename: &LispString,
    id_format: FileIdFormat,
    time_output: LispTimeOutput,
) -> Option<Value> {
    let path = super::fileio::lisp_file_name_to_path_buf(filename);

    // Use symlink_metadata first to detect symlinks.
    let sym_meta = fs::symlink_metadata(&path).ok()?;

    // Determine file type.
    let file_type = if sym_meta.file_type().is_symlink() {
        // Read the symlink target, preserving raw file-name bytes.
        match fs::read_link(&path) {
            Ok(target) => Value::heap_string(super::fileio::path_to_lisp_file_name(&target)),
            Err(_) => Value::string(""),
        }
    } else if sym_meta.is_dir() {
        Value::T
    } else {
        Value::NIL
    };

    // GNU (src/dired.c `file_attributes`) performs exactly ONE
    // `emacs_fstatat (..., AT_SYMLINK_NOFOLLOW)` — an lstat — and derives every
    // field from that single result, including the size (`s.st_size`).  For a
    // symlink that means the size is the byte length of the link target string,
    // NOT the resolved target's size.  So never follow the link here: use the
    // lstat metadata (`sym_meta`) for all fields.
    let meta = sym_meta.clone();

    // Number of hard links.
    #[cfg(unix)]
    let nlinks = {
        use std::os::unix::fs::MetadataExt;
        Value::fixnum(sym_meta.nlink() as i64)
    };
    #[cfg(not(unix))]
    let nlinks = Value::fixnum(1);

    // UID / GID. GNU requests accurate Windows security-descriptor ownership
    // specifically for file-attributes (src/dired.c:1070-1080); the platform
    // boundary supplies that without exposing raw SID pointers here.
    let ownership = file_identity::for_path(&path, &sym_meta);
    let (uid_val, gid_val) = if id_format.ids_as_strings() {
        (
            Value::string(
                ownership
                    .user
                    .name
                    .unwrap_or_else(|| ownership.user.id.to_string()),
            ),
            Value::string(
                ownership
                    .group
                    .name
                    .unwrap_or_else(|| ownership.group.id.to_string()),
            ),
        )
    } else {
        (
            Value::fixnum(ownership.user.id),
            Value::fixnum(ownership.group.id),
        )
    };

    // Access time.
    #[cfg(unix)]
    let atime = {
        use std::os::unix::fs::MetadataExt;
        make_lisp_time(sym_meta.atime(), sym_meta.atime_nsec(), time_output)
    };
    #[cfg(not(unix))]
    let atime = meta
        .accessed()
        .ok()
        .and_then(system_time_to_secs_nanos)
        .map(|(secs, nanos)| make_lisp_time(secs, nanos, time_output))
        .unwrap_or(Value::NIL);

    // Modification time.
    #[cfg(unix)]
    let mtime = {
        use std::os::unix::fs::MetadataExt;
        make_lisp_time(meta.mtime(), meta.mtime_nsec(), time_output)
    };
    #[cfg(not(unix))]
    let mtime = meta
        .modified()
        .ok()
        .and_then(system_time_to_secs_nanos)
        .map(|(secs, nanos)| make_lisp_time(secs, nanos, time_output))
        .unwrap_or(Value::NIL);

    // Status change time (ctime on Unix, creation time on other platforms).
    #[cfg(unix)]
    let ctime = {
        use std::os::unix::fs::MetadataExt;
        make_lisp_time(sym_meta.ctime(), sym_meta.ctime_nsec(), time_output)
    };
    #[cfg(not(unix))]
    let ctime = meta
        .created()
        .ok()
        .and_then(system_time_to_secs_nanos)
        .map(|(secs, nanos)| make_lisp_time(secs, nanos, time_output))
        .unwrap_or(Value::NIL);

    // Size.
    let size = Value::fixnum(meta.len() as i64);

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
    let gid_changep = Value::T;
    #[cfg(not(unix))]
    let gid_changep = Value::NIL;

    // Inode.
    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        Value::fixnum(sym_meta.ino() as i64)
    };
    #[cfg(not(unix))]
    let inode = Value::fixnum(0);

    // Device.
    #[cfg(unix)]
    let device = {
        use std::os::unix::fs::MetadataExt;
        Value::fixnum(sym_meta.dev() as i64)
    };
    #[cfg(not(unix))]
    let device = Value::fixnum(0);

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
        if mode & 0o100 != 0 { 's' } else { 'S' }
    } else if mode & 0o100 != 0 {
        'x'
    } else {
        '-'
    });

    // Group permissions.
    s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o2000 != 0 {
        if mode & 0o010 != 0 { 's' } else { 'S' }
    } else if mode & 0o010 != 0 {
        'x'
    } else {
        '-'
    });

    // Other permissions.
    s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    s.push(if mode & 0o1000 != 0 {
        if mode & 0o001 != 0 { 't' } else { 'T' }
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

/// Context-backed variant of `directory-files-and-attributes`.
/// Expands DIRECTORY through Lisp state before handler dispatch or local I/O.
pub(crate) fn builtin_directory_files_and_attributes(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("directory-files-and-attributes", &args, 1, 6)?;
    expect_lisp_string("directory-files-and-attributes", &args[0])?;
    let dir = match super::fileio::expand_file_operation(
        eval,
        "directory-files-and-attributes",
        &args,
        6,
    )? {
        super::fileio::ExpandedFileOperation::Handled(result) => return Ok(result),
        super::fileio::ExpandedFileOperation::Local { expanded_filename } => {
            expect_lisp_string("directory-files-and-attributes", &expanded_filename)?
        }
    };
    let time_output = LispTimeOutput::from_context(eval)?;
    let syntax = super::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
    directory_files_and_attributes_with_dir(
        &args,
        &dir,
        time_output,
        syntax,
        &eval.obarray,
        &eval.buffers,
        |bytes| super::fileio::decode_file_name_lisp(eval, bytes),
    )
}

#[allow(clippy::too_many_arguments)] // match-time state stays explicit at the GNU-regexp boundary
fn directory_files_and_attributes_with_dir(
    args: &[Value],
    dir: &LispString,
    time_output: LispTimeOutput,
    syntax: super::builtins::search::FastStringMatchSyntax,
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    decode_name: impl Fn(&[u8]) -> LispString,
) -> EvalResult {
    let full_name = args.get(1).is_some_and(|v| v.is_truthy());
    let match_regexp = match args.get(2) {
        Some(v) if v.is_truthy() => Some(expect_lisp_string("directory-files-and-attributes", v)?),
        _ => None,
    };
    let nosort = args.get(3).is_some_and(|v| v.is_truthy());
    // GNU Emacs: return string names unless ID-FORMAT is nil or 'integer.
    let id_format = FileIdFormat::from_id_format_arg(args.get(4));
    let count = parse_wholenump_count(args.get(5))?;
    if count == Some(0) {
        return Ok(Value::NIL);
    }

    let names = read_directory_names(dir)?;

    let dir_with_slash = ensure_trailing_slash_lisp(dir);
    // (DISPLAY-NAME, FULL-PATH) — both kept byte-faithfully as LispStrings.
    let mut items: VecDeque<(LispString, LispString)> = VecDeque::new();
    let mut remaining = count.unwrap_or(usize::MAX);
    for raw_name in names {
        let name = decode_name(raw_name.as_bytes());
        if let Some(pattern) = match_regexp.as_ref() {
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
                    signal(
                        LispCondition::InvalidRegexp,
                        vec![Value::string(format!(
                            "Invalid regexp \"{}\": {}",
                            super::emacs_char::to_utf8_lossy(pattern.as_bytes()),
                            msg
                        ))],
                    )
                })?;
            if matched.is_none() {
                continue;
            }
        }

        let full_path = concat_dir_entry_lisp(&dir_with_slash, &name);
        let display_name = if full_name { full_path.clone() } else { name };
        items.push_front((display_name, full_path));

        if remaining != usize::MAX {
            remaining -= 1;
            if remaining == 0 {
                break;
            }
        }
    }

    let mut items: Vec<(LispString, LispString)> = items.into_iter().collect();
    // Sort unless NOSORT is non-nil. Compare byte-faithfully so eight-bit
    // file names order exactly as GNU's string_lessp does.
    if !nosort {
        items.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    }

    // Build result list of (NAME . ATTRIBUTES) cons cells.
    let result: Vec<Value> = items
        .into_iter()
        .map(|(display_name, full_path)| {
            let attrs =
                build_file_attributes(&full_path, id_format, time_output).unwrap_or(Value::NIL);
            Value::cons(file_name_value(display_name), attrs)
        })
        .collect();

    Ok(Value::list(result))
}

/// Context-backed variant of `file-name-completion`.
/// This supports arbitrary callable predicates and matches Emacs behavior of
/// binding `default-directory` to DIRECTORY while predicate is invoked.
pub(crate) fn builtin_file_name_completion(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("file-name-completion", &args, 2, 3)?;
    let file = expect_lisp_string("file-name-completion", &args[0])?;
    let directory_arg = expect_lisp_string("file-name-completion", &args[1])?;
    let directory = expand_file_completion_directory(eval, directory_arg)?;
    if let Some(result) = dispatch_file_completion_handler(
        eval,
        "file-name-completion",
        &file,
        &directory,
        &[
            args[0],
            Value::heap_string(directory.clone()),
            args.get(2).copied().unwrap_or(Value::NIL),
        ],
    )? {
        return Ok(result);
    }

    let args = vec![
        args[0],
        Value::heap_string(directory),
        args.get(2).copied().unwrap_or(Value::NIL),
    ];
    let syntax = super::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
    let plan = prepare_file_name_completion_in_state(
        &eval.obarray,
        &[],
        &eval.buffers,
        syntax,
        &args,
        |bytes| super::fileio::decode_file_name_lisp(eval, bytes),
    )?;
    let predicate = args.get(2);
    finish_file_name_completion_with_eval_predicate(
        eval,
        predicate,
        plan.directory,
        plan.file,
        plan.completions,
        plan.ignore_case,
    )
}

/// Context-backed variant of `file-name-all-completions`.
/// Resolves relative DIRECTORY against dynamic/default `default-directory`.
pub(crate) fn builtin_file_name_all_completions(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("file-name-all-completions", &args, 2, 2)?;

    let file = expect_lisp_string("file-name-all-completions", &args[0])?;
    let directory_arg = expect_lisp_string("file-name-all-completions", &args[1])?;
    let directory = expand_file_completion_directory(eval, directory_arg)?;
    if let Some(result) = dispatch_file_completion_handler(
        eval,
        "file-name-all-completions",
        &file,
        &directory,
        &[args[0], Value::heap_string(directory.clone())],
    )? {
        return Ok(result);
    }

    if file.as_bytes().contains(&b'/') {
        return Ok(Value::NIL);
    }
    let ignore_case = get_completion_ignore_case(&eval.obarray, &eval.buffers);
    let regexps = super::minibuffer::completion_regexp_lisp_list_from_obarray(&eval.obarray);
    // GNU Emacs: file-name-all-completions does NOT filter by
    // completion-ignored-extensions (the "all_flag" path).
    let syntax = super::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
    let completions = filter_by_completion_regexps(
        syntax,
        &eval.obarray,
        &eval.buffers,
        collect_file_name_completions(&file, &directory, ignore_case, true, |bytes| {
            super::fileio::decode_file_name_lisp(eval, bytes)
        })?,
        &regexps,
        ignore_case,
    )?;
    Ok(Value::list(
        completions.into_iter().map(file_name_value).collect(),
    ))
}

fn expand_file_completion_directory(
    eval: &mut Context,
    directory: LispString,
) -> Result<LispString, Flow> {
    let expanded = super::fileio::builtin_expand_file_name(
        eval,
        vec![Value::heap_string(directory), Value::NIL],
    )?;
    expect_lisp_string("expand-file-name", &expanded)
}

fn dispatch_file_completion_handler(
    eval: &mut Context,
    operation_name: &str,
    file: &LispString,
    directory: &LispString,
    call_args: &[Value],
) -> Result<Option<Value>, Flow> {
    let operation = Value::symbol(operation_name);

    // GNU dired.c first consults the expanded DIRECTORY, then FILE.
    let handler = super::fileio::find_file_name_handler_lisp_for_eval(eval, directory, operation);
    if !handler.is_nil() {
        let mut args = Vec::with_capacity(call_args.len() + 1);
        args.push(operation);
        args.extend_from_slice(call_args);
        return Ok(Some(eval.funcall_general(handler, args)?));
    }

    let handler = super::fileio::find_file_name_handler_lisp_for_eval(eval, file, operation);
    if !handler.is_nil() {
        let mut args = Vec::with_capacity(call_args.len() + 1);
        args.push(operation);
        args.extend_from_slice(call_args);
        return Ok(Some(eval.funcall_general(handler, args)?));
    }

    Ok(None)
}

/// Byte-faithful prefix test. File-name syntax is ASCII, so ASCII case-folding
/// matches GNU's completion-ignore-case behavior; eight-bit bytes never fold and
/// must compare exactly.
fn byte_prefix_matches(name: &[u8], file: &[u8], ignore_case: bool) -> bool {
    if name.len() < file.len() {
        return false;
    }
    let head = &name[..file.len()];
    if ignore_case {
        head.eq_ignore_ascii_case(file)
    } else {
        head == file
    }
}

fn collect_file_name_completions(
    file: &LispString,
    directory: &LispString,
    ignore_case: bool,
    reverse: bool,
    decode_name: impl Fn(&[u8]) -> LispString,
) -> Result<Vec<LispString>, Flow> {
    let names = read_directory_names(directory)?;
    let dir_path = super::fileio::lisp_file_name_to_path_buf(directory);
    let mut completions = Vec::new();

    for raw_name in names {
        if !byte_prefix_matches(raw_name.as_bytes(), file.as_bytes(), ignore_case) {
            continue;
        }

        let entry_path = dir_path.join(super::fileio::lisp_file_name_to_path_buf(&raw_name));
        let name = decode_name(raw_name.as_bytes());
        let completion = if entry_path.is_dir() {
            ensure_trailing_slash_lisp(&name)
        } else {
            name
        };
        completions.push(completion);
    }

    if reverse {
        completions.reverse();
    }

    Ok(completions)
}

fn filter_by_completion_regexps(
    syntax: super::builtins::search::FastStringMatchSyntax,
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    completions: Vec<LispString>,
    regexps: &[LispString],
    ignore_case: bool,
) -> Result<Vec<LispString>, Flow> {
    if regexps.is_empty() {
        return Ok(completions);
    }

    let mut filtered = Vec::with_capacity(completions.len());
    for completion in completions {
        // Match the name without any trailing '/' added for directories.
        let bytes = completion.as_bytes();
        let candidate = if bytes.ends_with(b"/") {
            file_name_lisp_from_bytes(bytes[..bytes.len() - 1].to_vec())
        } else {
            completion.clone()
        };
        if super::minibuffer::lisp_string_matches_completion_regexps(
            syntax,
            obarray,
            buffers,
            &candidate,
            regexps,
            ignore_case,
        )? {
            filtered.push(completion);
        }
    }
    Ok(filtered)
}

/// Extract the list of ignored extensions from the `completion-ignored-extensions` variable.
fn get_ignored_extensions(obarray: &super::symbol::Obarray) -> Vec<LispString> {
    let Some(val) = obarray.symbol_value("completion-ignored-extensions") else {
        return Vec::new();
    };
    let val = *val;
    let Some(items) = list_to_vec(&val) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|v| v.as_lisp_string().cloned())
        .collect()
}

/// GNU's `completion_ignore_case` -- the `DEFVAR_BOOL` cell
/// (`src/minibuf.c:2585`) that `src/dired.c` dereferences as a bare C `bool`
/// throughout `file_name_completion` (`:592`, `:599`, `:633`, `:886`).
///
/// The swap-in (`src/data.c:1573-1603`) keeps that cell equal to the current
/// buffer's binding, and six `.el` files in this tree localise the name on
/// purpose, so the read names the buffer. Ledger 196.
fn get_completion_ignore_case(
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
) -> bool {
    obarray
        .value_in_buffer(buffers.current_buffer(), "completion-ignore-case")
        .is_some_and(|v| v.is_truthy())
}

/// Apply `completion-ignored-extensions` filtering to a set of completions.
///
/// This follows GNU Emacs semantics:
/// - If a file name (not exact match with FILE) ends with an ignored extension,
///   it can be excluded.
/// - If a directory name ends with an ignored extension that itself ends in '/',
///   it can be excluded.
/// - "." and ".." directories are always excludable.
/// - If there is at least one non-excludable match, all excludable matches are
///   dropped. If ALL matches are excludable, they are all kept (the "includeall"
///   fallback).
///
/// Byte-faithful suffix test. Extension syntax is ASCII, so ASCII case-folding
/// matches GNU's completion-ignore-case behavior; eight-bit bytes never fold.
fn byte_suffix_matches(base: &[u8], ext: &[u8], ignore_case: bool) -> bool {
    if base.len() < ext.len() {
        return false;
    }
    let tail = &base[base.len() - ext.len()..];
    if ignore_case {
        tail.eq_ignore_ascii_case(ext)
    } else {
        tail == ext
    }
}

fn filter_by_ignored_extensions(
    file: &LispString,
    completions: Vec<LispString>,
    ignored_extensions: &[LispString],
    ignore_case: bool,
) -> Vec<LispString> {
    if completions.is_empty() {
        return completions;
    }

    let file_len = file.as_bytes().len();

    // Classify each completion as excludable or not.
    let mut classified: Vec<(LispString, bool)> = Vec::with_capacity(completions.len());
    for comp in completions {
        let comp_bytes = comp.as_bytes();
        let is_dir = comp_bytes.ends_with(b"/");
        // The base name (without trailing '/' for directories)
        let base = if is_dir {
            &comp_bytes[..comp_bytes.len() - 1]
        } else {
            comp_bytes
        };

        let mut can_exclude = false;

        // "." and ".." are always excludable
        if base == b"." || base == b".." {
            can_exclude = true;
        } else if base.len() > file_len {
            // Only check ignored-extensions when the name is longer than FILE
            // (i.e., not an exact match).
            for ext in ignored_extensions {
                let ext_bytes = ext.as_bytes();
                if is_dir {
                    // For directories, only match extensions that end in '/'.
                    if !ext_bytes.ends_with(b"/") {
                        continue;
                    }
                    let ext_base = &ext_bytes[..ext_bytes.len() - 1]; // strip trailing '/'
                    if ext_base.is_empty() {
                        continue;
                    }
                    if byte_suffix_matches(base, ext_base, ignore_case) {
                        can_exclude = true;
                        break;
                    }
                } else {
                    // For files, match extensions (which should not end in '/').
                    if ext_bytes.ends_with(b"/") {
                        continue;
                    }
                    if byte_suffix_matches(base, ext_bytes, ignore_case) {
                        can_exclude = true;
                        break;
                    }
                }
            }
        }

        classified.push((comp, can_exclude));
    }

    // GNU Emacs "includeall" logic:
    // If there's at least one non-excludable match, drop all excludable ones.
    // Otherwise (all are excludable), keep them all.
    let has_non_excludable = classified.iter().any(|(_, excl)| !excl);

    if has_non_excludable {
        classified
            .into_iter()
            .filter(|(_, excl)| !excl)
            .map(|(comp, _)| comp)
            .collect()
    } else {
        classified.into_iter().map(|(comp, _)| comp).collect()
    }
}

pub(crate) struct FileNameCompletionPlan {
    pub(crate) file: LispString,
    pub(crate) directory: LispString,
    pub(crate) completions: Vec<LispString>,
    pub(crate) ignore_case: bool,
}

pub(crate) fn prepare_file_name_completion_in_state(
    obarray: &super::symbol::Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    syntax: super::builtins::search::FastStringMatchSyntax,
    args: &[Value],
    decode_name: impl Fn(&[u8]) -> LispString,
) -> Result<FileNameCompletionPlan, Flow> {
    expect_args_range("file-name-completion", args, 2, 3)?;

    let file = expect_lisp_string("file-name-completion", &args[0])?;
    let directory_arg = expect_lisp_string("file-name-completion", &args[1])?;
    let directory =
        super::fileio::resolve_filename_lisp_in_state(obarray, dynamic, buffers, &directory_arg);
    let ignore_case = get_completion_ignore_case(obarray, buffers);
    let ignored_extensions = get_ignored_extensions(obarray);
    let regexps = super::minibuffer::completion_regexp_lisp_list_from_obarray(obarray);
    let completions = if file.as_bytes().contains(&b'/') {
        Vec::new()
    } else {
        let raw =
            collect_file_name_completions(&file, &directory, ignore_case, false, decode_name)?;
        // Apply completion-ignored-extensions filtering for file-name-completion
        // (but not for file-name-all-completions, per GNU Emacs).
        let filtered = filter_by_ignored_extensions(&file, raw, &ignored_extensions, ignore_case);
        filter_by_completion_regexps(syntax, obarray, buffers, filtered, &regexps, ignore_case)?
    };

    Ok(FileNameCompletionPlan {
        file,
        directory,
        completions,
        ignore_case,
    })
}

pub(crate) fn finish_file_name_completion_with_eval_predicate(
    eval: &mut Context,
    predicate: Option<&Value>,
    directory: LispString,
    file: LispString,
    completions: Vec<LispString>,
    ignore_case: bool,
) -> EvalResult {
    let Some(predicate) = predicate.copied() else {
        return Ok(resolve_file_name_completion(
            &file,
            completions,
            ignore_case,
        ));
    };
    if predicate.is_nil() {
        return Ok(resolve_file_name_completion(
            &file,
            completions,
            ignore_case,
        ));
    }

    let use_absolute_path = predicate_uses_absolute_file_argument(&eval.obarray, &predicate);
    let bound_directory = directory.clone();
    finish_file_name_completion_with_callable_predicate(
        use_absolute_path,
        directory,
        file,
        completions,
        ignore_case,
        |predicate_arg| {
            with_default_directory_binding(eval, &bound_directory, |eval| {
                eval.apply(predicate, vec![predicate_arg])
            })
        },
    )
}

pub(crate) fn predicate_uses_absolute_file_argument(
    obarray: &super::symbol::Obarray,
    predicate: &Value,
) -> bool {
    let Some(symbol) = predicate_callable_name(predicate) else {
        return false;
    };
    obarray.symbol_function(symbol).is_none() && is_builtin_path_predicate(symbol)
}

pub(crate) fn finish_file_name_completion_with_callable_predicate(
    use_absolute_path: bool,
    directory: LispString,
    file: LispString,
    completions: Vec<LispString>,
    ignore_case: bool,
    predicate_call: impl FnMut(Value) -> Result<Value, Flow>,
) -> EvalResult {
    let completions = filter_completions_by_callable_predicate(
        use_absolute_path,
        &directory,
        completions,
        predicate_call,
    )?;
    Ok(resolve_file_name_completion(
        &file,
        completions,
        ignore_case,
    ))
}

fn resolve_file_name_completion(
    file: &LispString,
    completions: Vec<LispString>,
    ignore_case: bool,
) -> Value {
    if completions.is_empty() {
        return Value::NIL;
    }

    let filtered = filter_completion_candidates(file, completions);
    if filtered.is_empty() {
        return Value::heap_string(file.clone());
    }

    // If there is exactly one completion and it matches FILE exactly, return t.
    // For directory candidates ending in '/', Emacs returns the completion
    // string when FILE lacks the trailing slash (e.g. ".." -> "../").
    if filtered.len() == 1 {
        let comp = &filtered[0];
        let eq = if ignore_case {
            comp.as_bytes().eq_ignore_ascii_case(file.as_bytes())
        } else {
            comp.as_bytes() == file.as_bytes()
        };
        if eq {
            return Value::T;
        }
        return Value::heap_string(comp.clone());
    }

    // Find the longest common prefix among completions.
    // When completion-ignore-case is set, use case-insensitive comparison
    // but preserve the case of the first match (which GNU Emacs refines to
    // prefer the match whose case matches the input). Operate over Emacs
    // characters so eight-bit/multibyte file-name bytes never split.
    let mut prefix_bytes = filtered[0].as_bytes().to_vec();
    for comp in &filtered[1..] {
        let common = common_prefix_byte_len(&prefix_bytes, comp.as_bytes(), ignore_case);
        prefix_bytes.truncate(common);
    }

    // If the prefix equals the input exactly and there are multiple matches,
    // return the prefix (Emacs returns what was typed if ambiguous but valid prefix).
    Value::heap_string(file_name_lisp_from_bytes(prefix_bytes))
}

/// Length (in bytes) of the longest common prefix of two Emacs-byte file-name
/// sequences, measured at Emacs-character boundaries. Names are ASCII for the
/// structural parts; eight-bit/multibyte chars compare by codepoint and only
/// ASCII case-folds under `ignore_case`.
fn common_prefix_byte_len(a: &[u8], b: &[u8], ignore_case: bool) -> usize {
    let mut pos = 0usize;
    while pos < a.len() && pos < b.len() {
        let (ca, la) = super::emacs_char::string_char(&a[pos..]);
        let (cb, lb) = super::emacs_char::string_char(&b[pos..]);
        let eq = if ca == cb {
            true
        } else if ignore_case {
            ascii_lower_codepoint(ca) == ascii_lower_codepoint(cb)
        } else {
            false
        };
        if !eq {
            break;
        }
        pos += la.max(1);
        // Defensive: if the two encodings disagree in length they cannot be the
        // same character, so they should have compared unequal above.
        debug_assert_eq!(la, lb);
    }
    pos
}

/// ASCII-only lowercasing of an Emacs codepoint (non-ASCII passes through).
fn ascii_lower_codepoint(c: u32) -> u32 {
    if (b'A' as u32..=b'Z' as u32).contains(&c) {
        c + 32
    } else {
        c
    }
}

fn filter_completion_candidates(
    file: &LispString,
    completions: Vec<LispString>,
) -> Vec<LispString> {
    let file_starts_dotdot = file.as_bytes().starts_with(b"..");
    completions
        .into_iter()
        .filter(|completion| completion.as_bytes() != b"./")
        .filter(|completion| file_starts_dotdot || completion.as_bytes() != b"../")
        .collect()
}

/// Join a directory file-name with a candidate entry, preserving raw file-name
/// bytes. Mirrors `Path::join`: an absolute candidate replaces the directory; a
/// separator is inserted between the two components otherwise.
fn join_dir_candidate_lisp(directory: &LispString, candidate: &LispString) -> LispString {
    let cand = candidate.as_bytes();
    if cand.first() == Some(&b'/') {
        return candidate.clone();
    }
    let dir = directory.as_bytes();
    if dir.is_empty() {
        return candidate.clone();
    }
    let mut out = Vec::with_capacity(dir.len() + 1 + cand.len());
    out.extend_from_slice(dir);
    if out.last() != Some(&b'/') {
        out.push(b'/');
    }
    out.extend_from_slice(cand);
    file_name_lisp_from_bytes(out)
}

fn filter_completions_by_callable_predicate(
    use_absolute_path: bool,
    directory: &LispString,
    completions: Vec<LispString>,
    mut predicate_call: impl FnMut(Value) -> Result<Value, Flow>,
) -> Result<Vec<LispString>, Flow> {
    let mut filtered = Vec::new();
    for candidate in completions {
        let predicate_arg =
            predicate_argument_for_callable_predicate(use_absolute_path, directory, &candidate);
        let keep = predicate_call(predicate_arg)?.is_truthy();
        if keep {
            filtered.push(candidate);
        }
    }
    Ok(filtered)
}

fn with_default_directory_binding(
    eval: &mut Context,
    directory: &LispString,
    f: impl FnOnce(&mut Context) -> EvalResult,
) -> EvalResult {
    let count = eval.specpdl.len();
    eval.try_specbind_or_unwind_to(
        count,
        intern("default-directory"),
        Value::heap_string(directory.clone()),
    )?;
    let result = f(eval);
    eval.unbind_to_with_result(count, result)
}

fn predicate_argument_for_callable_predicate(
    use_absolute_path: bool,
    directory: &LispString,
    candidate: &LispString,
) -> Value {
    if use_absolute_path {
        return file_name_value(join_dir_candidate_lisp(directory, candidate));
    }

    Value::heap_string(candidate.clone())
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
    match predicate.kind() {
        ValueKind::Symbol(id) => Some(resolve_sym(id)),
        ValueKind::Subr(id) => Some(resolve_sym(id)),
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = predicate.as_subr_id().unwrap();
            Some(resolve_sym(id))
        }
        _ => None,
    }
}

/// Context-backed variant of `file-attributes`.
/// Expands FILENAME and dispatches file-name handlers like GNU `file-attributes`.
pub(crate) fn builtin_file_attributes(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("file-attributes", &args, 1, 2)?;

    // GNU dired.c:Ffile_attributes wraps expand-file-name in an all-error
    // condition handler and returns nil if expansion fails or returns non-string.
    let expanded = match super::fileio::builtin_expand_file_name(eval, vec![args[0], Value::NIL]) {
        Ok(value) => value,
        Err(_) => return Ok(Value::NIL),
    };
    let filename_lisp = match expanded.as_lisp_string() {
        Some(string) => string.clone(),
        None => return Ok(Value::NIL),
    };

    let mut handler_args = vec![Value::heap_string(filename_lisp.clone())];
    if args.get(1).is_some_and(|value| value.is_truthy()) {
        handler_args.push(args[1]);
    }
    if let Some(result) =
        super::fileio::dispatch_file_handler(eval, "file-attributes", &handler_args)?
    {
        return Ok(result);
    }

    // GNU Emacs: return string names unless ID-FORMAT is nil or 'integer.
    let id_format = FileIdFormat::from_id_format_arg(args.get(1));
    let time_output = LispTimeOutput::from_context(eval)?;

    match build_file_attributes(&filename_lisp, id_format, time_output) {
        Some(attrs) => Ok(attrs),
        None => Ok(Value::NIL),
    }
}

/// (file-attributes-lessp F1 F2)
///
/// Return t if the first element (filename) of F1 is less than that of F2.
/// F1 and F2 are each (NAME . ATTRIBUTES) cons cells as returned by
/// `directory-files-and-attributes`.
pub(crate) fn builtin_file_attributes_lessp(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("file-attributes-lessp", &args, 2, 2)?;

    // GNU (src/dired.c): `return Fstring_lessp (Fcar (f1), Fcar (f2));`.
    // It performs NO type-check on the cars itself — it simply delegates to
    // `string-lessp`, which accepts strings *and* symbols (including `nil`,
    // via SYMBOL_NAME) and only rejects other types.  C function-argument
    // evaluation runs right-to-left with the system compiler, so `Fcar (f2)`
    // is observed before `Fcar (f1)`; mirror that ordering for error-equivalence.
    let car2 = file_attributes_lessp_car(&args[1])?;
    let car1 = file_attributes_lessp_car(&args[0])?;

    super::builtins::builtin_string_lessp_2(eval, car1, car2)
}

/// Take the car of a `file-attributes-lessp` argument with GNU `Fcar`
/// semantics: `nil` yields `nil`, a cons yields its car, and any other type
/// signals `(wrong-type-argument listp ...)`.
fn file_attributes_lessp_car(val: &Value) -> Result<Value, Flow> {
    match val.kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Cons => Ok(val.cons_car()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *val],
        )),
    }
}

/// (system-users)
///
/// Return a list of user names on the system.
/// Reads `/etc/passwd` and returns account names in oracle-compatible order.
pub(crate) fn builtin_system_users(args: Vec<Value>) -> EvalResult {
    expect_args_range("system-users", &args, 0, 0)?;

    let mut users = read_colon_file_names(&system_users_passwd_path());
    if users.is_empty() {
        let fallback_user = std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| "unknown".to_string());
        users.push(fallback_user);
    }

    Ok(Value::list(
        users.into_iter().map(Value::string).collect::<Vec<_>>(),
    ))
}

/// (system-groups)
///
/// Return a list of group names on the system.
/// Reads `/etc/group` and returns group names in oracle-compatible order.
pub(crate) fn builtin_system_groups(args: Vec<Value>) -> EvalResult {
    expect_args_range("system-groups", &args, 0, 0)?;
    let groups = read_colon_file_names(&system_groups_path());
    if groups.is_empty() {
        return Ok(Value::NIL);
    }
    Ok(Value::list(
        groups.into_iter().map(Value::string).collect::<Vec<_>>(),
    ))
}

fn parse_colon_file_names(contents: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((name, _rest)) = trimmed.split_once(':') {
            let name = name.trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
        }
    }
    // Emacs' output order matches reverse file order.
    names.reverse();
    names
}

fn system_users_passwd_path() -> String {
    "/etc/passwd".to_string()
}

fn system_groups_path() -> String {
    "/etc/group".to_string()
}

fn read_colon_file_names(path: &str) -> Vec<String> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_colon_file_names(&contents),
        Err(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Bootstrap variables
// ---------------------------------------------------------------------------

/// Register the variables GNU's `syms_of_dired` (src/dired.c) installs in C.
pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    // dired.c:1206 — DEFVAR_LISP (Vcompletion_ignored_extensions), then
    // dired.c:1212 initializes it to nil. `lisp/bindings.el' supplies the real
    // list at load time, so do not seed one here.
    //
    // DEFVAR_LISP is what makes the symbol special. Without it a `let' around
    // `completion-ignored-extensions' in a lexical-binding file binds
    // lexically, and callees like `completion-pcm--filename-try-filter' keep
    // reading the global list — file-name completion then quietly ignores the
    // caller's rebinding.
    obarray.set_symbol_value("completion-ignored-extensions", Value::NIL);
    obarray.make_special("completion-ignored-extensions");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
