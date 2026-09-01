//! Oracle parity tests for `cl-defgeneric` / `cl-defmethod` (EIEIO/cl-generic).
//!
//! Covers: basic method dispatch, specializer types (class, eql, head),
//! :before/:after/:around qualifiers, `cl-call-next-method`,
//! `cl-next-method-p`, multiple dispatch, method combination,
//! `cl-no-applicable-method`, default methods, and method redefinition.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Basic cl-defgeneric / cl-defmethod dispatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_basic_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (cl-defgeneric neovm--gf-describe (obj)
    "Return a description of OBJ.")

  ;; Default method (no specializer = t)
  (cl-defmethod neovm--gf-describe (obj)
    (format "unknown:%S" obj))

  ;; Specializer on integer
  (cl-defmethod neovm--gf-describe ((obj integer))
    (format "int:%d" obj))

  ;; Specializer on string
  (cl-defmethod neovm--gf-describe ((obj string))
    (format "str:%s" obj))

  ;; Specializer on symbol
  (cl-defmethod neovm--gf-describe ((obj symbol))
    (format "sym:%s" obj))

  (unwind-protect
      (list
       ;; Dispatch on integer
       (neovm--gf-describe 42)
       ;; Dispatch on string
       (neovm--gf-describe "hello")
       ;; Dispatch on symbol
       (neovm--gf-describe 'world)
       ;; Dispatch on cons (falls through to default t method)
       (neovm--gf-describe '(1 2 3))
       ;; Dispatch on nil (symbol)
       (neovm--gf-describe nil)
       ;; Dispatch on float (falls to default since no float specializer)
       (neovm--gf-describe 3.14))
    (fmakunbound 'neovm--gf-describe)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"int:42\" \"str:hello\" \"sym:world\" \"unknown:(1 2 3)\" \"sym:nil\" \"unknown:3.14\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eql specializer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_eql_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (cl-defgeneric neovm--gf-handle-code (code)
    "Handle a status code.")

  (cl-defmethod neovm--gf-handle-code ((code (eql 200)))
    'ok)

  (cl-defmethod neovm--gf-handle-code ((code (eql 404)))
    'not-found)

  (cl-defmethod neovm--gf-handle-code ((code (eql 500)))
    'server-error)

  ;; Default for any other code
  (cl-defmethod neovm--gf-handle-code ((code integer))
    (list 'other code))

  (unwind-protect
      (list
       (neovm--gf-handle-code 200)
       (neovm--gf-handle-code 404)
       (neovm--gf-handle-code 500)
       ;; Not an eql match, falls to integer method
       (neovm--gf-handle-code 301)
       (neovm--gf-handle-code 418)
       ;; eql specializers are checked before type specializers
       (neovm--gf-handle-code 200))
    (fmakunbound 'neovm--gf-handle-code)))"#;
    let expect =
        expect_test::expect![[r#""OK (ok not-found server-error (other 301) (other 418) ok)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// head specializer (dispatches on (car arg))
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_head_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (cl-defgeneric neovm--gf-eval-expr (expr)
    "Evaluate a simple expression AST.")

  (cl-defmethod neovm--gf-eval-expr ((expr (head add)))
    (+ (nth 1 expr) (nth 2 expr)))

  (cl-defmethod neovm--gf-eval-expr ((expr (head mul)))
    (* (nth 1 expr) (nth 2 expr)))

  (cl-defmethod neovm--gf-eval-expr ((expr (head neg)))
    (- (nth 1 expr)))

  (cl-defmethod neovm--gf-eval-expr ((expr (head if-pos)))
    (if (> (nth 1 expr) 0) (nth 2 expr) (nth 3 expr)))

  ;; Default for unrecognized head
  (cl-defmethod neovm--gf-eval-expr (expr)
    (list 'unknown-expr expr))

  (unwind-protect
      (list
       (neovm--gf-eval-expr '(add 3 4))
       (neovm--gf-eval-expr '(mul 5 6))
       (neovm--gf-eval-expr '(neg 7))
       (neovm--gf-eval-expr '(if-pos 1 yes no))
       (neovm--gf-eval-expr '(if-pos -1 yes no))
       ;; Unknown head
       (neovm--gf-eval-expr '(sub 10 3))
       ;; Non-cons falls to default
       (neovm--gf-eval-expr 42))
    (fmakunbound 'neovm--gf-eval-expr)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 30 -7 yes no (unknown-expr (sub 10 3)) (unknown-expr 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :before / :after qualifiers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_before_after_qualifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (defvar neovm--gf-log nil)

  (cl-defgeneric neovm--gf-process (item)
    "Process an item with logging.")

  (cl-defmethod neovm--gf-process :before ((item integer))
    (push (format "before-int:%d" item) neovm--gf-log))

  (cl-defmethod neovm--gf-process ((item integer))
    (push (format "primary-int:%d" item) neovm--gf-log)
    (* item 2))

  (cl-defmethod neovm--gf-process :after ((item integer))
    (push (format "after-int:%d" item) neovm--gf-log))

  (cl-defmethod neovm--gf-process :before ((item string))
    (push (format "before-str:%s" item) neovm--gf-log))

  (cl-defmethod neovm--gf-process ((item string))
    (push (format "primary-str:%s" item) neovm--gf-log)
    (concat item "!"))

  (cl-defmethod neovm--gf-process :after ((item string))
    (push (format "after-str:%s" item) neovm--gf-log))

  (unwind-protect
      (progn
        (setq neovm--gf-log nil)
        (let ((r1 (neovm--gf-process 5)))
          (let ((log1 (reverse neovm--gf-log)))
            (setq neovm--gf-log nil)
            (let ((r2 (neovm--gf-process "hi")))
              (let ((log2 (reverse neovm--gf-log)))
                (list
                 ;; Integer: before -> primary -> after
                 r1 log1
                 ;; String: before -> primary -> after
                 r2 log2))))))
    (fmakunbound 'neovm--gf-process)
    (makunbound 'neovm--gf-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (10 (\"before-int:5\" \"primary-int:5\" \"after-int:5\") \"hi!\" (\"before-str:hi\" \"primary-str:hi\" \"after-str:hi\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// :around qualifier and cl-call-next-method
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_around_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (cl-defgeneric neovm--gf-compute (x)
    "Compute with around method wrapping.")

  (cl-defmethod neovm--gf-compute ((x integer))
    (* x x))

  (cl-defmethod neovm--gf-compute :around ((x integer))
    ;; Wrap the primary method: add 1 to result
    (let ((result (cl-call-next-method)))
      (+ result 1)))

  (cl-defgeneric neovm--gf-transform (val)
    "Transform with next-method-p check.")

  (cl-defmethod neovm--gf-transform ((val integer))
    (list 'primary val))

  (cl-defmethod neovm--gf-transform :around ((val integer))
    (if (cl-next-method-p)
        (list 'around (cl-call-next-method))
      (list 'around-no-next val)))

  (unwind-protect
      (list
       ;; around wraps primary: 5*5=25, +1=26
       (neovm--gf-compute 5)
       ;; around wraps primary: 3*3=9, +1=10
       (neovm--gf-compute 3)
       ;; around with cl-next-method-p check
       (neovm--gf-transform 42)
       ;; 0 case
       (neovm--gf-compute 0))
    (fmakunbound 'neovm--gf-compute)
    (fmakunbound 'neovm--gf-transform)))"#;
    let expect = expect_test::expect![[r#""OK (26 10 (around (primary 42)) 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple dispatch (multi-argument specialization)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_multiple_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (cl-defgeneric neovm--gf-combine (a b)
    "Combine two values with type-based dispatch.")

  (cl-defmethod neovm--gf-combine ((a integer) (b integer))
    (+ a b))

  (cl-defmethod neovm--gf-combine ((a string) (b string))
    (concat a b))

  (cl-defmethod neovm--gf-combine ((a integer) (b string))
    (format "%d:%s" a b))

  (cl-defmethod neovm--gf-combine ((a string) (b integer))
    (format "%s:%d" a b))

  ;; Fallback
  (cl-defmethod neovm--gf-combine (a b)
    (list 'generic a b))

  (unwind-protect
      (list
       ;; int + int
       (neovm--gf-combine 3 4)
       ;; string + string
       (neovm--gf-combine "foo" "bar")
       ;; int + string
       (neovm--gf-combine 42 "hello")
       ;; string + int
       (neovm--gf-combine "world" 99)
       ;; Fallback: symbol + symbol
       (neovm--gf-combine 'a 'b)
       ;; Fallback: list + list
       (neovm--gf-combine '(1) '(2)))
    (fmakunbound 'neovm--gf-combine)))"#;
    let expect = expect_test::expect![[
        r#""OK (7 \"foobar\" \"42:hello\" \"world:99\" (generic a b) (generic (1) (2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Default method (no specializer = t)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_default_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (cl-defgeneric neovm--gf-stringify (obj)
    "Convert OBJ to a canonical string form.")

  ;; Only a default method
  (cl-defmethod neovm--gf-stringify (obj)
    (format "%S" obj))

  (unwind-protect
      (list
       (neovm--gf-stringify 42)
       (neovm--gf-stringify "hello")
       (neovm--gf-stringify 'foo)
       (neovm--gf-stringify nil)
       (neovm--gf-stringify '(1 2 3))
       (neovm--gf-stringify [a b c])
       (neovm--gf-stringify t))
    (fmakunbound 'neovm--gf-stringify)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"\\\"hello\\\"\" \"foo\" \"nil\" \"(1 2 3)\" \"[a b c]\" \"t\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// cl-no-applicable-method signaled on mismatch
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_no_applicable_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  ;; A generic with only a specialized method and NO default
  (cl-defgeneric neovm--gf-strict (x)
    "Only works on integers.")

  (cl-defmethod neovm--gf-strict ((x integer))
    (* x 10))

  (unwind-protect
      (list
       ;; Works for integer
       (neovm--gf-strict 5)
       ;; Calling with wrong type triggers cl-no-applicable-method
       (condition-case err
           (neovm--gf-strict "oops")
         (cl-no-applicable-method
          (list 'caught (car err))))
       ;; Another wrong type
       (condition-case err
           (neovm--gf-strict '(1 2))
         (cl-no-applicable-method
          (list 'caught (car err)))))
    (fmakunbound 'neovm--gf-strict)))"#;
    let expect = expect_test::expect![[
        r#""OK (50 (caught cl-no-applicable-method) (caught cl-no-applicable-method))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Method redefinition / override
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_method_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (cl-defgeneric neovm--gf-greet (name)
    "Greet someone.")

  (cl-defmethod neovm--gf-greet ((name string))
    (concat "Hello, " name))

  (unwind-protect
      (let ((r1 (neovm--gf-greet "Alice")))
        ;; Redefine the method
        (cl-defmethod neovm--gf-greet ((name string))
          (concat "Hi there, " name "!"))
        (let ((r2 (neovm--gf-greet "Alice")))
          (list
           r1   ;; Original definition
           r2   ;; Redefined
           ;; They should differ
           (not (equal r1 r2)))))
    (fmakunbound 'neovm--gf-greet)))"#;
    let expect = expect_test::expect![[r#""OK (\"Hello, Alice\" \"Hi there, Alice!\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined :before + :around + primary + :after execution order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_generic_function_full_method_combination_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (require 'cl-generic)

  (defvar neovm--gf-order-log nil)

  (cl-defgeneric neovm--gf-pipeline (x)
    "Test full method combination order.")

  (cl-defmethod neovm--gf-pipeline :before ((x integer))
    (push 'before neovm--gf-order-log))

  (cl-defmethod neovm--gf-pipeline :around ((x integer))
    (push 'around-start neovm--gf-order-log)
    (let ((result (cl-call-next-method)))
      (push 'around-end neovm--gf-order-log)
      (+ result 100)))

  (cl-defmethod neovm--gf-pipeline ((x integer))
    (push 'primary neovm--gf-order-log)
    x)

  (cl-defmethod neovm--gf-pipeline :after ((x integer))
    (push 'after neovm--gf-order-log))

  (unwind-protect
      (progn
        (setq neovm--gf-order-log nil)
        (let ((result (neovm--gf-pipeline 7)))
          (list
           ;; Return value: around wraps, so 7 + 100 = 107
           result
           ;; Execution order (reversed since we push):
           ;; around-start -> before -> primary -> after -> around-end
           (nreverse neovm--gf-order-log))))
    (fmakunbound 'neovm--gf-pipeline)
    (makunbound 'neovm--gf-order-log)))"#;
    let expect =
        expect_test::expect![[r#""OK (107 (around-start before primary after around-end))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
