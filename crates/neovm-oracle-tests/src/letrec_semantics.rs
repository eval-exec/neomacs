//! Oracle parity tests for GNU `letrec` macro semantics.
//!
//! GNU implements `letrec` in `lisp/subr.el`, not as a primitive special form.
//! Its macro expands initial non-recursive binders into `let*`, keeps recursive
//! binders in a `let` plus `setq` block, and accepts an omitted initializer as
//! nil via the same binder syntax as `let`/`let*`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_letrec_macroexpansion_rewrite_shapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (macroexpand
  '(letrec ((neovm--lr-a 1)
            (neovm--lr-b neovm--lr-a))
     (+ neovm--lr-a neovm--lr-b)))
 (macroexpand
  '(letrec ((neovm--lr-a (lambda () (funcall neovm--lr-b)))
            (neovm--lr-b (lambda () 42)))
     (funcall neovm--lr-a)))
 (macroexpand
  '(letrec ((neovm--lr-a 1)
            (neovm--lr-b (lambda () neovm--lr-c))
            (neovm--lr-c 3))
     (list neovm--lr-a (funcall neovm--lr-b) neovm--lr-c))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((let* ((neovm--lr-a 1) (neovm--lr-b neovm--lr-a)) (+ neovm--lr-a neovm--lr-b)) (let (neovm--lr-a neovm--lr-b) (setq neovm--lr-a (lambda nil (funcall neovm--lr-b))) (setq neovm--lr-b (lambda nil 42)) (funcall neovm--lr-a)) (let* ((neovm--lr-a 1)) (let (neovm--lr-b neovm--lr-c) (setq neovm--lr-b (lambda nil neovm--lr-c)) (setq neovm--lr-c 3) (list neovm--lr-a (funcall neovm--lr-b) neovm--lr-c))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_letrec_runtime_omitted_initializers_and_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (letrec ((a (lambda () (funcall c)))
          (b)
          (c (lambda () b)))
   (setq b 'ok)
   (funcall a))
 (let ((events nil))
   (letrec ((a (progn (push 'init-a events) 1))
            (b (progn (push (list 'init-b a) events) (1+ a)))
            (c (lambda () (list a b events))))
     (funcall c)))
 (letrec ((a)
          (b (lambda () a)))
   (list a (funcall b))))
"#;

    let expect = expect_test::expect![[r#""OK (ok (1 2 ((init-b 1) init-a)) (nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_letrec_nonrecursive_rewrite_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (macroexpand '(letrec () 1 2 3))
 (macroexpand '(letrec ((neovm--lr-x 1)) neovm--lr-x))
 (macroexpand '(letrec ((neovm--lr-x 1)
                        (neovm--lr-y 2))
                 neovm--lr-y))
 (letrec () 1 2 3)
 (letrec ((x 1)) x)
 (letrec ((x 1)
          (y (+ x 2)))
   (list x y)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((progn 1 2 3) 1 (let* ((neovm--lr-x 1) (neovm--lr-y 2)) neovm--lr-y) 3 1 (1 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
