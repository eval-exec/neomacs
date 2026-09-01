//! Regression tests for `call-process` / `call-process-region`
//! working-directory validation, relative-INFILE expansion, the DELETE
//! read-only check, and file-open error data — asserting GNU behavior.
//!
//! GNU oracle (emacs --batch) for each case is quoted at the test.

use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::error::Flow;
use crate::emacs_core::intern::resolve_sym;
use crate::heap_types::LispString;
use std::path::{Path, PathBuf};

fn workspace_temp_dir(prefix: &str) -> tempfile::TempDir {
    let workspace_root = PathBuf::from(env!("CARGO_WORKSPACE_DIR"));
    let temp_root = workspace_root.join("tmp");
    std::fs::create_dir_all(&temp_root).expect("create workspace tmp directory");
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(temp_root)
        .expect("create workspace-local temp directory")
}

fn directory_name(path: &Path) -> String {
    let mut name = path.to_string_lossy().into_owned();
    if !name.ends_with(std::path::MAIN_SEPARATOR) {
        name.push(std::path::MAIN_SEPARATOR);
    }
    name
}

fn find_bin(name: &str) -> String {
    for dir in ["/bin", "/usr/bin", "/run/current-system/sw/bin"] {
        let path = format!("{dir}/{name}");
        if std::path::Path::new(&path).exists() {
            return path;
        }
    }
    if let Ok(output) = std::process::Command::new("which").arg(name).output() {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    name.to_string()
}

/// Set the current buffer's `default-directory` (GNU `BVAR (current_buffer,
/// directory)`), which is what `get_current_directory`/INFILE expansion read.
fn set_default_directory(eval: &mut Context, dir: &str) {
    let id = eval.buffers.current_buffer_id().expect("current buffer");
    let buf = eval.buffers.get_mut(id).expect("buffer");
    buf.set_buffer_local("default-directory", Value::string(dir));
}

/// Extract the (symbol-name, data-vec) of a signal, or panic with the Ok value.
fn expect_signal(r: EvalResult) -> (String, Vec<Value>) {
    match r {
        Ok(v) => panic!("expected a signal, got Ok({v:?})"),
        Err(Flow::Signal(sd)) => (resolve_sym(sd.symbol).to_string(), sd.data.clone()),
        Err(other) => panic!("expected a signal, got {other:?}"),
    }
}

fn data_string(value: &Value) -> String {
    let ls = value
        .as_lisp_string()
        .expect("expected a string in signal data");
    String::from_utf8_lossy(ls.as_bytes()).into_owned()
}

/// Bug 5: GNU validates that `default-directory` is an accessible directory
/// before spawning. With a nonexistent dir:
///   (file-missing "Setting current directory" "No such file or directory" DIR)
#[test]
fn call_process_signals_setting_current_directory_for_missing_dir() {
    crate::test_utils::init_test_tracing();
    let temp = workspace_temp_dir("callproc-missing-dir-");
    let missing = directory_name(&temp.path().join("missing"));
    let mut eval = Context::new();
    set_default_directory(&mut eval, &missing);

    let r = builtin_call_process(
        &mut eval,
        vec![
            Value::string(find_bin("true")),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );

    let (symbol, data) = expect_signal(r);
    assert_eq!(symbol, "file-missing");
    assert_eq!(data.len(), 3, "data was {data:?}");
    assert_eq!(data_string(&data[0]), "Setting current directory");
    assert_eq!(data_string(&data[1]), "No such file or directory");
    assert_eq!(data_string(&data[2]), missing);
}

/// Bug 5 must NOT fire for an accessible directory — the common path still runs
/// the program and returns its exit status.
#[test]
fn call_process_runs_with_valid_default_directory() {
    crate::test_utils::init_test_tracing();
    let temp = workspace_temp_dir("callproc-valid-dir-");
    let dir = directory_name(temp.path());
    let mut eval = Context::new();
    set_default_directory(&mut eval, &dir);

    let r = builtin_call_process(
        &mut eval,
        vec![
            Value::string(find_bin("true")),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("call-process with a valid dir should not signal");
    assert_eq!(r.as_fixnum(), Some(0));
}

/// Bug 6: a relative INFILE is expanded against `default-directory`, not the
/// editor's process cwd. GNU:
///   (with-temp-buffer (call-process "cat" "p7_oneline.txt" t nil)
///                     (buffer-string)) => "alpha beta gamma\n"
#[test]
fn call_process_expands_relative_infile_against_default_directory() {
    crate::test_utils::init_test_tracing();
    let temp = workspace_temp_dir("callproc-relative-infile-");
    let dir = directory_name(temp.path());
    let infile = temp.path().join("callproc_relinfile_test.txt");
    std::fs::write(&infile, b"alpha beta gamma\n").expect("write infile");

    let mut eval = Context::new();
    let buffer_id = eval.buffers.create_buffer("callproc-relinfile-out");
    assert!(eval.buffers.switch_current(buffer_id));
    set_default_directory(&mut eval, &dir);

    let r = builtin_call_process(
        &mut eval,
        vec![
            Value::string(find_bin("cat")),
            // Bare relative name — must resolve against default-directory.
            Value::string("callproc_relinfile_test.txt"),
            Value::T,
            Value::NIL,
        ],
    )
    .expect("call-process should expand the relative INFILE and succeed");
    assert_eq!(r.as_fixnum(), Some(0));

    let buf = eval.buffers.get(buffer_id).expect("output buffer");
    let range = buf.full_emacs_byte_range();
    let text = buf.buffer_substring_lisp_string_range(range);
    assert_eq!(
        String::from_utf8_lossy(text.as_bytes()),
        "alpha beta gamma\n"
    );
}

/// Bug 10: `call-process-region` with DELETE=t honors `buffer-read-only` — GNU
/// runs `barf_if_buffer_read_only` (via `Fdelete_region`) before touching the
/// region, signaling `(buffer-read-only BUFFER)` and leaving the text intact.
#[test]
fn call_process_region_delete_respects_buffer_read_only() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.create_buffer("callproc-region-ro");
    assert!(eval.buffers.switch_current(buffer_id));
    eval.buffers
        .insert_lisp_string_into_buffer(buffer_id, &LispString::from_utf8("rodata"))
        .expect("insert");
    eval.buffers
        .get_mut(buffer_id)
        .unwrap()
        .set_buffer_local("buffer-read-only", Value::T);

    let r = builtin_call_process_region(
        &mut eval,
        vec![
            Value::fixnum(1),
            Value::fixnum(3),
            Value::string(find_bin("cat")),
            Value::T, // DELETE
            Value::NIL,
            Value::NIL,
        ],
    );

    let (symbol, data) = expect_signal(r);
    assert_eq!(symbol, "buffer-read-only");
    assert_eq!(data.len(), 1, "data was {data:?}");
    assert!(
        data[0].is_buffer(),
        "expected the buffer object, got {:?}",
        data[0]
    );

    // The region must be untouched because the read-only check precedes delete.
    let buf = eval.buffers.get(buffer_id).expect("buffer");
    let range = buf.full_emacs_byte_range();
    let text = buf.buffer_substring_lisp_string_range(range);
    assert_eq!(String::from_utf8_lossy(text.as_bytes()), "rodata");
}

/// GNU's `Fcall_process_region` (src/callproc.c:1099-1147) performs the DELETE
/// deletion *before* it calls `call_process`, and `call_process`
/// (src/callproc.c:390, 447-476) is where PROGRAM is type-checked and searched
/// for on `exec-path`.  A missing executable therefore leaves the region
/// already deleted.  Neomacs used to resolve the executable first, so the
/// region survived a failed lookup.
///
/// GNU oracle (emacs -Q --batch):
///   (with-temp-buffer (insert "hello")
///     (list (condition-case e (call-process-region (point-min) (point-max)
///                                                  "neomacs-no-such-program-xyz" t nil)
///             (error e))
///           (buffer-string)))
///   => ((file-missing "Searching for program" "No such file or directory"
///        "neomacs-no-such-program-xyz") "")
/// The same holds with START = nil (whole-buffer delete).
#[test]
fn call_process_region_delete_happens_before_the_program_lookup_like_gnu() {
    crate::test_utils::init_test_tracing();

    for start in [Value::fixnum(1), Value::NIL] {
        let mut eval = Context::new();
        let buffer_id = eval.buffers.create_buffer("callproc-region-delete-order");
        assert!(eval.buffers.switch_current(buffer_id));
        eval.buffers
            .insert_lisp_string_into_buffer(buffer_id, &LispString::from_utf8("hello"))
            .expect("insert");

        let r = builtin_call_process_region(
            &mut eval,
            vec![
                start,
                Value::fixnum(6),
                Value::string("neomacs-no-such-program-xyz"),
                Value::T, // DELETE
                Value::NIL,
                Value::NIL,
            ],
        );

        let (symbol, _data) = expect_signal(r);
        assert_eq!(symbol, "file-missing", "START = {start:?}");

        let buf = eval.buffers.get(buffer_id).expect("buffer");
        let range = buf.full_emacs_byte_range();
        let text = buf.buffer_substring_lisp_string_range(range);
        assert_eq!(
            String::from_utf8_lossy(text.as_bytes()),
            "",
            "GNU deletes the region before it looks the program up (START = {start:?})"
        );
    }
}

/// Bug 16: opening a nonexistent INFILE reports GNU's
///   (file-missing "Opening process input file" "No such file or directory" FILENAME)
/// — bare strerror (no Rust "(os error N)") plus the (expanded) filename.
#[test]
fn call_process_missing_infile_error_data_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let temp = workspace_temp_dir("callproc-missing-infile-");
    let missing = temp
        .path()
        .join("missing.txt")
        .to_string_lossy()
        .into_owned();
    let mut eval = Context::new();

    let r = builtin_call_process(
        &mut eval,
        vec![
            Value::string(find_bin("cat")),
            Value::string(&missing),
            Value::NIL,
            Value::NIL,
        ],
    );

    let (symbol, data) = expect_signal(r);
    assert_eq!(symbol, "file-missing");
    assert_eq!(data.len(), 3, "data was {data:?}");
    assert_eq!(data_string(&data[0]), "Opening process input file");
    assert_eq!(data_string(&data[1]), "No such file or directory");
    assert_eq!(data_string(&data[2]), missing);
    // The strerror must not carry Rust's "(os error N)" suffix.
    assert!(!data_string(&data[1]).contains("os error"));
}

/// Bug 16 (part 2): a `(:file DEST)` output target that can't be opened reports
/// GNU's `report_file_errno ("Opening process output file", output_file, ...)`
/// (callproc.c:591) — the operation string is "Opening process output file"
/// (not "Writing process output"), with bare strerror + the filename.
#[test]
fn call_process_file_output_open_error_data_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let temp = workspace_temp_dir("callproc-missing-output-dir-");
    let dest = temp
        .path()
        .join("missing")
        .join("out.txt")
        .to_string_lossy()
        .into_owned();
    let mut eval = Context::new();

    let r = builtin_call_process(
        &mut eval,
        vec![
            Value::string(find_bin("echo")),
            Value::NIL,
            Value::list(vec![Value::keyword(":file"), Value::string(&dest)]),
            Value::NIL,
            Value::string("hi"),
        ],
    );

    let (symbol, data) = expect_signal(r);
    assert_eq!(symbol, "file-missing");
    assert_eq!(data.len(), 3, "data was {data:?}");
    assert_eq!(data_string(&data[0]), "Opening process output file");
    assert_eq!(data_string(&data[1]), "No such file or directory");
    assert_eq!(data_string(&data[2]), dest);
    assert!(!data_string(&data[1]).contains("os error"));
}

/// Bug 5 errno fidelity: a `default-directory` whose path component is a plain
/// file is ENOTDIR — GNU `(file-error "Setting current directory" "Not a
/// directory" DIR)`.
#[test]
fn call_process_setting_current_directory_reports_enotdir() {
    crate::test_utils::init_test_tracing();
    let temp = workspace_temp_dir("callproc-enotdir-");
    let base = temp.path().join("file");
    std::fs::write(&base, b"x").expect("write file");
    let dir = directory_name(&base.join("sub"));

    let mut eval = Context::new();
    set_default_directory(&mut eval, &dir);

    let r = builtin_call_process(
        &mut eval,
        vec![
            Value::string(find_bin("true")),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );

    let (symbol, data) = expect_signal(r);
    assert_eq!(symbol, "file-error");
    assert_eq!(data_string(&data[0]), "Setting current directory");
    assert_eq!(data_string(&data[1]), "Not a directory");
    assert_eq!(data_string(&data[2]), dir);
}
