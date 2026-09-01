//! Advanced oracle parity tests for lambda / anonymous function patterns.
//!
//! Covers: complex parameter lists, immediate invocation, closures
//! over loop variables, recursive self-reference, higher-order
//! factories, finite state machines, lambda calculus combinators.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Lambda with complex parameter lists (&optional + &rest combined)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lambda_adv_optional_rest_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test &optional with default-checking and &rest in one lambda.
    // Call with varying argument counts.
    let form = r####"(let ((f (lambda (a &optional b c &rest rest)
                             (list a
                                   (or b 'default-b)
                                   (or c 'default-c)
                                   (length rest)
                                   rest))))
                    (list
                      (funcall f 1)
                      (funcall f 1 2)
                      (funcall f 1 2 3)
                      (funcall f 1 2 3 4 5 6)))"####;
    let expect = expect_test::expect![[
        r#""OK ((1 default-b default-c 0 nil) (1 2 default-c 0 nil) (1 2 3 0 nil) (1 2 3 3 (4 5 6)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lambda as immediate invocation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lambda_adv_iife_with_complex_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Immediately-invoked lambda doing non-trivial work:
    // build a frequency table of characters in a string.
    let form = r####"((lambda (s)
                     (let ((freq nil))
                       (dotimes (i (length s))
                         (let* ((ch (aref s i))
                                (entry (assq ch freq)))
                           (if entry
                               (setcdr entry (1+ (cdr entry)))
                             (setq freq (cons (cons ch 1) freq)))))
                       ;; Sort by char code for deterministic output
                       (sort freq (lambda (a b) (< (car a) (car b))))))
                   "abracadabra")"####;
    let expect = expect_test::expect![[r#""OK ((97 . 5) (98 . 2) (99 . 1) (100 . 1) (114 . 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lambda in mapcar with closure over loop variable
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lambda_adv_mapcar_closure_over_loop_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a list of adder functions in a loop, then call them all.
    // Each should capture its own value of the iteration variable.
    let form = r####"(let ((makers nil))
                    (dotimes (i 5)
                      (let ((n i))
                        (setq makers
                              (cons (lambda (x) (+ x n)) makers))))
                    (setq makers (nreverse makers))
                    ;; Call each maker with 100
                    (mapcar (lambda (f) (funcall f 100)) makers))"####;
    let expect = expect_test::expect![[r#""OK (100 101 102 103 104)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(100 101 102 103 104)", &o, &n);
}

// ---------------------------------------------------------------------------
// Lambda with recursive self-reference via funcall
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lambda_adv_y_combinator_factorial() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Y-combinator-style: pass self as first argument for recursion.
    // Compute factorial(10) = 3628800.
    let form = r####"(let ((fact
                         (lambda (self n)
                           (if (<= n 1) 1
                             (* n (funcall self self (1- n)))))))
                    (list
                      (funcall fact fact 0)
                      (funcall fact fact 1)
                      (funcall fact fact 5)
                      (funcall fact fact 10)))"####;
    let expect = expect_test::expect![[r#""OK (1 1 120 3628800)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(1 1 120 3628800)", &o, &n);
}

// ---------------------------------------------------------------------------
// Lambda returning lambda (higher-order factory)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lambda_adv_higher_order_pipeline_factory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Factory that composes a chain of transformations.
    // compose takes a list of functions and returns their composition.
    let form = r####"(let ((compose
                         (lambda (fns)
                           (lambda (x)
                             (let ((result x)
                                   (remaining fns))
                               (while remaining
                                 (setq result (funcall (car remaining) result)
                                       remaining (cdr remaining)))
                               result)))))
                    (let ((pipeline
                           (funcall compose
                                    (list (lambda (x) (* x 2))
                                          (lambda (x) (+ x 3))
                                          (lambda (x) (* x x))))))
                      ;; (5 * 2) = 10, + 3 = 13, ^2 = 169
                      (list (funcall pipeline 5)
                            (funcall pipeline 0)
                            (funcall pipeline 1))))"####;
    let expect = expect_test::expect![[r#""OK (169 9 25)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(169 9 25)", &o, &n);
}

// ---------------------------------------------------------------------------
// Lambda-based finite state machine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lambda_adv_finite_state_machine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FSM that recognizes strings matching /ab+c/:
    //   start --a--> seen-a --b--> seen-b --b--> seen-b --c--> accept
    // Each state is a lambda that takes a char and returns (next-state . accepted?).
    let form = r####"(let (start seen-a seen-b accept reject)
                    (setq reject (lambda (ch) (cons reject nil)))
                    (setq accept (lambda (ch) (cons accept t)))
                    (setq seen-b
                          (lambda (ch)
                            (cond ((= ch ?b) (cons seen-b nil))
                                  ((= ch ?c) (cons accept t))
                                  (t (cons reject nil)))))
                    (setq seen-a
                          (lambda (ch)
                            (if (= ch ?b)
                                (cons seen-b nil)
                              (cons reject nil))))
                    (setq start
                          (lambda (ch)
                            (if (= ch ?a)
                                (cons seen-a nil)
                              (cons reject nil))))
                    ;; Run the FSM on a string
                    (let ((run-fsm
                           (lambda (input)
                             (let ((state start)
                                   (accepted nil)
                                   (i 0))
                               (while (< i (length input))
                                 (let ((result (funcall state (aref input i))))
                                   (setq state (car result)
                                         accepted (cdr result)))
                                 (setq i (1+ i)))
                               accepted))))
                      (list
                        (funcall run-fsm "abc")     ; t
                        (funcall run-fsm "abbc")    ; t
                        (funcall run-fsm "abbbc")   ; t
                        (funcall run-fsm "ac")      ; nil (no b)
                        (funcall run-fsm "abbb")    ; nil (no c)
                        (funcall run-fsm "xbc")     ; nil (no a)
                        (funcall run-fsm "ab"))))"# ; nil (no c)
    ;; --- SKI combinators applied to simple numeric functions ---
    // I x = x
    // K x y = x
    // S f g x = (f x (g x))
    // B f g x = f (g x)            (composition)
    // C f x y = f y x              (flip)
    let form = r#"(let ((I (lambda (x) x))
                        (K (lambda (x) (lambda (y) x)))
                        (S (lambda (f)
                             (lambda (g)
                               (lambda (x)
                                 (funcall (funcall f x) (funcall g x))))))
                        (B (lambda (f)
                             (lambda (g)
                               (lambda (x)
                                 (funcall f (funcall g x))))))
                        (C (lambda (f)
                             (lambda (x)
                               (lambda (y)
                                 (funcall (funcall f y) x))))))
                    (list
                      ;; I 42 = 42
                      (funcall I 42)
                      ;; K 1 2 = 1
                      (funcall (funcall K 1) 2)
                      ;; S K K x = I x = x  (SKK = I)
                      (funcall (funcall (funcall S K) K) 7)
                      ;; B double inc 5 = double(inc(5)) = double(6) = 12
                      (let ((double (lambda (x) (* 2 x)))
                            (inc (lambda (x) (1+ x))))
                        (funcall (funcall (funcall B double) inc) 5))
                      ;; C (lambda (a) (lambda (b) (- a b))) 3 10 = (- 10 3) = 7
                      (funcall
                        (funcall
                          (funcall C (lambda (a) (lambda (b) (- a b))))
                          3)
                        10)))"####;
    let expect = expect_test::expect![[r#""OK (42 1 7 12 7)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    // Oracle (GNU Emacs) returns (t t t nil nil nil nil) under lexical binding —
    // the FSM lambdas capture state variables lexically and produce correct results.
    assert_eq!(n, o, "neovm and oracle should match");
}
