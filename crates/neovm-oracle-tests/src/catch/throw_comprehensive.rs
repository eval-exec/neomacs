//! Oracle parity tests for comprehensive catch/throw patterns.
//!
//! Tests: nested catch with same/different tags, throw across function
//! boundaries, throw value types (nil, integers, strings, lists, vectors),
//! catch with no throw, dynamic extent of catch tags, interaction with
//! unwind-protect, interaction with condition-case, signal vs throw
//! differences, and tag identity (eq vs equal).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// ---------------------------------------------------------------------------
// Test 1: Nested catch with same tags — throw goes to innermost
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_same_tag_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Same tag nested — throw hits innermost
  (catch 'tag
    (list 'outer-before
          (catch 'tag
            (throw 'tag 'inner-caught))
          'outer-after))
  ;; Three levels of same tag
  (catch 'tag
    (list 'L1
          (catch 'tag
            (list 'L2
                  (catch 'tag
                    (throw 'tag 'L3-caught))
                  'L2-after))
          'L1-after))
  ;; Same tag, throw in innermost after some computation
  (catch 'x
    (+ 1
       (catch 'x
         (+ 2
            (catch 'x
              (+ 3 (throw 'x 100)))))))
  ;; Throw past exhausted inner, caught by outer
  (catch 'tag
    (progn
      (catch 'tag
        'inner-no-throw)
      (throw 'tag 'hit-outer))))
"#;
    let expect = expect_test::expect![
        r#""OK ((outer-before inner-caught outer-after) (L1 (L2 L3-caught L2-after) L1-after) 103 hit-outer)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 2: Nested catch with different tags — selective catching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_different_tag_nesting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Throw to outer skipping inner with different tag
  (catch 'outer
    (catch 'inner
      (throw 'outer 'skipped-inner)))
  ;; Throw to inner, outer continues
  (catch 'outer
    (list 'before
          (catch 'inner
            (throw 'inner 'inner-val))
          'after))
  ;; Multiple different tags, throw to middle one
  (catch 'a
    (catch 'b
      (catch 'c
        (throw 'b 'hit-b))))
  ;; Layered: inner catches its tag, outer gets different value
  (catch 'alpha
    (list
      (catch 'beta
        (throw 'beta 'beta-val))
      (catch 'gamma
        (throw 'gamma 'gamma-val))
      'alpha-continues))
  ;; Throw across two catch boundaries to outermost
  (catch 'level-0
    (catch 'level-1
      (catch 'level-2
        (catch 'level-3
          (throw 'level-0 'escaped-all))))))
"#;
    let expect = expect_test::expect![
        r#""OK (skipped-inner (before inner-val after) hit-b (beta-val gamma-val alpha-continues) escaped-all)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 3: Throw across function boundaries — dynamic extent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_throw_across_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (defun neovm--test-ctc-thrower (tag val)
    (throw tag val))

  (defun neovm--test-ctc-indirect (tag val)
    (neovm--test-ctc-thrower tag val))

  (defun neovm--test-ctc-deep (tag val depth)
    (if (<= depth 0)
        (throw tag val)
      (1+ (neovm--test-ctc-deep tag val (1- depth)))))

  (unwind-protect
      (list
        ;; Direct throw from called function
        (catch 'escape
          (neovm--test-ctc-thrower 'escape 'direct))
        ;; Throw through two function calls
        (catch 'escape
          (neovm--test-ctc-indirect 'escape 'indirect))
        ;; Throw through recursive depth
        (catch 'escape
          (neovm--test-ctc-deep 'escape 'from-depth-5 5))
        ;; Lambda-based throw across call boundary
        (catch 'lam-tag
          (funcall (lambda ()
                     (funcall (lambda ()
                                (throw 'lam-tag 'from-nested-lambda)))))))
    (fmakunbound 'neovm--test-ctc-thrower)
    (fmakunbound 'neovm--test-ctc-indirect)
    (fmakunbound 'neovm--test-ctc-deep)))
"#;
    let expect = expect_test::expect![r#""OK (direct indirect from-depth-5 from-nested-lambda)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 4: Throw value types — nil, integers, strings, lists, vectors
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_throw_value_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; nil
  (catch 'tag (throw 'tag nil))
  ;; integer
  (catch 'tag (throw 'tag 42))
  ;; negative integer
  (catch 'tag (throw 'tag -999))
  ;; float
  (catch 'tag (throw 'tag 3.14))
  ;; string
  (catch 'tag (throw 'tag "hello world"))
  ;; symbol
  (catch 'tag (throw 'tag 'some-symbol))
  ;; cons cell
  (catch 'tag (throw 'tag (cons 'a 'b)))
  ;; proper list
  (catch 'tag (throw 'tag '(1 2 3 4 5)))
  ;; nested list
  (catch 'tag (throw 'tag '((a b) (c d) (e (f g)))))
  ;; vector
  (catch 'tag (throw 'tag [1 2 3]))
  ;; mixed vector
  (catch 'tag (throw 'tag [1 "two" three (4 5)]))
  ;; boolean t
  (catch 'tag (throw 'tag t))
  ;; character
  (catch 'tag (throw 'tag ?A)))
"#;
    let expect = expect_test::expect![[
        r#""OK (nil 42 -999 3.14 \"hello world\" some-symbol (a . b) (1 2 3 4 5) ((a b) (c d) (e (f g))) [1 2 3] [1 \"two\" three (4 5)] t 65)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 5: Catch with no throw — returns body value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_no_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Simple body returns its value
  (catch 'tag 42)
  ;; Progn body returns last
  (catch 'tag (progn 1 2 3))
  ;; Nil body
  (catch 'tag nil)
  ;; Complex computation with no throw
  (catch 'tag
    (let ((x 10) (y 20))
      (* (+ x y) (- y x))))
  ;; Conditional that doesn't throw
  (catch 'tag
    (if (> 3 2) 'yes 'no))
  ;; Loop that completes without throwing
  (catch 'tag
    (let ((sum 0))
      (dolist (x '(1 2 3 4 5))
        (setq sum (+ sum x)))
      sum))
  ;; Nested catch, neither throws
  (catch 'outer
    (catch 'inner
      (+ 10 20)))
  ;; Empty catch body
  (catch 'tag))
"#;
    let expect = expect_test::expect![r#""OK (42 3 nil 300 yes 15 30 nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 6: Catch/throw with unwind-protect — cleanup always runs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_unwind_protect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil))
  (list
    ;; Basic: unwind-protect cleanup runs on throw
    (catch 'tag
      (unwind-protect
          (throw 'tag 'thrown)
        (setq log (cons 'cleanup1 log))))
    ;; Nested unwind-protect layers
    (catch 'tag
      (unwind-protect
          (unwind-protect
              (unwind-protect
                  (throw 'tag 'deep-throw)
                (setq log (cons 'inner-cleanup log)))
            (setq log (cons 'mid-cleanup log)))
        (setq log (cons 'outer-cleanup log))))
    ;; unwind-protect with no throw — body form completes normally
    (catch 'tag
      (unwind-protect
          'normal-exit
        (setq log (cons 'normal-cleanup log))))
    ;; Throw inside unwind-protect body, cleanup modifies state
    (let ((resource 'acquired))
      (catch 'tag
        (unwind-protect
            (progn
              (setq resource 'in-use)
              (throw 'tag 'done))
          (setq resource 'released)))
      (setq log (cons resource log)))
    ;; Complete log
    (nreverse log)))
"#;
    let expect = expect_test::expect![
        r#""OK (thrown deep-throw normal-exit (released) (cleanup1 inner-cleanup mid-cleanup outer-cleanup normal-cleanup released))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 7: Catch/throw with condition-case interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_condition_case_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; throw is NOT an error — condition-case doesn't catch it
  (catch 'tag
    (condition-case _
        (throw 'tag 'thrown-past-handler)
      (error 'would-not-catch)))
  ;; error inside catch — condition-case catches it, not catch
  (catch 'tag
    (condition-case _
        (error "boom")
      (error 'error-caught)))
  ;; throw after error is caught
  (catch 'tag
    (condition-case _
        (error "boom")
      (error (throw 'tag 'error-then-throw))))
  ;; condition-case wrapping catch
  (condition-case _
      (catch 'tag
        (throw 'tag 'inner-throw))
    (error 'not-reached))
  ;; Nested: condition-case inside catch, error then throw in handler
  (catch 'outer
    (condition-case _
        (progn
          (catch 'inner
            (error "trigger"))
          'after-inner)
      (error
       (throw 'outer 'escaped-via-handler))))
  ;; signal vs throw: signal is caught by condition-case, not catch
  (list
    (condition-case err
        (signal 'wrong-type-argument '(numberp "x"))
      (wrong-type-argument (car (cdr err))))
    (catch 'tag
      (condition-case _
          (signal 'void-variable '(undefined-var))
        (void-variable 'signal-caught)))))
"#;
    let expect = expect_test::expect![
        r#""OK (thrown-past-handler error-caught error-then-throw inner-throw escaped-via-handler (numberp signal-caught))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 8: Tag identity — eq semantics (symbols are eq, strings are not)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_tag_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Symbol tags work (eq comparison)
  (catch 'my-tag
    (throw 'my-tag 'symbol-tag-works))
  ;; Same symbol from variable
  (let ((tag 'dynamic-tag))
    (catch tag
      (throw tag 'dynamic-works)))
  ;; Computed tag — both sides eval to same symbol
  (let ((base "my"))
    (catch (intern (concat base "-tag"))
      (throw (intern (concat base "-tag")) 'intern-works)))
  ;; Integer tags (eq for small integers)
  (catch 42
    (throw 42 'int-tag-works))
  ;; nil as tag
  (catch nil
    (throw nil 'nil-tag-works))
  ;; t as tag
  (catch t
    (throw t 't-tag-works))
  ;; Tag from function return value
  (catch (car '(found-it))
    (throw (car '(found-it)) 'car-tag-works)))
"#;
    let expect = expect_test::expect![r#""ERR (no-catch nil nil-tag-works)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 9: Dynamic extent — catch only active during its body
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_dynamic_extent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Demonstrate dynamic extent: throw must happen during catch body
  ;; Save a closure that throws, call it inside and outside catch
  (defun neovm--test-ctc-dyn-call-in-catch (fn)
    (catch 'dyn-tag
      (funcall fn)))

  (unwind-protect
      (list
        ;; Works: closure throws while catch is active
        (neovm--test-ctc-dyn-call-in-catch
         (lambda () (throw 'dyn-tag 'caught)))
        ;; Catch with no matching throw — closure returns normally
        (neovm--test-ctc-dyn-call-in-catch
         (lambda () 'no-throw))
        ;; Multiple calls, alternating throw/no-throw
        (list
          (neovm--test-ctc-dyn-call-in-catch (lambda () (throw 'dyn-tag 1)))
          (neovm--test-ctc-dyn-call-in-catch (lambda () 2))
          (neovm--test-ctc-dyn-call-in-catch (lambda () (throw 'dyn-tag 3)))
          (neovm--test-ctc-dyn-call-in-catch (lambda () 4))))
    (fmakunbound 'neovm--test-ctc-dyn-call-in-catch)))
"#;
    let expect = expect_test::expect![r#""OK (caught no-throw (1 2 3 4))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 10: Complex control flow — catch as early return in iteration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_early_return_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  ;; Find first pair in alist whose cdr satisfies a predicate
  (defun neovm--test-ctc-find-pair (alist pred)
    (catch 'found
      (dolist (pair alist)
        (when (funcall pred (cdr pair))
          (throw 'found pair)))
      nil))

  ;; Accumulate with early termination on sentinel
  (defun neovm--test-ctc-accum-until (lst sentinel)
    (catch 'done
      (let ((acc nil))
        (dolist (x lst)
          (if (equal x sentinel)
              (throw 'done (nreverse acc))
            (setq acc (cons (* x x) acc))))
        (nreverse acc))))

  ;; Nested search: find first list containing a target
  (defun neovm--test-ctc-nested-search (lists target)
    (catch 'outer
      (dolist (lst lists)
        (catch 'inner
          (dolist (x lst)
            (when (equal x target)
              (throw 'outer (list 'found-in lst 'at x))))))
      'not-found))

  (unwind-protect
      (list
        (neovm--test-ctc-find-pair '((a . 1) (b . 5) (c . 3) (d . 8))
                                   (lambda (v) (> v 4)))
        (neovm--test-ctc-find-pair '((x . 0) (y . 0))
                                   (lambda (v) (> v 10)))
        (neovm--test-ctc-accum-until '(1 2 3 :stop 4 5) :stop)
        (neovm--test-ctc-accum-until '(1 2 3 4 5) :stop)
        (neovm--test-ctc-nested-search '((1 2 3) (4 5 6) (7 8 9)) 5)
        (neovm--test-ctc-nested-search '((1 2) (3 4)) 99))
    (fmakunbound 'neovm--test-ctc-find-pair)
    (fmakunbound 'neovm--test-ctc-accum-until)
    (fmakunbound 'neovm--test-ctc-nested-search)))
"#;
    let expect = expect_test::expect![
        r#""OK ((b . 5) nil (1 4 9) (1 4 9 16 25) (found-in (4 5 6) at 5) not-found)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 11: Throw value computed from complex expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_complex_throw_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; Throw value is a computed list
  (catch 'tag
    (let ((x 10) (y 20))
      (throw 'tag (list (+ x y) (* x y) (- y x)))))
  ;; Throw value from mapcar
  (catch 'tag
    (throw 'tag (mapcar '1+ '(1 2 3 4 5))))
  ;; Throw value from let* chain
  (catch 'tag
    (let* ((a 2)
           (b (* a 3))
           (c (+ a b)))
      (throw 'tag (list a b c))))
  ;; Throw value is result of catch (catch returns, then thrown)
  (catch 'outer
    (throw 'outer
           (catch 'inner
             (throw 'inner '(from inner)))))
  ;; Throw value computed by recursive function
  (catch 'tag
    (let ((factorial nil))
      (setq factorial
            (lambda (n)
              (if (<= n 1) 1
                (* n (funcall factorial (1- n))))))
      (throw 'tag (funcall factorial 6)))))
"#;
    let expect = expect_test::expect![r#""OK ((30 200 10) (2 3 4 5 6) (2 6 8) (from inner) 720)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Test 12: signal vs throw — behavioral differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_comprehensive_signal_vs_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
  ;; signal is caught by condition-case, NOT by catch
  (catch 'error
    (condition-case err
        (signal 'error '("test error"))
      (error (list 'condition-case-caught (car (cdr err))))))
  ;; throw is caught by catch, NOT by condition-case
  (condition-case _
      (catch 'done
        (throw 'done 'catch-caught))
    (error 'not-reached))
  ;; Both in same context — each caught by its own handler
  (let ((results nil))
    (catch 'escape
      (condition-case _
          (progn
            (setq results (cons 'before results))
            (signal 'arith-error nil)
            (setq results (cons 'unreachable results)))
        (arith-error
         (setq results (cons 'error-handled results))
         (throw 'escape 'done))))
    (list (nreverse results) 'final))
  ;; Unhandled signal propagates past catch
  (condition-case _
      (catch 'tag
        (signal 'void-function '(nonexistent)))
    (void-function 'signal-escaped-catch))
  ;; throw with no matching catch signals an error
  (condition-case err
      (throw 'no-such-catch-tag 'value)
    (no-catch (list 'no-catch-error (car (cdr err))))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((condition-case-caught \"test error\") catch-caught ((before error-handled) final) signal-escaped-catch (no-catch-error no-such-catch-tag))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
