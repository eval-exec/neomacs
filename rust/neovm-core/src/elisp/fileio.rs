//! File I/O primitives for the Elisp VM.
//!
//! Provides path manipulation, file predicates, read/write operations,
//! directory operations, and file attribute queries.

use std::fs;
use std::path::{Path, PathBuf};

use super::error::{signal, EvalResult, Flow};
use super::value::Value;

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

    // If already absolute, just clean it up
    if path.is_absolute() {
        return clean_path(&PathBuf::from(&expanded));
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
    clean_path(&joined)
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

/// Return the extension of FILENAME, or None if it has none.
/// Does not include the leading dot.
pub fn file_name_extension(filename: &str) -> Option<String> {
    let path = Path::new(filename);
    path.extension().map(|e| e.to_string_lossy().into_owned())
}

/// Return FILENAME without its extension.
pub fn file_name_sans_extension(filename: &str) -> String {
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

// ===========================================================================
// File I/O operations
// ===========================================================================

/// Read the contents of FILENAME as a UTF-8 string.
pub fn read_file_contents(filename: &str) -> Result<String, String> {
    fs::read_to_string(filename).map_err(|e| format!("Opening input file: {}: {}", filename, e))
}

/// Write CONTENT to FILENAME, optionally appending.
pub fn write_string_to_file(content: &str, filename: &str, append: bool) -> Result<(), String> {
    use std::io::Write;
    let file = if append {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(filename)
    } else {
        fs::File::create(filename)
    };
    match file {
        Ok(mut f) => f
            .write_all(content.as_bytes())
            .map_err(|e| format!("Writing to {}: {}", filename, e)),
        Err(e) => Err(format!("Opening output file {}: {}", filename, e)),
    }
}

// ===========================================================================
// Directory operations
// ===========================================================================

/// Return a list of file names in DIR.
/// If FULL is true, return absolute paths.
/// If MATCH_REGEX is Some, only include entries whose names match the regex.
pub fn directory_files(
    dir: &str,
    full: bool,
    match_regex: Option<&str>,
) -> Result<Vec<String>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Opening directory {}: {}", dir, e))?;

    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Reading directory entry: {}", e))?;
        let name = entry.file_name().to_string_lossy().into_owned();

        // Apply regex filter if provided
        if let Some(pattern) = match_regex {
            // Simple substring match (not full regex, to avoid pulling in the
            // regex crate). Matches if the pattern appears anywhere in the name.
            if !name.contains(pattern) {
                continue;
            }
        }

        if full {
            let full_path = Path::new(dir).join(&name);
            result.push(full_path.to_string_lossy().into_owned());
        } else {
            result.push(name);
        }
    }
    result.sort();
    Ok(result)
}

/// Create directory DIR.  If PARENTS is true, create parent directories as needed.
pub fn make_directory(dir: &str, parents: bool) -> Result<(), String> {
    let result = if parents {
        fs::create_dir_all(dir)
    } else {
        fs::create_dir(dir)
    };
    result.map_err(|e| format!("Creating directory {}: {}", dir, e))
}

// ===========================================================================
// File management
// ===========================================================================

/// Delete FILENAME.
pub fn delete_file(filename: &str) -> Result<(), String> {
    fs::remove_file(filename).map_err(|e| format!("Deleting {}: {}", filename, e))
}

/// Rename file FROM to TO.
pub fn rename_file(from: &str, to: &str) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| format!("Renaming {} to {}: {}", from, to, e))
}

/// Copy file FROM to TO.
pub fn copy_file(from: &str, to: &str) -> Result<(), String> {
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("Copying {} to {}: {}", from, to, e))
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

/// (expand-file-name NAME &optional DEFAULT-DIRECTORY) -> string
pub(crate) fn builtin_expand_file_name(args: Vec<Value>) -> EvalResult {
    expect_min_args("expand-file-name", &args, 1)?;
    let name = expect_string(&args[0])?;
    let default_dir = if args.len() > 1 && args[1].is_truthy() {
        Some(expect_string(&args[1])?)
    } else {
        None
    };
    Ok(Value::string(expand_file_name(
        &name,
        default_dir.as_deref(),
    )))
}

/// (file-name-directory FILENAME) -> string or nil
pub(crate) fn builtin_file_name_directory(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-directory", &args, 1)?;
    let filename = expect_string(&args[0])?;
    match file_name_directory(&filename) {
        Some(dir) => Ok(Value::string(dir)),
        None => Ok(Value::Nil),
    }
}

/// (file-name-nondirectory FILENAME) -> string
pub(crate) fn builtin_file_name_nondirectory(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-nondirectory", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::string(file_name_nondirectory(&filename)))
}

/// (file-name-extension FILENAME) -> string or nil
pub(crate) fn builtin_file_name_extension(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-extension", &args, 1)?;
    let filename = expect_string(&args[0])?;
    match file_name_extension(&filename) {
        Some(ext) => Ok(Value::string(ext)),
        None => Ok(Value::Nil),
    }
}

/// (file-name-sans-extension FILENAME) -> string
pub(crate) fn builtin_file_name_sans_extension(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-sans-extension", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::string(file_name_sans_extension(&filename)))
}

/// (file-name-as-directory FILENAME) -> string
pub(crate) fn builtin_file_name_as_directory(args: Vec<Value>) -> EvalResult {
    expect_args("file-name-as-directory", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::string(file_name_as_directory(&filename)))
}

/// (directory-file-name FILENAME) -> string
pub(crate) fn builtin_directory_file_name(args: Vec<Value>) -> EvalResult {
    expect_args("directory-file-name", &args, 1)?;
    let filename = expect_string(&args[0])?;
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

/// (file-exists-p FILENAME) -> t or nil
pub(crate) fn builtin_file_exists_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-exists-p", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::bool(file_exists_p(&filename)))
}

/// (file-readable-p FILENAME) -> t or nil
pub(crate) fn builtin_file_readable_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-readable-p", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::bool(file_readable_p(&filename)))
}

/// (file-writable-p FILENAME) -> t or nil
pub(crate) fn builtin_file_writable_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-writable-p", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::bool(file_writable_p(&filename)))
}

/// (file-directory-p FILENAME) -> t or nil
pub(crate) fn builtin_file_directory_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-directory-p", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::bool(file_directory_p(&filename)))
}

/// (file-regular-p FILENAME) -> t or nil
pub(crate) fn builtin_file_regular_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-regular-p", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::bool(file_regular_p(&filename)))
}

/// (file-symlink-p FILENAME) -> t or nil
pub(crate) fn builtin_file_symlink_p(args: Vec<Value>) -> EvalResult {
    expect_args("file-symlink-p", &args, 1)?;
    let filename = expect_string(&args[0])?;
    Ok(Value::bool(file_symlink_p(&filename)))
}

/// (delete-file FILENAME) -> nil
pub(crate) fn builtin_delete_file(args: Vec<Value>) -> EvalResult {
    expect_args("delete-file", &args, 1)?;
    let filename = expect_string(&args[0])?;
    delete_file(&filename).map_err(|e| signal("file-error", vec![Value::string(e)]))?;
    Ok(Value::Nil)
}

/// (rename-file FROM TO) -> nil
pub(crate) fn builtin_rename_file(args: Vec<Value>) -> EvalResult {
    expect_args("rename-file", &args, 2)?;
    let from = expect_string(&args[0])?;
    let to = expect_string(&args[1])?;
    rename_file(&from, &to).map_err(|e| signal("file-error", vec![Value::string(e)]))?;
    Ok(Value::Nil)
}

/// (copy-file FROM TO) -> nil
pub(crate) fn builtin_copy_file(args: Vec<Value>) -> EvalResult {
    expect_min_args("copy-file", &args, 2)?;
    let from = expect_string(&args[0])?;
    let to = expect_string(&args[1])?;
    copy_file(&from, &to).map_err(|e| signal("file-error", vec![Value::string(e)]))?;
    Ok(Value::Nil)
}

/// (make-directory DIR &optional PARENTS) -> nil
pub(crate) fn builtin_make_directory(args: Vec<Value>) -> EvalResult {
    expect_min_args("make-directory", &args, 1)?;
    let dir = expect_string(&args[0])?;
    let parents = args.get(1).is_some_and(|v| v.is_truthy());
    make_directory(&dir, parents).map_err(|e| signal("file-error", vec![Value::string(e)]))?;
    Ok(Value::Nil)
}

/// (directory-files DIR &optional FULL MATCH) -> list of strings
pub(crate) fn builtin_directory_files(args: Vec<Value>) -> EvalResult {
    expect_min_args("directory-files", &args, 1)?;
    let dir = expect_string(&args[0])?;
    let full = args.get(1).is_some_and(|v| v.is_truthy());
    let match_pattern = if let Some(val) = args.get(2) {
        if val.is_truthy() {
            Some(expect_string(val)?)
        } else {
            None
        }
    } else {
        None
    };
    let files = directory_files(&dir, full, match_pattern.as_deref())
        .map_err(|e| signal("file-error", vec![Value::string(e)]))?;
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
    let visit = args.get(1).is_some_and(|v| v.is_truthy());

    // Read file contents
    let contents =
        read_file_contents(&filename).map_err(|e| signal("file-error", vec![Value::string(e)]))?;

    let char_count = contents.chars().count() as i64;

    // Insert into current buffer
    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.insert(&contents);

    if visit {
        let abs_path = expand_file_name(&filename, None);
        buf.file_name = Some(abs_path.clone());
        buf.set_modified(false);
    }

    let abs_filename = expand_file_name(&filename, None);
    Ok(Value::list(vec![
        Value::string(abs_filename),
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

    write_string_to_file(&content, &filename, append)
        .map_err(|e| signal("file-error", vec![Value::string(e)]))?;

    if visit {
        // Need mutable access to set file_name and modified flag
        let buf_mut = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let abs_path = expand_file_name(&filename, None);
        buf_mut.file_name = Some(abs_path);
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
    let abs_path = expand_file_name(&filename, None);

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
            .map_err(|e| signal("file-error", vec![Value::string(e)]))?;

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
        assert_eq!(file_name_extension("test.txt"), Some("txt".to_string()));
        assert_eq!(
            file_name_extension("/home/user/file.el"),
            Some("el".to_string())
        );
        assert_eq!(file_name_extension("no_ext"), None);
        assert_eq!(
            file_name_extension("archive.tar.gz"),
            Some("gz".to_string())
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
        let files = directory_files(&base_str, false, None).unwrap();
        assert!(files.contains(&"foo.txt".to_string()));
        assert!(files.contains(&"bar.txt".to_string()));
        assert!(files.contains(&"baz.el".to_string()));

        // List with filter
        let filtered = directory_files(&base_str, false, Some(".el")).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0], "baz.el");

        // List with full paths
        let full = directory_files(&base_str, true, None).unwrap();
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
    fn test_builtin_file_name_ops() {
        let result = builtin_file_name_directory(vec![Value::string("/home/user/test.el")]);
        assert_eq!(result.unwrap().as_str(), Some("/home/user/"));

        let result = builtin_file_name_nondirectory(vec![Value::string("/home/user/test.el")]);
        assert_eq!(result.unwrap().as_str(), Some("test.el"));

        let result = builtin_file_name_extension(vec![Value::string("/home/user/test.el")]);
        assert_eq!(result.unwrap().as_str(), Some("el"));

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
