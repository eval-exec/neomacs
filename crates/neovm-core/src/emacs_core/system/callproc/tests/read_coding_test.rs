//! `call-process` must decode its child's output through the coding system GNU
//! resolves in `Fcall_process` (src/callproc.c:729-763), not through a fixed
//! UTF-8.
//!
//! The values pinned here were taken by RUNNING each probe under GNU Emacs
//! 31.0.90; 4194243 and 4194217 are the eight-bit characters for the raw bytes
//! 0xC3 and 0xA9, which is what a byte-faithful decode leaves in a multibyte
//! buffer.  Decoding the same two bytes as UTF-8 instead yields the single
//! character 233 (e-acute) and a buffer one character shorter — the collapse
//! that made `insert-directory` mislay every `dired-filename` property after a
//! non-ASCII file name (DIVERGENCES.md entry 128).

use crate::emacs_core::{Context, format_eval_result};

/// `printf` turns this argument into the five bytes `c a f 0xC3 0xA9`.
const PAYLOAD: &str = r#""caf\\303\\251""#;

fn find_bin(name: &str) -> String {
    for dir in ["/bin", "/usr/bin", "/run/current-system/sw/bin"] {
        let path = format!("{dir}/{name}");
        if std::path::Path::new(&path).exists() {
            return path;
        }
    }
    name.to_string()
}

/// Run BODY (an elisp form that writes the payload into the current buffer)
/// and report `(BUFFER-SIZE CHAR-AT-4 CHAR-AT-5)`.
fn probe(body: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let printf = find_bin("printf");
    // A fresh unit-test context has no `with-temp-buffer` macro; its current
    // buffer is empty, so the child's output lands at position 1 either way.
    let form = format!(
        r#"(progn
             (set-buffer-multibyte t)
             {body}
             (list (buffer-size) (char-after 4) (char-after 5)))"#
    );
    let form = form.replace("PRINTF", &printf).replace("PAYLOAD", PAYLOAD);
    format_eval_result(&eval.eval_str(&form))
}

#[cfg(unix)]
#[test]
fn call_process_output_defaults_to_default_process_coding_system() {
    // GNU src/callproc.c:748-749 — with no `coding-system-for-read` and no
    // matching alist entry, the car of `default-process-coding-system` decodes
    // the output.  Bound explicitly to the value a started editor holds,
    // because a bare unit-test context has not run `set-language-environment`.
    assert_eq!(
        probe(
            r#"(let ((default-process-coding-system '(utf-8-unix . utf-8-unix)))
                 (call-process "PRINTF" nil t nil PAYLOAD))"#
        ),
        "OK (4 233 nil)"
    );
}

#[cfg(unix)]
#[test]
fn call_process_output_with_no_coding_system_at_all_is_detected_not_copied() {
    // GNU src/coding.c:5675-5676 — `setup_coding_system` rewrites a nil coding
    // system to `undecided`, so the last resort DETECTS rather than copying
    // bytes.  Measured under GNU with `default-process-coding-system` nil.
    assert_eq!(
        probe(
            r#"(let ((default-process-coding-system nil))
                 (call-process "PRINTF" nil t nil PAYLOAD))"#
        ),
        "OK (4 233 nil)"
    );
}

#[cfg(unix)]
#[test]
fn call_process_output_honors_coding_system_for_read_no_conversion() {
    // GNU src/callproc.c:732-733 — `coding-system-for-read` wins outright.
    // `insert-directory` binds it to `no-conversion` so the `//DIRED//` byte
    // offsets index the buffer one-for-one.
    assert_eq!(
        probe(
            r#"(let ((coding-system-for-read 'no-conversion))
                 (call-process "PRINTF" nil t nil PAYLOAD))"#
        ),
        "OK (5 4194243 4194217)"
    );
}

#[cfg(unix)]
#[test]
fn call_process_output_honors_coding_system_for_read_binary_and_raw_text() {
    for coding in ["binary", "raw-text"] {
        assert_eq!(
            probe(&format!(
                r#"(let ((coding-system-for-read '{coding}))
                     (call-process "PRINTF" nil t nil PAYLOAD))"#
            )),
            "OK (5 4194243 4194217)",
            "coding-system-for-read {coding}"
        );
    }
}

#[cfg(unix)]
#[test]
fn call_process_output_honors_a_charset_coding_system_for_read() {
    // latin-1 maps each byte to the same code point, so the two payload bytes
    // stay two characters (195, 169) instead of collapsing to one.
    assert_eq!(
        probe(
            r#"(let ((coding-system-for-read 'latin-1))
                 (call-process "PRINTF" nil t nil PAYLOAD))"#
        ),
        "OK (5 195 169)"
    );
}

#[cfg(unix)]
#[test]
fn call_process_output_honors_the_coding_systems_eol_conversion() {
    // GNU runs `decode_eol` after every decoder, so a `-dos` read coding turns
    // the child's CR LF into a bare LF.
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let printf = find_bin("printf");
    let observed = eval.eval_str(&format!(
        r#"(progn
             (set-buffer-multibyte t)
             (let ((coding-system-for-read 'utf-8-dos))
               (call-process "{printf}" nil t nil "a\\r\\nb\\r\\n"))
             (list (buffer-size) (buffer-string)))"#
    ));
    assert_eq!(format_eval_result(&observed), "OK (4 \"a\nb\n\")");
}

#[cfg(unix)]
#[test]
fn call_process_output_falls_back_to_default_process_coding_system() {
    // GNU src/callproc.c:748-749.
    assert_eq!(
        probe(
            r#"(let ((default-process-coding-system '(binary . binary)))
                 (call-process "PRINTF" nil t nil PAYLOAD))"#
        ),
        "OK (5 4194243 4194217)"
    );
}

#[cfg(unix)]
#[test]
fn call_process_output_consults_process_coding_system_alist() {
    // GNU src/callproc.c:736-747 — `find-operation-coding-system` for the
    // `call-process` operation matches `process-coding-system-alist` against
    // PROGRAM, and its car is the decode coding system.
    let printf = find_bin("printf");
    assert_eq!(
        probe(&format!(
            r#"(let ((process-coding-system-alist '(("{printf}" binary . binary))))
                 (call-process "PRINTF" nil t nil PAYLOAD))"#
        )),
        "OK (5 4194243 4194217)"
    );
}

#[cfg(unix)]
#[test]
fn coding_system_for_read_beats_process_coding_system_alist() {
    let printf = find_bin("printf");
    assert_eq!(
        probe(&format!(
            r#"(let ((process-coding-system-alist '(("{printf}" utf-8 . utf-8)))
                     (coding-system-for-read 'binary))
                 (call-process "PRINTF" nil t nil PAYLOAD))"#
        )),
        "OK (5 4194243 4194217)"
    );
}

#[cfg(unix)]
#[test]
fn call_process_signals_coding_system_error_for_an_unknown_read_coding() {
    // GNU src/callproc.c:753 — `Fcheck_coding_system` on the resolved value.
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let printf = find_bin("printf");
    let observed = eval.eval_str(&format!(
        r#"(condition-case e
               (let ((coding-system-for-read 'no-such-coding-xyz))
                 (call-process "{printf}" nil t nil "hi"))
             (error (list (car e) (car (cdr e)))))"#
    ));
    assert_eq!(
        format_eval_result(&observed),
        "OK (coding-system-error no-such-coding-xyz)"
    );
}

/// Run BODY with a UNIBYTE current buffer and report `(BUFFER-SIZE BYTES)`.
fn unibyte_probe(body: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let printf = find_bin("printf");
    let form = format!(
        r#"(progn
             (set-buffer-multibyte nil)
             {body}
             (list (buffer-size) (append (buffer-string) nil)))"#
    );
    let form = form.replace("PRINTF", &printf).replace("PAYLOAD", PAYLOAD);
    format_eval_result(&eval.eval_str(&form))
}

#[cfg(unix)]
#[test]
fn a_unibyte_destination_buffer_gets_no_character_code_conversion() {
    // GNU src/callproc.c:754-759 — "In unibyte mode, character code conversion
    // should not take place but EOL conversion should."  Every coding system
    // therefore leaves the same five raw bytes in a unibyte buffer.
    for coding in [
        "'(utf-8-unix . utf-8-unix)-DEFAULT",
        "'utf-8",
        "'no-conversion",
        "'latin-1",
    ] {
        let body = if let Some(default) = coding.strip_suffix("-DEFAULT") {
            format!(
                r#"(let ((default-process-coding-system {default}))
                     (call-process "PRINTF" nil t nil PAYLOAD))"#
            )
        } else {
            format!(
                r#"(let ((coding-system-for-read {coding}))
                     (call-process "PRINTF" nil t nil PAYLOAD))"#
            )
        };
        assert_eq!(
            unibyte_probe(&body),
            "OK (5 (99 97 102 195 169))",
            "unibyte destination, {coding}"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_unibyte_destination_buffer_still_gets_the_eol_conversion() {
    // The other half of src/callproc.c:754-759: GNU downgrades to the `raw-text`
    // subsidiary carrying the resolved coding's OWN end-of-line type, so a
    // `-dos` read coding still folds CR LF while a `-unix` one does not.
    crate::test_utils::init_test_tracing();
    for (coding, expected) in [
        ("utf-8-dos", "OK (4 (97 10 98 10))"),
        ("utf-8-unix", "OK (6 (97 13 10 98 13 10))"),
        ("no-conversion", "OK (6 (97 13 10 98 13 10))"),
    ] {
        let mut eval = Context::new();
        let printf = find_bin("printf");
        let observed = eval.eval_str(&format!(
            r#"(progn
                 (set-buffer-multibyte nil)
                 (let ((coding-system-for-read '{coding}))
                   (call-process "{printf}" nil t nil "a\\r\\nb\\r\\n"))
                 (list (buffer-size) (append (buffer-string) nil)))"#
        ));
        assert_eq!(
            format_eval_result(&observed),
            expected,
            "unibyte destination EOL, {coding}"
        );
    }
}

#[cfg(unix)]
#[test]
fn call_process_region_output_honors_coding_system_for_read() {
    // `Fcall_process_region` delegates to `call_process`, so the same chain
    // applies (GNU src/callproc.c:1149-1163).
    assert_eq!(
        probe(
            r#"(let ((coding-system-for-read 'no-conversion))
                 (call-process-region "" nil "PRINTF" nil t nil PAYLOAD))"#
        ),
        "OK (5 4194243 4194217)"
    );
}

#[cfg(unix)]
#[test]
fn call_process_output_eol_is_detected_for_an_undecided_eol_coding_system() {
    // A coding system has TWO axes, and `coding-system-for-read` sets both.
    // `raw-text` drops character-code conversion but its eol_type is a VECTOR,
    // so GNU's `decode_eol` (src/coding.c:6783-6806) scans the decoded output
    // and converts the child's CR LF; `binary` and `no-conversion` are `Qunix`
    // on that axis and copy every CR through.  Neomacs read the undecided state
    // as "no conversion" and left CR LF in the buffer for all of them -- the
    // residual DIVERGENCES.md entry 131 recorded, and the reason entry 128's
    // `raw-text` row was understated.
    //
    // The child writes `caf <c3> <a9> CR LF x CR LF`.  Every expected value was
    // taken by running the probe under GNU Emacs 31.0.90; 4194243 and 4194217
    // are the eight-bit characters for the raw bytes 0xC3 and 0xA9.
    let printf = find_bin("printf");
    for (coding, expected) in [
        ("raw-text", "OK (99 97 102 4194243 4194217 10 120 10)"),
        ("undecided", "OK (99 97 102 233 10 120 10)"),
        ("utf-8", "OK (99 97 102 233 10 120 10)"),
        ("latin-1", "OK (99 97 102 195 169 10 120 10)"),
        ("utf-8-dos", "OK (99 97 102 233 10 120 10)"),
        // The three that must NOT move: both convert-nothing codings, and the
        // raw-text subsidiary that names `unix` outright.
        ("binary", "OK (99 97 102 4194243 4194217 13 10 120 13 10)"),
        (
            "no-conversion",
            "OK (99 97 102 4194243 4194217 13 10 120 13 10)",
        ),
        (
            "raw-text-unix",
            "OK (99 97 102 4194243 4194217 13 10 120 13 10)",
        ),
    ] {
        crate::test_utils::init_test_tracing();
        let mut eval = Context::new();
        let observed = eval.eval_str(&format!(
            r#"(progn
                 (set-buffer-multibyte t)
                 (let ((coding-system-for-read '{coding}))
                   (call-process "{printf}" nil t nil "caf\\303\\251\\r\\nx\\r\\n"))
                 (append (buffer-string) nil))"#
        ));
        assert_eq!(
            format_eval_result(&observed),
            expected,
            "coding-system-for-read {coding}"
        );
    }
}

/// DIVERGENCES.md entry 143: `call-process` honours `inhibit-eol-conversion`
/// through the same `struct coding_system` its output is decoded with.
///
/// `Fcall_process` reads the child into `process_coding` and then reports
/// `CODING_ID_NAME (process_coding.id)` (src/callproc.c:913).  With the flag
/// set, `decode_eol` returned before `adjust_coding_eol_type`
/// (src/coding.c:6767), so the id never moved and the reported name is the one
/// the chain resolved -- `utf-8`, not `utf-8-dos`.
///
/// Measured under GNU Emacs 31.0.90 (`tmp/pw49/gnu.txt`).
#[cfg(unix)]
#[test]
fn call_process_output_honours_inhibit_eol_conversion() {
    crate::test_utils::init_test_tracing();
    let printf = find_bin("printf");
    for (coding, inhibit, expected) in [
        ("utf-8-dos", "t", "OK ((97 13 10 98 13 10) utf-8-dos)"),
        ("utf-8-dos", "nil", "OK ((97 10 98 10) utf-8-dos)"),
        ("utf-8", "t", "OK ((97 13 10 98 13 10) utf-8)"),
        ("utf-8", "nil", "OK ((97 10 98 10) utf-8-dos)"),
        ("undecided", "t", "OK ((97 13 10 98 13 10) undecided)"),
        ("undecided", "nil", "OK ((97 10 98 10) undecided-dos)"),
    ] {
        let mut eval = Context::new();
        let observed = eval.eval_str(&format!(
            r#"(progn
                 (setq last-coding-system-used 'unset)
                 (let ((inhibit-eol-conversion {inhibit})
                       (coding-system-for-read '{coding}))
                   (call-process "{printf}" nil t nil "a\\r\\nb\\r\\n"))
                 (list (append (buffer-string) nil) last-coding-system-used))"#
        ));
        assert_eq!(
            format_eval_result(&observed),
            expected,
            "coding {coding}, inhibit-eol-conversion {inhibit}"
        );
    }
}
