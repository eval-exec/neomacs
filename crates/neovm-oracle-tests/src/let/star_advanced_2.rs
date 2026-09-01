//! Oracle parity tests for advanced `let*` patterns (part 2):
//! deep dependency chains, destructuring idioms, shadow/restore,
//! computed bindings, accumulator patterns, side-effectful bindings,
//! nested cross-scope references, and condition-case integration.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Deep sequential dependency chain with mixed arithmetic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star2_deep_dependency_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each binding depends on all previous bindings, not just the last one.
    // This tests that all prior bindings are visible in each init form.
    let form = r#"(let* ((a 2)
                         (b (* a 3))               ;; 6
                         (c (+ a b))                ;; 8
                         (d (- (* c b) a))          ;; 46
                         (e (/ (+ d c) (- b a)))    ;; 13  (54/4 = 13)
                         (f (mod d (+ a b)))        ;; 6   (46 mod 8)
                         (g (logand (+ a b c d e f) #xff))  ;; 75 & 255 = 75
                         (h (ash g -2)))            ;; 18  (75 >> 2)
                    (list a b c d e f g h
                          ;; Verify the chain is consistent
                          (= d (- (* c b) a))
                          (= e (/ (+ d c) (- b a)))
                          (= f (mod d (+ a b)))))"#;
    let expect = expect_test::expect![[r#""OK (2 6 8 46 13 6 81 20 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Destructuring nested alists via car/cdr/assoc in let* bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star2_destructure_nested_alists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a complex nested alist structure step by step
    let form = r#"(let* ((data '((name . "Alice")
                                  (address . ((street . "123 Main")
                                              (city . "Springfield")
                                              (zip . 62701)))
                                  (scores . (95 87 92 78 100))))
                         (name (cdr (assq 'name data)))
                         (addr-alist (cdr (assq 'address data)))
                         (street (cdr (assq 'street addr-alist)))
                         (city (cdr (assq 'city addr-alist)))
                         (zip (cdr (assq 'zip addr-alist)))
                         (scores (cdr (assq 'scores data)))
                         (top-score (apply #'max scores))
                         (avg-score (/ (float (apply #'+ scores))
                                       (length scores)))
                         (passing (length (seq-filter
                                           (lambda (s) (>= s 80))
                                           scores))))
                    (list name street city zip
                          top-score avg-score passing
                          (format "%s, %s %d" street city zip)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" \"123 Main\" \"Springfield\" 62701 100 90.4 4 \"123 Main, Springfield 62701\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Shadow + restore pattern: outer let, inner let* rebinding, verify restore
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star2_shadow_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Outer bindings are shadowed by inner let*, then restored after
    let form = r#"(let ((x 100)
                        (y 200)
                        (z 300))
                    (let* ((before-x x)
                           (before-y y)
                           (inner-result
                            (let* ((x (* x 2))       ;; shadow x=200
                                   (y (+ x y))       ;; y = 200+200 = 400
                                   (z (- y x)))      ;; z = 400-200 = 200
                              (list x y z)))
                           (after-x x)
                           (after-y y)
                           (after-z z))
                      (list
                       ;; Before entering inner let*
                       (list 'before before-x before-y)
                       ;; Inner result (shadowed values)
                       (list 'inner inner-result)
                       ;; After inner let*: originals restored
                       (list 'after after-x after-y after-z)
                       ;; Verify restoration
                       (list (= after-x 100)
                             (= after-y 200)
                             (= after-z 300)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((before 100 200) (inner (200 400 200)) (after 100 200 300) (t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Computed bindings: funcall, lambda, apply in init forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star2_computed_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Binding values come from function calls, inline lambdas, and apply
    let form = r#"(let* ((make-pair (lambda (a b) (cons a b)))
                         (pair (funcall make-pair 'hello 'world))
                         (transform (lambda (lst fn)
                                      (mapcar fn lst)))
                         (nums '(1 2 3 4 5))
                         (doubled (funcall transform nums
                                          (lambda (x) (* x 2))))
                         (sum-fn (lambda (&rest args) (apply #'+ args)))
                         (total (apply sum-fn doubled))
                         (compose (lambda (f g)
                                    (lambda (x) (funcall f (funcall g x)))))
                         (inc-then-double (funcall compose
                                                   (lambda (x) (* x 2))
                                                   (lambda (x) (+ x 1))))
                         (composed-results (mapcar inc-then-double nums)))
                    (list pair doubled total composed-results))"#;
    let expect = expect_test::expect![[r#""OK ((hello . world) (2 4 6 8 10) 30 (4 6 8 10 12))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Accumulator pattern: building a result through sequential bindings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star2_accumulator_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process a dataset step by step, each binding refining the result
    let form = r#"(let* ((raw-data '(("Alice" 95) ("Bob" 72)
                                      ("Carol" 88) ("Dave" 45)
                                      ("Eve" 91) ("Frank" 68)))
                         ;; Step 1: Convert to alist
                         (records (mapcar (lambda (r)
                                           (cons (car r) (cadr r)))
                                         raw-data))
                         ;; Step 2: Filter passing (>= 70)
                         (passing (seq-filter (lambda (r) (>= (cdr r) 70))
                                             records))
                         ;; Step 3: Sort by score descending
                         (sorted (sort (copy-sequence passing)
                                       (lambda (a b) (> (cdr a) (cdr b)))))
                         ;; Step 4: Extract top 3
                         (top3 (seq-take sorted 3))
                         ;; Step 5: Format as strings
                         (formatted (mapcar
                                     (lambda (r)
                                       (format "%s: %d" (car r) (cdr r)))
                                     top3))
                         ;; Step 6: Summary stats
                         (all-scores (mapcar #'cdr records))
                         (mean (/ (float (apply #'+ all-scores))
                                  (length all-scores)))
                         (above-mean (length (seq-filter
                                              (lambda (r) (> (cdr r) mean))
                                              records))))
                    (list (length records)
                          (length passing)
                          (mapcar #'car top3)
                          formatted
                          above-mean))"#;
    let expect = expect_test::expect![[
        r#""OK (6 4 (\"Alice\" \"Eve\" \"Carol\") (\"Alice: 95\" \"Eve: 91\" \"Carol: 88\") 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Side effects in let* bindings: buffer insert during init
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star2_side_effects_in_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each binding performs a side effect (buffer insert), and later
    // bindings depend on the buffer state left by earlier ones
    let form = r#"(with-temp-buffer
                    (let* ((_ (insert "Hello"))
                           (after-hello (buffer-string))
                           (pos1 (point))
                           (_ (insert " World"))
                           (after-world (buffer-string))
                           (pos2 (point))
                           (_ (goto-char pos1))
                           (_ (insert ", Beautiful"))
                           (after-insert (buffer-string))
                           (pos3 (point))
                           (_ (goto-char (point-min)))
                           (_ (insert ">>> "))
                           (final (buffer-string))
                           (total-len (length final))
                           (line-count (count-lines (point-min) (point-max))))
                      (list after-hello
                            after-world
                            after-insert
                            final
                            pos1 pos2 pos3
                            total-len
                            line-count)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello\" \"Hello World\" \"Hello, Beautiful World\" \">>> Hello, Beautiful World\" 6 12 17 26 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested let* with cross-scope references via closures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star2_nested_cross_scope_closures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Outer let* creates closures, inner let* calls them and creates
    // new closures that reference outer scope
    let form = r#"(let* ((x 10)
                         (make-adder (lambda (n) (lambda (v) (+ v n))))
                         (add-x (funcall make-adder x))
                         (y (funcall add-x 5)))   ;; y = 15
                    (let* ((z (* x y))             ;; z = 150
                           (scale (lambda (factor)
                                    ;; References outer x and inner z
                                    (list (* x factor) (* z factor))))
                           (scaled-2 (funcall scale 2))
                           (scaled-half (funcall scale 0.5))
                           (combine (lambda ()
                                      ;; References across both scopes
                                      (+ x y z))))
                      (list x y z
                            scaled-2
                            scaled-half
                            (funcall combine)
                            ;; Verify add-x still works
                            (funcall add-x 100))))"#;
    let expect = expect_test::expect![[r#""OK (10 15 150 (20 300) (5.0 75.0) 175 110)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// let* with condition-case in bindings: error handling during init
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_let_star2_condition_case_in_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Some bindings might fail; use condition-case to provide fallbacks.
    // Later bindings use the (possibly fallback) values from earlier ones.
    let form = r#"(let* ((safe-div (lambda (a b)
                                      (condition-case err
                                          (/ a b)
                                        (arith-error
                                         (list 'div-by-zero a)))))
                         (r1 (funcall safe-div 10 3))
                         (r2 (funcall safe-div 10 0))
                         (r3 (funcall safe-div (if (numberp r1) (* r1 6) 0) 2))
                         ;; Parse a number, fallback on error
                         (safe-parse (lambda (s)
                                       (condition-case nil
                                           (string-to-number s)
                                         (error 0))))
                         (n1 (funcall safe-parse "42"))
                         (n2 (funcall safe-parse "not-a-number"))
                         ;; Chain: use parsed values
                         (total (+ (if (numberp r1) r1 0)
                                   n1 n2))
                         ;; Nested condition-case with throw/catch interaction
                         (nested-result
                          (condition-case outer-err
                              (let* ((a 100)
                                     (b (condition-case inner-err
                                            (/ a 0)
                                          (arith-error
                                           (+ a 50)))))  ;; fallback: 150
                                (+ a b))  ;; 250
                            (error (list 'outer-failed outer-err)))))
                    (list r1 r2 r3
                          n1 n2 total
                          nested-result))"#;
    let expect = expect_test::expect![[r#""OK (3 (div-by-zero 10) 9 42 0 45 250)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
