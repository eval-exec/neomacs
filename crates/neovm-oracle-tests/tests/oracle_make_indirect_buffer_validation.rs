//! Argument-validation precedence for `make-indirect-buffer`.
//!
//! GNU Emacs 31.1, a360712c9d272d950d8d8255ef74570f7e90b7d9,
//! `src/buffer.c:Fmake_indirect_buffer`: validate NAME's type and reject
//! duplicate NAME before resolving BASE; reject empty NAME after BASE.
//! Catch conditions as (symbol data) so the offending argument and complete
//! error data remain observable. These empty expectations await an actual
//! attested-GNU refresh; they are not a completed RED/GREEN verification.

use crate::common::assert_oracle_parity_expect;

#[test]
fn make_indirect_buffer_bad_name_precedes_bad_base() {
    assert_oracle_parity_expect(
        r#"(condition-case err
                (make-indirect-buffer 23 42)
              (error (list (car err) (cdr err))))"#,
        expect_test::expect![[r#""OK (wrong-type-argument (stringp 42))""#]],
    );
}

#[test]
fn make_indirect_buffer_bad_name_with_live_base() {
    assert_oracle_parity_expect(
        r#"(condition-case err
                (make-indirect-buffer (current-buffer) 42)
              (error (list (car err) (cdr err))))"#,
        expect_test::expect![[r#""OK (wrong-type-argument (stringp 42))""#]],
    );
}

#[test]
fn make_indirect_buffer_bad_base_with_valid_name() {
    assert_oracle_parity_expect(
        r#"(condition-case err
                (make-indirect-buffer 23 "neovm--indirect-validation-uncreated")
              (error (list (car err) (cdr err))))"#,
        expect_test::expect![[r#""OK (wrong-type-argument (stringp 23))""#]],
    );
}

#[test]
fn make_indirect_buffer_bad_base_precedes_empty_name() {
    assert_oracle_parity_expect(
        r#"(condition-case err
                (make-indirect-buffer 23 "")
              (error (list (car err) (cdr err))))"#,
        expect_test::expect![[r#""OK (wrong-type-argument (stringp 23))""#]],
    );
}

#[test]
fn make_indirect_buffer_duplicate_name_precedes_bad_base() {
    assert_oracle_parity_expect(
        r#"(let ((existing (get-buffer-create "neovm--indirect-validation-existing")))
              (unwind-protect
                  (condition-case err
                      (make-indirect-buffer 23 (buffer-name existing))
                    (error (list (car err) (cdr err))))
                (kill-buffer existing)))"#,
        expect_test::expect![[
            r#""OK (error (\"Buffer name ‘neovm--indirect-validation-existing’ is in use\"))""#
        ]],
    );
}

#[test]
fn make_indirect_buffer_duplicate_name_quotes_follow_style_without_requoting_name() {
    assert_oracle_parity_expect(
        r#"(mapcar
             (lambda (name)
               (let ((existing (get-buffer-create name)))
                 (unwind-protect
                     (mapcar
                      (lambda (style)
                        (let ((text-quoting-style style))
                          (condition-case err
                              (make-indirect-buffer 23 name)
                            (error (list (car err) (cdr err))))))
                      '(curve straight grave))
                   (kill-buffer existing))))
             (list "neovm--indirect-`quote'%s-tail"
                   (concat "neovm--indirect-`quote'%s-" (string 0) "-tail")))"#,
        expect_test::expect![[""]],
    );
}
