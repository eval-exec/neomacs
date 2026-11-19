//! Peer 5833 (c): every reader entry point interns the symbol tokens it
//! reads, with the obarray membership the token needs.
//!
//! GNU's reader calls `Fintern` for every symbol token it consumes
//! (`src/lread.c:4674-4709`), and each entry point funnels through that same
//! `read1`/`readbyte` parser: `read-from-string`, `read` from a buffer or a
//! marker, a `load` of `.el` source, and the constants vector of a `.elc`
//! form. A token that only ever appears as runtime string data (or inside a
//! file written at runtime) can therefore become an obarray member only if
//! the entry point under test interns it.
//!
//! Attested GNU 31.1 with one fresh invocation per entry point on both
//! oracles (probe: `tmp/w1a/wired/read-entry-probe.el`):
//!
//! * linux-gtk3 `a360712c9d27-gsettings-treesit`:
//!   `(("neomacs-read-audit-from-string" . t)
//!     ("neomacs-read-audit-from-buffer" . t)
//!     ("neomacs-read-audit-from-marker" . t)
//!     ("neomacs-read-audit-from-load-el" . t)
//!     ("neomacs-read-audit-elc-token" . t)
//!     ("neomacs-read-audit-untouched"))`
//! * macos-ns `a360712c9d272d950d8d8255ef74570f7e90b7d9` (macmini): identical.
//!
//! The control row shows the strings themselves do not intern: only the
//! operation under test makes `intern-soft` answer.

/// Embed a filesystem path in an Elisp string literal.
fn elisp_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Run one audit form and format its result while the evaluator is alive:
/// `Value`s point into the evaluator's heap, so the context must outlive
/// `format_eval_result`.
fn assert_audit(src: &str, expected: &str, why: &str) {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval.eval_str(src);
    assert_eq!(
        crate::emacs_core::format_eval_result(&result),
        expected,
        "{why}"
    );
}

#[test]
fn read_from_string_interns_the_read_token() {
    assert_audit(
        r#"(let* ((name "neomacs-read-audit-from-string")
                  (value (car (read-from-string name))))
             (list (and (intern-soft name) t)
                   (eq value (intern-soft name))
                   (intern-soft "neomacs-read-audit-untouched")))"#,
        "OK (t t nil)",
        "read-from-string must intern the token it reads; the untouched control \
         must stay absent",
    );
}

#[test]
fn read_from_buffer_interns_the_read_token() {
    assert_audit(
        r#"(let ((name "neomacs-read-audit-from-buffer")
                 value)
             (with-temp-buffer
               (insert name)
               (goto-char (point-min))
               (setq value (read (current-buffer))))
             (list (and (intern-soft name) t)
                   (eq value (intern-soft name))))"#,
        "OK (t t)",
        "read from a buffer must intern the token it reads",
    );
}

#[test]
fn read_from_marker_interns_the_read_token() {
    assert_audit(
        r#"(let ((name "neomacs-read-audit-from-marker")
                 value marker)
             (with-temp-buffer
               (insert name)
               (setq marker (copy-marker (point-min)))
               (setq value (read marker)))
             (list (and (intern-soft name) t)
                   (eq value (intern-soft name))))"#,
        "OK (t t)",
        "read from a marker must intern the token it reads",
    );
}

#[test]
fn load_of_el_source_interns_the_read_token() {
    let dir = tempfile::tempdir().expect("tempdir for the .el audit file");
    let file = dir.path().join("read-audit-source.el");
    std::fs::write(
        &file,
        "(defconst neomacs-read-audit-load-el-value 'neomacs-read-audit-from-load-el)\n",
    )
    .expect("write the .el audit file");
    let src = format!(
        r#"(progn
             (load {file} nil nil t)
             (let ((name "neomacs-read-audit-from-load-el"))
               (list (and (intern-soft name) t)
                     (eq neomacs-read-audit-load-el-value (intern-soft name)))))"#,
        file = elisp_string(&file.to_string_lossy())
    );
    assert_audit(
        &src,
        "OK (t t)",
        "loading .el source must intern the tokens its reader reads",
    );
}

#[test]
fn load_of_elc_constants_interns_the_constant_token() {
    let dir = tempfile::tempdir().expect("tempdir for the .elc audit file");
    let file = dir.path().join("read-audit-bytecode.elc");
    // GNU's own header shape (`;ELC`, byte-compiler version 0x1F, NUL
    // padding, a `;;; in Emacs version` line matching
    // `bytecomp-version-regexp`, src/lread.c:5794), then one form whose
    // constants vector holds the token. `\300\207` pushes constant 0 and
    // returns it, so the loaded constant and the obarray name must be `eq`.
    let content = concat!(
        ";ELC\x1f\0\0\0\n",
        ";;; Compiled\n",
        ";;; in Emacs version 31.1\n",
        "(defconst neomacs-read-audit-elc-value ",
        "(byte-code \"\\300\\207\" [neomacs-read-audit-elc-token] 1))\n",
    );
    std::fs::write(&file, content).expect("write the .elc audit file");
    let src = format!(
        r#"(progn
             (load {file} nil nil t)
             (let ((name "neomacs-read-audit-elc-token"))
               (list (and (intern-soft name) t)
                     (eq neomacs-read-audit-elc-value (intern-soft name)))))"#,
        file = elisp_string(&file.to_string_lossy())
    );
    assert_audit(
        &src,
        "OK (t t)",
        "loading a .elc must intern the tokens in its constants vectors",
    );
}
