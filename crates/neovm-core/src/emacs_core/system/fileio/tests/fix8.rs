//! GNU-parity regression tests for file-error data shapes, `copy-file`
//! same-file/missing-source handling, the REPLACE affix-elision return value,
//! and `make-temp-name` file-name decoding.
//!
//! These call the fileio builtins directly with a bare [`Context`] (no full
//! loadup bootstrap), inspecting the resulting `Flow::Signal` DATA so the
//! complete shape — symbol, action, strerror, names — is checked, matching
//! GNU Emacs `--batch` output exactly (re-verified against the system Emacs).

use super::*;
use crate::emacs_core::eval::Context;

/// A scratch directory unique to this process.
fn scratch_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("neovm-fix8-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn signal_strings(flow: Flow) -> (String, Vec<String>) {
    match flow {
        Flow::Signal(sig) => {
            let symbol = sig.symbol_name().to_string();
            let data = sig
                .data
                .iter()
                .map(|v| {
                    v.as_utf8_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("{v:?}"))
                })
                .collect();
            (symbol, data)
        }
        other => panic!("expected Flow::Signal, got {other:?}"),
    }
}

/// Bug 2: a missing input file is reported by `insert-file-contents` as
/// `(file-missing "Opening input file" "No such file or directory" PATH)` with
/// the *bare* strerror (no Rust "(os error N)" suffix).
#[test]
fn insert_file_contents_missing_file_error_shape_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let missing = scratch_dir().join("no_such_fix8");
    let _ = std::fs::remove_file(&missing);
    let missing_str = missing.to_string_lossy().to_string();

    let mut eval = Context::new();
    let flow = builtin_insert_file_contents(&mut eval, vec![Value::string(&missing_str)])
        .expect_err("missing file must signal");
    let (symbol, data) = signal_strings(flow);
    assert_eq!(symbol, "file-missing");
    assert_eq!(
        data,
        vec![
            "Opening input file".to_string(),
            "No such file or directory".to_string(),
            missing_str,
        ]
    );
}

/// Bug 12: `copy-file` from a NONEXISTENT source fails at the input open with
/// `(file-missing "Opening input file" STRERROR SRC)` — 3 elements, the
/// destination is never reported (GNU `report_file_error ("Opening input
/// file", file)`).
#[test]
fn copy_file_missing_source_error_shape_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = scratch_dir();
    let missing = dir.join("no_such_copy_fix8");
    let dest = dir.join("dest_copy_fix8");
    let _ = std::fs::remove_file(&missing);
    let _ = std::fs::remove_file(&dest);
    let missing_str = missing.to_string_lossy().to_string();

    let mut eval = Context::new();
    let flow = builtin_copy_file(
        &mut eval,
        vec![
            Value::string(&missing_str),
            Value::string(dest.to_string_lossy()),
        ],
    )
    .expect_err("missing source must signal");
    let (symbol, data) = signal_strings(flow);
    assert_eq!(symbol, "file-missing");
    assert_eq!(
        data,
        vec![
            "Opening input file".to_string(),
            "No such file or directory".to_string(),
            missing_str,
        ],
        "only the source filename is reported, not the destination"
    );
}

/// Bug 13: a `write-region` open failure uses the action "Opening output file"
/// (GNU `report_file_errno ("Opening output file", ...)`), never "Writing to".
#[test]
fn write_region_open_failure_action_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let nope = scratch_dir().join("NOPE_fix8/deep/f");
    let _ = std::fs::remove_dir_all(scratch_dir().join("NOPE_fix8"));
    let nope_str = nope.to_string_lossy().to_string();

    let mut eval = Context::new();
    let flow = builtin_write_region(
        &mut eval,
        vec![Value::string("z"), Value::NIL, Value::string(&nope_str)],
    )
    .expect_err("write into missing dir must signal");
    let (symbol, data) = signal_strings(flow);
    assert_eq!(symbol, "file-missing");
    assert_eq!(
        data,
        vec![
            "Opening output file".to_string(),
            "No such file or directory".to_string(),
            nope_str,
        ]
    );
}

/// Bug 11: a second `make-directory-internal` of an existing path signals
/// EEXIST, whose DATA *omits* the action string:
/// `(file-already-exists "File exists" PATH)`.
#[test]
fn make_directory_eexist_omits_action_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let ex = scratch_dir().join("ex_fix8");
    let _ = std::fs::remove_dir_all(&ex);
    let ex_str = ex.to_string_lossy().to_string();

    let mut eval = Context::new();
    builtin_make_directory_internal(&mut eval, vec![Value::string(&ex_str)])
        .expect("first make-directory should succeed");
    let flow = builtin_make_directory_internal(&mut eval, vec![Value::string(&ex_str)])
        .expect_err("second make-directory must signal EEXIST");
    let (symbol, data) = signal_strings(flow);
    assert_eq!(symbol, "file-already-exists");
    assert_eq!(
        data,
        vec!["File exists".to_string(), ex_str],
        "EEXIST data omits the action string and uses the bare strerror"
    );
    let _ = std::fs::remove_dir_all(scratch_dir().join("ex_fix8"));
}

/// Bug 10: `copy-file` with src == dest signals
/// `(file-error "Input and output files are the same" "Success" SRC DEST)` —
/// GNU compares dev+inode and reports with errno 0 (strerror "Success").
#[test]
#[cfg(unix)]
fn copy_file_same_src_dest_signals_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let same = scratch_dir().join("same_fix8");
    std::fs::write(&same, b"hello\n").expect("write same");
    let same_str = same.to_string_lossy().to_string();

    let mut eval = Context::new();
    let flow = builtin_copy_file(
        &mut eval,
        vec![Value::string(&same_str), Value::string(&same_str), Value::T],
    )
    .expect_err("same src/dest must signal");
    let (symbol, data) = signal_strings(flow);
    assert_eq!(symbol, "file-error");
    assert_eq!(
        data,
        vec![
            "Input and output files are the same".to_string(),
            "Success".to_string(),
            same_str.clone(),
            same_str,
        ]
    );
}

/// Bug 14: `insert-file-contents` with REPLACE=t reports only the *net*
/// inserted chars after eliding unchanged head/tail affixes.  Re-reading
/// byte-identical content yields `(FILE 0)`; a partial change reports the
/// number of differing chars (here "world" -> 5).
#[test]
fn insert_file_contents_replace_reports_net_inserted_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let rep = scratch_dir().join("rep_fix8.txt");
    std::fs::write(&rep, b"hello world!").expect("write rep");
    let rep_str = rep.to_string_lossy().to_string();

    // Identical re-read: net inserted should be 0.
    let mut eval = Context::new();
    builtin_insert_file_contents(&mut eval, vec![Value::string(&rep_str)])
        .expect("initial insert should succeed");
    let result = builtin_insert_file_contents(
        &mut eval,
        vec![
            Value::string(&rep_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
        ],
    )
    .expect("replace re-read should succeed");
    let parts = crate::emacs_core::value::list_to_vec(&result).expect("list");
    assert_eq!(parts.len(), 2);
    assert_eq!(
        parts[1].as_int(),
        Some(0),
        "identical content under REPLACE reports 0 net inserted, not the full count"
    );

    // Partial change: buffer "hello WORLD!" replaced by file "hello world!" ->
    // only "world" differs, so net inserted is 5.
    let mut eval = Context::new();
    eval.buffers
        .current_buffer_mut()
        .expect("current buffer")
        .insert("hello WORLD!");
    let result = builtin_insert_file_contents(
        &mut eval,
        vec![
            Value::string(&rep_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
        ],
    )
    .expect("partial replace should succeed");
    let parts = crate::emacs_core::value::list_to_vec(&result).expect("list");
    assert_eq!(
        parts[1].as_int(),
        Some(5),
        "partial change reports 5 net inserted"
    );
    let buf = eval.buffers.current_buffer().expect("current buffer");
    assert_eq!(buf.buffer_string(), "hello world!");
}

#[test]
fn insert_file_contents_replace_advances_after_insertion_markers_like_gnu() {
    crate::test_utils::init_test_tracing();
    let path = scratch_dir().join("replace_marker_fix8.txt");
    std::fs::write(&path, "alpha 发布\nzeta release\nβeta publish\n")
        .expect("write replacement fixture");
    let path_str = path.to_string_lossy().to_string();

    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
              (insert "zeta release\nβeta publish\nalpha 发布\n")
              (goto-char (point-min))
              (search-forward "publish")
              (setq insert-file-contents-marker-probe
                    (copy-marker (point) t)))"#,
    )
    .expect("prepare insertion-type marker");

    builtin_insert_file_contents(
        &mut eval,
        vec![
            Value::string(&path_str),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
        ],
    )
    .expect("replace accessible buffer from file");

    assert_eq!(
        crate::emacs_core::format_eval_result(&eval.eval_str(
            r#"(list (buffer-string)
                     (point)
                     (marker-position insert-file-contents-marker-probe)
                     (marker-insertion-type insert-file-contents-marker-probe))"#,
        )),
        "OK (\"alpha 发布\nzeta release\nβeta publish\n\" 26 35 t)"
    );
}

/// Bug 9: `make-temp-name` decodes the constructed path through the file-name
/// coding system, so a non-ASCII PREFIX (here UTF-8 "é", 2 bytes) comes back as
/// a *multibyte* string where the 2 raw bytes collapse to 1 char — matching
/// GNU's `val = DECODE_FILE (val)`.
#[test]
fn make_temp_name_decodes_file_name_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let dir = scratch_dir();
    // PREFIX = "<dir>/t-é-" as raw file-name bytes (é = 0xC3 0xA9), unibyte.
    let mut prefix_bytes = dir.to_string_lossy().as_bytes().to_vec();
    prefix_bytes.extend_from_slice(b"/t-\xc3\xa9-");
    let prefix = crate::heap_types::LispString::from_unibyte(prefix_bytes);
    let prefix_char_len = prefix.schars();

    let mut eval = Context::new();
    // The full runtime derives this from the locale during loadup; a bare
    // context starts with nil, so set the GNU default explicitly.
    eval.set_variable(
        "default-file-name-coding-system",
        Value::symbol("utf-8-unix"),
    );

    let result = builtin_make_temp_name(&eval, vec![Value::heap_string(prefix)])
        .expect("make-temp-name should succeed");
    let result_str = result.as_lisp_string().expect("string result");
    assert!(
        result_str.is_multibyte(),
        "non-ASCII prefix must round-trip through DECODE_FILE to a multibyte string"
    );
    // 6 'X' chars are appended; the 2-byte "é" decodes to 1 char, so the char
    // length grows by 6 - (2 - 1) = 5 relative to the unibyte prefix.
    let delta = result_str.schars() as i64 - prefix_char_len as i64;
    assert_eq!(
        delta, 5,
        "delta should be 5 (6 X's minus the 1-byte é collapse)"
    );
}
