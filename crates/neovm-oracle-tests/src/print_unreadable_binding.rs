//! Oracle parity for the `print-unreadable-function` binding made around the
//! unreadable-object callback.
//!
//! GNU print.c:1706-1732 captures the visible printer, then
//! `specbind (Qprint_unreadable_function, Qnil)` in the CALLER's current
//! buffer BEFORE switching to the secret callback buffer. GNU specbind
//! (src/eval.c:3674-3710) chooses LET_LOCAL when the caller has a setq-local
//! binding, LET_DEFAULT when it has none, and a plain LET for a forwarded
//! DEFVAR before it is localized. The watcher must therefore report op `let`
//! with where = the buffer for LET_LOCAL and nil otherwise, and `unlet` on
//! unwind.
//!
//! These cases are the peer-requested set: no local, caller-local +
//! prin1-to-string, caller-local + direct prin1, and add-variable-watcher
//! over all three. Expects are GNU 31.1 answers recorded with
//! NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_print_unreadable_no_local_prin1_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((print-unreadable-function
       (lambda (obj _escape) (format "CB:%s" (type-of obj)))))
  (prin1-to-string (make-marker)))
"#;
    let expect = expect_test::expect![[r#""OK \"CB:marker\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_unreadable_caller_local_prin1_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((calls 0))
    (setq-local print-unreadable-function
                (lambda (obj _escape)
                  (setq calls (1+ calls))
                  (format "CB:%s" (type-of obj))))
    (list (prin1-to-string (make-marker)) calls)))
"#;
    let expect = expect_test::expect![[r##""OK (\"#<marker in no buffer>\" 0)""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_unreadable_caller_local_direct_prin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (let ((standard-output (current-buffer)))
    (setq-local print-unreadable-function
                (lambda (obj _escape) (format "CB:%s" (type-of obj))))
    (prin1 (make-marker))
    (buffer-string)))
"#;
    let expect = expect_test::expect![[r#""OK \"CB:marker\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_print_unreadable_watcher_let_where() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar gdm-pur-log nil)
  (defun gdm-pur-watch (_sym newval op where)
    (push (list op (and (bufferp where) t) (null newval)) gdm-pur-log))
  (with-temp-buffer
    (let ((standard-output (current-buffer)))
      (setq gdm-pur-log nil)
      (add-variable-watcher 'print-unreadable-function #'gdm-pur-watch)
      (setq-local print-unreadable-function (lambda (_o _e) "X"))
      (prin1 (make-marker))
      (remove-variable-watcher 'print-unreadable-function #'gdm-pur-watch)
      (list (buffer-string) (nreverse gdm-pur-log)))))
"#;
    let expect = expect_test::expect![[r#""OK (\"X\" ((set t nil) (let t t) (unlet t nil)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
