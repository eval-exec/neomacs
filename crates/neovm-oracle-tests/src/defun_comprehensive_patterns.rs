//! Comprehensive oracle parity tests for defun: all argument patterns,
//! docstrings, interactive specs, nested defuns, overwriting, mutual recursion,
//! symbol-function inspection, fmakunbound, and complex body side effects.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// defun with &optional: default nil, explicit values, mixed
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_optional_args_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-opt1 (a &optional b c)
    "Function with two optional args."
    (list a b c))

  (defun neovm--test-opt2 (a &optional b c d e)
    "Function with many optional args."
    (list a b c d e))

  (unwind-protect
      (list
        ;; All args provided
        (neovm--test-opt1 1 2 3)
        ;; Missing optional args default to nil
        (neovm--test-opt1 10)
        (neovm--test-opt1 10 20)
        ;; Many optionals
        (neovm--test-opt2 'x)
        (neovm--test-opt2 'x 'y 'z)
        (neovm--test-opt2 'x 'y 'z 'w 'v)
        ;; Optional with nil explicitly passed
        (neovm--test-opt1 1 nil 3)
        (neovm--test-opt1 1 nil nil))
    (fmakunbound 'neovm--test-opt1)
    (fmakunbound 'neovm--test-opt2)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3) (10 nil nil) (10 20 nil) (x nil nil nil nil) (x y z nil nil) (x y z w v) (1 nil 3) (1 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defun with &rest: collect variadic arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_rest_args_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-rest1 (a &rest args)
    "One required, rest collected."
    (list a args (length args)))

  (defun neovm--test-rest2 (&rest all)
    "No required, all in rest."
    (list (length all) (apply #'+ (or all '(0)))))

  (defun neovm--test-opt-rest (a &optional b &rest more)
    "Mix of required, optional, and rest."
    (list a b more))

  (unwind-protect
      (list
        (neovm--test-rest1 'x)
        (neovm--test-rest1 'x 1 2 3)
        (neovm--test-rest1 'x 'a 'b 'c 'd 'e)
        (neovm--test-rest2)
        (neovm--test-rest2 1 2 3 4 5)
        (neovm--test-rest2 10 20 30)
        (neovm--test-opt-rest 1)
        (neovm--test-opt-rest 1 2)
        (neovm--test-opt-rest 1 2 3 4 5))
    (fmakunbound 'neovm--test-rest1)
    (fmakunbound 'neovm--test-rest2)
    (fmakunbound 'neovm--test-opt-rest)))"#;
    let expect = expect_test::expect![[
        r#""OK ((x nil 0) (x (1 2 3) 3) (x (a b c d e) 5) (0 0) (5 15) (3 60) (1 nil nil) (1 2 nil) (1 2 (3 4 5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defun with docstring: verify documentability
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_docstring_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-doc1 (x)
    "This function doubles X."
    (* x 2))

  (defun neovm--test-doc2 ()
    "A no-arg documented function.\nWith newlines in docstring."
    42)

  ;; defun without docstring
  (defun neovm--test-nodoc (x) (+ x 1))

  (unwind-protect
      (list
        (neovm--test-doc1 21)
        (neovm--test-doc2)
        (neovm--test-nodoc 99)
        ;; Check that the function actually works
        (neovm--test-doc1 0)
        (neovm--test-doc1 -5)
        ;; Verify symbol-function returns something non-nil
        (not (null (symbol-function 'neovm--test-doc1)))
        (not (null (symbol-function 'neovm--test-nodoc))))
    (fmakunbound 'neovm--test-doc1)
    (fmakunbound 'neovm--test-doc2)
    (fmakunbound 'neovm--test-nodoc)))"#;
    let expect = expect_test::expect![[r#""OK (42 42 100 0 -10 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defun overwrite: redefine the same function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_overwrite_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-overwrite (x) (+ x 1))

  (unwind-protect
      (let ((r1 (neovm--test-overwrite 10)))
        ;; Redefine with different behavior
        (defun neovm--test-overwrite (x) (* x 10))
        (let ((r2 (neovm--test-overwrite 10)))
          ;; Redefine again with different arity
          (defun neovm--test-overwrite (x y) (+ x y))
          (let ((r3 (neovm--test-overwrite 10 20)))
            ;; Redefine back to unary
            (defun neovm--test-overwrite (x) (- x))
            (let ((r4 (neovm--test-overwrite 10)))
              (list r1 r2 r3 r4)))))
    (fmakunbound 'neovm--test-overwrite)))"#;
    let expect = expect_test::expect![[r#""OK (11 100 30 -10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defun returning lambda: function factories
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_returning_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-make-adder (n)
    "Return a function that adds N to its argument."
    (lambda (x) (+ x n)))

  (defun neovm--test-make-multiplier (factor)
    "Return a function that multiplies by FACTOR."
    (lambda (x) (* x factor)))

  (defun neovm--test-compose (f g)
    "Return composition f(g(x))."
    (lambda (x) (funcall f (funcall g x))))

  (unwind-protect
      (let* ((add5 (neovm--test-make-adder 5))
             (mul3 (neovm--test-make-multiplier 3))
             (add5-then-mul3 (neovm--test-compose mul3 add5))
             (mul3-then-add5 (neovm--test-compose add5 mul3)))
        (list
          (funcall add5 10)          ;; 15
          (funcall add5 -5)          ;; 0
          (funcall mul3 7)           ;; 21
          (funcall add5-then-mul3 2) ;; (2+5)*3 = 21
          (funcall mul3-then-add5 2) ;; (2*3)+5 = 11
          (functionp add5)
          (functionp mul3)
          ;; Multiple adders don't interfere
          (let ((add10 (neovm--test-make-adder 10))
                (add0 (neovm--test-make-adder 0)))
            (list (funcall add10 5) (funcall add0 5) (funcall add5 5)))))
    (fmakunbound 'neovm--test-make-adder)
    (fmakunbound 'neovm--test-make-multiplier)
    (fmakunbound 'neovm--test-compose)))"#;
    let expect = expect_test::expect![[r#""OK (15 0 21 21 11 t t (15 5 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mutual recursion via defun: even/odd
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_mutual_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-my-even-p (n)
    "Check if N is even via mutual recursion."
    (if (= n 0) t
      (neovm--test-my-odd-p (1- n))))

  (defun neovm--test-my-odd-p (n)
    "Check if N is odd via mutual recursion."
    (if (= n 0) nil
      (neovm--test-my-even-p (1- n))))

  ;; Another mutual recursion: collatz-style alternation
  (defun neovm--test-collatz-a (n acc)
    "Process even numbers in modified Collatz."
    (cond
      ((<= n 1) (nreverse (cons n acc)))
      ((= (% n 2) 0) (neovm--test-collatz-b (/ n 2) (cons n acc)))
      (t (neovm--test-collatz-b (+ (* 3 n) 1) (cons n acc)))))

  (defun neovm--test-collatz-b (n acc)
    "Bounce back to collatz-a with logging."
    (neovm--test-collatz-a n acc))

  (unwind-protect
      (list
        (neovm--test-my-even-p 0)
        (neovm--test-my-even-p 1)
        (neovm--test-my-even-p 10)
        (neovm--test-my-even-p 7)
        (neovm--test-my-odd-p 0)
        (neovm--test-my-odd-p 1)
        (neovm--test-my-odd-p 15)
        (neovm--test-my-odd-p 20)
        ;; Collatz sequence from 6: 6->3->10->5->16->8->4->2->1
        (neovm--test-collatz-a 6 nil)
        (neovm--test-collatz-a 1 nil))
    (fmakunbound 'neovm--test-my-even-p)
    (fmakunbound 'neovm--test-my-odd-p)
    (fmakunbound 'neovm--test-collatz-a)
    (fmakunbound 'neovm--test-collatz-b)))"#;
    let expect =
        expect_test::expect![[r#""OK (t nil t nil nil t t nil (6 3 10 5 16 8 4 2 1) (1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested defun: defun inside defun body
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_nested_definitions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-outer (x)
    "Define inner helper and use it."
    (defun neovm--test-inner (y) (* y y))
    (+ (neovm--test-inner x) (neovm--test-inner (1+ x))))

  (unwind-protect
      (list
        ;; First call defines inner, then uses it
        (neovm--test-outer 3)    ;; 9 + 16 = 25
        (neovm--test-outer 5)    ;; 25 + 36 = 61
        ;; Inner is now globally defined
        (neovm--test-inner 10)   ;; 100
        ;; Redefine outer to use different inner
        (progn
          (defun neovm--test-outer2 (x)
            (defun neovm--test-inner2 (y) (+ y 100))
            (neovm--test-inner2 x))
          (list
            (neovm--test-outer2 5)   ;; 105
            (neovm--test-inner2 0)))) ;; 100
    (fmakunbound 'neovm--test-outer)
    (fmakunbound 'neovm--test-inner)
    (fmakunbound 'neovm--test-outer2)
    (fmakunbound 'neovm--test-inner2)))"#;
    let expect = expect_test::expect![[r#""OK (25 61 100 (105 100))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// symbol-function and fmakunbound interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_symbol_function_fmakunbound() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-sf1 (x) (+ x 1))
  (defun neovm--test-sf2 (x) (* x 2))

  (unwind-protect
      (list
        ;; symbol-function returns the function object
        (not (null (symbol-function 'neovm--test-sf1)))
        (functionp (symbol-function 'neovm--test-sf1))
        ;; funcall via symbol-function
        (funcall (symbol-function 'neovm--test-sf1) 10)  ;; 11
        (funcall (symbol-function 'neovm--test-sf2) 10)  ;; 20
        ;; fmakunbound removes the function binding
        (progn
          (fmakunbound 'neovm--test-sf1)
          (fboundp 'neovm--test-sf1))  ;; nil
        ;; sf2 should still work
        (neovm--test-sf2 5)  ;; 10
        ;; Re-defun after fmakunbound
        (progn
          (defun neovm--test-sf1 (x) (- x 1))
          (neovm--test-sf1 10))  ;; 9
        ;; fboundp checks
        (fboundp 'neovm--test-sf1)  ;; t
        (fboundp 'neovm--test-sf2)  ;; t
        (fboundp 'neovm--nonexistent-fn-xyz))  ;; nil
    (fmakunbound 'neovm--test-sf1)
    (fmakunbound 'neovm--test-sf2)))"#;
    let expect = expect_test::expect![[r#""OK (t t 11 20 nil 10 9 t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defun with complex arglists: mixing all parameter types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_complex_arglists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  ;; Required + optional + rest
  (defun neovm--test-complex1 (a b &optional c d &rest more)
    (list 'a a 'b b 'c c 'd d 'more more))

  ;; Single required + rest
  (defun neovm--test-complex2 (x &rest xs)
    (cons x xs))

  ;; No args
  (defun neovm--test-complex3 ()
    'no-args)

  ;; Many required
  (defun neovm--test-complex4 (a b c d e)
    (+ a b c d e))

  (unwind-protect
      (list
        (neovm--test-complex1 1 2)
        (neovm--test-complex1 1 2 3)
        (neovm--test-complex1 1 2 3 4)
        (neovm--test-complex1 1 2 3 4 5 6 7)
        (neovm--test-complex2 'only)
        (neovm--test-complex2 1 2 3 4 5)
        (neovm--test-complex3)
        (neovm--test-complex4 1 2 3 4 5)
        ;; Passing different types
        (neovm--test-complex1 "a" nil t :kw 'sym 3.14))
    (fmakunbound 'neovm--test-complex1)
    (fmakunbound 'neovm--test-complex2)
    (fmakunbound 'neovm--test-complex3)
    (fmakunbound 'neovm--test-complex4)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a 1 b 2 c nil d nil more nil) (a 1 b 2 c 3 d nil more nil) (a 1 b 2 c 3 d 4 more nil) (a 1 b 2 c 3 d 4 more (5 6 7)) (only) (1 2 3 4 5) no-args 15 (a \"a\" b nil c t d :kw more (sym 3.14)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defun body side effects: setq, let, progn, multiple expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_body_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--test-counter 0)

  (defun neovm--test-side-effect (n)
    "Increment counter N times, return final counter."
    (let ((i 0))
      (while (< i n)
        (setq neovm--test-counter (1+ neovm--test-counter))
        (setq i (1+ i))))
    neovm--test-counter)

  (defun neovm--test-multi-body (x)
    "Multiple expressions in body; last is return value."
    (setq neovm--test-counter (+ neovm--test-counter x))
    (let ((doubled (* x 2)))
      (setq neovm--test-counter (+ neovm--test-counter doubled)))
    ;; Return value: the triple
    (* x 3))

  (unwind-protect
      (progn
        (setq neovm--test-counter 0)
        (let ((r1 (neovm--test-side-effect 5))
              ;; counter is now 5
              (r2-pre neovm--test-counter))
          (let ((r2 (neovm--test-side-effect 3)))
            ;; counter is now 8
            (let ((r3 (neovm--test-multi-body 10)))
              ;; counter incremented by 10 and by 20 = 38
              (list r1 r2-pre r2 r3 neovm--test-counter)))))
    (fmakunbound 'neovm--test-side-effect)
    (fmakunbound 'neovm--test-multi-body)
    (makunbound 'neovm--test-counter)))"#;
    let expect = expect_test::expect![[r#""OK (5 5 8 30 38)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defun: interactive spec (basic non-interactive testing)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_interactive_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-interactive1 (x)
    "A command with interactive spec."
    (interactive "p")
    (* x x))

  (defun neovm--test-interactive2 ()
    "A no-arg interactive command."
    (interactive)
    42)

  (defun neovm--test-interactive3 (a b)
    "Interactive with two numeric args."
    (interactive "nFirst: \nnSecond: ")
    (+ a b))

  (unwind-protect
      (list
        ;; Can still call interactively-declared functions non-interactively
        (neovm--test-interactive1 7)   ;; 49
        (neovm--test-interactive2)     ;; 42
        (neovm--test-interactive3 3 4) ;; 7
        ;; commandp should return t
        (commandp 'neovm--test-interactive1)
        (commandp 'neovm--test-interactive2)
        (commandp 'neovm--test-interactive3)
        ;; Non-interactive defun is not a command
        (progn
          (defun neovm--test-non-cmd () 99)
          (commandp 'neovm--test-non-cmd)))
    (fmakunbound 'neovm--test-interactive1)
    (fmakunbound 'neovm--test-interactive2)
    (fmakunbound 'neovm--test-interactive3)
    (fmakunbound 'neovm--test-non-cmd)))"#;
    let expect = expect_test::expect![[r#""OK (49 42 7 t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// defun: recursive with accumulator pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_defun_recursive_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defun neovm--test-factorial (n)
    "Naive recursive factorial."
    (if (<= n 1) 1
      (* n (neovm--test-factorial (1- n)))))

  (defun neovm--test-factorial-acc (n &optional acc)
    "Tail-recursive factorial with accumulator."
    (let ((a (or acc 1)))
      (if (<= n 1) a
        (neovm--test-factorial-acc (1- n) (* n a)))))

  (defun neovm--test-flatten (tree)
    "Flatten a nested list structure."
    (cond
      ((null tree) nil)
      ((atom tree) (list tree))
      (t (append (neovm--test-flatten (car tree))
                 (neovm--test-flatten (cdr tree))))))

  (unwind-protect
      (list
        (neovm--test-factorial 0)
        (neovm--test-factorial 1)
        (neovm--test-factorial 5)
        (neovm--test-factorial 10)
        (neovm--test-factorial-acc 0)
        (neovm--test-factorial-acc 1)
        (neovm--test-factorial-acc 5)
        (neovm--test-factorial-acc 10)
        ;; Both should give same results
        (= (neovm--test-factorial 8) (neovm--test-factorial-acc 8))
        ;; Flatten
        (neovm--test-flatten '(1 (2 (3 4) 5) (6 7)))
        (neovm--test-flatten '((a b) (c (d e (f))) g))
        (neovm--test-flatten nil)
        (neovm--test-flatten 42))
    (fmakunbound 'neovm--test-factorial)
    (fmakunbound 'neovm--test-factorial-acc)
    (fmakunbound 'neovm--test-flatten)))"#;
    let expect = expect_test::expect![[
        r#""OK (1 1 120 3628800 1 1 120 3628800 t (1 2 3 4 5 6 7) (a b c d e f g) nil (42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
