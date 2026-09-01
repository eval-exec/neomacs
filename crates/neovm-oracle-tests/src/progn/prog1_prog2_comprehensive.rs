//! Oracle parity tests for comprehensive progn/prog1/prog2 interactions.
//!
//! Tests: progn return patterns with various body shapes, prog1 return-first
//! semantics with side effects, prog2 return-second semantics, nested
//! combinations of all three, interaction with let/let*, interaction with
//! condition-case, complex multi-expression body forms, and edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ---------------------------------------------------------------------------
// Test 1: progn with various return patterns — nil, atoms, lists, nested progn
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_progn_comprehensive_return_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Empty progn returns nil
  (progn)
  ;; Single atom
  (progn 42)
  ;; Single nil
  (progn nil)
  ;; Multiple forms — returns last
  (progn 'a 'b 'c 'd 'e)
  ;; Last form is a list
  (progn 1 2 '(3 4 5))
  ;; Nested progn — outer returns inner's last
  (progn (progn 'x 'y 'z))
  ;; Multiple nested progns in sequence
  (progn (progn 1 2) (progn 3 4) (progn 5 6))
  ;; progn as argument to a function
  (+ (progn 10) (progn 20) (progn 30))
  ;; progn with side effects returning different type
  (let ((x 0))
    (progn (setq x (+ x 1))
           (setq x (+ x 2))
           (format "%d" x)))
  ;; Deeply nested progn — 5 levels
  (progn (progn (progn (progn (progn 'deep))))))
"#;
    let expect = expect_test::expect![[r#""OK (nil 42 nil e (3 4 5) z 6 60 \"3\" deep)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 2: prog1 return value semantics with varied side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog1_comprehensive_return_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((trace nil))
  (list
    ;; prog1 with no body forms — just returns first
    (prog1 'solo)
    ;; prog1 returns first even when body forms return different types
    (prog1 42 "string" '(a b) [1 2 3])
    ;; prog1 first form is nil
    (prog1 nil 'discarded1 'discarded2)
    ;; prog1 captures value before mutation chain
    (let ((x 100))
      (list
        (prog1 x
          (setq x (* x 2))
          (setq x (+ x 50))
          (setq x (/ x 5)))
        x))
    ;; prog1 with complex first form (lambda call)
    (prog1
        (funcall (lambda (a b) (+ a b)) 3 7)
      (setq trace (cons 'after-lambda trace)))
    ;; prog1 nested in prog1 — outer captures inner's first
    (prog1
        (prog1 'inner-first 'inner-body)
      'outer-body)
    ;; prog1 where first form signals condition-case
    (condition-case err
        (prog1 (/ 10 0) 'never-reached)
      (arith-error 'caught-in-prog1))
    ;; Trace proves body forms ran
    (nreverse trace)))
"#;
    let expect = expect_test::expect![[
        r#""OK (solo 42 nil (100 50) 10 inner-first caught-in-prog1 (after-lambda))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 3: prog2 return value semantics — setup/compute/cleanup idiom
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog2_comprehensive_return_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (list
    ;; prog2 returns second form
    (prog2 'first 'second)
    ;; prog2 with body forms
    (prog2 'setup 'compute 'cleanup1 'cleanup2)
    ;; prog2 where first form has side effects
    (prog2
        (setq log (cons 'p2-setup log))
        (progn (setq log (cons 'p2-compute log)) (* 6 7))
      (setq log (cons 'p2-cleanup log)))
    ;; prog2 second form is nil
    (prog2 'first nil 'body)
    ;; prog2 with complex second form
    (prog2
        (setq log (cons 'complex-setup log))
        (let ((a 10) (b 20))
          (+ a b (* a b)))
      (setq log (cons 'complex-cleanup log)))
    ;; prog2 nested in prog2
    (prog2
        'outer-first
        (prog2 'inner-first 'inner-second 'inner-body)
      'outer-body)
    ;; Complete log
    (nreverse log)))
"#;
    let expect = expect_test::expect![[
        r#""OK (second compute 42 nil 230 inner-second (p2-setup p2-compute p2-cleanup complex-setup complex-cleanup))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 4: progn/prog1/prog2 interaction with let/let*
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_forms_with_let_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; progn inside let body — multiple bindings then sequenced work
  (let ((x 1) (y 2))
    (progn
      (setq x (+ x 10))
      (setq y (* y 5))
      (+ x y)))
  ;; prog1 captures let binding before mutation
  (let ((state 'initial))
    (list
      (prog1 state
        (setq state 'mutated))
      state))
  ;; let* with progn initializers
  (let* ((a (progn 1 2 3))
         (b (progn (+ a 10)))
         (c (prog1 a (+ a b))))
    (list a b c))
  ;; Nested let + progn mutation chain
  (let ((x 0))
    (let ((y (progn (setq x 5) (+ x 10))))
      (let ((z (prog1 y (setq x (* x 2)))))
        (list x y z))))
  ;; prog2 with let for resource-like pattern
  (let ((opened nil)
        (result nil))
    (setq result
          (prog2
              (setq opened t)
              (let ((data '(1 2 3 4 5)))
                (apply '+ data))
            (setq opened nil)))
    (list result opened)))
"#;
    let expect =
        expect_test::expect![[r#""OK (21 (initial mutated) (3 13 3) (10 15 15) (15 nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 5: progn/prog1/prog2 interaction with condition-case
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_forms_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; progn error short-circuits — captures state at error point
  (let ((x 0))
    (condition-case _
        (progn
          (setq x 1)
          (setq x 2)
          (error "boom")
          (setq x 3))
      (error (list 'caught x))))
  ;; prog1 error in first form — whole thing signals
  (condition-case _
      (prog1 (error "in-first") 'body)
    (error 'prog1-first-error))
  ;; prog1 error in body — still signals, first value lost
  (condition-case _
      (prog1 'captured (error "in-body") 'more)
    (error 'prog1-body-error))
  ;; prog2 error in first form
  (condition-case _
      (prog2 (error "p2-first") 'second)
    (error 'prog2-first-error))
  ;; prog2 error in second form
  (condition-case _
      (prog2 'first (error "p2-second") 'body)
    (error 'prog2-second-error))
  ;; prog2 error in body form
  (condition-case _
      (prog2 'first 'second (error "p2-body"))
    (error 'prog2-body-error))
  ;; condition-case inside progn — error handled mid-sequence
  (progn
    'before
    (condition-case _ (error "mid") (error 'handled))
    'after)
  ;; Nested: progn in condition-case body, prog1 in handler
  (condition-case _
      (progn 1 (/ 1 0) 3)
    (arith-error
     (prog1 'error-captured 'handler-cleanup))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((caught 2) prog1-first-error prog1-body-error prog2-first-error prog2-second-error prog2-body-error after error-captured)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 6: Complex multi-expression body forms with accumulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_complex_accumulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((acc nil))
  ;; Use progn to build up an accumulator through multiple phases
  (progn
    ;; Phase 1: push numbers
    (setq acc (cons 1 acc))
    (setq acc (cons 2 acc))
    (setq acc (cons 3 acc))
    ;; Phase 2: use prog1 to capture intermediate state
    (let ((snapshot (prog1 (copy-sequence acc)
                     (setq acc (cons 4 acc))
                     (setq acc (cons 5 acc)))))
      ;; Phase 3: use prog2 to do setup + compute + cleanup
      (let ((result (prog2
                        (setq acc (cons 'marker acc))
                        (length acc)
                      (setq acc (cons 'done acc)))))
        (list
          (nreverse snapshot)      ;; (3 2 1)
          (nreverse acc)           ;; (1 2 3 4 5 marker done)
          result)))))              ;; 6 (length at time of prog2's second form)
"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 2 3 4 5 marker done) 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 7: prog1/prog2 as function arguments (evaluated for value)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_as_function_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((x 0))
  (list
    ;; prog1 as argument to +
    (+ (prog1 10 (setq x (1+ x)))
       (prog1 20 (setq x (1+ x)))
       (prog1 30 (setq x (1+ x))))
    ;; x should be 3 now
    x
    ;; prog2 as argument to cons
    (cons (prog2 (setq x (+ x 10)) 'head 'cleanup)
          (prog2 (setq x (+ x 100)) 'tail 'cleanup2))
    ;; x should be 113
    x
    ;; Nested: prog1 returning prog2's result
    (prog1
        (prog2 'setup (* 7 8) 'teardown)
      (setq x 0))
    ;; progn as cond test
    (cond
      ((progn nil) 'branch-nil)
      ((progn t)   'branch-t)
      (t           'fallthrough))))
"#;
    let expect = expect_test::expect![[r#""OK (60 3 (head . tail) 113 56 branch-t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 8: prog1 for swap/rotate idioms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog1_swap_idiom() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((a 'alpha) (b 'beta) (c 'gamma))
  ;; Swap a and b using prog1
  (setq a (prog1 b (setq b a)))
  (let ((after-swap (list a b c)))
    ;; Rotate a <- b <- c <- a using prog1 chain
    (let ((tmp (prog1 a
                 (setq a (prog1 b
                           (setq b (prog1 c
                                     (setq c a))))))))
      ;; After rotate: a=beta, b=alpha, c=beta (original a was beta after swap)
      ;; Actually trace carefully:
      ;; Before rotate: a=beta, b=alpha, c=gamma
      ;; Step 1: tmp captures a=beta
      ;; Step 2: a captures b=alpha, b captures c=gamma, c gets original a=beta
      ;; After: a=alpha, b=gamma, c=beta, tmp=beta
      (list after-swap (list a b c) tmp))))
"#;
    let expect = expect_test::expect![[r#""OK ((beta alpha gamma) (alpha gamma beta) beta)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 9: progn/prog1/prog2 with catch/throw
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_forms_with_catch_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; progn throw skips remaining forms
  (catch 'tag
    (progn 1 2 (throw 'tag 'thrown) 4 5))
  ;; prog1 captures value before throw in body
  (let ((x 0))
    (catch 'tag
      (list
        (prog1 (setq x 42)
          (throw 'tag (list 'thrown-with x))
          (setq x 999)))))
  ;; prog2 throw in first form — second never evaluates
  (catch 'tag
    (prog2 (throw 'tag 'from-first) 'never 'also-never))
  ;; prog2 throw in body — second form's value lost
  (catch 'tag
    (prog2 'setup 'compute (throw 'tag 'from-body)))
  ;; Nested catch with progn
  (catch 'outer
    (progn
      (catch 'inner
        (progn
          (throw 'inner 'inner-done)))
      'after-inner-catch))
  ;; unwind-protect inside prog1 body
  (let ((cleaned nil))
    (list
      (catch 'tag
        (prog1 'first-value
          (unwind-protect
              (throw 'tag 'bail)
            (setq cleaned t))))
      cleaned)))
"#;
    let expect = expect_test::expect![[
        r#""OK (thrown (thrown-with 42) from-first from-body after-inner-catch (bail t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 10: progn with defun/defmacro, prog1/prog2 in function bodies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_forms_in_defun_bodies() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Function that uses prog1 to return old value after update
  (defun neovm--test-pppc-update (cell new-val)
    (prog1 (car cell)
      (setcar cell new-val)))

  ;; Function that uses prog2 for logging pattern
  (defun neovm--test-pppc-logged-compute (log-cell a b)
    (prog2
        (setcar log-cell (cons 'start (car log-cell)))
        (+ a b)
      (setcar log-cell (cons 'end (car log-cell)))))

  ;; Function with progn body doing multi-step transform
  (defun neovm--test-pppc-transform (lst)
    (progn
      (setq lst (mapcar '1+ lst))
      (setq lst (nreverse lst))
      (setq lst (mapcar (lambda (x) (* x x)) lst))
      lst))

  (unwind-protect
      (let ((cell (list 'old))
            (log-cell (list nil)))
        (list
          ;; prog1: returns 'old, cell now has 'new
          (neovm--test-pppc-update cell 'new)
          (car cell)
          ;; prog2: returns sum, log has entries
          (neovm--test-pppc-logged-compute log-cell 10 20)
          (nreverse (car log-cell))
          ;; progn transform
          (neovm--test-pppc-transform '(1 2 3 4 5))))
    (fmakunbound 'neovm--test-pppc-update)
    (fmakunbound 'neovm--test-pppc-logged-compute)
    (fmakunbound 'neovm--test-pppc-transform)))
"#;
    let expect = expect_test::expect![[r#""OK (old new 30 (start end) (36 25 16 9 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 11: progn implicit in various special forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_progn_implicit_in_special_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((x 0))
  (list
    ;; when body is implicit progn
    (when t
      (setq x 1)
      (setq x 2)
      x)
    ;; unless body is implicit progn
    (unless nil
      (setq x 10)
      (setq x 20)
      x)
    ;; let body is implicit progn
    (let ((y 5))
      (setq y (* y 2))
      (setq y (+ y 3))
      y)
    ;; cond clause body is implicit progn
    (cond
      (t (setq x 100)
         (setq x (+ x 50))
         x))
    ;; save-excursion body is implicit progn
    (with-temp-buffer
      (insert "hello")
      (goto-char (point-min))
      (buffer-string))
    ;; lambda body is implicit progn
    (funcall (lambda (n)
               (setq n (* n 2))
               (setq n (+ n 1))
               n)
             10)))
"#;
    let expect = expect_test::expect![[r#""OK (2 20 13 150 \"hello\" 21)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 12: prog1/prog2/progn ordering guarantees
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_evaluation_order_guarantees() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((order nil))
  ;; Verify strict left-to-right evaluation in all prog forms
  (let ((r1 (progn
              (setq order (cons 'pn1 order))
              (setq order (cons 'pn2 order))
              (setq order (cons 'pn3 order))
              'progn-result)))
    (let ((r2 (prog1
                  (progn (setq order (cons 'p1-first order)) 'p1-val)
                (setq order (cons 'p1-body1 order))
                (setq order (cons 'p1-body2 order)))))
      (let ((r3 (prog2
                    (setq order (cons 'p2-first order))
                    (progn (setq order (cons 'p2-second order)) 'p2-val)
                  (setq order (cons 'p2-body1 order))
                  (setq order (cons 'p2-body2 order)))))
        (list r1 r2 r3 (nreverse order))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (progn-result p1-val p2-val (pn1 pn2 pn3 p1-first p1-body1 p1-body2 p2-first p2-second p2-body1 p2-body2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
