//! Oracle parity tests for advanced `cond` patterns: multi-clause,
//! side-effect clauses, nested cond, cond as dispatch table,
//! and cond with complex predicates.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Multi-clause cond with various return forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cond_multi_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((classify
                         (lambda (x)
                           (cond
                            ((not (numberp x)) 'not-a-number)
                            ((< x 0) 'negative)
                            ((= x 0) 'zero)
                            ((< x 10) 'small)
                            ((< x 100) 'medium)
                            ((< x 1000) 'large)
                            (t 'huge)))))
                    (mapcar classify
                            '(-5 0 3 42 500 9999 "hello" nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (negative zero small medium large huge not-a-number not-a-number)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cond with multiple body forms per clause
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cond_multi_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each clause can have multiple body forms
    let form = r#"(let ((log nil) (x 15))
                    (cond
                     ((< x 10)
                      (setq log (cons 'small log))
                      (setq log (cons x log))
                      'small)
                     ((< x 20)
                      (setq log (cons 'medium log))
                      (setq log (cons x log))
                      (setq log (cons (* x 2) log))
                      'medium)
                     (t
                      (setq log (cons 'large log))
                      'large))
                    (list (nreverse log)))"#;
    let expect = expect_test::expect![[r#""OK ((medium 15 30))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cond as pattern matcher
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cond_pattern_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Dispatch on structure of input
    let form = r#"(let ((describe
                         (lambda (form)
                           (cond
                            ((null form) "empty")
                            ((atom form) (format "atom:%s" form))
                            ((and (consp form) (eq (car form) 'quote))
                             (format "quoted:%s" (cadr form)))
                            ((and (consp form) (eq (car form) '+))
                             (format "add(%d args)"
                                     (1- (length form))))
                            ((and (consp form) (eq (car form) 'lambda))
                             (format "lambda(%d params)"
                                     (length (cadr form))))
                            ((listp form)
                             (format "list(%d)" (length form)))
                            (t "unknown")))))
                    (mapcar describe
                            (list nil
                                  42
                                  'hello
                                  '(quote x)
                                  '(+ 1 2 3)
                                  '(lambda (a b) body)
                                  '(foo bar baz))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"empty\" \"atom:42\" \"atom:hello\" \"quoted:x\" \"add(3 args)\" \"lambda(2 params)\" \"list(3)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cond with side effects and fallthrough
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cond_clause_returns_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A clause with just a test (no body) returns the test value
    let form = r#"(list
                    (cond (42))          ;; returns 42
                    (cond (nil) (t 99))  ;; nil fails, t matches
                    (cond ('hello))      ;; returns hello
                    (cond (nil) (nil) ('found-it)))  ;; third
"#;
    let expect = expect_test::expect![[r#""OK (42 99 hello found-it)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested cond for multi-dimensional dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cond_nested_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((format-value
                         (lambda (type value)
                           (cond
                            ((eq type 'int)
                             (cond
                              ((< value 0) (format "-%d" (abs value)))
                              ((= value 0) "0")
                              (t (format "+%d" value))))
                            ((eq type 'bool)
                             (cond
                              ((eq value t) "true")
                              ((eq value nil) "false")
                              (t "?")))
                            ((eq type 'string)
                             (cond
                              ((= (length value) 0) "\"\"")
                              ((> (length value) 10)
                               (format "\"%s...\"" (substring value 0 10)))
                              (t (format "\"%s\"" value))))
                            (t "unsupported")))))
                    (list
                     (funcall format-value 'int -5)
                     (funcall format-value 'int 0)
                     (funcall format-value 'int 42)
                     (funcall format-value 'bool t)
                     (funcall format-value 'bool nil)
                     (funcall format-value 'string "")
                     (funcall format-value 'string "hello")
                     (funcall format-value 'string "a very long string indeed")
                     (funcall format-value 'float 3.14)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"-5\" \"0\" \"+42\" \"true\" \"false\" \"\\\"\\\"\" \"\\\"hello\\\"\" \"\\\"a very lon...\\\"\" \"unsupported\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: cond-based mini type system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cond_type_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a type checker using cond chains
    let form = r#"(let ((type-check
                         (lambda (expr env)
                           (cond
                            ((integerp expr) 'int)
                            ((floatp expr) 'float)
                            ((stringp expr) 'string)
                            ((eq expr t) 'bool)
                            ((eq expr nil) 'bool)
                            ((symbolp expr)
                             (cdr (assq expr env)))
                            ((and (consp expr) (eq (car expr) '+))
                             (let ((t1 (funcall type-check (nth 1 expr) env))
                                   (t2 (funcall type-check (nth 2 expr) env)))
                               (cond
                                ((and (eq t1 'int) (eq t2 'int)) 'int)
                                ((and (eq t1 'float) (eq t2 'float)) 'float)
                                ((or (eq t1 'float) (eq t2 'float)) 'float)
                                ((and (eq t1 'string) (eq t2 'string)) 'string)
                                (t 'error))))
                            ((and (consp expr) (eq (car expr) 'if))
                             (let ((cond-type (funcall type-check
                                                       (nth 1 expr) env))
                                   (then-type (funcall type-check
                                                        (nth 2 expr) env))
                                   (else-type (funcall type-check
                                                        (nth 3 expr) env)))
                               (cond
                                ((not (eq cond-type 'bool)) 'error)
                                ((eq then-type else-type) then-type)
                                (t 'error))))
                            (t 'unknown)))))
                    (let ((env '((x . int) (y . float) (flag . bool))))
                      (list
                       (funcall type-check 42 env)
                       (funcall type-check 'x env)
                       (funcall type-check '(+ x 1) env)
                       (funcall type-check '(+ x y) env)
                       (funcall type-check '(if flag 1 2) env)
                       (funcall type-check '(if flag "a" "b") env)
                       (funcall type-check '(if x 1 2) env))))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable type-check)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
