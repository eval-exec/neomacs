//! Comprehensive oracle parity tests for error types and signaling.
//!
//! Covers: `error`, `user-error`, `signal`, all standard error types
//! (`wrong-type-argument`, `wrong-number-of-arguments`, `void-variable`,
//! `void-function`, `setting-constant`, `invalid-function`,
//! `args-out-of-range`, `arith-error`, `overflow-error`, `range-error`,
//! `domain-error`), `define-error` for custom hierarchies, error
//! inheritance (`error` catches all), `error-message-string`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// error function with format strings and multiple arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_types_error_function_format_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // error with format args produces a well-formed error message
    // that condition-case can catch and introspect
    let form = r#"
(list
  ;; Basic error with format string
  (condition-case err
      (error "expected %s but got %d" "string" 42)
    (error (list (car err) (cadr err))))

  ;; error with no format args
  (condition-case err
      (error "plain message")
    (error (cadr err)))

  ;; error with multiple format specifiers
  (condition-case err
      (error "a=%d b=%S c=%s" 1 'hello "world")
    (error (cadr err)))

  ;; error always signals 'error symbol
  (condition-case err
      (error "test %d" 99)
    (error (car err))))"#;
    let expect = expect_test::expect![[
        r#""OK ((error \"expected string but got 42\") \"plain message\" \"a=1 b=hello c=world\" error)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// user-error is caught by user-error but also by error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_types_user_error_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; user-error caught by user-error handler
  (condition-case err
      (signal 'user-error '("bad input from user"))
    (user-error (list 'caught-user (cadr err))))

  ;; user-error also caught by generic error handler
  (condition-case err
      (signal 'user-error '("also an error"))
    (error (list 'caught-generic (car err))))

  ;; user-error handler does NOT catch plain error
  (condition-case err
      (condition-case inner
          (error "plain error")
        (user-error 'should-not-catch))
    (error (list 'outer-caught (car inner))))

  ;; Specific user-error beats generic error when both present
  (condition-case err
      (signal 'user-error '("specific wins"))
    (user-error 'specific-handler)
    (error 'generic-handler)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable inner)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// signal with various standard error symbols
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_types_wrong_type_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Direct signal of wrong-type-argument
  (condition-case err
      (signal 'wrong-type-argument '(numberp "not-a-number"))
    (wrong-type-argument (list (car err) (cadr err) (nth 2 err))))

  ;; Naturally triggered wrong-type-argument via (+ "a" 1)
  (condition-case err
      (+ "hello" 1)
    (wrong-type-argument (car err)))

  ;; caught by error handler too
  (condition-case err
      (car 42)
    (error (car err))))"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument numberp \"not-a-number\") wrong-type-argument wrong-type-argument)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_types_wrong_number_of_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Too few arguments
  (condition-case err
      (+ )
    (wrong-number-of-arguments (car err))
    (error (car err)))

  ;; Signal directly with details
  (condition-case err
      (signal 'wrong-number-of-arguments '(my-func 3))
    (wrong-number-of-arguments (list (car err) (cadr err) (nth 2 err))))

  ;; wrong-number-of-arguments is caught by error
  (condition-case err
      (signal 'wrong-number-of-arguments '(foo 0))
    (error (car err))))"#;
    let expect = expect_test::expect![[
        r#""OK (0 (wrong-number-of-arguments my-func 3) wrong-number-of-arguments)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_types_void_variable_and_void_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; void-variable: reference to unbound variable
  (condition-case err
      (symbol-value 'neovm--truly-unbound-var-xyz-999)
    (void-variable (list (car err) (cadr err))))

  ;; void-function: call to unbound function
  (condition-case err
      (neovm--truly-unbound-func-xyz-999)
    (void-function (list (car err) (cadr err))))

  ;; void-variable caught by error
  (condition-case err
      (symbol-value 'neovm--another-unbound-999)
    (error (car err)))

  ;; void-function caught by error
  (condition-case err
      (neovm--missing-fn-xyz-999 1 2 3)
    (error (car err))))"#;
    let expect = expect_test::expect![[
        r#""OK ((void-variable neovm--truly-unbound-var-xyz-999) (void-function neovm--truly-unbound-func-xyz-999) void-variable void-function)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_types_setting_constant() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Cannot set nil
  (condition-case err
      (set 'nil 42)
    (setting-constant (list (car err) (cadr err)))
    (error (list 'generic (car err))))

  ;; Cannot set t
  (condition-case err
      (set 't 42)
    (setting-constant (list (car err) (cadr err)))
    (error (list 'generic (car err))))

  ;; Cannot set keyword symbol
  (condition-case err
      (set ':my-keyword 42)
    (setting-constant (list (car err) (cadr err)))
    (error (list 'generic (car err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((setting-constant nil) (setting-constant t) (setting-constant :my-keyword))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_types_invalid_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Calling a non-function (number)
  (condition-case err
      (funcall 42)
    (invalid-function (car err))
    (error (list 'generic (car err))))

  ;; Calling a non-function (string)
  (condition-case err
      (funcall "not-a-function")
    (invalid-function (car err))
    (error (list 'generic (car err))))

  ;; signal directly
  (condition-case err
      (signal 'invalid-function '(42))
    (invalid-function (list (car err) (cadr err)))))"#;
    let expect =
        expect_test::expect![[r#""OK (invalid-function invalid-function (invalid-function 42))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_types_args_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; substring with out-of-range indices
  (condition-case err
      (substring "hello" 10)
    (args-out-of-range (car err))
    (error (car err)))

  ;; aref on vector with bad index
  (condition-case err
      (aref [1 2 3] 10)
    (args-out-of-range (car err))
    (error (car err)))

  ;; elt on list with negative index
  (condition-case err
      (elt '(a b c) -1)
    (args-out-of-range (car err))
    (error (car err)))

  ;; Caught by generic error handler
  (condition-case err
      (aref [1 2 3] 100)
    (error (car err))))"#;
    let expect =
        expect_test::expect![[r#""OK (args-out-of-range args-out-of-range a args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_error_types_arith_error_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Division by zero
  (condition-case err
      (/ 1 0)
    (arith-error (car err)))

  ;; Modulo by zero
  (condition-case err
      (% 10 0)
    (arith-error (car err)))

  ;; arith-error caught by error
  (condition-case err
      (/ 1 0)
    (error (car err)))

  ;; Signal arith-error directly
  (condition-case err
      (signal 'arith-error nil)
    (arith-error (list (car err) (cdr err))))

  ;; overflow-error is a sub-type of arith-error
  (condition-case err
      (signal 'overflow-error '("overflow"))
    (arith-error (list 'caught-by-arith (car err))))

  ;; range-error is a sub-type of arith-error
  (condition-case err
      (signal 'range-error '("out of range"))
    (arith-error (list 'caught-by-arith (car err))))

  ;; domain-error is a sub-type of arith-error
  (condition-case err
      (signal 'domain-error '("bad domain"))
    (arith-error (list 'caught-by-arith (car err)))))"#;
    let expect = expect_test::expect![[
        r#""OK (arith-error arith-error arith-error (arith-error nil) (caught-by-arith overflow-error) (caught-by-arith range-error) (caught-by-arith domain-error))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-error: custom error hierarchies
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_types_define_error_custom_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a custom error hierarchy and verify inheritance
    let form = r#"
(progn
  (define-error 'neovm-test-base-err "base error")
  (define-error 'neovm-test-child-err "child error" 'neovm-test-base-err)
  (define-error 'neovm-test-grandchild-err "grandchild error" 'neovm-test-child-err)

  (unwind-protect
      (list
        ;; Child caught by parent
        (condition-case err
            (signal 'neovm-test-child-err '("child data"))
          (neovm-test-base-err (list 'base-caught (car err) (cadr err))))

        ;; Grandchild caught by grandparent
        (condition-case err
            (signal 'neovm-test-grandchild-err '("gc data"))
          (neovm-test-base-err (list 'base-caught (car err))))

        ;; Grandchild caught by parent
        (condition-case err
            (signal 'neovm-test-grandchild-err '("gc data 2"))
          (neovm-test-child-err (list 'child-caught (car err))))

        ;; Parent NOT caught by child handler
        (condition-case err
            (condition-case inner
                (signal 'neovm-test-base-err '("base only"))
              (neovm-test-child-err 'should-not-catch))
          (neovm-test-base-err (list 'outer-base (car inner))))

        ;; All custom errors caught by error
        (condition-case err
            (signal 'neovm-test-grandchild-err '("any custom"))
          (error (list 'generic-caught (car err))))

        ;; error-conditions reflects hierarchy
        (get 'neovm-test-grandchild-err 'error-conditions))

    ;; Cleanup: remove error definitions
    (put 'neovm-test-base-err 'error-conditions nil)
    (put 'neovm-test-base-err 'error-message nil)
    (put 'neovm-test-child-err 'error-conditions nil)
    (put 'neovm-test-child-err 'error-message nil)
    (put 'neovm-test-grandchild-err 'error-conditions nil)
    (put 'neovm-test-grandchild-err 'error-message nil)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable inner)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// define-error with multiple parent conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_types_define_error_multiple_parents() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (define-error 'neovm-test-mp-a "error A")
  (define-error 'neovm-test-mp-b "error B")
  ;; Child inherits from multiple parents via explicit conditions list
  (define-error 'neovm-test-mp-ab "error AB" '(neovm-test-mp-a neovm-test-mp-b))

  (unwind-protect
      (list
        ;; Caught by first parent
        (condition-case err
            (signal 'neovm-test-mp-ab '("multi"))
          (neovm-test-mp-a (list 'a-caught (car err))))

        ;; Also caught by second parent
        (condition-case err
            (signal 'neovm-test-mp-ab '("multi 2"))
          (neovm-test-mp-b (list 'b-caught (car err))))

        ;; First matching handler wins
        (condition-case err
            (signal 'neovm-test-mp-ab '("multi 3"))
          (neovm-test-mp-a 'a-wins)
          (neovm-test-mp-b 'b-loses))

        (get 'neovm-test-mp-ab 'error-conditions))

    (put 'neovm-test-mp-a 'error-conditions nil)
    (put 'neovm-test-mp-a 'error-message nil)
    (put 'neovm-test-mp-b 'error-conditions nil)
    (put 'neovm-test-mp-b 'error-message nil)
    (put 'neovm-test-mp-ab 'error-conditions nil)
    (put 'neovm-test-mp-ab 'error-message nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((a-caught neovm-test-mp-ab) (b-caught neovm-test-mp-ab) a-wins (neovm-test-mp-ab neovm-test-mp-a error neovm-test-mp-b))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// error-message-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_types_error_message_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; error-message-string on a plain error
  (condition-case err
      (error "formatted %s %d" "msg" 7)
    (error (error-message-string err)))

  ;; error-message-string on wrong-type-argument
  (condition-case err
      (+ "x" 1)
    (error (error-message-string err)))

  ;; error-message-string on void-variable
  (condition-case err
      (symbol-value 'neovm--unbound-for-msg-test-999)
    (error (stringp (error-message-string err))))

  ;; error-message-string on arith-error
  (condition-case err
      (/ 1 0)
    (error (stringp (error-message-string err))))

  ;; error-message-string returns a string
  (condition-case err
      (error "test")
    (error (stringp (error-message-string err)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"formatted msg 7\" \"Wrong type argument: number-or-marker-p, \\\"x\\\"\" t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// error symbol inheritance: error catches everything
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_error_types_error_catches_all_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that (error ...) handler catches all standard error types
    let form = r#"
(list
  (condition-case err (signal 'wrong-type-argument '(x)) (error (car err)))
  (condition-case err (signal 'wrong-number-of-arguments '(x 1)) (error (car err)))
  (condition-case err (signal 'void-variable '(x)) (error (car err)))
  (condition-case err (signal 'void-function '(x)) (error (car err)))
  (condition-case err (signal 'invalid-function '(x)) (error (car err)))
  (condition-case err (signal 'args-out-of-range '(x 1)) (error (car err)))
  (condition-case err (signal 'arith-error nil) (error (car err)))
  (condition-case err (signal 'overflow-error nil) (error (car err)))
  (condition-case err (signal 'range-error nil) (error (car err)))
  (condition-case err (signal 'domain-error nil) (error (car err)))
  (condition-case err (signal 'user-error '("x")) (error (car err)))
  (condition-case err (signal 'setting-constant '(nil)) (error (car err))))"#;
    let expect = expect_test::expect![[
        r#""OK (wrong-type-argument wrong-number-of-arguments void-variable void-function invalid-function args-out-of-range arith-error overflow-error range-error domain-error user-error setting-constant)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
