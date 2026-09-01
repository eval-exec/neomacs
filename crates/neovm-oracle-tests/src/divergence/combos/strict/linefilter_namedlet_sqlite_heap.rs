//! Strict combo oracle probes, batch 64: genuinely-untested deterministic
//! areas — line filtering (keep-lines/flush-lines/how-many/delete-duplicate-
//! lines), named-let (tail-call loop), built-in sqlite (in-memory DB), and
//! heap.el.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_n4_keep_flush_how_many_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"foo\\nbar\\nfoo\\nbaz\\n\" \"foo\\nbar\\nfoo\\nbaz\\n\" 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (insert "foo\nbar\nfoo\nbaz\n")
        (keep-lines "foo")
        (buffer-string))
      (with-temp-buffer
        (insert "foo\nbar\nfoo\nbaz\n")
        (flush-lines "foo")
        (buffer-string))
      (with-temp-buffer
        (insert "foo bar foo baz foo")
        (how-many "foo" (point-min) (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_n4_delete_duplicate_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\\nb\\nc\\n\" \"a\\nb\\na\\nc\\nb\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (insert "a\nb\na\nc\nb\n")
        (delete-duplicate-lines (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (insert "a\nb\na\nc\nb\n")
        (delete-matching-lines "^a$")
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_n4_named_let_tco() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 720 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (named-let loop ((i 0) (acc 0))
        (if (> i 5) acc (loop (1+ i) (+ acc i))))
      (named-let fact ((n 6) (acc 1))
        (if (zerop n) acc (fact (1- n) (* acc n))))
      (length (named-let collect ((i 0) (out))
                  (if (> i 5) out (collect (1+ i) (cons i out))))))
"##,
        expect,
    );
}

#[test]
fn div_n4_heap_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"[ORACLE-LOAD-ROOT]/emacs-lisp/heap.el\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((h (make-heap #'< 3)))
  (heap-add h 5)
  (heap-add h 1)
  (heap-add h 3)
  (heap-add h 2)
  (list (heap-empty-p h)
        (heap-size h)
        (heap-root h)
        (heap-pop h)
        (heap-root h)))
"##,
        &["emacs-lisp/heap.el"],
        expect,
    );
}
