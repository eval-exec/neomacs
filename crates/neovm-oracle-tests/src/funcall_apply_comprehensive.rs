//! Comprehensive oracle parity tests for funcall/apply patterns:
//! funcall with 0-10+ args, apply with various arg list constructions,
//! funcall vs apply equivalence, funcall/apply with lambda/closures/subrs,
//! apply with improper arg lists, nested funcall/apply, funcall with &rest
//! functions, apply spreading behavior, higher-order function composition
//! via funcall chains, funcall-interactively, and macros (should error).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// funcall with 0 through 10+ arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_arg_count_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK zero-args""#]];
    // 0 args
    crate::common::assert_oracle_parity_expect("(funcall (lambda () 'zero-args))", expect);
    let expect = expect_test::expect![[r#""OK one""#]];
    // 1 arg
    crate::common::assert_oracle_parity_expect("(funcall (lambda (a) a) 'one)", expect);
    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    // 2 args
    crate::common::assert_oracle_parity_expect("(funcall (lambda (a b) (list a b)) 1 2)", expect);
    let expect = expect_test::expect![[r#""OK 60""#]];
    // 3 args
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a b c) (+ a b c)) 10 20 30)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK 21""#]];
    // 4 args
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a b c d) (* (+ a b) (+ c d))) 1 2 3 4)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (5 4 3 2 1)""#]];
    // 5 args
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a b c d e) (list e d c b a)) 1 2 3 4 5)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK 21""#]];
    // 6 args
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a b c d e f) (+ a b c d e f)) 1 2 3 4 5 6)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (1 5 22)""#]];
    // 7 args
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a b c d e f g) (list a (+ b c) (+ d e f g))) 1 2 3 4 5 6 7)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK 36""#]];
    // 8 args via built-in +
    crate::common::assert_oracle_parity_expect("(funcall #'+ 1 2 3 4 5 6 7 8)", expect);
    let expect = expect_test::expect![[r#""OK 45""#]];
    // 9 args
    crate::common::assert_oracle_parity_expect("(funcall #'+ 1 2 3 4 5 6 7 8 9)", expect);
    let expect = expect_test::expect![[r#""OK 55""#]];
    // 10 args
    crate::common::assert_oracle_parity_expect("(funcall #'+ 1 2 3 4 5 6 7 8 9 10)", expect);
    let expect = expect_test::expect![[r#""OK (a b c d e f g h i j k l)""#]];
    // 12 args via list
    crate::common::assert_oracle_parity_expect(
        "(funcall #'list 'a 'b 'c 'd 'e 'f 'g 'h 'i 'j 'k 'l)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"abcdefghijklmno\"""#]];
    // 15 args via concat
    crate::common::assert_oracle_parity_expect(
        r#"(funcall #'concat "a" "b" "c" "d" "e" "f" "g" "h" "i" "j" "k" "l" "m" "n" "o")"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// funcall vs apply equivalence for various arg patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_apply_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    // funcall with explicit args should equal apply with those args as a list
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (lambda (a b c) (+ a b c))))
             (list (= (funcall f 10 20 30)
                       (apply f '(10 20 30)))
                   (= (funcall f 1 2 3)
                       (apply f 1 '(2 3)))
                   (= (funcall f 100 200 300)
                       (apply f 100 200 '(300)))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    // Equivalence with &rest
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (lambda (&rest xs) (apply #'+ xs))))
             (list (equal (funcall f) (apply f nil))
                   (equal (funcall f 1) (apply f '(1)))
                   (equal (funcall f 1 2 3) (apply f '(1 2 3)))
                   (equal (funcall f 1 2 3 4 5) (apply f 1 2 3 '(4 5)))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    // Equivalence with &optional
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (lambda (a &optional b c) (list a b c))))
             (list (equal (funcall f 1) (apply f '(1)))
                   (equal (funcall f 1 2) (apply f 1 '(2)))
                   (equal (funcall f 1 2 3) (apply f '(1 2 3)))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    // Equivalence with subrs
    crate::common::assert_oracle_parity_expect(
        r#"(list (equal (funcall #'list 1 2 3) (apply #'list '(1 2 3)))
               (equal (funcall #'+ 10 20) (apply #'+ '(10 20)))
               (equal (funcall #'cons 'a 'b) (apply #'cons '(a b)))
               (equal (funcall #'concat "x" "y") (apply #'concat '("x" "y"))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// apply spreading behavior: multiple spread args before final list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_spreading_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 15""#]];
    // 0 spread args + full list
    crate::common::assert_oracle_parity_expect("(apply #'+ '(1 2 3 4 5))", expect);
    let expect = expect_test::expect![[r#""OK 103""#]];
    // 1 spread + list
    crate::common::assert_oracle_parity_expect("(apply #'+ 100 '(1 2))", expect);
    let expect = expect_test::expect![[r#""OK (a b c d)""#]];
    // 2 spread + list
    crate::common::assert_oracle_parity_expect("(apply #'list 'a 'b '(c d))", expect);
    let expect = expect_test::expect![[r#""OK 21""#]];
    // 3 spread + list
    crate::common::assert_oracle_parity_expect("(apply #'+ 1 2 3 '(4 5 6))", expect);
    let expect = expect_test::expect![[r#""OK (a b c d e f)""#]];
    // 4 spread + list
    crate::common::assert_oracle_parity_expect("(apply #'list 'a 'b 'c 'd '(e f))", expect);
    let expect = expect_test::expect![[r#""OK 15""#]];
    // 5 spread + empty list (all from spread)
    crate::common::assert_oracle_parity_expect("(apply #'+ 1 2 3 4 5 '())", expect);
    let expect = expect_test::expect![[r#""OK (1 2 3 4 5 6)""#]];
    // 6 spread + nil
    crate::common::assert_oracle_parity_expect("(apply #'list 1 2 3 4 5 6 nil)", expect);
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p (+ 1 1))""#]];
    // Spread args are complex expressions
    crate::common::assert_oracle_parity_expect(
        "(apply #'+ (* 2 3) (+ 4 5) (- 10 3) '((+ 1 1)))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (head a b)""#]];
    // Apply with cons-constructed final arg
    crate::common::assert_oracle_parity_expect(
        "(apply #'list 'head (cons 'a (cons 'b nil)))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK 10""#]];
    // Apply with append-constructed final arg
    crate::common::assert_oracle_parity_expect("(apply #'+ (append '(1 2) '(3 4)))", expect);
    let expect = expect_test::expect![[r#""OK 15""#]];
    // Apply with mapcar-constructed final arg
    crate::common::assert_oracle_parity_expect("(apply #'+ (mapcar #'1+ '(0 1 2 3 4)))", expect);
}

// ---------------------------------------------------------------------------
// apply with empty and nil arg lists
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_empty_and_nil_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    // apply + with empty list => 0
    crate::common::assert_oracle_parity_expect("(apply #'+ '())", expect);
    let expect = expect_test::expect![[r#""OK 0""#]];
    // apply + with nil => 0
    crate::common::assert_oracle_parity_expect("(apply #'+ nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    // apply list with nil => ()
    crate::common::assert_oracle_parity_expect("(apply #'list nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    // apply list with empty list => ()
    crate::common::assert_oracle_parity_expect("(apply #'list '())", expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    // apply concat with nil => ""
    crate::common::assert_oracle_parity_expect("(apply #'concat nil)", expect);
    let expect = expect_test::expect![[r#""OK (a b c)""#]];
    // apply with spread args and empty final list
    crate::common::assert_oracle_parity_expect("(apply #'list 'a 'b 'c '())", expect);
    let expect = expect_test::expect![[r#""OK 42""#]];
    // apply with only a single-element list
    crate::common::assert_oracle_parity_expect("(apply #'1+ '(41))", expect);
    let expect = expect_test::expect![[r#""OK 55""#]];
    // apply with deeply nested argument construction
    crate::common::assert_oracle_parity_expect(
        "(apply #'+ (let ((r nil)) (dotimes (i 10) (setq r (cons (1+ i) r))) (nreverse r)))",
        expect,
    );
}

// ---------------------------------------------------------------------------
// funcall/apply with closures capturing mutable state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_apply_closures_mutable_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 7 17 18 18)""#]];
    // Counter closure called via funcall and apply
    crate::common::assert_oracle_parity_expect(
        r#"(let ((n 0))
             (let ((inc (lambda (&optional amount)
                          (setq n (+ n (or amount 1)))
                          n)))
               (list (funcall inc)
                     (funcall inc)
                     (funcall inc 5)
                     (apply inc '(10))
                     (apply inc nil)
                     n)))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (1 3 6 6 (a b c d e f))""#]];
    // Closure over a list, mutated via nconc
    crate::common::assert_oracle_parity_expect(
        r#"(let ((log nil))
             (let ((logger (lambda (&rest msgs)
                             (setq log (append log msgs))
                             (length log))))
               (list (funcall logger 'a)
                     (funcall logger 'b 'c)
                     (apply logger '(d e f))
                     (funcall logger)
                     log)))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (12 24 16 28 4)""#]];
    // Closure factory: each call creates a new closure sharing state
    crate::common::assert_oracle_parity_expect(
        r#"(let ((state 0))
             (let ((make-adder (lambda (base)
                                 (lambda (x)
                                   (setq state (+ state 1))
                                   (+ base x state)))))
               (let ((add10 (funcall make-adder 10))
                     (add20 (funcall make-adder 20)))
                 (list (funcall add10 1)
                       (funcall add20 2)
                       (funcall add10 3)
                       (apply add20 '(4))
                       state))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// funcall/apply with subrs: all major built-in types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_apply_subr_variety() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 6""#]];
    // Arithmetic subrs
    crate::common::assert_oracle_parity_expect("(funcall #'+ 1 2 3)", expect);
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'- 10 3 2)", expect);
    let expect = expect_test::expect![[r#""OK 24""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'* 2 3 4)", expect);
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'/ 100 5 4)", expect);
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'% 17 5)", expect);
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'mod 17 5)", expect);

    let expect = expect_test::expect![[r#""OK t""#]];
    // Comparison subrs
    crate::common::assert_oracle_parity_expect("(funcall #'< 1 2 3)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'> 3 2 1)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'= 5 5 5)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'<= 1 1 2)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'>= 3 3 2)", expect);

    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    // String subrs
    crate::common::assert_oracle_parity_expect(r#"(funcall #'concat "hello" " " "world")"#, expect);
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(r#"(funcall #'string-to-number "42")"#, expect);
    let expect = expect_test::expect![[r#""OK \"world\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(funcall #'substring "hello world" 6)"#, expect);
    let expect = expect_test::expect![[r#""OK \"ABC\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(funcall #'upcase "abc")"#, expect);

    let expect = expect_test::expect![[r#""OK (a b c)""#]];
    // List subrs
    crate::common::assert_oracle_parity_expect("(funcall #'cons 'a '(b c))", expect);
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'car '(1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK (2 3)""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'cdr '(1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'length '(a b c d))", expect);
    let expect = expect_test::expect![[r#""OK c""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'nth 2 '(a b c d))", expect);
    let expect = expect_test::expect![[r#""OK (c d)""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'nthcdr 2 '(a b c d))", expect);
    let expect = expect_test::expect![[r#""OK (5 4 3 2 1)""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'reverse '(1 2 3 4 5))", expect);
    let expect = expect_test::expect![[r#""OK (1 2 3 4 5)""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'append '(1 2) '(3 4) '(5))", expect);

    let expect = expect_test::expect![[r#""OK t""#]];
    // Predicate subrs
    crate::common::assert_oracle_parity_expect("(funcall #'null nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'null t)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'numberp 42)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'stringp \"hi\")", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'symbolp 'foo)", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'consp '(1))", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'listp '(1 2))", expect);
}

// ---------------------------------------------------------------------------
// apply with non-list final arg should error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_non_list_final_arg_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (got-error wrong-type-argument)""#]];
    // apply with integer as final arg
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (apply #'+ 1 2 3)
           (wrong-type-argument (list 'got-error (car err))))"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK (got-error wrong-type-argument)""#]];
    // apply with string as final arg
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (apply #'+ "not-a-list")
           (wrong-type-argument (list 'got-error (car err))))"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK (got-error wrong-type-argument)""#]];
    // apply with symbol as final arg (not nil)
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (apply #'+ 'not-a-list)
           (wrong-type-argument (list 'got-error (car err))))"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK (got-error wrong-type-argument)""#]];
    // apply with vector as final arg
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (apply #'+ [1 2 3])
           (wrong-type-argument (list 'got-error (car err))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// funcall with macros should error (invalid-function)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_macro_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Calling a macro via funcall should signal invalid-function or similar
    let form = r#"(progn
      (defmacro neovm--test-fac-mac (x) (list '+ x 1))
      (unwind-protect
          (condition-case err
              (funcall (symbol-function 'neovm--test-fac-mac) 5)
            (invalid-function (list 'invalid-function-caught))
            (error (list 'other-error (car err))))
        (fmakunbound 'neovm--test-fac-mac)))"#;
    let expect = expect_test::expect![[r#""OK (invalid-function-caught)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);

    // apply with macro should also error
    let form2 = r#"(progn
      (defmacro neovm--test-fac-mac2 (x) (list '* x 2))
      (unwind-protect
          (condition-case err
              (apply (symbol-function 'neovm--test-fac-mac2) '(5))
            (invalid-function (list 'invalid-function-caught))
            (error (list 'other-error (car err))))
        (fmakunbound 'neovm--test-fac-mac2)))"#;
    let expect = expect_test::expect![[r#""OK (invalid-function-caught)""#]];
    crate::common::assert_oracle_parity_expect(form2, expect);
}

// ---------------------------------------------------------------------------
// Nested funcall/apply chains: currying, composition, pipelines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_funcall_apply_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4)""#]];
    // 4-level currying
    crate::common::assert_oracle_parity_expect(
        r#"(let ((f (lambda (a)
                     (lambda (b)
                       (lambda (c)
                         (lambda (d) (list a b c d)))))))
             (funcall (funcall (funcall (funcall f 1) 2) 3) 4))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK 15""#]];
    // funcall result as arg to apply
    crate::common::assert_oracle_parity_expect(r#"(apply #'+ (funcall #'list 1 2 3 4 5))"#, expect);

    let expect = expect_test::expect![[r#""OK 11""#]];
    // apply result as arg to funcall
    crate::common::assert_oracle_parity_expect(r#"(funcall #'1+ (apply #'+ '(1 2 3 4)))"#, expect);

    let expect = expect_test::expect![[r#""OK (14 15)""#]];
    // Double composition
    crate::common::assert_oracle_parity_expect(
        r#"(let ((compose (lambda (f g) (lambda (&rest args) (funcall f (apply g args))))))
             (let ((add1-then-double (funcall compose (lambda (x) (* x 2)) #'+))
                   (double-then-add1 (funcall compose #'1+ (lambda (&rest xs) (* 2 (apply #'+ xs))))))
               (list (funcall add1-then-double 3 4)
                     (funcall double-then-add1 3 4))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK 29""#]];
    // Pipeline of 5 functions
    crate::common::assert_oracle_parity_expect(
        r#"(let ((pipe (lambda (fns val)
                         (let ((result val))
                           (dolist (f fns result)
                             (setq result (funcall f result)))))))
             (funcall pipe
                      (list #'1+ (lambda (x) (* x 3)) #'1+ (lambda (x) (- x 5)) #'abs)
                      10))"#,
        expect,
    );

    // Mutual recursion via funcall depth=20
    let form = r#"(progn
      (fset 'neovm--test-fac-is-even
        (lambda (n) (if (= n 0) t (funcall 'neovm--test-fac-is-odd (1- n)))))
      (fset 'neovm--test-fac-is-odd
        (lambda (n) (if (= n 0) nil (funcall 'neovm--test-fac-is-even (1- n)))))
      (unwind-protect
          (list (funcall 'neovm--test-fac-is-even 0)
                (funcall 'neovm--test-fac-is-even 1)
                (funcall 'neovm--test-fac-is-even 10)
                (funcall 'neovm--test-fac-is-even 19)
                (funcall 'neovm--test-fac-is-odd 0)
                (funcall 'neovm--test-fac-is-odd 1)
                (funcall 'neovm--test-fac-is-odd 20)
                (funcall 'neovm--test-fac-is-odd 21))
        (fmakunbound 'neovm--test-fac-is-even)
        (fmakunbound 'neovm--test-fac-is-odd)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// funcall with &rest: various combinations of fixed + rest args
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_rest_arg_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 0""#]];
    // Pure &rest, 0 args
    crate::common::assert_oracle_parity_expect("(funcall (lambda (&rest xs) (length xs)))", expect);
    let expect = expect_test::expect![[r#""OK (42)""#]];
    // Pure &rest, 1 arg
    crate::common::assert_oracle_parity_expect("(funcall (lambda (&rest xs) xs) 42)", expect);
    let expect = expect_test::expect![[r#""OK (1 2 3 4 5 6 7 8)""#]];
    // Pure &rest, many args
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (&rest xs) xs) 1 2 3 4 5 6 7 8)",
        expect,
    );

    let expect = expect_test::expect![[r#""OK (only nil)""#]];
    // 1 required + &rest, 0 rest
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a &rest xs) (list a xs)) 'only)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (h 5 15)""#]];
    // 1 required + &rest, many rest
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a &rest xs) (list a (length xs) (apply #'+ xs))) 'h 1 2 3 4 5)",
        expect,
    );

    let expect = expect_test::expect![[r#""OK (1 2 nil)""#]];
    // 2 required + &rest
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a b &rest xs) (list a b xs)) 1 2)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (1 2 (3 4 5))""#]];
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a b &rest xs) (list a b xs)) 1 2 3 4 5)",
        expect,
    );

    let expect = expect_test::expect![[r#""OK (1 nil nil)""#]];
    // 1 required + 1 optional + &rest
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a &optional b &rest xs) (list a b xs)) 1)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (1 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a &optional b &rest xs) (list a b xs)) 1 2)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (1 2 (3 4))""#]];
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (a &optional b &rest xs) (list a b xs)) 1 2 3 4)",
        expect,
    );

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    // 2 optional + &rest
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (&optional a b &rest xs) (list a b xs)))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (x nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (&optional a b &rest xs) (list a b xs)) 'x)",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (x y (z1 z2))""#]];
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (&optional a b &rest xs) (list a b xs)) 'x 'y 'z1 'z2)",
        expect,
    );
}

// ---------------------------------------------------------------------------
// apply spreading with &rest functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_spreading_with_rest() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (first second third)""#]];
    // apply + rest: spread args contribute to both fixed and rest
    crate::common::assert_oracle_parity_expect(
        "(apply (lambda (a &rest xs) (cons a xs)) 'first '(second third))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (1 2 (3 4 5))""#]];
    crate::common::assert_oracle_parity_expect(
        "(apply (lambda (a b &rest xs) (list a b xs)) 1 2 '(3 4 5))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (1 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(apply (lambda (a b &rest xs) (list a b xs)) 1 '(2))",
        expect,
    );

    let expect = expect_test::expect![[r#""OK (10 20 30 (40 50))""#]];
    // apply where all args come from the spread list
    crate::common::assert_oracle_parity_expect(
        "(apply (lambda (a b c &rest xs) (list a b c xs)) '(10 20 30 40 50))",
        expect,
    );

    let expect = expect_test::expect![[r#""OK 55""#]];
    // apply with dynamically constructed arg list
    crate::common::assert_oracle_parity_expect(
        r#"(let ((args (number-sequence 1 10)))
             (apply (lambda (&rest xs) (apply #'+ xs)) args))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Higher-order function composition via funcall chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_higher_order_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((2 4 6 8 10) (1 3 5 7 9) 30 945)""#]];
    // map + filter + reduce via funcall
    crate::common::assert_oracle_parity_expect(
        r#"(let ((my-filter (lambda (pred lst)
                              (let ((result nil))
                                (dolist (x lst (nreverse result))
                                  (when (funcall pred x)
                                    (setq result (cons x result)))))))
               (my-reduce (lambda (fn init lst)
                            (let ((acc init))
                              (dolist (x lst acc)
                                (setq acc (funcall fn acc x)))))))
             (let* ((data '(1 2 3 4 5 6 7 8 9 10))
                    (evens (funcall my-filter #'evenp data))
                    (odds (funcall my-filter #'oddp data))
                    (sum-evens (funcall my-reduce #'+ 0 evens))
                    (prod-odds (funcall my-reduce #'* 1 odds)))
               (list evens odds sum-evens prod-odds)))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (15 5 21 (header a b c))""#]];
    // Partial application helper
    crate::common::assert_oracle_parity_expect(
        r#"(let ((partial (lambda (f &rest initial-args)
                            (lambda (&rest more-args)
                              (apply f (append initial-args more-args))))))
             (let ((add5 (funcall partial #'+ 5))
                   (mul3 (funcall partial #'* 3))
                   (prefix-list (funcall partial #'list 'header)))
               (list (funcall add5 10)
                     (funcall add5 0)
                     (funcall mul3 7)
                     (funcall prefix-list 'a 'b 'c))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (6 7 24 5)""#]];
    // Function dispatch table using alist + funcall
    crate::common::assert_oracle_parity_expect(
        r#"(let ((dispatch '((add . +) (sub . -) (mul . *) (div . /))))
             (let ((run-op (lambda (op &rest args)
                             (let ((fn (cdr (assq op dispatch))))
                               (if fn (apply fn args) (error "Unknown op"))))))
               (list (funcall run-op 'add 1 2 3)
                     (funcall run-op 'sub 10 3)
                     (funcall run-op 'mul 2 3 4)
                     (funcall run-op 'div 100 5 4))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// funcall with symbol-function indirection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_symbol_function_indirection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three ways to call via symbol: quote, sharp-quote, symbol-function
    let form = r#"(progn
      (fset 'neovm--test-fac-triple (lambda (x) (* x 3)))
      (unwind-protect
          (list
           ;; Via quoted symbol
           (funcall 'neovm--test-fac-triple 7)
           ;; Via sharp-quote
           (funcall #'neovm--test-fac-triple 7)
           ;; Via symbol-function
           (funcall (symbol-function 'neovm--test-fac-triple) 7)
           ;; Apply all three ways
           (apply 'neovm--test-fac-triple '(10))
           (apply #'neovm--test-fac-triple '(10))
           (apply (symbol-function 'neovm--test-fac-triple) '(10))
           ;; Verify they all produce the same result
           (= (funcall 'neovm--test-fac-triple 5)
              (funcall #'neovm--test-fac-triple 5)
              (funcall (symbol-function 'neovm--test-fac-triple) 5)))
        (fmakunbound 'neovm--test-fac-triple)))"#;
    let expect = expect_test::expect![[r#""OK (21 21 21 30 30 30 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Wrong number of arguments errors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_wrong_arg_count_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK too-few""#]];
    // Too few args to fixed-arity lambda
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall (lambda (a b c) (+ a b c)) 1 2)
           (wrong-number-of-arguments 'too-few))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK too-many""#]];
    // Too many args to fixed-arity lambda
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall (lambda (a b) (+ a b)) 1 2 3)
           (wrong-number-of-arguments 'too-many))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK too-few""#]];
    // Too few for required + optional (need at least 1)
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall (lambda (a &optional b) (list a b)))
           (wrong-number-of-arguments 'too-few))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK too-many""#]];
    // Too many for required + optional (no &rest)
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall (lambda (a &optional b) (list a b)) 1 2 3)
           (wrong-number-of-arguments 'too-many))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (invalid-function 42)""#]];
    // funcall with non-function value
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall 42)
           (invalid-function (list 'invalid-function (cadr err))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (void neovm--test-fac-nonexistent-fn123)""#]];
    // funcall with void symbol
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall 'neovm--test-fac-nonexistent-fn123)
           (void-function (list 'void (cadr err))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK too-few-apply""#]];
    // apply with too few for required params
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (apply (lambda (a b c) (+ a b c)) '(1))
           (wrong-number-of-arguments 'too-few-apply))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Complex: Y-combinator style self-application and trampolining
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_apply_self_application() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 120 3628800 479001600)""#]];
    // Factorial via self-passing
    crate::common::assert_oracle_parity_expect(
        r#"(let ((fact (lambda (self n)
                         (if (<= n 1) 1
                           (* n (funcall self self (1- n)))))))
             (list (funcall fact fact 0)
                   (funcall fact fact 1)
                   (funcall fact fact 5)
                   (funcall fact fact 10)
                   (funcall fact fact 12)))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (0 1 1 2 3 5 55 610 6765 75025)""#]];
    // Fibonacci via self-passing with memoization
    crate::common::assert_oracle_parity_expect(
        r#"(let ((memo (make-hash-table :test 'eql)))
             (let ((fib (lambda (self n)
                          (or (gethash n memo)
                              (let ((r (if (< n 2) n
                                         (+ (funcall self self (- n 1))
                                            (funcall self self (- n 2))))))
                                (puthash n r memo)
                                r)))))
               (mapcar (lambda (k) (funcall fib fib k))
                       '(0 1 2 3 4 5 10 15 20 25))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (0 5 100)""#]];
    // Trampoline pattern: functions return thunks until non-function
    crate::common::assert_oracle_parity_expect(
        r#"(let ((trampoline (lambda (fn &rest args)
                                (let ((result (apply fn args)))
                                  (while (functionp result)
                                    (setq result (funcall result)))
                                  result))))
             (let ((count-down (lambda (n acc)
                                 (if (= n 0) acc
                                   (let ((nn (1- n)) (aa (1+ acc)))
                                     (lambda () (funcall 'neovm--test-fac-cd nn aa)))))))
               (fset 'neovm--test-fac-cd count-down)
               (unwind-protect
                   (list (funcall trampoline count-down 0 0)
                         (funcall trampoline count-down 5 0)
                         (funcall trampoline count-down 100 0))
                 (fmakunbound 'neovm--test-fac-cd))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// funcall/apply with hash-table and vector manipulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_apply_data_structure_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Alice\" 30 95 missing 3)""#]];
    // Use funcall to build and query hash tables
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'equal)))
             (let ((set-val (lambda (k v) (puthash k v ht)))
                   (get-val (lambda (k) (gethash k ht 'missing))))
               (funcall set-val "name" "Alice")
               (funcall set-val "age" 30)
               (funcall set-val "score" 95)
               (list (funcall get-val "name")
                     (funcall get-val "age")
                     (funcall get-val "score")
                     (funcall get-val "nonexistent")
                     (hash-table-count ht))))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (150 10 50 5)""#]];
    // Use apply with vector operations
    crate::common::assert_oracle_parity_expect(
        r#"(let ((v [10 20 30 40 50]))
             (list (apply #'+ (append v nil))
                   (funcall #'aref v 0)
                   (funcall #'aref v 4)
                   (funcall #'length v)))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK (9 1 31)""#]];
    // Reduce over a vector via funcall
    crate::common::assert_oracle_parity_expect(
        r#"(let ((vec [3 1 4 1 5 9 2 6]))
             (let ((max-val (aref vec 0))
                   (min-val (aref vec 0))
                   (sum 0))
               (dotimes (i (length vec))
                 (let ((v (aref vec i)))
                   (setq max-val (funcall #'max max-val v))
                   (setq min-val (funcall #'min min-val v))
                   (setq sum (funcall #'+ sum v))))
               (list max-val min-val sum)))"#,
        expect,
    );
}
