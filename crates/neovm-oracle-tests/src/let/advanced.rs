//! Advanced oracle parity tests for `let` bindings:
//! many bindings, evaluation order, nil bindings, shadowing,
//! nested scope capture, complex init expressions,
//! state encapsulation, and mutual references through closures.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// let with many bindings (10+)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_many_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // 15 bindings, using arithmetic on them
    let form = r#"(let ((a 1) (b 2) (c 3) (d 4) (e 5)
                        (f 6) (g 7) (h 8) (i 9) (j 10)
                        (k 11) (l 12) (m 13) (n 14) (o 15))
      ;; Various computations using all 15 bindings
      (list
       (+ a b c d e)
       (+ f g h i j)
       (+ k l m n o)
       (* a e j o)
       (- o a)
       (/ (+ a b c d e f g h i j k l m n o) 5)
       ;; Use all in a single expression
       (+ (* a b) (* c d) (* e f) (* g h)
          (* i j) (* k l) (* m n) o)))"#;
    let expect = expect_test::expect![[r#""OK (15 40 65 750 14 24 519)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let binding order: all from outer scope (unlike let*)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_parallel_binding_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate that let bindings are computed in parallel from outer scope.
    // Swap values using let (impossible with let* without a temp):
    let form = r#"(let ((x 10) (y 20))
      ;; In let, both rhs computed before any binding takes effect
      (let ((x y) (y x))
        (list x y)))"#;
    let expect = expect_test::expect![[r#""OK (20 10)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(20 10)", &oracle, &neovm);
}

#[test]
fn oracle_prop_let_parallel_binding_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple cascading swaps and the outer values remain intact
    let form = r#"(let ((a 1) (b 2) (c 3))
      (let ((new-a (+ b c))    ;; 5
            (new-b (+ a c))    ;; 4
            (new-c (+ a b)))   ;; 3
        (let ((sum (+ new-a new-b new-c))  ;; 12
              (orig-sum (+ a b c)))         ;; still 6 from outer
          (list new-a new-b new-c sum orig-sum))))"#;
    let expect = expect_test::expect![[r#""OK (5 4 3 12 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let with nil bindings (just symbols, bound to nil)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_nil_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Bare symbol in let binding list binds to nil
    let form = r#"(let (a b c (d 42) e)
      (list a b c d e
            (null a) (null b) (null c) (null e)
            (not (null d))))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil 42 nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_let_nil_binding_then_setq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Start as nil, mutate within body
    let form = r#"(let (result count)
      (setq count 0)
      (dotimes (i 5)
        (setq count (1+ count))
        (setq result (cons (* i i) result)))
      (list (nreverse result) count))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 4 9 16) 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let binding shadowing outer variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_shadowing_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three-level shadowing: each level sees its own x
    let form = r#"(let ((x 'outer)
                        (log nil))
      (setq log (cons (list 'level-0 x) log))
      (let ((x 'middle))
        (setq log (cons (list 'level-1 x) log))
        (let ((x 'inner))
          (setq log (cons (list 'level-2 x) log)))
        ;; After inner let, x is back to 'middle
        (setq log (cons (list 'back-to-1 x) log)))
      ;; After middle let, x is back to 'outer
      (setq log (cons (list 'back-to-0 x) log))
      (nreverse log))"#;
    let expect = expect_test::expect![[
        r#""OK ((level-0 outer) (level-1 middle) (level-2 inner) (back-to-1 middle) (back-to-0 outer))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested let creating closures that capture different scopes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_closure_scope_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each closure captures a different value of 'val'
    let form = r#"(let ((makers nil))
      ;; Build a list of closures, each capturing a different val
      (let ((val 10))
        (setq makers
              (cons (lambda () val) makers))
        (let ((val 20))
          (setq makers
                (cons (lambda () val) makers))
          (let ((val 30))
            (setq makers
                  (cons (lambda () val) makers)))))
      ;; Call all closures and collect results
      (mapcar #'funcall (nreverse makers)))"#;
    let expect = expect_test::expect![[r#""OK (10 20 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_let_adder_factory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Factory pattern: each let level creates an adder with captured offset
    let form = r#"(let ((adders nil))
      (dolist (offset '(1 5 10 100))
        (let ((n offset))
          (setq adders
                (cons (lambda (x) (+ x n)) adders))))
      (let ((fns (nreverse adders)))
        (list
         (mapcar (lambda (f) (funcall f 0)) fns)
         (mapcar (lambda (f) (funcall f 42)) fns)
         (mapcar (lambda (f) (funcall f -10)) fns))))"#;
    let expect = expect_test::expect![[r#""OK ((1 5 10 100) (43 47 52 142) (-9 -5 0 90))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let with complex init expressions (mapcar, funcall, arithmetic)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_complex_init_expressions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Init expressions involve mapcar, apply, funcall, conditionals
    let form = r#"(let ((data '(3 1 4 1 5 9 2 6)))
      (let ((sorted (sort (copy-sequence data) #'<))
            (sum (apply #'+ data))
            (squares (mapcar (lambda (x) (* x x)) data))
            (count (length data))
            (has-five (if (memq 5 data) t nil))
            (filtered (delq nil (mapcar (lambda (x) (when (> x 3) x)) data)))
            (max-val (apply #'max data))
            (min-val (apply #'min data)))
        (list
         sorted
         sum
         squares
         count
         has-five
         filtered
         max-val
         min-val
         ;; Derived from the bindings above
         (/ sum count)
         (- max-val min-val)
         (apply #'+ squares))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 1 2 3 4 5 6 9) 31 (9 1 16 1 25 81 4 36) 8 t (4 5 9 6) 9 1 3 8 173)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: let-based state encapsulation (closure-based object)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_state_encapsulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Emulate an object with private state using let-over-lambda
    let form = r#"(let ((make-counter
                   (lambda (initial)
                     (let ((count initial)
                           (history nil))
                       ;; Return an alist of "methods"
                       (list
                        (cons 'get (lambda () count))
                        (cons 'inc (lambda ()
                                     (setq history (cons count history))
                                     (setq count (1+ count))
                                     count))
                        (cons 'dec (lambda ()
                                     (setq history (cons count history))
                                     (setq count (1- count))
                                     count))
                        (cons 'add (lambda (n)
                                     (setq history (cons count history))
                                     (setq count (+ count n))
                                     count))
                        (cons 'history (lambda () (nreverse (copy-sequence history))))
                        (cons 'reset (lambda ()
                                       (setq history (cons count history))
                                       (setq count initial)
                                       count)))))))
      ;; Helper to "send a message"
      (let ((send (lambda (obj msg &rest args)
                    (let ((method (cdr (assq msg obj))))
                      (if method
                          (apply method args)
                        (signal 'error (list "unknown method" msg)))))))
        ;; Create two independent counters
        (let ((c1 (funcall make-counter 0))
              (c2 (funcall make-counter 100)))
          (funcall send c1 'inc)
          (funcall send c1 'inc)
          (funcall send c1 'inc)
          (funcall send c1 'add 10)
          (funcall send c2 'dec)
          (funcall send c2 'dec)
          (funcall send c2 'add -50)
          (funcall send c1 'dec)
          (funcall send c2 'reset)
          (list
           (funcall send c1 'get)
           (funcall send c2 'get)
           (funcall send c1 'history)
           (funcall send c2 'history)))))"#;
    let expect = expect_test::expect![[r#""OK (12 100 (0 1 2 3 13) (100 99 98 48))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
