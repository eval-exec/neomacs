//! Comprehensive oracle parity tests for `let` and `let*` binding forms:
//! parallel vs sequential binding, nested mixing, shadowing, complex
//! expressions, `pcase-let`/`pcase-let*`, closure variable capture,
//! tail-position let, `let-alist`, and very deep nesting.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// let: parallel binding semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_parallel_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In `let`, all init forms are evaluated before any binding takes effect
    let form = r#"(let ((x 10))
                     (let ((x 20)
                           (y x))
                       (list x y)))"#;
    let expect = expect_test::expect![[r#""OK (20 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Parallel: x is still 1 when y is computed
    let form2 = r#"(let ((x 1))
                      (let ((x (+ x 100))
                            (y (* x 2)))
                        (list x y)))"#;
    let expect = expect_test::expect![[r#""OK (101 2)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Multiple interdependent bindings—all see outer scope
    let form3 = r#"(let ((a 5) (b 10))
                      (let ((a (+ a b))
                            (b (- b a))
                            (c (* a b)))
                        (list a b c)))"#;
    let expect = expect_test::expect![[r#""OK (15 5 50)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    let expect = expect_test::expect![[r#""OK nil""#]];
    // Binding to nil by default
    crate::common::assert_oracle_parity_expect("(let ((x)) x)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(let (x) x)", expect);
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect("(let (x y z) (list x y z))", expect);
}

// ---------------------------------------------------------------------------
// let*: sequential binding semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star_sequential_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In `let*`, each binding can see previously-bound variables
    let form = r#"(let* ((x 10)
                          (y (* x 2))
                          (z (+ x y)))
                     (list x y z))"#;
    let expect = expect_test::expect![[r#""OK (10 20 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Contrast with let: same form but sequential
    let form2 = r#"(let ((x 1))
                      (let* ((x (+ x 100))
                             (y (* x 2)))
                        (list x y)))"#;
    let expect = expect_test::expect![[r#""OK (101 202)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Chain of dependent computations
    let form3 = r#"(let* ((a 2)
                           (b (* a a))
                           (c (* b b))
                           (d (* c c))
                           (e (* d d)))
                      (list a b c d e))"#;
    let expect = expect_test::expect![[r#""OK (2 4 16 256 65536)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// Nested let/let* mixing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_nested_mixing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((x 1))
                     (let* ((y (+ x 10))
                            (z (* y 2)))
                       (let ((x z)
                             (w (+ x y)))
                         (list x w y z))))"#;
    let expect = expect_test::expect![[r#""OK (22 12 11 22)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Triple nesting with shadowing at each level
    let form2 = r#"(let ((a 1) (b 2))
                      (let* ((a (+ a b))
                             (c (* a 3)))
                        (let ((b c)
                              (d (+ a b)))
                          (let* ((e (+ a b c d)))
                            (list a b c d e)))))"#;
    let expect = expect_test::expect![[r#""OK (3 9 9 5 26)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // let inside let* init form
    let form3 = r#"(let* ((x 5)
                           (y (let ((z (* x 3)))
                                (+ z 1)))
                           (w (let* ((p y) (q (* p 2)))
                                (- q x))))
                      (list x y w))"#;
    let expect = expect_test::expect![[r#""OK (5 16 27)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// Shadowing outer variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Shadow and verify outer value is restored
    let form = r#"(let ((x 'outer))
                     (let ((result-inner
                            (let ((x 'inner))
                              x)))
                       (list x result-inner)))"#;
    let expect = expect_test::expect![[r#""OK (outer inner)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Multiple levels of shadowing
    let form2 = r#"(let ((n 1))
                      (let ((n (+ n 10)))
                        (let ((n (+ n 100)))
                          (let ((n (+ n 1000)))
                            n))))"#;
    let expect = expect_test::expect![[r#""OK 1111""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Shadow function-like binding
    let form3 = r#"(progn
                      (defvar neovm--let-shadow-test 'global)
                      (unwind-protect
                          (let ((neovm--let-shadow-test 'local-1))
                            (let ((neovm--let-shadow-test 'local-2))
                              (list neovm--let-shadow-test))
                            )
                        (makunbound 'neovm--let-shadow-test)))"#;
    let expect = expect_test::expect![[r#""OK (local-2)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// Binding to complex expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_complex_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Binding to progn, condition-case, if, mapcar results
    let form = r#"(let ((a (progn 1 2 3))
                         (b (if t 'yes 'no))
                         (c (condition-case nil
                                (/ 10 2)
                              (error 'err)))
                         (d (mapcar '1+ '(1 2 3)))
                         (e (apply '+ '(10 20 30))))
                     (list a b c d e))"#;
    let expect = expect_test::expect![[r#""OK (3 yes 5 (2 3 4) 60)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Binding to lambda invocation
    let form2 = r#"(let ((result (funcall (lambda (x y) (* x y)) 6 7)))
                      result)"#;
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Binding to recursive computation via named closure
    let form3 = r#"(progn
                      (fset 'neovm--let-test-fact
                        (lambda (n)
                          (let ((acc 1) (i n))
                            (while (> i 1)
                              (setq acc (* acc i))
                              (setq i (1- i)))
                            acc)))
                      (unwind-protect
                          (let ((f5 (funcall 'neovm--let-test-fact 5))
                                (f10 (funcall 'neovm--let-test-fact 10)))
                            (list f5 f10))
                        (fmakunbound 'neovm--let-test-fact)))"#;
    let expect = expect_test::expect![[r#""OK (120 3628800)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// pcase-let and pcase-let* (pattern matching destructuring)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_pcase_let_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // pcase-let with backquote patterns
    let form = r#"(progn
                     (require 'pcase)
                     (pcase-let ((`(,a ,b ,c) '(1 2 3)))
                       (list a b c)))"#;
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Nested destructuring
    let form2 = r#"(progn
                      (require 'pcase)
                      (pcase-let ((`(,x (,y ,z)) '(10 (20 30))))
                        (+ x y z)))"#;
    let expect = expect_test::expect![[r#""OK 60""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // pcase-let* with sequential patterns
    let form3 = r#"(progn
                      (require 'pcase)
                      (pcase-let* ((`(,a . ,rest) '(1 2 3 4))
                                   (`(,b . ,rest2) rest))
                        (list a b rest2)))"#;
    let expect = expect_test::expect![[r#""OK (1 2 (3 4))""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // pcase-let with _ wildcard
    let form4 = r#"(progn
                      (require 'pcase)
                      (pcase-let ((`(,first _ ,third) '(a b c)))
                        (list first third)))"#;
    let expect = expect_test::expect![[r#""OK (a c)""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);
}

// ---------------------------------------------------------------------------
// Closure variable capture
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closure captures let-bound variable
    let form = r#"(let ((x 10))
                     (let ((f (lambda () x)))
                       (funcall f)))"#;
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Multiple closures sharing captured environment
    let form2 = r#"(let ((count 0))
                      (let ((inc (lambda () (setq count (1+ count))))
                            (get (lambda () count)))
                        (funcall inc)
                        (funcall inc)
                        (funcall inc)
                        (funcall get)))"#;
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Closure captures different let levels
    let form3 = r#"(let ((a 1))
                      (let ((b 2))
                        (let ((f (lambda () (+ a b))))
                          (let ((a 100) (b 200))
                            (list (funcall f) a b)))))"#;
    let expect = expect_test::expect![[r#""OK (3 100 200)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);

    // Generate list of closures capturing loop variable
    let form4 = r#"(let ((fns nil))
                      (dotimes (i 5)
                        (let ((captured i))
                          (push (lambda () captured) fns)))
                      (mapcar #'funcall (nreverse fns)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(form4, expect);
}

// ---------------------------------------------------------------------------
// let in tail position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_tail_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // let as last expression in various forms
    let form = r#"(progn
                     (fset 'neovm--let-tail-test
                       (lambda (n)
                         (if (<= n 0)
                             0
                           (let ((result (* n n)))
                             result))))
                     (unwind-protect
                         (list (funcall 'neovm--let-tail-test 5)
                               (funcall 'neovm--let-tail-test 0)
                               (funcall 'neovm--let-tail-test -1))
                       (fmakunbound 'neovm--let-tail-test)))"#;
    let expect = expect_test::expect![[r#""OK (25 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // let in cond clause tail
    let form2 = r#"(let ((x 3))
                      (cond
                       ((= x 1) (let ((r 'one)) r))
                       ((= x 2) (let ((r 'two)) r))
                       ((= x 3) (let ((r 'three)) r))
                       (t (let ((r 'other)) r))))"#;
    let expect = expect_test::expect![[r#""OK three""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);
}

// ---------------------------------------------------------------------------
// let-alist (association list destructuring)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // let-alist binds dotted-pair values from an alist
    let form = r#"(progn
                     (require 'subr-x)
                     (let-alist '((name . "Alice") (age . 30) (active . t))
                       (list .name .age .active)))"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" 30 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Nested let-alist
    let form2 = r#"(progn
                      (require 'subr-x)
                      (let-alist '((x . 10) (y . 20))
                        (let-alist '((x . 100) (z . 300))
                          (list .x .z))))"#;
    let expect = expect_test::expect![[r#""OK (100 300)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // let-alist with computation on values
    let form3 = r#"(progn
                      (require 'subr-x)
                      (let-alist '((width . 800) (height . 600))
                        (* .width .height)))"#;
    let expect = expect_test::expect![[r#""OK 480000""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}

// ---------------------------------------------------------------------------
// Very deep nesting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_very_deep_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // 20 levels of nested let, each adding 1
    let form = r#"(let ((v 0))
  (let ((v (1+ v)))
    (let ((v (1+ v)))
      (let ((v (1+ v)))
        (let ((v (1+ v)))
          (let ((v (1+ v)))
            (let ((v (1+ v)))
              (let ((v (1+ v)))
                (let ((v (1+ v)))
                  (let ((v (1+ v)))
                    (let ((v (1+ v)))
                      (let ((v (1+ v)))
                        (let ((v (1+ v)))
                          (let ((v (1+ v)))
                            (let ((v (1+ v)))
                              (let ((v (1+ v)))
                                (let ((v (1+ v)))
                                  (let ((v (1+ v)))
                                    (let ((v (1+ v)))
                                      (let ((v (1+ v)))
                                        v))))))))))))))))))))"#;
    let expect = expect_test::expect![[r#""OK 19""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // Deep let* chain building a list incrementally
    let form2 = r#"(let* ((a '(1))
                           (b (cons 2 a))
                           (c (cons 3 b))
                           (d (cons 4 c))
                           (e (cons 5 d))
                           (f (cons 6 e))
                           (g (cons 7 f))
                           (h (cons 8 g))
                           (i (cons 9 h))
                           (j (cons 10 i)))
                      j)"#;
    let expect = expect_test::expect![[r#""OK (10 9 8 7 6 5 4 3 2 1)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);

    // Alternating let/let* at depth
    let form3 = r#"(let ((x 1))
                      (let* ((y (+ x 1))
                             (z (+ y 1)))
                        (let ((a (+ z 1))
                              (b (+ x y z)))
                          (let* ((c (+ a b))
                                 (d (* c 2)))
                            (list x y z a b c d)))))"#;
    let expect = expect_test::expect![[r#""OK (1 2 3 4 6 10 20)""#]];
    crate::common::assert_oracle_parity_expect(form3, expect);
}
