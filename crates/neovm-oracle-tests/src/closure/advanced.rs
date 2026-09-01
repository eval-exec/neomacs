//! Advanced oracle parity tests for closure semantics: capturing,
//! mutable state, shared environments, factory patterns, nested
//! closures, rest args, and closure-based object dispatch.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Closure capturing variables from outer let with shadowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_capture_with_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closure captures from the correct lexical scope when there are
    // multiple levels of shadowing.
    let form = r#"(let ((x 1) (y 10))
      (let ((f (let ((x 2) (z 100))
                 (lambda () (list x y z)))))
        (let ((x 999) (y 999))
          ;; x and y here are different bindings; closure sees x=2, y=10, z=100
          (funcall f))))"#;
    let expect = expect_test::expect![r#""OK (2 10 100)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure over mutable state (counter pattern with reset)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_mutable_counter_with_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Counter object with increment, decrement, get, and reset.
    let form = r#"(let ((count 0))
      (let ((inc   (lambda (&optional n) (setq count (+ count (or n 1)))))
            (dec   (lambda () (setq count (1- count))))
            (get   (lambda () count))
            (reset (lambda () (setq count 0))))
        (funcall inc)
        (funcall inc)
        (funcall inc 5)
        (let ((after-inc (funcall get)))
          (funcall dec)
          (let ((after-dec (funcall get)))
            (funcall reset)
            (list after-inc after-dec (funcall get))))))"#;
    let expect = expect_test::expect![[r#""OK (7 6 0)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(7 6 0)", &o, &n);
}

// ---------------------------------------------------------------------------
// Multiple closures sharing the same environment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closures_shared_environment_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A stack implemented with push/pop/peek closures sharing one list.
    let form = r#"(let ((stack nil))
      (let ((push  (lambda (x) (setq stack (cons x stack))))
            (pop   (lambda ()
                     (let ((top (car stack)))
                       (setq stack (cdr stack))
                       top)))
            (peek  (lambda () (car stack)))
            (size  (lambda () (length stack)))
            (items (lambda () (copy-sequence stack))))
        (funcall push 'a)
        (funcall push 'b)
        (funcall push 'c)
        (let ((s1 (funcall items))
              (p1 (funcall peek)))
          (funcall pop)
          (funcall pop)
          (funcall push 'z)
          (list s1 p1
                (funcall items)
                (funcall size)
                (funcall peek)))))"#;
    let expect = expect_test::expect![r#""OK ((c b a) c (z a) 2 z)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure as callback (mapcar with complex lambda)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_mapcar_complex_callback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a pipeline of transformations using closures as callbacks
    // in mapcar chains.
    let form = r#"(let ((multiplier 3)
                        (offset 10)
                        (data '(1 2 3 4 5)))
      (let ((step1 (mapcar (lambda (x) (* x multiplier)) data))
            (step2 (mapcar (lambda (x) (+ x offset))
                           (mapcar (lambda (x) (* x multiplier)) data))))
        ;; Nested mapcar: filter even, square, convert to strings
        (let ((step3 (mapcar
                       (lambda (x) (number-to-string (* x x)))
                       (delq nil
                             (mapcar (lambda (x)
                                       (if (= (% x 2) 0) x nil))
                                     step2)))))
          (list step1 step2 step3))))"#;
    let expect =
        expect_test::expect![[r#""OK ((3 6 9 12 15) (13 16 19 22 25) (\"256\" \"484\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Closure returned from function (factory pattern)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_factory_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Factory functions that return configured closures.
    // Each closure captures its own independent parameters.
    let form = r#"(let ((make-validator
                     (lambda (min-val max-val)
                       (lambda (x)
                         (and (>= x min-val) (<= x max-val)))))
                    (make-formatter
                     (lambda (prefix suffix)
                       (lambda (s) (concat prefix s suffix)))))
      (let ((valid-age (funcall make-validator 0 150))
            (valid-score (funcall make-validator 0 100))
            (html-bold (funcall make-formatter "<b>" "</b>"))
            (parens (funcall make-formatter "(" ")")))
        (list
          (funcall valid-age 25)
          (funcall valid-age -1)
          (funcall valid-age 200)
          (funcall valid-score 85)
          (funcall valid-score 101)
          (funcall html-bold "hello")
          (funcall parens "test"))))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil \"<b>hello</b>\" \"(test)\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested closures (closure returning closure)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_nested_returning_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A closure returns another closure, forming a chain of captured
    // environments at different levels.
    let form = r#"(let ((make-curried-add
                     (lambda (a)
                       (lambda (b)
                         (lambda (c) (+ a b c))))))
      (let ((add-10 (funcall make-curried-add 10)))
        (let ((add-10-20 (funcall add-10 20)))
          (let ((add-1 (funcall make-curried-add 1))
                (add-100 (funcall make-curried-add 100)))
            (list
              (funcall add-10-20 30)
              (funcall (funcall add-1 2) 3)
              (funcall (funcall add-100 200) 300)
              ;; Verify each factory call creates independent closures
              (funcall (funcall (funcall make-curried-add 0) 0) 0)
              (funcall add-10-20 0))))))"#;
    let expect = expect_test::expect![[r#""OK (60 6 600 0 30)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(60 6 600 0 30)", &o, &n);
}

// ---------------------------------------------------------------------------
// Closure with rest args (&rest)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_rest_args_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Closures with &rest that process variable arguments in non-trivial ways.
    let form = r#"(let ((make-logger
                     (lambda (tag)
                       (lambda (&rest parts)
                         (concat "[" tag "] "
                                 (mapconcat
                                   (lambda (p)
                                     (cond ((stringp p) p)
                                           ((numberp p) (number-to-string p))
                                           ((symbolp p) (symbol-name p))
                                           (t "?")))
                                   parts " ")))))
                    (make-aggregator
                     (lambda (op init)
                       (lambda (&rest vals)
                         (let ((result init))
                           (dolist (v vals)
                             (setq result (funcall op result v)))
                           result)))))
      (let ((info (funcall make-logger "INFO"))
            (err  (funcall make-logger "ERR"))
            (sum  (funcall make-aggregator #'+ 0))
            (prod (funcall make-aggregator #'* 1)))
        (list
          (funcall info "user" 'logged-in 42)
          (funcall err "fail" 'code 500)
          (funcall sum 1 2 3 4 5)
          (funcall prod 2 3 4)
          (funcall sum)
          (funcall prod))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[INFO] user logged-in 42\" \"[ERR] fail code 500\" 15 24 0 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: closure-based object system (dispatch on message)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_closure_object_system_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate an object system using closures: a "bank account" object
    // that responds to messages.
    let form = r#"(let ((make-account
                     (lambda (initial-balance owner)
                       (let ((balance initial-balance)
                             (history nil))
                         (lambda (msg &rest args)
                           (cond
                             ((eq msg 'deposit)
                              (let ((amount (car args)))
                                (setq balance (+ balance amount))
                                (setq history (cons (list 'deposit amount balance) history))
                                balance))
                             ((eq msg 'withdraw)
                              (let ((amount (car args)))
                                (if (> amount balance)
                                    (signal 'error (list "Insufficient funds"))
                                  (setq balance (- balance amount))
                                  (setq history (cons (list 'withdraw amount balance) history))
                                  balance)))
                             ((eq msg 'balance) balance)
                             ((eq msg 'owner) owner)
                             ((eq msg 'history) (reverse history))
                             (t (signal 'error (list "Unknown message" msg)))))))))
      (let ((acct1 (funcall make-account 1000 "Alice"))
            (acct2 (funcall make-account 500 "Bob")))
        (funcall acct1 'deposit 200)
        (funcall acct1 'withdraw 150)
        (funcall acct2 'deposit 300)
        (funcall acct2 'withdraw 100)
        (funcall acct1 'deposit 50)
        (list
          (funcall acct1 'owner)
          (funcall acct1 'balance)
          (funcall acct2 'owner)
          (funcall acct2 'balance)
          (funcall acct1 'history)
          (length (funcall acct2 'history)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\" 1100 \"Bob\" 700 ((deposit 200 1200) (withdraw 150 1050) (deposit 50 1100)) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
