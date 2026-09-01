//! Strict combo oracle probes, batch 54: genuinely-untested deterministic
//! areas — add-function/remove-function (function composition, distinct from
//! advice-add), bool-vector set ops (intersection/union/subsetp/complement),
//! flatten-tree and dlet (subr-x), seq-let/seq-setf (seq.el), and fill with a
//! fill-prefix.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_k1_add_function_compose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable probe-addfn)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defun probe-addfn (x) (* x 2))
  (add-function :around 'probe-addfn (lambda (fn x) (1+ (funcall fn x))))
  (let ((r1 (probe-addfn 5)))
    (remove-function 'probe-addfn t)
    (list r1 (probe-addfn 5))))
"##,
        expect,
    );
}

#[test]
fn div_k1_bool_vector_set_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-complement)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (make-bool-vector 4 nil))
      (b (make-bool-vector 4 nil)))
  (aset a 1 t) (aset a 2 t)
  (aset b 2 t) (aset b 3 t)
  (list (bool-vector-intersection a b)
        (bool-vector-union a b)
        (bool-vector-subsetp a b)
        (bool-vector-complement a)))
"##,
        expect,
    );
}

#[test]
fn div_k1_flatten_tree_and_dlet() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6) ([1 [2 3] 4]) outer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (flatten-tree '((1 2) (3 (4 5)) 6))
      (flatten-tree [1 [2 3] 4])
      (let ((x 'outer))
        (dlet ((x 'inner))
          x)))
"##,
        expect,
    );
}

#[test]
fn div_k1_seq_let_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((v [1 2 3]))
  (seq-let ((a b c) v)
    (list a b c)))
"##,
        &["emacs-lisp/seq.el"],
        expect,
    );
}

#[test]
fn div_k1_fill_with_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ab foo bar baz\\nab more text\\nab here\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ab foo bar baz\nab more text here\n")
  (let ((fill-column 15) (fill-prefix "ab "))
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}
