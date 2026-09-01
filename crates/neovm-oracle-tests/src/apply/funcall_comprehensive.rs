//! Comprehensive oracle parity tests for `apply` and `funcall`:
//! spread arguments, empty final lists, single list argument, lambda/closure/subr
//! targets, &rest and &optional parameters, nested chains, higher-order
//! function results as arguments, and error edge cases.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// apply with spread arguments before final list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_spread_args_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 60""#];
    // 0 spread args, just a list
    crate::common::assert_oracle_parity_expect("(apply #'+ '(10 20 30))", expect);
    let expect = expect_test::expect![r#""OK 106""#];
    // 1 spread arg before list
    crate::common::assert_oracle_parity_expect("(apply #'+ 100 '(1 2 3))", expect);
    let expect = expect_test::expect![r#""OK 120""#];
    // 2 spread args before list
    crate::common::assert_oracle_parity_expect("(apply #'* 2 3 '(4 5))", expect);
    let expect = expect_test::expect![r#""OK (a b c d e f)""#];
    // 3 spread args before list
    crate::common::assert_oracle_parity_expect("(apply #'list 'a 'b 'c '(d e f))", expect);
    let expect = expect_test::expect![r#""OK 55""#];
    // 5 spread args before list
    crate::common::assert_oracle_parity_expect("(apply #'+ 1 2 3 4 5 '(6 7 8 9 10))", expect);
    let expect = expect_test::expect![[r#""OK \"hello world!\"""#]];
    // Spread args with string concat
    crate::common::assert_oracle_parity_expect(
        r#"(apply #'concat "hello" " " '("world" "!"))"#,
        expect,
    );
    let expect = expect_test::expect![r#""OK ((1 2) (3 4) (5 6) (7 8))""#];
    // Nested list construction via spread + final list
    crate::common::assert_oracle_parity_expect(
        "(apply #'list '(1 2) '(3 4) '((5 6) (7 8)))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (42 \"str\" sym 65 3.14 nil t)""#]];
    // Mixed types in spread args
    crate::common::assert_oracle_parity_expect(
        r#"(apply #'list 42 "str" 'sym ?A '(3.14 nil t))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// apply with empty final list and only a list argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_empty_final_list_and_only_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 6""#];
    // Empty final list with spread args: all args come from spread
    crate::common::assert_oracle_parity_expect("(apply #'+ 1 2 3 '())", expect);
    let expect = expect_test::expect![r#""OK (a b)""#];
    crate::common::assert_oracle_parity_expect("(apply #'list 'a 'b '())", expect);
    let expect = expect_test::expect![r#""OK 600""#];
    // Only a list argument (no spread args)
    crate::common::assert_oracle_parity_expect("(apply #'+ '(100 200 300))", expect);
    let expect = expect_test::expect![r#""OK (x y z)""#];
    crate::common::assert_oracle_parity_expect("(apply #'list '(x y z))", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    // Empty final list, no spread args: zero-arg call
    crate::common::assert_oracle_parity_expect("(apply #'+ '())", expect);
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect("(apply #'list '())", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    // Only nil as final list
    crate::common::assert_oracle_parity_expect("(apply #'+ nil)", expect);
    let expect = expect_test::expect![r#""OK 10""#];
    // Deeply nested: apply constructing apply's args
    crate::common::assert_oracle_parity_expect("(apply #'+ (apply #'list 1 2 '(3 4)))", expect);
    let expect = expect_test::expect![r#""OK 15""#];
    // apply with vector-producing function result as arg list
    // (mapcar produces a list, suitable as final arg)
    crate::common::assert_oracle_parity_expect("(apply #'+ (mapcar #'1+ '(0 1 2 3 4)))", expect);
}

// ---------------------------------------------------------------------------
// funcall with lambda, closure, and subr targets
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_lambda_closure_subr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 60""#];
    // funcall with a built-in subr
    crate::common::assert_oracle_parity_expect("(funcall #'+ 10 20 30)", expect);
    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect("(funcall #'concat \"a\" \"b\" \"c\")", expect);
    let expect = expect_test::expect![r#""OK 1""#];
    crate::common::assert_oracle_parity_expect("(funcall #'car '(1 2 3))", expect);

    let expect = expect_test::expect![r#""OK 17""#];
    // funcall with a lambda
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (x y z) (+ (* x y) z)) 3 4 5)",
        expect,
    );

    let expect = expect_test::expect![r#""OK (101 150 0)""#];
    // funcall with a lexical closure (captures variable)
    crate::common::assert_oracle_parity_expect(
        r#"(let ((base 100))
             (let ((adder (lambda (x) (+ base x))))
               (list (funcall adder 1)
                     (funcall adder 50)
                     (funcall adder -100))))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK (1 2 3 3)""#];
    // funcall with a closure that captures a mutable cell
    crate::common::assert_oracle_parity_expect(
        r#"(let ((counter 0))
             (let ((inc (lambda () (setq counter (1+ counter)) counter))
                   (get (lambda () counter)))
               (list (funcall inc)
                     (funcall inc)
                     (funcall inc)
                     (funcall get))))"#,
        expect,
    );

    // funcall with symbol naming a function
    let form = r#"(progn
      (fset 'neovm--test-afc-sq (lambda (x) (* x x)))
      (unwind-protect
          (list (funcall 'neovm--test-afc-sq 7)
                (funcall #'neovm--test-afc-sq 7)
                (funcall (symbol-function 'neovm--test-afc-sq) 7))
        (fmakunbound 'neovm--test-afc-sq)))"#;
    let expect = expect_test::expect![r#""OK (49 49 49)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// funcall with &rest parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_rest_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (1 2 3 4 5)""#];
    // Simple &rest: collect all args
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (&rest xs) xs) 1 2 3 4 5)",
        expect,
    );
    let expect = expect_test::expect![r#""OK nil""#];
    // &rest with no args
    crate::common::assert_oracle_parity_expect("(funcall (lambda (&rest xs) xs))", expect);
    let expect = expect_test::expect![r#""OK (a . 4)""#];
    // Required arg + &rest
    crate::common::assert_oracle_parity_expect(
        "(funcall (lambda (head &rest tail) (cons head (length tail))) 'a 'b 'c 'd 'e)",
        expect,
    );
    let expect = expect_test::expect![r#""OK (1 2 (3 4 5))""#];
    // apply with &rest function
    crate::common::assert_oracle_parity_expect(
        "(apply (lambda (a b &rest cs) (list a b cs)) 1 2 '(3 4 5))",
        expect,
    );
    let expect = expect_test::expect![r#""OK (6 0 60)""#];
    // Nested rest: inner function collects and outer spreads
    crate::common::assert_oracle_parity_expect(
        r#"(let ((collector (lambda (&rest items) (apply #'+ items))))
             (list (funcall collector 1 2 3)
                   (funcall collector)
                   (apply collector '(10 20 30))))"#,
        expect,
    );
    let expect = expect_test::expect![r#""OK (15 60 0)""#];
    // &rest with recursive processing
    crate::common::assert_oracle_parity_expect(
        r#"(let ((my-sum (lambda (&rest args)
                          (let ((total 0))
                            (dolist (x args total)
                              (setq total (+ total x)))))))
             (list (funcall my-sum 1 2 3 4 5)
                   (apply my-sum '(10 20 30))
                   (funcall my-sum)))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// funcall with &optional parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_funcall_optional_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK ((1 nil) (1 2))""#];
    // Single optional, supplied and not supplied
    crate::common::assert_oracle_parity_expect(
        "(list (funcall (lambda (a &optional b) (list a b)) 1)
               (funcall (lambda (a &optional b) (list a b)) 1 2))",
        expect,
    );
    let expect = expect_test::expect![r#""OK ((nil nil nil) (1 nil nil) (1 2 nil) (1 2 3))""#];
    // Multiple optionals
    crate::common::assert_oracle_parity_expect(
        "(list (funcall (lambda (&optional a b c) (list a b c)))
               (funcall (lambda (&optional a b c) (list a b c)) 1)
               (funcall (lambda (&optional a b c) (list a b c)) 1 2)
               (funcall (lambda (&optional a b c) (list a b c)) 1 2 3))",
        expect,
    );
    let expect = expect_test::expect![r#""OK ((1 nil nil) (1 2 nil) (1 2 (3 4 5)))""#];
    // &optional + &rest combined
    crate::common::assert_oracle_parity_expect(
        "(list (funcall (lambda (a &optional b &rest c) (list a b c)) 1)
               (funcall (lambda (a &optional b &rest c) (list a b c)) 1 2)
               (funcall (lambda (a &optional b &rest c) (list a b c)) 1 2 3 4 5))",
        expect,
    );
    let expect =
        expect_test::expect![[r#""OK (\"Hello, World!\" \"Hello, Alice!\" \"Hi, Bob!\")""#]];
    // Optional with default-like behavior via (or arg default)
    crate::common::assert_oracle_parity_expect(
        r#"(let ((make-greeter
                  (lambda (&optional name greeting)
                    (let ((n (or name "World"))
                          (g (or greeting "Hello")))
                      (concat g ", " n "!")))))
             (list (funcall make-greeter)
                   (funcall make-greeter "Alice")
                   (funcall make-greeter "Bob" "Hi")))"#,
        expect,
    );
    let expect = expect_test::expect![r#""OK (1 2 nil)""#];
    // apply with optional params
    crate::common::assert_oracle_parity_expect(
        "(apply (lambda (a &optional b c) (list a b c)) 1 '(2))",
        expect,
    );
}

// ---------------------------------------------------------------------------
// Nested apply/funcall chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_apply_funcall_chains() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 42""#];
    // funcall returning function, called again
    crate::common::assert_oracle_parity_expect(
        "(funcall (funcall (lambda (x) (lambda (y) (* x y))) 6) 7)",
        expect,
    );

    let expect = expect_test::expect![r#""OK (x y z)""#];
    // Three levels of currying via nested funcall
    crate::common::assert_oracle_parity_expect(
        r#"(let ((curry3
                 (lambda (a)
                   (lambda (b)
                     (lambda (c)
                       (list a b c))))))
             (funcall (funcall (funcall curry3 'x) 'y) 'z))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK 9""#];
    // apply inside funcall inside apply
    crate::common::assert_oracle_parity_expect(
        "(apply #'+ (funcall (lambda (xs) (mapcar #'1+ xs)) '(1 2 3)))",
        expect,
    );

    let expect = expect_test::expect![r#""OK (12 60 40)""#];
    // Chain: compose two functions, then apply the composition
    crate::common::assert_oracle_parity_expect(
        r#"(let ((compose
                 (lambda (f g)
                   (lambda (&rest args) (funcall f (apply g args))))))
             (let ((double-sum (funcall compose
                                        (lambda (x) (* x 2))
                                        #'+)))
               (list (funcall double-sum 1 2 3)
                     (funcall double-sum 10 20)
                     (apply double-sum '(5 5 5 5)))))"#,
        expect,
    );

    // Mutual recursion via funcall with fset
    let form = r#"(progn
      (fset 'neovm--test-afc-even-p
        (lambda (n)
          (if (= n 0) t
            (funcall 'neovm--test-afc-odd-p (1- n)))))
      (fset 'neovm--test-afc-odd-p
        (lambda (n)
          (if (= n 0) nil
            (funcall 'neovm--test-afc-even-p (1- n)))))
      (unwind-protect
          (list (funcall 'neovm--test-afc-even-p 0)
                (funcall 'neovm--test-afc-even-p 1)
                (funcall 'neovm--test-afc-even-p 10)
                (funcall 'neovm--test-afc-odd-p 7)
                (funcall 'neovm--test-afc-odd-p 8))
        (fmakunbound 'neovm--test-afc-even-p)
        (fmakunbound 'neovm--test-afc-odd-p)))"#;
    let expect = expect_test::expect![r#""OK (t nil t t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// apply/funcall with higher-order functions (mapcar result as arg)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_higher_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (1 1 2 4 3 9 4 16 5 25)""#];
    // apply #'append on mapcar result (flatten one level)
    crate::common::assert_oracle_parity_expect(
        "(apply #'append (mapcar (lambda (x) (list x (* x x))) '(1 2 3 4 5)))",
        expect,
    );

    let expect = expect_test::expect![r#""OK 10""#];
    // funcall with result of mapcar as single arg
    crate::common::assert_oracle_parity_expect(
        "(funcall #'length (mapcar #'1+ '(1 2 3 4 5 6 7 8 9 10)))",
        expect,
    );

    let expect = expect_test::expect![r#""OK 27""#];
    // Build a pipeline: list of functions, reduce with funcall
    crate::common::assert_oracle_parity_expect(
        r#"(let ((pipeline (list (lambda (x) (+ x 10))
                               (lambda (x) (* x 2))
                               (lambda (x) (- x 3)))))
             (let ((result 5))
               (dolist (fn pipeline result)
                 (setq result (funcall fn result)))))"#,
        expect,
    );

    let expect = expect_test::expect![r#""ERR (wrong-number-of-arguments #<subr mapcar> 4)""#];
    // apply with mapcar to transpose a matrix
    crate::common::assert_oracle_parity_expect(
        "(apply #'mapcar #'list '((1 2 3) (4 5 6) (7 8 9)))",
        expect,
    );

    let expect = expect_test::expect![r#""ERR (wrong-number-of-arguments #<subr mapcar> 3)""#];
    // Compose mapcar results with apply for zip-style operation
    crate::common::assert_oracle_parity_expect(
        r#"(let ((xs '(1 2 3 4))
                 (ys '(10 20 30 40)))
             (apply #'mapcar (lambda (a b) (+ a b)) (list xs ys)))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK (10 16 -7 200)""#];
    // funcall a function selected from a dispatching alist
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ops '((double . (lambda (x) (* x 2)))
                        (square . (lambda (x) (* x x)))
                        (negate . (lambda (x) (- x))))))
             (mapcar (lambda (pair)
                       (funcall (cdr (assq (car pair) ops)) (cdr pair)))
                     '((double . 5) (square . 4) (negate . 7) (double . 100))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Error cases: wrong number of args, non-function, apply with non-list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_error_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (wrong-number ((closure (t) (a b) (+ a b)) 1))""#];
    // funcall with too few args => error
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall (lambda (a b) (+ a b)) 1)
           (wrong-number-of-arguments
            (list 'wrong-number (cdr err))))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK (wrong-number ((closure (t) (a b) (+ a b)) 3))""#];
    // funcall with too many args => error
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall (lambda (a b) (+ a b)) 1 2 3)
           (wrong-number-of-arguments
            (list 'wrong-number (cdr err))))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK (invalid-function 42)""#];
    // funcall with non-function => error
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall 42 1 2)
           (invalid-function
            (list 'invalid-function (cadr err))))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK (wrong-type listp)""#];
    // apply with non-list as final arg => error
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (apply #'+ 1 2 3)
           (wrong-type-argument
            (list 'wrong-type (car (cdr err)))))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK (void-function neovm--test-afc-nonexistent)""#];
    // funcall with void symbol => error
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (funcall 'neovm--test-afc-nonexistent 1)
           (void-function
            (list 'void-function (cadr err))))"#,
        expect,
    );

    let expect =
        expect_test::expect![r#""OK (wrong-number ((closure (t) (a &optional b) (list a b)) 3))""#];
    // apply with &optional: too many args still works (extras ignored by &rest)
    // but without &rest, too many args is an error
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case err
             (apply (lambda (a &optional b) (list a b)) '(1 2 3))
           (wrong-number-of-arguments
            (list 'wrong-number (cdr err))))"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Complex: apply/funcall with Y-combinator-like patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_apply_funcall_y_combinator_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK (1 1 120 3628800)""#];
    // Implement factorial via a self-passing pattern (poor man's Y combinator)
    crate::common::assert_oracle_parity_expect(
        r#"(let ((fact-step
                 (lambda (self n)
                   (if (<= n 1) 1
                     (* n (funcall self self (1- n)))))))
             (list (funcall fact-step fact-step 0)
                   (funcall fact-step fact-step 1)
                   (funcall fact-step fact-step 5)
                   (funcall fact-step fact-step 10)))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK (0 1 1 2 3 5 8 13 21 34 55 610 6765)""#];
    // Fibonacci via self-passing with memoization in a hash table
    crate::common::assert_oracle_parity_expect(
        r#"(let ((memo (make-hash-table :test 'eql)))
             (let ((fib-step
                    (lambda (self n)
                      (or (gethash n memo)
                          (let ((result
                                 (cond
                                  ((= n 0) 0)
                                  ((= n 1) 1)
                                  (t (+ (funcall self self (- n 1))
                                        (funcall self self (- n 2)))))))
                            (puthash n result memo)
                            result)))))
               (mapcar (lambda (k) (funcall fib-step fib-step k))
                       '(0 1 2 3 4 5 6 7 8 9 10 15 20))))"#,
        expect,
    );

    let expect = expect_test::expect![r#""OK (0 1 15 55 (1 2 3 4))""#];
    // Apply with dynamically built argument lists
    crate::common::assert_oracle_parity_expect(
        r#"(let ((build-args
                 (lambda (n)
                   (let ((args nil) (i 0))
                     (while (< i n)
                       (setq args (cons (1+ i) args))
                       (setq i (1+ i)))
                     (nreverse args)))))
             (list (apply #'+ (funcall build-args 0))
                   (apply #'+ (funcall build-args 1))
                   (apply #'+ (funcall build-args 5))
                   (apply #'+ (funcall build-args 10))
                   (apply #'list (funcall build-args 4))))"#,
        expect,
    );
}
