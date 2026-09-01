//! Oracle parity tests for `apply` and `lambda` comprehensive patterns.
//!
//! Covers: apply with spread args, funcall vs apply, lambda with complex
//! arglists, lambda in higher-order contexts, lambda as data (alists/hash-tables),
//! self-referencing lambda via funcall, lambda with &optional and &rest,
//! `function` vs `quote` for lambdas, applying to empty args, apply with many arguments.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// apply with spread args — prefix arguments spliced before tail list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_spread_args_multi_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (10 20 30 40 50 60)""#];
    // apply with 4 prefix args plus a tail list
    crate::common::assert_oracle_parity_expect(
        r#"(apply (lambda (a b c d e f) (list a b c d e f))
             10 20 30 40 '(50 60))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// funcall vs apply — same result, different calling conventions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_funcall_vs_apply_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((fn (lambda (a b c) (+ (* a b) c))))
  (list
   (funcall fn 3 4 5)
   (apply fn '(3 4 5))
   (apply fn 3 '(4 5))
   (apply fn 3 4 '(5))
   (equal (funcall fn 3 4 5) (apply fn '(3 4 5)))
   (equal (apply fn 3 '(4 5)) (apply fn 3 4 '(5)))))"#;
    let expect = expect_test::expect![r#""OK (17 17 17 17 t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lambda with complex arglists (&optional defaults, &rest interaction)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_complex_arglist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; All optional omitted
  (push (funcall (lambda (a &optional b c &rest xs) (list a b c xs)) 1) results)
  ;; One optional supplied
  (push (funcall (lambda (a &optional b c &rest xs) (list a b c xs)) 1 2) results)
  ;; All optional supplied, no rest
  (push (funcall (lambda (a &optional b c &rest xs) (list a b c xs)) 1 2 3) results)
  ;; All optional supplied plus rest
  (push (funcall (lambda (a &optional b c &rest xs) (list a b c xs)) 1 2 3 4 5 6) results)
  ;; Only &rest in arglist
  (push (funcall (lambda (&rest all) (length all)) 'a 'b 'c 'd 'e) results)
  ;; Only &optional in arglist, no required
  (push (funcall (lambda (&optional a b c) (list a b c))) results)
  (nreverse results))"#;
    let expect = expect_test::expect![
        r#""OK ((1 nil nil nil) (1 2 nil nil) (1 2 3 nil) (1 2 3 (4 5 6)) 5 (nil nil nil))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lambda in higher-order contexts: mapcar, sort, cl-reduce
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_higher_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; mapcar with lambda
  (mapcar (lambda (x) (* x x)) '(1 2 3 4 5))
  ;; sort with lambda comparator
  (sort (list 5 3 1 4 2) (lambda (a b) (< a b)))
  ;; nested mapcar: matrix transpose via lambda
  (let ((matrix '((1 2 3) (4 5 6) (7 8 9))))
    (mapcar (lambda (col)
              (mapcar (lambda (row) (nth col row)) matrix))
            '(0 1 2)))
  ;; apply + mapcar composition: sum of squares
  (apply '+ (mapcar (lambda (x) (* x x)) '(1 2 3 4 5)))
  ;; lambda as predicate in remove-if-like pattern
  (let ((pred (lambda (x) (> x 3)))
        (result nil))
    (dolist (x '(1 5 2 4 3 6))
      (when (funcall pred x) (push x result)))
    (nreverse result)))"#;
    let expect = expect_test::expect![
        r#""OK ((1 4 9 16 25) (1 2 3 4 5) ((1 4 7) (2 5 8) (3 6 9)) 55 (5 4 6))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lambda as data: stored in alists and hash-tables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_as_data_in_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((ops (list (cons 'add (lambda (a b) (+ a b)))
                      (cons 'sub (lambda (a b) (- a b)))
                      (cons 'mul (lambda (a b) (* a b)))
                      (cons 'square (lambda (a) (* a a)))))
        (add-fn (cdr (assq 'add ops)))
        (sub-fn (cdr (assq 'sub ops)))
        (mul-fn (cdr (assq 'mul ops)))
        (sq-fn (cdr (assq 'square ops))))
  (list
   (funcall add-fn 10 20)
   (funcall sub-fn 100 37)
   (funcall mul-fn 6 7)
   (funcall sq-fn 9)
   ;; Chain: square(add(3, 4))
   (funcall sq-fn (funcall add-fn 3 4))
   ;; Apply from alist lookup
   (apply (cdr (assq 'add ops)) '(100 200))))"#;
    let expect = expect_test::expect![r#""OK (30 63 42 81 49 300)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lambda stored in hash-tables, dispatched by key
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_in_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((dispatch (make-hash-table :test 'eq)))
  (puthash 'double (lambda (x) (* 2 x)) dispatch)
  (puthash 'negate (lambda (x) (- x)) dispatch)
  (puthash 'inc (lambda (x) (1+ x)) dispatch)
  (puthash 'compose2 (lambda (f g x) (funcall f (funcall g x))) dispatch)
  (list
   (funcall (gethash 'double dispatch) 21)
   (funcall (gethash 'negate dispatch) 42)
   (funcall (gethash 'inc dispatch) 99)
   ;; compose: double(inc(5))
   (funcall (gethash 'compose2 dispatch)
            (gethash 'double dispatch)
            (gethash 'inc dispatch)
            5)
   ;; compose: negate(double(7))
   (funcall (gethash 'compose2 dispatch)
            (gethash 'negate dispatch)
            (gethash 'double dispatch)
            7)
   ;; apply over hash lookup
   (apply (gethash 'double dispatch) '(50))))"#;
    let expect = expect_test::expect![r#""OK (42 -42 100 12 -14 100)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Self-referencing lambda via explicit funcall on a stored binding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_self_referencing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-self-ref-factorial
    (lambda (n)
      (if (<= n 1) 1
        (* n (funcall 'neovm--test-self-ref-factorial (1- n))))))
  (fset 'neovm--test-self-ref-fib
    (lambda (n)
      (cond ((<= n 0) 0)
            ((= n 1) 1)
            (t (+ (funcall 'neovm--test-self-ref-fib (- n 1))
                  (funcall 'neovm--test-self-ref-fib (- n 2)))))))
  (unwind-protect
      (list
       (funcall 'neovm--test-self-ref-factorial 0)
       (funcall 'neovm--test-self-ref-factorial 1)
       (funcall 'neovm--test-self-ref-factorial 5)
       (funcall 'neovm--test-self-ref-factorial 10)
       (funcall 'neovm--test-self-ref-fib 0)
       (funcall 'neovm--test-self-ref-fib 1)
       (funcall 'neovm--test-self-ref-fib 8)
       (funcall 'neovm--test-self-ref-fib 10))
    (fmakunbound 'neovm--test-self-ref-factorial)
    (fmakunbound 'neovm--test-self-ref-fib)))"#;
    let expect = expect_test::expect![r#""OK (1 1 120 3628800 0 1 21 55)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// function vs quote for lambdas: #'(lambda ...) vs '(lambda ...)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_function_vs_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; #'(lambda ...) — proper closure / function object
  (funcall #'(lambda (x) (* x x)) 7)
  ;; '(lambda ...) — quoted list, still callable in dynamic scope
  (funcall '(lambda (x) (* x x)) 7)
  ;; apply with #'
  (apply #'(lambda (a b c) (+ a b c)) '(10 20 30))
  ;; apply with '
  (apply '(lambda (a b c) (+ a b c)) '(10 20 30))
  ;; function returns a function object; functionp should be true
  (functionp #'(lambda (x) x))
  ;; Quoted lambda is a list starting with lambda — also callable
  (functionp '(lambda (x) x))
  ;; They produce the same result
  (equal (funcall #'(lambda (x) (1+ x)) 41)
         (funcall '(lambda (x) (1+ x)) 41)))"#;
    let expect = expect_test::expect![r#""OK (49 49 60 60 t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Applying to empty args and nil
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_empty_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; apply with nil tail, no prefix
  (apply '+ nil)
  (apply 'list nil)
  (apply 'concat nil)
  (apply 'vector nil)
  ;; funcall with zero-arity lambda
  (funcall (lambda () 42))
  ;; apply with zero-arity lambda
  (apply (lambda () 99) nil)
  ;; apply '* with empty list = 1 (identity element)
  (apply '* nil)
  ;; &rest with no args passed
  (funcall (lambda (&rest xs) (length xs)))
  ;; &optional with no args: all nil
  (funcall (lambda (&optional a b c) (list a b c))))"#;
    let expect = expect_test::expect![[r#""OK (0 nil \"\" [] 42 99 1 0 (nil nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Apply with many arguments — stress test with 20+ args
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_many_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; apply '+ with a long list
  (apply '+ (number-sequence 1 100))
  ;; apply 'list with many prefix args
  (length (apply 'list 1 2 3 4 5 6 7 8 9 10 '(11 12 13 14 15 16 17 18 19 20)))
  ;; lambda accepting &rest, called with many args
  (let ((summer (lambda (&rest xs) (apply '+ xs))))
    (apply summer (number-sequence 1 50)))
  ;; Verify: sum of 1..100 = 5050
  (= (apply '+ (number-sequence 1 100)) 5050)
  ;; apply 'max over a list
  (apply 'max (number-sequence -50 50))
  ;; apply 'min over a list
  (apply 'min (number-sequence -50 50))
  ;; Nested apply: apply a lambda that itself uses apply
  (apply (lambda (&rest xs) (apply '* xs)) '(1 2 3 4 5)))"#;
    let expect = expect_test::expect![r#""OK (5050 20 1275 t 50 -50 120)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// lambda closures capturing mutable bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_closure_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((counter 0)
        (inc (lambda () (setq counter (1+ counter)) counter))
        (get (lambda () counter)))
  (list
   (funcall get)
   (funcall inc)
   (funcall inc)
   (funcall inc)
   (funcall get)
   ;; Make adder factory
   (let ((make-adder (lambda (n) (lambda (x) (+ n x)))))
     (let ((add5 (funcall make-adder 5))
           (add10 (funcall make-adder 10)))
       (list (funcall add5 3)
             (funcall add10 3)
             (funcall add5 (funcall add10 0)))))))"#;
    let expect = expect_test::expect![r#""OK (0 1 2 3 3 (8 13 15))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// apply with lambda returning lambda (higher-order return)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_lambda_returning_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((make-multiplier (lambda (factor)
                             (lambda (x) (* factor x))))
        (times3 (funcall make-multiplier 3))
        (times7 (funcall make-multiplier 7)))
  (list
   (funcall times3 10)
   (funcall times7 10)
   (apply times3 '(15))
   (apply times7 '(15))
   ;; Compose two multipliers via funcall
   (funcall times3 (funcall times7 2))
   ;; Build pipeline of multipliers
   (let ((pipeline (list times3 times7
                         (funcall make-multiplier 2)))
         (val 1))
     (dolist (fn pipeline)
       (setq val (funcall fn val)))
     val)))"#;
    let expect = expect_test::expect![r#""OK (30 70 45 105 42 42)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
