//! Strict combo oracle probes, batch 81: undo-redo/undo-only (Emacs 28+ undo
//! variants with explicit boundary control) and insert-for-yank (yank handler
//! with yank-excluded property).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_p5_undo_redo_and_undo_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abcdef\" \"abc\" \"abcdef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abc")
  (undo-boundary)
  (insert "def")
  (undo-boundary)
  (let ((full (buffer-string)))
    (undo-only)
    (let ((after-undo (buffer-string)))
      (undo-redo)
      (list full after-undo (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_p5_undo_amalgamate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function undo-amalgamate)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (undo-amalgamate
    (insert "a")
    (insert "b")
    (insert "c"))
  (let ((full (buffer-string)))
    (undo)
    (list full (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_p5_insert_for_yank_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments propertize 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (insert-for-yank (propertize "kept-excluded" 'yank-excluded t))
        (buffer-string))
      (with-temp-buffer
        (insert-for-yank "plain text")
        (buffer-string))
      (with-temp-buffer
        (let ((str (propertize "prefix-suffix" 0 6 '(yank-handler (lambda (s) (concat "[" s "]"))))))
          (insert-for-yank str)
          (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_p5_undo_buffer_undo_list_inspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument symbolp ((6 . 12) nil (1 . 6) (t . 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello")
  (undo-boundary)
  (insert " world")
  (list (consp buffer-undo-list)
        (null (get buffer-undo-list 'pending-undo-list))
        (consp (car-safe buffer-undo-list))))
"##,
        expect,
    );
}
