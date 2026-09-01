//! Oracle parity tests for GNU autoload semantics.
//!
//! GNU implements `autoload` and `autoload-do-load` in `src/eval.c`;
//! `autoloadp` is Lisp in `lisp/subr.el` and is exactly an `eq` check against
//! `(car-safe OBJECT)`.  These tests cover the user-visible function-cell
//! shape and error ordering without depending on loading real files.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use std::path::PathBuf;

fn autoload_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/autoload")
}

#[test]
fn oracle_autoloadp_uses_interned_autoload_car_safe_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((uninterned (make-symbol "autoload")))
  (list
   (autoloadp '(autoload "file" "doc" t nil))
   (autoloadp '(autoload . dotted-tail))
   (autoloadp (cons 'autoload nil))
   (autoloadp (cons uninterned nil))
   (autoloadp nil)
   (autoloadp 42)
   (autoloadp "autoload")))
"#;

    let expect = expect_test::expect![r#""OK (t t t nil nil nil nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_autoload_preserves_existing_real_definition_and_replaces_autoloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (fset 'neomacs--oracle-autoload-target
            (lambda () 'real-definition))
      (let ((real-cell (symbol-function 'neomacs--oracle-autoload-target)))
        (list
         (autoload 'neomacs--oracle-autoload-target
           "ignored-file" "Ignored doc." t)
         (eq (symbol-function 'neomacs--oracle-autoload-target) real-cell)
         (neomacs--oracle-autoload-target)
         (fmakunbound 'neomacs--oracle-autoload-target)
         (autoload 'neomacs--oracle-autoload-target
           "first-file" "First doc." nil 'macro)
         (symbol-function 'neomacs--oracle-autoload-target)
         (autoload 'neomacs--oracle-autoload-target
           "second-file" "Second doc." t 'keymap)
         (symbol-function 'neomacs--oracle-autoload-target))))
  (when (fboundp 'neomacs--oracle-autoload-target)
    (fmakunbound 'neomacs--oracle-autoload-target)))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil t real-definition neomacs--oracle-autoload-target neomacs--oracle-autoload-target (autoload \"first-file\" \"First doc.\" nil macro) neomacs--oracle-autoload-target (autoload \"second-file\" \"Second doc.\" t keymap))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_autoload_argument_errors_and_function_cell_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (list
     (condition-case err
         (autoload 42 "file")
       (error (list (car err) (cdr err))))
     (condition-case err
         (autoload 'neomacs--oracle-autoload-bad-file 42)
       (error (list (car err) (cdr err))))
     (fboundp 'neomacs--oracle-autoload-bad-file)
     (autoload 'neomacs--oracle-autoload-good
       "good-file" nil '(mode-a mode-b) t)
     (let ((cell (symbol-function 'neomacs--oracle-autoload-good)))
       (list
        (autoloadp cell)
        (nth 1 cell)
        (nth 2 cell)
        (nth 3 cell)
        (nth 4 cell))))
  (dolist (sym '(neomacs--oracle-autoload-bad-file
                 neomacs--oracle-autoload-good))
    (when (fboundp sym)
      (fmakunbound sym))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (symbolp 42)) (wrong-type-argument (stringp 42)) nil neomacs--oracle-autoload-good (t \"good-file\" nil (mode-a mode-b) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_autoload_do_load_macro_only_ordering_without_file_load() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((function-autoload '(autoload "missing-function-file" nil nil nil))
      (macro-autoload '(autoload "missing-macro-file" nil nil macro))
      (t-autoload '(autoload "missing-t-file" nil nil t)))
  (list
   (autoload-do-load 17 'ignored 'macro)
   (eq (autoload-do-load function-autoload 42 'macro)
       function-autoload)
   (condition-case err
       (autoload-do-load macro-autoload 42 'macro)
     (error (list (car err) (cdr err))))
   (condition-case err
       (autoload-do-load t-autoload 42 'macro)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![
        r#""OK (17 t (wrong-type-argument (symbolp 42)) (wrong-type-argument (symbolp 42)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_autoload_do_load_macro_only_requires_literal_macro_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((function-autoload '(autoload "missing-function-file" nil nil nil)))
  (list
   (condition-case err
       (autoload-do-load function-autoload 42 t)
     (error (list (car err) (cdr err))))
   (condition-case err
       (autoload-do-load function-autoload 42 'not-macro)
     (error (list (car err) (cdr err))))
   (condition-case err
       (autoload-do-load function-autoload 42 17)
     (error (list (car err) (cdr err)))))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_autoload_from_loaded_source_preserves_buffer_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (autoload 'neovm--autoload-match-data-probe "autoload-match-data-probe")
  (with-temp-buffer
    (insert "aa target zz")
    (goto-char (point-min))
    (re-search-forward "\\(target\\)")
    (let ((before (list (match-beginning 0) (match-end 0)
                        (match-beginning 1) (match-end 1))))
      (list (neovm--autoload-match-data-probe)
            before
            (list (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1))))))
"#;

    let expect = expect_test::expect![r#""OK (autoload-loaded (4 10 4 10) (4 10 4 10))""#];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

#[test]
fn oracle_autoload_load_error_restores_buffer_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (autoload 'neovm--autoload-match-data-error-probe
            "autoload-match-data-error-probe")
  (with-temp-buffer
    (insert "aa target zz")
    (goto-char (point-min))
    (re-search-forward "\\(target\\)")
    (let ((before (list (match-beginning 0) (match-end 0)
                        (match-beginning 1) (match-end 1))))
      (list
       (condition-case err
           (neovm--autoload-match-data-error-probe)
         (error (car err)))
       before
       (list (match-beginning 0) (match-end 0)
             (match-beginning 1) (match-end 1))))))
"#;

    let expect = expect_test::expect![r#""OK (error (4 10 4 10) (4 10 4 10))""#];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

#[test]
fn oracle_require_from_loaded_source_preserves_buffer_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Frequire routes the load through load_with_autoload_queue,
    // whose save_match_data_load boundary restores the caller's registers
    // after arbitrary top-level forms in the required file.
    let form = r#"
(with-temp-buffer
  (insert "aa target zz")
  (goto-char (point-min))
  (re-search-forward "\\(target\\)")
  (let ((before (list (match-beginning 0) (match-end 0)
                      (match-beginning 1) (match-end 1))))
    (list (require 'autoload-match-data-probe)
          before
          (list (match-beginning 0) (match-end 0)
                (match-beginning 1) (match-end 1)))))
"#;

    let expect =
        expect_test::expect![r#""OK (autoload-match-data-probe (4 10 4 10) (4 10 4 10))""#];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

#[test]
fn oracle_require_suppresses_implicit_load_progress_messages() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Frequire calls load_with_autoload_queue with NOMESSAGE=t.
    // Capture `message' itself so this guards the public Lisp contract rather
    // than relying on batch-mode stderr or the current message-log backend.
    let form = r#"
(let ((original-message (symbol-function 'message))
      messages)
  (unwind-protect
      (progn
        (fset 'message
              (lambda (format-string &rest arguments)
                (push (apply #'format format-string arguments)
                      messages)))
        (list (require 'require-message-probe)
              (nreverse messages)))
    (fset 'message original-message)
    (setq features (delq 'require-message-probe features))))
"#;

    let expect = expect_test::expect![r#""OK (require-message-probe nil)""#];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

#[test]
fn oracle_require_load_error_restores_buffer_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The same GNU unwind boundary runs when a required file signals.  Match
    // data is caller state, so a failed dependency load must not leak the
    // fixture's string-match registers into the error handler.
    let form = r#"
(with-temp-buffer
  (insert "aa target zz")
  (goto-char (point-min))
  (re-search-forward "\\(target\\)")
  (let ((before (list (match-beginning 0) (match-end 0)
                      (match-beginning 1) (match-end 1))))
    (list
     (condition-case err
         (require 'autoload-match-data-error-probe)
       (error (car err)))
     before
     (list (match-beginning 0) (match-end 0)
           (match-beginning 1) (match-end 1)))))
"#;

    let expect = expect_test::expect![r#""OK (error (4 10 4 10) (4 10 4 10))""#];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}
