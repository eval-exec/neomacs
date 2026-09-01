//! Strict combo oracle probes, batch 34: more loaded-library coverage via
//! assert_oracle_parity_with_load — thingatpt.el (thing-at-point bounds for
//! word/symbol/sexp/list), ring.el (bounded ring insert/ref/elements),
//! pp.el (pp-to-string pretty-printing), and subword.el (subword motion).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h1_thing_at_point_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Hello\" \"Hello\" (1 . 6) \"(a b c)\" \"(a b c)\" (15 . 22))""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "Hello world\n  (a b c)\n")
  (goto-char 1)
  (list (thing-at-point 'word)
        (thing-at-point 'symbol)
        (bounds-of-thing-at-point 'word)
        (progn (forward-line 1) (forward-char 2) (thing-at-point 'sexp))
        (thing-at-point 'list)
        (bounds-of-thing-at-point 'list)))
"##,
        &["thingatpt.el"],
        expect,
    );
}

#[test]
fn div_h1_ring_overflow_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 d c (d c b))""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((r (make-ring 3)))
  (ring-insert r 'a)
  (ring-insert r 'b)
  (ring-insert r 'c)
  (ring-insert r 'd)
  (list (ring-length r)
        (ring-size r)
        (ring-ref r 0)
        (ring-ref r 1)
        (ring-elements r)))
"##,
        &["emacs-lisp/ring.el"],
        expect,
    );
}

#[test]
fn div_h1_pp_to_string_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(a (b (c (d))) e)\\n\" \"(1 2 3)\\n\" \"((one . 1) (two . 2) (three . 3))\\n\" 12)""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (pp-to-string '(a (b (c (d))) e))
      (pp-to-string '(1 2 3))
      (pp-to-string '((one . 1) (two . 2) (three . 3)))
      (length (pp-to-string '(1 2 3 4 5))))
"##,
        &["emacs-lisp/pp.el"],
        expect,
    );
}

#[test]
fn div_h1_subword_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 10 14)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "camelCaseWord HTTPResponse XML")
  (subword-mode 1)
  (goto-char 1)
  (let ((p1 (progn (subword-forward 1) (point)))
        (p2 (progn (subword-forward 1) (point)))
        (p3 (progn (subword-forward 1) (point))))
    (list p1 p2 p3)))
"##,
        &["progmodes/subword.el"],
        expect,
    );
}

#[test]
fn div_h1_thing_at_point_word_neighborhood() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"second\" \"second\" \"second\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (insert "first second third fourth")
  (goto-char 8)
  (list (thing-at-point 'word)
        (save-excursion (forward-word -1) (thing-at-point 'word))
        (save-excursion (forward-word 1) (thing-at-point 'word))))
"##,
        &["thingatpt.el"],
        expect,
    );
}
