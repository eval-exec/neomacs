//! Oracle parity tests for GNU change-group and silent-modification semantics.
//!
//! These target `atomic-change-group`, `prepare-change-group`, and
//! `with-silent-modifications`.  They also pin `combine-change-calls` and
//! `combine-after-change-calls`, whose public macros live in `lisp/subr.el`.
//! The latter's coalescing behavior is implemented by GNU `src/insdel.c` on
//! top of buffer change state.  These tests compare the observable Elisp
//! contract rather than approximating it.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_atomic_change_group_success_keeps_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "base")
  (let ((result (atomic-change-group
                  (goto-char (point-max))
                  (insert "-ok")
                  (buffer-string))))
    (list result
          (buffer-string)
          (buffer-modified-p)
          (eq buffer-undo-list t)
          (consp buffer-undo-list))))
"#;

    let expect = expect_test::expect![[r#""OK (\"base-ok\" \"base-ok\" t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_atomic_change_group_error_rolls_back_changes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "base")
  (let ((before-undo buffer-undo-list))
    (list
     (condition-case err
         (atomic-change-group
           (goto-char (point-max))
           (insert "-bad")
           (error "stop"))
       (error (list (car err) (cadr err))))
     (buffer-string)
     (equal before-undo buffer-undo-list)
     (eq buffer-undo-list t)
     (consp buffer-undo-list))))
"#;

    let expect = expect_test::expect![[r#""OK ((error \"stop\") \"base\" t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_manual_change_group_cancel_and_accept() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (let ((cancel-handle (prepare-change-group)))
    (activate-change-group cancel-handle)
    (goto-char (point-max))
    (insert "-cancel")
    (cancel-change-group cancel-handle)
    (let ((after-cancel (buffer-string)))
      (let ((accept-handle (prepare-change-group)))
        (activate-change-group accept-handle)
        (goto-char (point-max))
        (insert "-accept")
        (accept-change-group accept-handle)
        (list after-cancel
              (buffer-string)
              (buffer-modified-p)
              (eq buffer-undo-list t)
              (consp buffer-undo-list))))))
"#;

    let expect = expect_test::expect![[r#""OK (\"abc\" \"abc-accept\" t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_with_undo_amalgamate_removes_inner_undo_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/subr.el:with-undo-amalgamate wraps a change group, then
    // GNU lisp/simple.el:undo-amalgamate-change-group removes nil undo
    // boundaries from the recorded group.
    let form = r#"
(with-temp-buffer
  (buffer-enable-undo)
  (setq buffer-undo-list nil)
  (with-undo-amalgamate
    (insert "a")
    (undo-boundary)
    (insert "b")
    (undo-boundary)
    (insert "c"))
  (list (buffer-string) buffer-undo-list))
"#;
    let expect = expect_test::expect![[r#""OK (\"abc\" ((3 . 4) (2 . 3) (1 . 2) (t . 0)))""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        r#"("abc" ((3 . 4) (2 . 3) (1 . 2) (t . 0)))"#,
        &oracle,
        &neovm,
    );
}

#[test]
fn oracle_with_undo_amalgamate_keeps_disabled_undo_disabled() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (setq buffer-undo-list t)
  (let ((result (with-undo-amalgamate
                  (insert "x")
                  (buffer-string))))
    (list result (buffer-string) buffer-undo-list)))
"#;
    let expect = expect_test::expect![[r#""OK (\"x\" \"x\" t)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(r#"("x" "x" t)"#, &oracle, &neovm);
}

#[test]
fn oracle_prop_with_silent_modifications_restores_modified_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (set-buffer-modified-p nil)
  (let ((before-hooks nil)
        (after-hooks nil))
    (add-hook 'before-change-functions
              (lambda (beg end)
                (push (list beg end) before-hooks))
              nil t)
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len) after-hooks))
              nil t)
    (with-silent-modifications
      (goto-char (point-max))
      (insert "X"))
    (list (buffer-string)
          (buffer-modified-p)
          before-hooks
          after-hooks
          (eq buffer-undo-list t)
          (consp buffer-undo-list))))
"#;

    let expect = expect_test::expect![[r#""OK (\"abcX\" nil nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combine_after_change_calls_coalesces_without_before_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (let ((after-log nil))
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len (buffer-string)) after-log))
              nil t)
    (combine-after-change-calls
      (goto-char 2)
      (insert "Y")
      (goto-char (point-max))
      (insert "Z"))
    (list (buffer-string)
          (nreverse after-log))))
"#;

    let expect = expect_test::expect![[r#""OK (\"aYbcdefZ\" ((2 9 5 \"aYbcdefZ\")))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combine_after_change_calls_disabled_by_before_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (let ((before-log nil)
        (after-log nil))
    (add-hook 'before-change-functions
              (lambda (beg end)
                (push (list beg end (buffer-string)) before-log))
              nil t)
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len (buffer-string)) after-log))
              nil t)
    (combine-after-change-calls
      (goto-char 2)
      (insert "Y")
      (goto-char (point-max))
      (insert "Z"))
    (list (buffer-string)
          (nreverse before-log)
          (nreverse after-log))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"aYbcdefZ\" ((2 2 \"abcdef\") (8 8 \"aYbcdef\")) ((2 3 0 \"aYbcdef\") (8 9 0 \"aYbcdefZ\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combine_after_change_calls_flushes_during_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (let ((after-log nil))
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len (buffer-string)) after-log))
              nil t)
    (list
     (condition-case err
         (combine-after-change-calls
           (goto-char (point-max))
           (insert "X")
           (error "stop"))
       (error (list (car err) (cadr err))))
     (buffer-string)
     (nreverse after-log))))
"#;

    let expect = expect_test::expect![[r#""OK ((error \"stop\") \"abcX\" ((4 5 0 \"abcX\")))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_nested_combine_after_change_calls_defers_until_outer_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (let ((after-log nil)
        (inside-log nil))
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len (buffer-string)) after-log))
              nil t)
    (list
     (combine-after-change-calls
       (goto-char 2)
       (insert "X")
       (setq inside-log (list :after-first after-log))
       (let ((inner
              (combine-after-change-calls
                (goto-char (point-max))
                (insert "Y")
                (setq inside-log
                      (cons (list :inside-inner after-log) inside-log))
                :inner-value)))
         (setq inside-log
               (cons (list :after-inner inner after-log) inside-log)))
       :outer-value)
     (buffer-string)
     (nreverse inside-log)
     (nreverse after-log))))
"#;

    let expect = expect_test::expect![[
        r#""OK (:outer-value \"aXbcdefY\" (nil :after-first (:inside-inner nil) (:after-inner :inner-value nil)) ((2 4 0 \"aXbcdefY\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combine_change_calls_runs_hooks_once_and_suppresses_body_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (let ((before-log nil)
        (after-log nil)
        (inside-log nil))
    (add-hook 'before-change-functions
              (lambda (beg end)
                (push (list beg end (buffer-string)) before-log))
              nil t)
    (add-hook 'after-change-functions
              (lambda (beg end len)
                (push (list beg end len (buffer-string)) after-log))
              nil t)
    (let ((result
           (combine-change-calls 2 5
             (setq inside-log
                   (list :entry before-log after-log
                         (local-variable-p 'before-change-functions)
                         (local-variable-p 'after-change-functions)
                         before-change-functions
                         after-change-functions))
             (goto-char 3)
             (delete-char 2)
             (insert "XY")
             (setq inside-log
                   (cons (list :exit before-log after-log (buffer-string))
                         inside-log))
             :body-value)))
      (list result
            (buffer-string)
            (nreverse inside-log)
            (nreverse before-log)
            (nreverse after-log)
            (local-variable-p 'before-change-functions)
            (local-variable-p 'after-change-functions)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (:body-value \"abXYef\" (nil nil t t nil ((2 5 \"abcdef\")) :entry (:exit ((2 5 \"abcdef\")) nil \"abXYef\")) ((2 5 \"abcdef\")) ((2 5 3 \"abXYef\")) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combine_change_calls_records_single_undo_apply_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (setq buffer-undo-list nil)
  (combine-change-calls 2 5
    (goto-char 3)
    (delete-char 2)
    (insert "XY")
    (goto-char (point-max))
    (insert "!"))
  (list (buffer-string)
        (mapcar (lambda (entry)
                  (cond
                   ((and (consp entry) (eq (car entry) 'apply))
                    (list 'apply (nth 1 entry) (nth 2 entry) (nth 3 entry)
                          (eq (nth 4 entry) 'undo--wrap-and-run-primitive-undo)
                          (nth 5 entry) (nth 6 entry)
                          (consp (nth 7 entry))))
                   ((integerp entry) :boundary)
                   ((eq entry nil) :nil)
                   (t (if (consp entry) (car entry) entry))))
                buffer-undo-list)))
"#;

    let expect = expect_test::expect![[r#""OK (\"abXYef!\" ((apply 0 2 5 t 2 5 t)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
