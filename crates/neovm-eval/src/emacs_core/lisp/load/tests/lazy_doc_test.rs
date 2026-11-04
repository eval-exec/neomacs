//! GNU `load-force-doc-strings` tests, derived from GNU 31.1 probes
//! (`--batch` runs recorded in `tmp/jitcall12/gnu/`): the dotted-completion
//! conversion, its identity requirement, absolute/negative positions,
//! `^A`/NUL quoting, GNU's nil and "Cannot open" answers, the cross-form
//! saved-`#@` block ring, the jka-compr compressed-load activator, and pdump
//! fidelity of the converted unibyte docstring.

use super::*;
use crate::emacs_core::eval::Context;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

fn rendered(eval: &mut Context, form: &str) -> String {
    super::tests::eval_rendered(eval, form)
}

fn assert_rendered(eval: &mut Context, form: &str, expected: &str) {
    assert_eq!(rendered(eval, form), expected, "form: {form}");
}

fn bootstrap() -> Context {
    let mut eval = create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    eval
}

/// The `;ELC` magic, its version bytes and a header comment. The loader skips
/// these before the reader sees the bytes, but the reader's saved `#@`
/// positions are absolute file positions.
fn elc_header() -> Vec<u8> {
    b";ELC\x1f\x00\x00\x00\n;;; lazy doc fixture\n\n".to_vec()
}

/// Append a GNU `#@` record and return the absolute position of its body (the
/// value a `(#$ . POS)` reference names). `body` must not contain a raw `^_`.
fn push_record(file: &mut Vec<u8>, body: &[u8]) -> u64 {
    // GNU counts the separator and the `^_` terminator: skipping consumes
    // `count - 1` bytes after the space.
    let count = body.len() + 2;
    file.extend_from_slice(format!("#@{count} ").as_bytes());
    let position = file.len() as u64;
    file.extend_from_slice(body);
    file.push(0x1f);
    position
}

fn push_form(file: &mut Vec<u8>, form: &str) {
    file.extend_from_slice(form.as_bytes());
    file.push(b'\n');
}

/// A compiled-file fixture whose blocks are each separated from the form that
/// references them by one intervening top-level form: the first block is
/// skipped while reading form 1 and referenced while reading form 3.
fn saved_block_elc() -> Vec<u8> {
    let mut file = elc_header();
    let body_a = b"Alpha\x01\x01Beta\x010Nul\x01_End";
    let pos_a = push_record(&mut file, body_a);
    push_form(&mut file, "(setq lazy-probe-ignored-a 1)");
    let body_b = b"Second\x01\x01Record";
    let pos_b = push_record(&mut file, body_b);
    push_form(&mut file, "(setq lazy-probe-ignored-b 2)");
    push_form(
        &mut file,
        &format!("(setq lazy-probe-positive '(#$ . {pos_a}))"),
    );
    push_form(
        &mut file,
        &format!("(setq lazy-probe-negative '(#$ . -{pos_b}))"),
    );
    let body_c = b"Bad\x01xEscape";
    let pos_c = push_record(&mut file, body_c);
    push_form(&mut file, "(setq lazy-probe-ignored-c 3)");
    push_form(
        &mut file,
        &format!("(setq lazy-probe-tolerant '(#$ . {pos_c}))"),
    );
    file
}

fn load_form(path: &Path, forced: bool) -> String {
    let path = path.display();
    if forced {
        format!("(let ((load-force-doc-strings t)) (load \"{path}\" nil t t))")
    } else {
        format!("(load \"{path}\" nil t t)")
    }
}

const UNQUOTED_BYTES: &str = "(65 108 112 104 97 1 66 101 116 97 0 78 117 108 31 69 110 100)";

#[test]
fn elc_saved_blocks_convert_across_top_level_forms() {
    crate::test_utils::init_test_tracing();
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("lazy-doc-saved.elc");
    fs::write(&path, saved_block_elc()).expect("write compiled fixture");
    let mut eval = bootstrap();
    let load = load_form(&path, true);
    assert_rendered(&mut eval, &load, "OK t");

    assert_rendered(
        &mut eval,
        "(list (stringp lazy-probe-positive) (multibyte-string-p lazy-probe-positive) (string-to-list lazy-probe-positive))",
        &format!("OK (t nil {UNQUOTED_BYTES})"),
    );
    assert_rendered(
        &mut eval,
        "(list (stringp lazy-probe-negative) (multibyte-string-p lazy-probe-negative) (string-to-list lazy-probe-negative))",
        "OK (t nil (83 101 99 111 110 100 1 82 101 99 111 114 100))",
    );
    // The saved-block arm tolerates an unknown ^A escape; the file fallback
    // would signal `Invalid data in documentation file`, so this proves the
    // block was kept across the intervening forms.
    assert_rendered(
        &mut eval,
        "(list (stringp lazy-probe-tolerant) (multibyte-string-p lazy-probe-tolerant) (string-to-list lazy-probe-tolerant))",
        "OK (t nil (66 97 100 120 69 115 99 97 112 101))",
    );
}

#[test]
fn force_off_leaves_the_dotted_pair_unchanged() {
    crate::test_utils::init_test_tracing();
    let dir = tempdir().expect("tempdir");
    let mut data = elc_header();
    let position = push_record(&mut data, b"Alpha\x01\x01Beta\x010Nul\x01_End");
    push_form(
        &mut data,
        &format!("(setq lazy-probe-off '(#$ . {position}))"),
    );
    let path = dir.path().join("lazy-doc-off.elc");
    fs::write(&path, data).expect("write compiled fixture");
    let mut eval = bootstrap();
    let load = load_form(&path, false);
    assert_rendered(&mut eval, &load, "OK t");

    assert_rendered(
        &mut eval,
        "(list (consp lazy-probe-off) (stringp (car lazy-probe-off)) (integerp (cdr lazy-probe-off)))",
        "OK (t t t)",
    );
    assert_rendered(
        &mut eval,
        &format!("(cdr lazy-probe-off)"),
        &format!("OK {position}"),
    );
}

#[test]
fn read_from_string_admits_only_the_live_identity_and_a_fixnum_cdr() {
    crate::test_utils::init_test_tracing();
    let mut eval = bootstrap();

    // A literal string with the same bytes is not `BASE_EQ` to the live
    // load-file-name, so it stays ordinary data; `#$` is that exact object and
    // converts (the missing file answers GNU's sentence).
    assert_rendered(
        &mut eval,
        r#"(let ((load-file-name "jitcall12-equal.elc") (load-force-doc-strings t))
             (let ((literal (read-from-string "(\"jitcall12-equal.elc\" . 12)"))
                   (hash (read-from-string "(#$ . 12)")))
               (list (consp (car literal)) (stringp (car hash))
                     (multibyte-string-p (car hash))
                     (string-prefix-p "Cannot open doc string file" (car hash))
                     (cdr hash))))"#,
        "OK (t t nil t 9)",
    );

    // Force off: `#$` still expands to load-file-name, and the pair survives.
    assert_rendered(
        &mut eval,
        r#"(let ((load-file-name "jitcall12-equal.elc") (load-force-doc-strings nil))
             (let ((r (read-from-string "(#$ . 12)")))
               (list (consp (car r)) (stringp (car (car r))) (cdr (car r)))))"#,
        "OK (t t 12)",
    );

    // Ordinary dotted data, a three-element dotted list, and a nil
    // load-file-name are never admitted.
    assert_rendered(
        &mut eval,
        r#"(let ((load-file-name "jitcall12-equal.elc") (load-force-doc-strings t))
             (list (car (read-from-string "(a . 12)"))
                   (let ((r (car (read-from-string "(#$ a . 12)"))))
                     (list (stringp (car r)) (eq (car (cdr r)) 'a)))
                   (let ((load-file-name nil))
                     (car (read-from-string "(#$ . 12)")))))"#,
        "OK ((a . 12) (t t) (nil . 12))",
    );
}

#[test]
fn callable_reader_policy_flip_before_completion_is_observed() {
    crate::test_utils::init_test_tracing();
    let mut eval = bootstrap();
    // GNU calls a function stream with an optional unread argument. This
    // reader starts with the force flag nil and flips it while returning the
    // closing paren, so only a live consultation at the completion boundary
    // can convert the pair (GNU 31.1 answers the same cannot-open string).
    assert_rendered(
        &mut eval,
        r#"(let* ((text "(#$ . 12)")
                  (chars (append (string-to-list text) (list -1)))
                  (i 0)
                  (load-file-name "jitcall12-callable.elc")
                  (load-force-doc-strings nil)
                  (reader (lambda (&optional unread)
                            (if unread
                                (progn (setq i (1- i)) nil)
                              (let ((ch (nth i chars)))
                                (setq i (1+ i))
                                (when (= i (length text))
                                  (setq load-force-doc-strings t))
                                ch)))))
             (let ((r (read reader)))
               (list (stringp r) (multibyte-string-p r)
                     (string-prefix-p "Cannot open doc string file" r))))"#,
        "OK (t nil t)",
    );
}

#[test]
fn file_fallback_reads_absolute_and_negative_positions_as_unibyte() {
    crate::test_utils::init_test_tracing();
    let dir = tempdir().expect("tempdir");
    // A plain file holding a valid record; `read-from-string` has no saved
    // blocks, so both references take GNU's `get_doc_string (val, 1)` arm.
    let mut data = b";;; lazy doc fallback\n".to_vec();
    let position = push_record(&mut data, b"Alpha\x01\x01Beta\x010Nul\x01_End");
    let path = dir.path().join("lazy-doc-fallback.el");
    fs::write(&path, data).expect("write fallback fixture");
    let mut eval = bootstrap();

    let form = format!(
        r#"(let ((load-file-name "{path}") (load-force-doc-strings t))
             (let ((positive (read-from-string "(#$ . {position})"))
                   (negative (read-from-string "(#$ . -{position})")))
               (list (stringp (car positive)) (multibyte-string-p (car positive))
                     (string-to-list (car positive))
                     (equal (car positive) (car negative))
                     (multibyte-string-p (car negative)))))"#,
        path = path.display(),
    );
    assert_rendered(
        &mut eval,
        &form,
        &format!("OK (t nil {UNQUOTED_BYTES} t nil)"),
    );

    // A position whose preceding bytes are not a record header, and one past
    // EOF, are GNU's unresolved answers (nil), not errors or panics.
    let missing = format!(
        r#"(let ((load-file-name "{path}") (load-force-doc-strings t))
             (list (null (car (read-from-string "(#$ . 3)")))
                   (null (car (read-from-string "(#$ . 99999999)")))
                   (null (car (read-from-string "(#$ . -3)")))))"#,
        path = path.display(),
    );
    assert_rendered(&mut eval, &missing, "OK (t t t)");
}

#[test]
fn malformed_file_record_signals_the_documented_error() {
    crate::test_utils::init_test_tracing();
    let dir = tempdir().expect("tempdir");
    let mut data = b";;; malformed record\n".to_vec();
    let position = push_record(&mut data, b"Bad\x01xEscape");
    let path = dir.path().join("lazy-doc-malformed.el");
    fs::write(&path, data).expect("write malformed fixture");
    let mut eval = bootstrap();
    let form = format!(
        r#"(let ((load-file-name "{path}") (load-force-doc-strings t))
             (condition-case err
                 (read-from-string "(#$ . {position})")
               (error err)))"#,
        path = path.display(),
    );
    assert_rendered(
        &mut eval,
        &form,
        r#"OK (error "Invalid data in documentation file -- ^A followed by code 170")"#,
    );
}

/// GNU 31.1's own byte-compiled fixture (`byte-compile-file` with the default
/// dynamic docstrings): one `#@75` block and `(defalias 'jitcall12-probe-fn
/// #[257 "T\207" [] 2 (#$ . 83)])`. It is committed here as bytes so the test
/// exercises a real lazy reference without depending on a compiler at test time.
const LAZY_DOC_ELC: &[u8] = &[
    0x3b, 0x45, 0x4c, 0x43, 0x1f, 0x00, 0x00, 0x00, 0x0a, 0x3b, 0x3b, 0x3b, 0x20, 0x43, 0x6f, 0x6d,
    0x70, 0x69, 0x6c, 0x65, 0x64, 0x0a, 0x3b, 0x3b, 0x3b, 0x20, 0x69, 0x6e, 0x20, 0x45, 0x6d, 0x61,
    0x63, 0x73, 0x20, 0x76, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e, 0x20, 0x33, 0x31, 0x2e, 0x31, 0x0a,
    0x3b, 0x3b, 0x3b, 0x20, 0x77, 0x69, 0x74, 0x68, 0x20, 0x61, 0x6c, 0x6c, 0x20, 0x6f, 0x70, 0x74,
    0x69, 0x6d, 0x69, 0x7a, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x73, 0x2e, 0x0a, 0x0a, 0x0a, 0x23, 0x40,
    0x37, 0x35, 0x20, 0x44, 0x6f, 0x63, 0x73, 0x74, 0x72, 0x69, 0x6e, 0x67, 0x20, 0x66, 0x6f, 0x72,
    0x20, 0x6a, 0x69, 0x74, 0x63, 0x61, 0x6c, 0x6c, 0x31, 0x32, 0x2d, 0x70, 0x72, 0x6f, 0x62, 0x65,
    0x2d, 0x66, 0x6e, 0x3a, 0x20, 0x41, 0x6c, 0x70, 0x68, 0x61, 0x20, 0x42, 0x65, 0x74, 0x61, 0x20,
    0x47, 0x61, 0x6d, 0x6d, 0x61, 0x20, 0x44, 0x65, 0x6c, 0x74, 0x61, 0x20, 0x45, 0x70, 0x73, 0x69,
    0x6c, 0x6f, 0x6e, 0x2e, 0x0a, 0x0a, 0x28, 0x66, 0x6e, 0x20, 0x58, 0x29, 0x1f, 0x0a, 0x28, 0x64,
    0x65, 0x66, 0x61, 0x6c, 0x69, 0x61, 0x73, 0x20, 0x27, 0x6a, 0x69, 0x74, 0x63, 0x61, 0x6c, 0x6c,
    0x31, 0x32, 0x2d, 0x70, 0x72, 0x6f, 0x62, 0x65, 0x2d, 0x66, 0x6e, 0x20, 0x23, 0x5b, 0x32, 0x35,
    0x37, 0x20, 0x22, 0x54, 0x5c, 0x32, 0x30, 0x37, 0x22, 0x20, 0x5b, 0x5d, 0x20, 0x32, 0x20, 0x28,
    0x23, 0x24, 0x20, 0x2e, 0x20, 0x38, 0x33, 0x29, 0x5d, 0x29, 0x0a, 0x28, 0x70, 0x72, 0x6f, 0x76,
    0x69, 0x64, 0x65, 0x20, 0x27, 0x6a, 0x69, 0x74, 0x63, 0x61, 0x6c, 0x6c, 0x31, 0x32, 0x2d, 0x64,
    0x6f, 0x63, 0x29, 0x0a,
];

const PROBE_DOC_LENGTH: usize = 73;
const PROBE_DOC_PREFIX: &str =
    "Docstring for jitcall12-probe-fn: Alpha Beta Gamma Delta Epsilon.\n\n(fn X)";

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).expect("gzip fixture");
    encoder.finish().expect("gzip fixture")
}

fn probe_doc_check() -> String {
    format!(
        r#"(let ((raw (aref (symbol-function 'jitcall12-probe-fn) 4)))
             (list (stringp raw) (multibyte-string-p raw) (length raw)
                   (equal raw "{PROBE_DOC_PREFIX}")))"#
    )
}

#[test]
fn compressed_elc_load_inlines_docstrings_through_jka_compr() {
    crate::test_utils::init_test_tracing();
    let dir = tempdir().expect("tempdir");
    let gz = dir.path().join("jitcall12-doc.elc.gz");
    fs::write(&gz, gzip(LAZY_DOC_ELC)).expect("write gz fixture");
    let mut eval = bootstrap();
    assert_rendered(&mut eval, "(require 'jka-compr)", "OK jka-compr");
    let load = format!(r#"(load "{}" nil t t)"#, gz.display());
    assert_rendered(&mut eval, &load, "OK t");
    // jka-compr binds `load-force-doc-strings` around the real load; the raw
    // function slot must now hold the unquoted unibyte docstring.
    assert_rendered(
        &mut eval,
        &probe_doc_check(),
        &format!("OK (t nil {PROBE_DOC_LENGTH} t)"),
    );
}

#[test]
fn uncompressed_elc_load_inlines_docstrings_with_force_binding() {
    crate::test_utils::init_test_tracing();
    let dir = tempdir().expect("tempdir");
    let elc = dir.path().join("jitcall12-doc.elc");
    fs::write(&elc, LAZY_DOC_ELC).expect("write elc fixture");
    let mut eval = bootstrap();
    let load = format!(
        r#"(let ((load-force-doc-strings t)) (load "{}" nil t t))"#,
        elc.display()
    );
    assert_rendered(&mut eval, &load, "OK t");
    assert_rendered(
        &mut eval,
        &probe_doc_check(),
        &format!("OK (t nil {PROBE_DOC_LENGTH} t)"),
    );
}

#[test]
fn pdump_round_trip_preserves_the_converted_unibyte_docstring() {
    crate::test_utils::init_test_tracing();
    let dir = tempdir().expect("tempdir");
    let elc = dir.path().join("jitcall12-doc.elc");
    fs::write(&elc, LAZY_DOC_ELC).expect("write elc fixture");
    let mut eval = bootstrap();
    let load = format!(
        r#"(let ((load-force-doc-strings t)) (load "{}" nil t t))"#,
        elc.display()
    );
    assert_rendered(&mut eval, &load, "OK t");

    let snapshot = crate::emacs_core::pdump::snapshot_evaluator(&eval);
    drop(eval);
    let mut restored =
        crate::emacs_core::pdump::restore_snapshot(&snapshot).expect("restore the snapshot");
    assert_rendered(
        &mut restored,
        &probe_doc_check(),
        &format!("OK (t nil {PROBE_DOC_LENGTH} t)"),
    );
}
