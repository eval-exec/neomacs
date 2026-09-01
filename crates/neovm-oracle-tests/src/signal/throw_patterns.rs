//! Oracle parity tests for complex `signal`, `throw`, `catch` patterns:
//! signal with different error symbols, throw/catch across function
//! boundaries, nested catch blocks, value propagation, non-local exit
//! from deep recursion, custom error hierarchies, and catch as a loop
//! control mechanism.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// signal with different error symbols and data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_various_error_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test signal with standard error symbols (error, wrong-type-argument,
    // void-variable, args-out-of-range, etc.) and verify that
    // condition-case catches each one correctly with its data.
    let form = r#"(list
  ;; Standard 'error signal
  (condition-case err
      (signal 'error '("custom error message"))
    (error (list 'caught (car err) (cadr err))))

  ;; wrong-type-argument
  (condition-case err
      (signal 'wrong-type-argument '(numberp "not-a-number"))
    (wrong-type-argument (list 'wta (car err) (cadr err))))

  ;; void-variable
  (condition-case err
      (signal 'void-variable '(some-undefined-var))
    (void-variable (list 'void (car err))))

  ;; args-out-of-range
  (condition-case err
      (signal 'args-out-of-range '([1 2 3] 5))
    (args-out-of-range (list 'oor (car err) (cadr err))))

  ;; Signal with complex data (a list of lists)
  (condition-case err
      (signal 'error (list "complex" '(a b c) '(1 2 3) 42))
    (error (list 'complex-data (length err) (nth 0 err) (nth 1 err))))

  ;; Signal with nil data
  (condition-case err
      (signal 'error nil)
    (error (list 'nil-data err)))

  ;; Catching parent 'error matches child errors
  (condition-case err
      (signal 'wrong-type-argument '(integerp 3.14))
    (error (list 'parent-caught (car err) (cadr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught error \"custom error message\") (wta wrong-type-argument numberp) (void void-variable) (oor args-out-of-range [1 2 3]) (complex-data 5 error \"complex\") (nil-data (error)) (parent-caught wrong-type-argument integerp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// throw/catch across function boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_throw_catch_across_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // throw inside a lambda crosses multiple function call boundaries
    // to reach the matching catch. Test with multiple levels and
    // interleaved unwind-protect.
    let form = r#"(progn
  (fset 'neovm--tc-inner
    (lambda (val)
      (when (> val 10)
        (throw 'too-big (list 'overflow val)))
      (when (< val 0)
        (throw 'too-small (list 'underflow val)))
      (* val val)))

  (fset 'neovm--tc-middle
    (lambda (vals)
      (let ((results nil))
        (dolist (v vals)
          (setq results (cons (funcall 'neovm--tc-inner v) results)))
        (nreverse results))))

  (fset 'neovm--tc-outer
    (lambda (vals)
      (let ((big-result (catch 'too-big
                          (let ((small-result (catch 'too-small
                                                (funcall 'neovm--tc-middle vals))))
                            (list 'small-ok small-result)))))
        (if (and (consp big-result) (eq (car big-result) 'small-ok))
            big-result
          (list 'big-caught big-result)))))

  (unwind-protect
      (list
       ;; Normal case: all values in range
       (funcall 'neovm--tc-outer '(1 2 3 4 5))
       ;; Too big: 15 triggers throw
       (funcall 'neovm--tc-outer '(1 2 15 4))
       ;; Too small: -3 triggers throw
       (funcall 'neovm--tc-outer '(5 -3 7))
       ;; Edge cases
       (funcall 'neovm--tc-outer '(0 10))
       (funcall 'neovm--tc-outer '(11)))
    (fmakunbound 'neovm--tc-inner)
    (fmakunbound 'neovm--tc-middle)
    (fmakunbound 'neovm--tc-outer)))"#;
    let expect = expect_test::expect![[
        r#""OK ((small-ok (1 4 9 16 25)) (big-caught (overflow 15)) (small-ok (underflow -3)) (small-ok (0 100)) (big-caught (overflow 11)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested catch blocks: inner vs outer matching
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_catch_inner_outer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple nested catch blocks with different tags. Throw targets
    // specific tags and the correct catch should handle each one.
    // Also test what happens when inner catch doesn't match.
    let form = r#"(list
  ;; Inner tag matches: outer never sees the throw
  (catch 'outer
    (list 'outer-body
          (catch 'inner
            (throw 'inner 'inner-value))
          'outer-continues))

  ;; Outer tag matches: inner catch is bypassed
  (catch 'outer
    (list 'outer-body
          (catch 'inner
            (throw 'outer 'outer-value))
          'never-reached))

  ;; Nested with same-named tags: innermost catches
  (let ((log nil))
    (let ((r (catch 'tag
               (setq log (cons 'outer-start log))
               (let ((r2 (catch 'tag
                           (setq log (cons 'inner-start log))
                           (throw 'tag 'from-inner))))
                 (setq log (cons 'between log))
                 (list 'inner-result r2)))))
      (list r (nreverse log))))

  ;; Three levels, middle catches
  (catch 'level1
    (catch 'level2
      (catch 'level3
        (throw 'level2 'skipped-level3)))
    'level2-after)

  ;; Throw from a conditional branch
  (catch 'done
    (let ((x 42))
      (cond
       ((< x 0) (throw 'done 'negative))
       ((= x 0) (throw 'done 'zero))
       ((< x 10) (throw 'done 'small))
       ((< x 100) (throw 'done 'medium))
       (t (throw 'done 'large)))))

  ;; No throw: catch returns body value
  (catch 'unused
    (+ 10 20 30)))"#;
    let expect = expect_test::expect![[
        r#""OK ((outer-body inner-value outer-continues) outer-value ((inner-result from-inner) (outer-start inner-start between)) level2-after medium 60)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// throw value propagation through complex expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_throw_value_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The value passed to throw becomes the return value of catch.
    // Test with various value types and ensure they propagate correctly.
    let form = r#"(list
  ;; Throw an integer
  (catch 'tag (throw 'tag 42))
  ;; Throw a string
  (catch 'tag (throw 'tag "hello world"))
  ;; Throw nil
  (catch 'tag (throw 'tag nil))
  ;; Throw t
  (catch 'tag (throw 'tag t))
  ;; Throw a list
  (catch 'tag (throw 'tag '(1 2 3 4 5)))
  ;; Throw a vector
  (catch 'tag (throw 'tag [a b c]))
  ;; Throw a cons cell
  (catch 'tag (throw 'tag (cons 'key 'value)))
  ;; Throw result of computation
  (catch 'tag
    (let ((x 10) (y 20))
      (throw 'tag (list (+ x y) (* x y) (- x y)))))
  ;; Throw from inside mapcar
  (catch 'tag
    (mapcar (lambda (x)
              (when (= x 3) (throw 'tag (list 'found x)))
              (* x x))
            '(1 2 3 4 5)))
  ;; Throw value can itself be a throw-caught value
  (catch 'outer
    (throw 'outer
           (catch 'inner
             (throw 'inner 'inception)))))"#;
    let expect = expect_test::expect![[
        r#""OK (42 \"hello world\" nil t (1 2 3 4 5) [a b c] (key . value) (30 200 -10) (found 3) inception)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Non-local exit for early return from deep recursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nonlocal_exit_deep_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use throw to exit early from a recursive tree search.
    // The tree is searched depth-first and throw immediately
    // unwinds the entire recursion stack upon finding the target.
    let form = r#"(progn
  (fset 'neovm--tree-search
    (lambda (tree target path)
      "Search TREE for TARGET. PATH tracks current location.
       Throw 'found with full path on success."
      (cond
       ((null tree) nil)
       ((equal tree target)
        (throw 'found (nreverse (cons target path))))
       ((consp tree)
        (funcall 'neovm--tree-search (car tree) target (cons 'left path))
        (funcall 'neovm--tree-search (cdr tree) target (cons 'right path)))
       (t nil))))

  (fset 'neovm--find-in-tree
    (lambda (tree target)
      (catch 'found
        (funcall 'neovm--tree-search tree target nil)
        'not-found)))

  (unwind-protect
      (let ((tree '((1 . (2 . 3)) . ((4 . 5) . (6 . (7 . 8))))))
        (list
         ;; Find leaf nodes
         (funcall 'neovm--find-in-tree tree 1)
         (funcall 'neovm--find-in-tree tree 5)
         (funcall 'neovm--find-in-tree tree 8)
         (funcall 'neovm--find-in-tree tree 4)
         ;; Not found
         (funcall 'neovm--find-in-tree tree 99)
         ;; Find in smaller tree
         (funcall 'neovm--find-in-tree '(a . (b . c)) 'c)
         ;; Single element
         (funcall 'neovm--find-in-tree 'x 'x)
         (funcall 'neovm--find-in-tree 'x 'y)))
    (fmakunbound 'neovm--tree-search)
    (fmakunbound 'neovm--find-in-tree)))"#;
    let expect = expect_test::expect![[
        r#""OK ((left left 1) (right left right 5) (right right right right 8) (right left left 4) not-found (right right c) (x) not-found)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Custom error hierarchy using condition-case + signal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_custom_error_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define custom error types using put/error-conditions
    // and test that condition-case matches parent/child correctly.
    let form = r#"(progn
  ;; Define custom error hierarchy:
  ;;   app-error -> error
  ;;   network-error -> app-error -> error
  ;;   timeout-error -> network-error -> app-error -> error
  ;;   validation-error -> app-error -> error
  (put 'app-error 'error-conditions '(app-error error))
  (put 'app-error 'error-message "Application error")
  (put 'network-error 'error-conditions '(network-error app-error error))
  (put 'network-error 'error-message "Network error")
  (put 'timeout-error 'error-conditions '(timeout-error network-error app-error error))
  (put 'timeout-error 'error-message "Timeout error")
  (put 'validation-error 'error-conditions '(validation-error app-error error))
  (put 'validation-error 'error-message "Validation error")

  (unwind-protect
      (list
       ;; Catch specific leaf error
       (condition-case err
           (signal 'timeout-error '("connection timed out" 30))
         (timeout-error (list 'timeout (cadr err) (caddr err))))

       ;; Catch parent error matches child
       (condition-case err
           (signal 'timeout-error '("timed out"))
         (network-error (list 'network-caught (car err))))

       ;; Catch grandparent error matches grandchild
       (condition-case err
           (signal 'timeout-error '("deep match"))
         (app-error (list 'app-caught (car err))))

       ;; More specific handler wins (listed first)
       (condition-case err
           (signal 'timeout-error '("specific wins"))
         (timeout-error (list 'specific (car err)))
         (network-error (list 'general (car err)))
         (error (list 'base (car err))))

       ;; Validation error not caught by network-error
       (condition-case err
           (condition-case err2
               (signal 'validation-error '("bad input"))
             (network-error (list 'wrong-handler (car err2))))
         (app-error (list 'correct-handler (car err))))

       ;; Unmatched specific handler falls to next
       (condition-case err
           (signal 'validation-error '("falls through"))
         (timeout-error (list 'wrong))
         (network-error (list 'also-wrong))
         (app-error (list 'right (car err)))))

    ;; Cleanup: remove error properties
    (put 'app-error 'error-conditions nil)
    (put 'app-error 'error-message nil)
    (put 'network-error 'error-conditions nil)
    (put 'network-error 'error-message nil)
    (put 'timeout-error 'error-conditions nil)
    (put 'timeout-error 'error-message nil)
    (put 'validation-error 'error-conditions nil)
    (put 'validation-error 'error-message nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((timeout \"connection timed out\" 30) (network-caught timeout-error) (app-caught timeout-error) (specific timeout-error) (correct-handler validation-error) (right validation-error))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// catch as a loop control mechanism (break/continue)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_catch_as_loop_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use catch/throw to implement break and continue semantics
    // for iteration constructs.
    let form = r#"(list
  ;; Break: exit loop early with a result
  (catch 'break
    (let ((i 0) (sum 0))
      (while (< i 100)
        (setq sum (+ sum i))
        (when (> sum 50)
          (throw 'break (list 'broke-at i 'sum sum)))
        (setq i (1+ i)))
      (list 'completed 'sum sum)))

  ;; Continue: skip rest of body for certain items
  ;; (Simulated with nested catch per iteration)
  (let ((results nil))
    (dolist (x '(1 2 3 4 5 6 7 8 9 10))
      (catch 'continue
        (when (= (% x 3) 0)
          (throw 'continue nil))  ;; skip multiples of 3
        (setq results (cons (* x x) results))))
    (nreverse results))

  ;; Break from nested loops: only breaks the labeled loop
  (let ((found nil))
    (catch 'outer-break
      (dolist (row '((1 2 3) (4 5 6) (7 8 9)))
        (catch 'inner-break
          (dolist (cell row)
            (when (= cell 5)
              (setq found (list 'found cell 'in row))
              (throw 'outer-break nil))))))
    found)

  ;; Loop with multiple control flow paths
  (let ((processed nil)
        (skipped nil)
        (error-items nil))
    (dolist (item '(10 0 5 -1 20 3 -5 15 0 8))
      (catch 'next-item
        ;; Skip zeros
        (when (= item 0)
          (setq skipped (cons item skipped))
          (throw 'next-item nil))
        ;; Collect errors for negatives
        (when (< item 0)
          (setq error-items (cons item error-items))
          (throw 'next-item nil))
        ;; Process normally
        (setq processed (cons (* item 2) processed))))
    (list 'processed (nreverse processed)
          'skipped (nreverse skipped)
          'errors (nreverse error-items)))

  ;; Labeled break with accumulator: find first row with sum > 15
  (catch 'found-row
    (let ((row-idx 0))
      (dolist (row '((1 2 3) (4 5 6) (7 8 9) (10 11 12)))
        (let ((row-sum 0))
          (dolist (v row)
            (setq row-sum (+ row-sum v)))
          (when (> row-sum 15)
            (throw 'found-row (list 'row row-idx 'sum row-sum 'data row))))
        (setq row-idx (1+ row-idx)))
      'no-row-found)))"#;
    let expect = expect_test::expect![[
        r#""OK ((broke-at 10 sum 55) (1 4 16 25 49 64 100) (found 5 in (4 5 6)) (processed (20 10 40 6 30 16) skipped (0 0) errors (-1 -5)) (row 2 sum 24 data (7 8 9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// signal + condition-case + unwind-protect interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_signal_condition_unwind_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test the interaction between signal, condition-case, and
    // unwind-protect when all three are present and nested.
    let form = r#"(let ((log nil))
  (list
   ;; unwind-protect body signals, handler runs, cleanup runs
   (condition-case err
       (unwind-protect
           (progn
             (setq log (cons 'body log))
             (signal 'error '("boom"))
             (setq log (cons 'after-signal log)))
         (setq log (cons 'cleanup log)))
     (error
      (setq log (cons 'handler log))
      (list 'caught (cadr err))))

   ;; Verify execution order
   (nreverse log)

   ;; Nested: inner condition-case handles, outer never sees it
   (let ((inner-log nil))
     (condition-case outer-err
         (condition-case inner-err
             (progn
               (setq inner-log (cons 'try-inner inner-log))
               (signal 'error '("inner error")))
           (error
            (setq inner-log (cons 'handle-inner inner-log))
            'inner-handled))
       (error
        (setq inner-log (cons 'handle-outer inner-log))
        'outer-handled))
     (list (nreverse inner-log)))

   ;; Re-signal from handler: outer catches it
   (condition-case outer-err
       (condition-case inner-err
           (signal 'error '("original"))
         (error
          ;; Re-signal with modified data
          (signal 'error (list (concat "wrapped: " (cadr inner-err))))))
     (error (list 'rewrapped (cadr outer-err))))

   ;; throw inside condition-case handler bypasses error system
   (catch 'escape
     (condition-case err
         (signal 'error '("will be caught"))
       (error
        (throw 'escape (list 'escaped-from-handler (cadr err))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught \"boom\") (body cleanup handler) ((try-inner handle-inner)) (rewrapped \"wrapped: original\") (escaped-from-handler \"will be caught\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
