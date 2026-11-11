//! Oracle parity tests for GNU autoload semantics.
//!
//! GNU implements `autoload` and `autoload-do-load` in `src/eval.c`;
//! `autoloadp` is Lisp in `lisp/subr.el` and is exactly an `eq` check against
//! `(car-safe OBJECT)`.  These tests cover the user-visible function-cell
//! shape, keymap-prefix loading, and error ordering using local fixtures and
//! the startup F2 binding to the real `two-column` library's `2C-command`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use std::path::PathBuf;

fn autoload_fixture_root() -> PathBuf {
    neomacs_infra::crate_root!().join("fixtures/autoload")
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
     (error (list (car err) (cdr err))))))"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (symbolp 42)) (wrong-type-argument (symbolp 42)) (wrong-type-argument (symbolp 42)))""#
    ]];
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

/// The startup F2 prefix is an autoloaded keymap, not a non-prefix command.
#[test]
fn oracle_autoload_keymap_startup_f2_prefix_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let* ((map (current-global-map))
       (original (lookup-key map [f2]))
       (before (list original
                     (autoloadp (symbol-function '2C-command))
                     (featurep 'two-column))))
  (unwind-protect
      (list before
            (keymap-global-set "<f2> <f1>" 'save-buffers-kill-terminal)
            (lookup-key map [f2 f1])
            (lookup-key map [f2 ?2])
            (featurep 'two-column))
    (define-key map [f2] original)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((2C-command t nil) save-buffers-kill-terminal save-buffers-kill-terminal 2C-two-columns t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// Defining and removing nested bindings must load an aliased prefix first.
#[test]
fn oracle_autoload_keymap_define_and_remove_through_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (setq neovm--autoload-keymap-load-count 0)
  (autoload 'neovm--autoload-keymap-prefix "autoload-keymap-probe" nil t 'keymap)
  (fset 'neovm--autoload-keymap-alias 'neovm--autoload-keymap-prefix)
  (let ((map (make-sparse-keymap)))
    (define-key map [f2] 'neovm--autoload-keymap-alias)
    (let ((before (list (lookup-key map [f2])
                        (autoloadp (symbol-function 'neovm--autoload-keymap-prefix))
                        neovm--autoload-keymap-load-count)))
      (list before
            (define-key map [f2 ?n ?y] 'neovm--autoload-new-command)
            (lookup-key map [f2 ?n ?y])
            (lookup-key map [f2 ?n ?x])
            neovm--autoload-keymap-load-count
            (progn
              (fmakunbound 'neovm--autoload-keymap-prefix)
              (autoload 'neovm--autoload-keymap-prefix
                        "autoload-keymap-probe" nil t 'keymap)
              (define-key map [f2 ?n ?x] nil t)
              (lookup-key map [f2 ?n ?x]))
            neovm--autoload-keymap-load-count))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((neovm--autoload-keymap-alias t 0) neovm--autoload-new-command neovm--autoload-new-command neovm--autoload-own-command 1 neovm--autoload-inherited-command 2)""#
    ]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

/// Leaf lookup stays lazy; descending retries chained autoloads via aliases.
#[test]
fn oracle_autoload_keymap_lookup_descends_through_nested_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (setq neovm--autoload-keymap-load-count 0)
  (setq neovm--autoload-keymap-chain-count 0)
  (autoload 'neovm--autoload-keymap-prefix "autoload-keymap-chain-probe" nil t 'keymap)
  (fset 'neovm--autoload-keymap-alias 'neovm--autoload-keymap-prefix)
  (fset 'neovm--autoload-keymap-outer-alias 'neovm--autoload-keymap-alias)
  (let ((map (make-sparse-keymap)))
    (define-key map [f2] 'neovm--autoload-keymap-outer-alias)
    (list (lookup-key map [f2])
          (autoloadp (symbol-function 'neovm--autoload-keymap-prefix))
          neovm--autoload-keymap-load-count
          (lookup-key map [f2 ?n ?x])
          (lookup-key map [f2 ?n ?q])
          (lookup-key map [f2 ?n ?x ?z])
          (autoloadp (symbol-function 'neovm--autoload-keymap-prefix))
          neovm--autoload-keymap-load-count
          neovm--autoload-keymap-chain-count)))
"#;
    let expect = expect_test::expect![[
        r#""OK (neovm--autoload-keymap-outer-alias t 0 neovm--autoload-own-command nil 3 nil 1 1)""#
    ]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

/// Keymap load failures propagate; ordinary command autoloads stay non-prefix.
#[test]
fn oracle_autoload_keymap_prefix_load_errors_and_non_keymap_commands() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (autoload 'neovm--autoload-keymap-error-prefix
            "autoload-keymap-error-probe" nil t 'keymap)
  (autoload 'neovm--autoload-keymap-missing-prefix
            "neovm--autoload-keymap-no-such-file" nil t 'keymap)
  (autoload 'neovm--autoload-keymap-ordinary-command
            "autoload-keymap-error-probe" nil t)
  (let ((map (make-sparse-keymap)))
    (define-key map [f2] 'neovm--autoload-keymap-error-prefix)
    (define-key map [f3] 'neovm--autoload-keymap-missing-prefix)
    (define-key map [f4] 'neovm--autoload-keymap-ordinary-command)
    (list
     (condition-case err (define-key map [f2 ?x] 'ignore)
       (error (list (car err) (cadr err))))
     (autoloadp (symbol-function 'neovm--autoload-keymap-error-prefix))
     (condition-case err (lookup-key map [f2 ?x])
       (error (list (car err) (cadr err))))
     (autoloadp (symbol-function 'neovm--autoload-keymap-error-prefix))
     (condition-case err (define-key map [f3 ?x] 'ignore)
       (error (car err)))
     (condition-case err (lookup-key map [f3 ?x])
       (error (car err)))
     (condition-case err (define-key map [f4 ?x] 'ignore)
       (error (list (car err) (cadr err))))
     (lookup-key map [f4 ?x])
     (autoloadp (symbol-function 'neovm--autoload-keymap-ordinary-command)))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((error \"Autoload keymap oracle fixture failure\") nil nil nil file-missing file-missing (error \"Key sequence <f4> x starts with non-prefix key <f4>\") 1 t)""#
    ]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

/// Malformed prefix autoloads preserve their cells and ordered argument errors.
#[test]
fn oracle_autoload_keymap_malformed_file_and_funname_error_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((cell '(autoload 42 nil t keymap))
      (map (make-sparse-keymap)))
  (fset 'neovm--autoload-keymap-malformed-prefix cell)
  (define-key map [f2] 'neovm--autoload-keymap-malformed-prefix)
  (list
   (condition-case err (define-key map [f2 ?x] 'ignore)
     (error err))
   (eq (symbol-function 'neovm--autoload-keymap-malformed-prefix) cell)
   (condition-case err (lookup-key map [f2 ?x])
     (error err))
   (eq (symbol-function 'neovm--autoload-keymap-malformed-prefix) cell)
   (condition-case err (autoload-do-load cell 17)
     (error err))
   (eq (autoload-do-load cell 17 'macro) cell)
   (eq (autoload-do-load cell 'neovm--autoload-keymap-malformed-prefix 'macro) cell)
   (condition-case err
       (autoload-do-load cell 'neovm--autoload-keymap-malformed-prefix)
     (error err))
   (eq (symbol-function 'neovm--autoload-keymap-malformed-prefix) cell)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument stringp 42) t (wrong-type-argument stringp 42) t (wrong-type-argument symbolp 17) t t (wrong-type-argument stringp 42) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// An unchanged aliased autoload errors after one load rather than retrying forever.
#[test]
fn oracle_autoload_keymap_alias_without_definition_makes_no_progress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (setq neovm--autoload-keymap-empty-count 0)
  (autoload 'neovm--autoload-keymap-empty-prefix
            "autoload-keymap-empty-probe" nil t 'keymap)
  (fset 'neovm--autoload-keymap-empty-alias 'neovm--autoload-keymap-empty-prefix)
  (let ((map (make-sparse-keymap))
        (cell (symbol-function 'neovm--autoload-keymap-empty-prefix)))
    (define-key map [f2] 'neovm--autoload-keymap-empty-alias)
    (list
     (condition-case err (define-key map [f2 ?x] 'ignore)
       (error (car err)))
     (eq (symbol-function 'neovm--autoload-keymap-empty-prefix) cell)
     neovm--autoload-keymap-empty-count
     (condition-case err (lookup-key map [f2 ?x])
       (error (car err)))
     (eq (symbol-function 'neovm--autoload-keymap-empty-prefix) cell)
     neovm--autoload-keymap-empty-count
     (condition-case err
         (autoload-do-load cell 'neovm--autoload-keymap-empty-alias)
       (error (car err)))
     (eq (symbol-function 'neovm--autoload-keymap-empty-alias)
         'neovm--autoload-keymap-empty-prefix)
     neovm--autoload-keymap-empty-count
     (progn
       (setq neovm--autoload-keymap-reinstall t)
       (condition-case err
           (autoload-do-load cell 'neovm--autoload-keymap-empty-alias)
         (error (car err))))
     (eq (symbol-function 'neovm--autoload-keymap-empty-prefix) cell)
     (equal (symbol-function 'neovm--autoload-keymap-empty-prefix) cell)
     neovm--autoload-keymap-empty-count)))
"#;
    let expect = expect_test::expect![[r#""OK (error t 1 error t 2 error t 3 error nil t 4)""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

/// A filter's positioned prefix symbol must survive GC in its autoload file.
#[test]
fn oracle_autoload_keymap_preserves_ephemeral_positioned_prefix_during_load() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (setq neovm--autoload-keymap-gc t)
  (autoload 'neovm--autoload-keymap-prefix "autoload-keymap-probe" nil t 'keymap)
  (eval-after-load 'autoload-keymap-probe
    (lambda ()
      (dotimes (_ 3) (make-list 32 nil) (garbage-collect))
      (setq neovm--autoload-positioned-scratch
        (let ((scratch nil))
          (dotimes (i 10000) (push (position-symbol 'ignore i) scratch))
          scratch))))
  (let ((symbols-with-pos-enabled t)
        (map (make-sparse-keymap)))
    (define-key map [f2]
      '(menu-item "prefix" nil :filter
        (lambda (_) (position-symbol 'neovm--autoload-keymap-prefix 7))))
    (list (lookup-key map [f2 ?n ?x])
          (featurep 'autoload-keymap-probe))))
"#;
    // The wrapper has no Lisp owner: it exists only as the filter result.
    // Reuse collected positioned-symbol slots so a stale wrapper cannot pass
    // merely because its old symbol payload remains in reclaimed storage.
    let expect = expect_test::expect![[r#""OK (neovm--autoload-own-command t)""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

/// A filter-created intermediate prefix spans an autoload's explicit collection.
#[test]
fn oracle_autoload_keymap_filter_prefix_survives_load_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (setq neovm--autoload-keymap-gc t
        neovm--autoload-keymap-load-count 0)
  (autoload 'neovm--autoload-keymap-prefix "autoload-keymap-probe" nil t 'keymap)
  (let ((map (make-sparse-keymap)))
    (define-key map [f2]
      '(menu-item "" nil :filter
                  (lambda (_)
                    (let ((temporary (make-sparse-keymap)))
                      (define-key temporary [?n] 'neovm--autoload-keymap-prefix)
                      temporary))))
    (list (lookup-key map [f2 ?n ?n ?x])
          neovm--autoload-keymap-load-count)))
"#;
    let expect = expect_test::expect![[r#""OK (neovm--autoload-own-command 1)""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

/// Positioned aliases to the original autoload must fail after a single load.
#[test]
fn oracle_autoload_keymap_positioned_alias_without_progress() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((symbols-with-pos-enabled t))
  (mapcar
   (lambda (operation)
     (setq neovm--autoload-positioned-repeat-count 0)
     (fmakunbound 'neovm--autoload-positioned-repeat-prefix)
     (autoload 'neovm--autoload-positioned-repeat-prefix
               "autoload-keymap-positioned-no-progress-probe" nil t 'keymap)
     (let ((cell (symbol-function 'neovm--autoload-positioned-repeat-prefix))
           (map (make-sparse-keymap)))
       (fset 'neovm--autoload-positioned-repeat-target cell)
       (define-key map [f2] 'neovm--autoload-positioned-repeat-prefix)
       (list
        (condition-case err
            (progn
              (if (eq operation 'direct)
                  (autoload-do-load cell 'neovm--autoload-positioned-repeat-prefix)
                (lookup-key map [f2 ?x]))
              'unexpected-success)
          (error
           (cond
            ((string-match-p "failed to define function" (error-message-string err))
             'no-progress)
            ((string-match-p "positioned alias repeated" (error-message-string err))
             'repeat-limit)
            (t (car err)))))
        neovm--autoload-positioned-repeat-count
        (symbol-with-pos-p
         (symbol-function 'neovm--autoload-positioned-repeat-prefix)))))
   '(direct lookup)))
"#;
    let expect = expect_test::expect![[r#""OK ((no-progress 1 t) (no-progress 1 t))""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}

/// Prefix retries and post-load keymap checks observe a newly enabled symbol mode.
#[test]
fn oracle_autoload_keymap_load_changes_positioned_symbol_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(mapcar
 (lambda (operation)
   (let ((symbols-with-pos-enabled nil)
         (map (make-sparse-keymap)))
     (fmakunbound 'neovm--autoload-positioned-mode-prefix)
     (fmakunbound 'neovm--autoload-positioned-mode-target)
     (autoload 'neovm--autoload-positioned-mode-prefix
               "autoload-keymap-positioned-mode-probe" nil t 'keymap)
     (define-key map [f2] 'neovm--autoload-positioned-mode-prefix)
     (list
      (condition-case err
          (if (eq operation 'lookup)
              (lookup-key map [f2 ?x])
            (define-key map [f2 ?y] 'ignore)
            (lookup-key map [f2 ?y]))
        (error (car err)))
      symbols-with-pos-enabled)))
 '(lookup define))
"#;
    let expect =
        expect_test::expect![[r#""OK ((neovm--autoload-positioned-mode-command t) (ignore t))""#]];
    crate::common::assert_oracle_parity_with_load_root_expect(
        form,
        &[],
        &autoload_fixture_root(),
        expect,
    );
}
