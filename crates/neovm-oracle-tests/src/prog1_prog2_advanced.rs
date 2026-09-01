//! Oracle parity tests for `prog1`, `prog2`, `progn` interactions with
//! complex patterns.
//!
//! Tests return-value semantics, side effects in non-returned forms,
//! nesting, state-saving patterns, setup+compute+cleanup, and
//! pipeline combinations of all three.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// prog1 returns first form, evaluates all side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog1_returns_first_evaluates_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((trace nil))
  ;; prog1 returns the first form but evaluates all subsequent forms
  (let ((r1 (prog1
                (progn (setq trace (cons 'first trace)) 'alpha)
              (setq trace (cons 'second trace))
              (setq trace (cons 'third trace))
              (setq trace (cons 'fourth trace))))
        (r2 (prog1
                42
              ;; These side effects must happen
              (setq trace (cons 'after-42 trace))))
        (r3 (prog1 'only-one))  ;; no body forms
        (r4 (prog1 nil 'ignored1 'ignored2)))
    (list r1 r2 r3 r4
          ;; Trace proves all forms evaluated in order
          (nreverse trace))))"#;
    let expect = expect_test::expect![[
        r#""OK (alpha 42 only-one nil (first second third fourth after-42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// prog2 returns second form, evaluates all
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog2_returns_second_evaluates_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((trace nil))
  ;; prog2 returns the second form
  (let ((r1 (prog2
                (setq trace (cons 'setup trace))
                (progn (setq trace (cons 'compute trace)) 'result-value)
              (setq trace (cons 'cleanup trace))))
        ;; prog2 with many body forms after the second
        (r2 (prog2
                (setq trace (cons 'p2-first trace))
                (+ 100 200)
              (setq trace (cons 'p2-third trace))
              (setq trace (cons 'p2-fourth trace))
              (setq trace (cons 'p2-fifth trace))))
        ;; prog2 minimal: just two forms
        (r3 (prog2 'ignored 'kept)))
    (list r1 r2 r3
          (nreverse trace))))"#;
    let expect = expect_test::expect![[
        r#""OK (result-value 300 kept (setup compute cleanup p2-first p2-third p2-fourth p2-fifth))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Side effects in non-returned forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_side_effects_in_non_returned() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that mutation happens even in forms whose values are discarded
    let form = r#"(let ((counter 0)
                        (accum nil))
  ;; prog1: first form's value returned, but all forms mutate state
  (let ((val (prog1
                 (progn (setq counter (1+ counter)) counter)
               (setq counter (* counter 10))
               (setq accum (cons counter accum))
               (setq counter (+ counter 5))
               (setq accum (cons counter accum)))))
    ;; val should be 1 (the value of counter when first form ran)
    ;; counter should be 15 (1 * 10 + 5)
    ;; accum should be (15 10)
    (let ((prog1-result (list val counter (nreverse accum))))
      ;; Reset
      (setq counter 0 accum nil)
      ;; prog2: similar test
      (let ((val2 (prog2
                      (progn (setq counter (1+ counter)) 'discarded-first)
                      (progn (setq counter (* counter 3)) counter)
                    (setq counter (+ counter 100))
                    (setq accum (cons counter accum)))))
        ;; val2 should be 3 (1 * 3)
        ;; counter should be 103 (3 + 100)
        (list prog1-result val2 counter (nreverse accum))))))"#;
    let expect = expect_test::expect![[r#""OK ((1 15 (10 15)) 3 103 (103))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested prog1/prog2/progn
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_nested_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((log nil))
  ;; prog1 containing prog2 containing progn
  (let ((outer
         (prog1
             (prog2
                 (progn
                   (setq log (cons 'inner-progn-1 log))
                   (setq log (cons 'inner-progn-2 log))
                   'progn-last)
                 (progn
                   (setq log (cons 'prog2-second log))
                   'prog2-result)
               (setq log (cons 'prog2-body log)))
           (setq log (cons 'prog1-body log)))))
    ;; prog2 nesting: prog2 inside prog1 inside progn
    (let ((nested2
           (progn
             (setq log (cons 'n2-progn-start log))
             (prog1
                 (prog2
                     (setq log (cons 'n2-first log))
                     (+ 10 20)
                   (setq log (cons 'n2-third log)))
               (setq log (cons 'n2-prog1-body log))))))
      ;; Deep nesting: prog1 in prog1 in prog1
      (let ((deep
             (prog1
                 (prog1
                     (prog1
                         (progn (setq log (cons 'deepest log)) 'deep-val)
                       (setq log (cons 'deep-2 log)))
                   (setq log (cons 'deep-1 log)))
               (setq log (cons 'deep-0 log)))))
        (list outer nested2 deep (nreverse log))))))"#;
    let expect = expect_test::expect![[
        r#""OK (prog2-result 30 deep-val (inner-progn-1 inner-progn-2 prog2-second prog2-body prog1-body n2-progn-start n2-first n2-third n2-prog1-body deepest deep-2 deep-1 deep-0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: prog1 for saving state during mutation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog1_save_state_during_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The classic use of prog1: capture a value before mutating it
    let form = r#"(let ((stack '(a b c d e)))
  ;; Pop: return the top element, then mutate the stack
  (let ((popped1 (prog1 (car stack) (setq stack (cdr stack))))
        ;; After first pop, stack is (b c d e)
        )
    (let ((popped2 (prog1 (car stack) (setq stack (cdr stack)))))
      ;; Swap top two elements using prog1
      (let* ((top (prog1 (car stack) (setq stack (cdr stack))))
             (second (prog1 (car stack) (setq stack (cdr stack)))))
        (setq stack (cons top (cons second stack)))
        ;; Build a queue-like dequeue using prog1
        (let ((queue '(x y z)))
          (let ((front (prog1 (car queue) (setq queue (cdr queue)))))
            ;; Enqueue at end
            (setq queue (append queue (list 'w)))
            (list popped1 popped2 top second
                  stack queue front)))))))"#;
    let expect = expect_test::expect![[r#""OK (a b c d (c d e) (y z w) x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: prog2 for setup + compute + cleanup patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog2_setup_compute_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // prog2 is ideal for: (prog2 setup-form compute-form cleanup-forms...)
    let form = r#"(let ((resource-log nil)
                        (resources-held nil))
  ;; Pattern: acquire resource, compute, release resource
  (let ((result1
         (prog2
             ;; Setup: acquire resource
             (progn
               (setq resources-held (cons 'db-conn resources-held))
               (setq resource-log (cons '(acquire db-conn) resource-log)))
             ;; Compute: use resource
             (let ((data '(1 2 3 4 5)))
               (apply '+ data))
           ;; Cleanup: release resource
           (setq resources-held (delq 'db-conn resources-held))
           (setq resource-log (cons '(release db-conn) resource-log)))))
    ;; Nested setup-compute-cleanup
    (let ((result2
           (prog2
               ;; Outer setup
               (progn
                 (setq resources-held (cons 'file-handle resources-held))
                 (setq resource-log (cons '(acquire file-handle) resource-log)))
               ;; Compute with inner setup-compute-cleanup
               (prog2
                   (progn
                     (setq resources-held (cons 'lock resources-held))
                     (setq resource-log (cons '(acquire lock) resource-log)))
                   (* 6 7)
                 (setq resources-held (delq 'lock resources-held))
                 (setq resource-log (cons '(release lock) resource-log)))
             ;; Outer cleanup
             (setq resources-held (delq 'file-handle resources-held))
             (setq resource-log (cons '(release file-handle) resource-log)))))
      (list result1 result2
            resources-held  ;; should be nil (all released)
            (nreverse resource-log)))))"#;
    let expect = expect_test::expect![[
        r#""OK (15 42 nil ((acquire db-conn) (release db-conn) (acquire file-handle) (acquire lock) (release lock) (release file-handle)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: combining prog1/prog2/progn in pipeline patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_prog_pipeline_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a data processing pipeline where each stage uses different prog forms
    let form = r#"(let ((pipeline-log nil)
                        (data '(5 3 8 1 9 2 7 4 6)))
  ;; Stage 1 (prog1): capture original data while starting processing
  (let ((original
         (prog1
             (copy-sequence data)
           (setq pipeline-log (cons (list 'stage1-input (length data)) pipeline-log)))))
    ;; Stage 2 (prog2): setup counter, compute sorted data, log completion
    (let ((sorted
           (prog2
               (setq pipeline-log (cons 'stage2-start pipeline-log))
               (sort (copy-sequence data) '<)
             (setq pipeline-log (cons 'stage2-done pipeline-log)))))
      ;; Stage 3 (progn): transform sorted data with multiple steps
      (let ((transformed
             (progn
               (setq pipeline-log (cons 'stage3-start pipeline-log))
               ;; Double each element
               (let ((doubled (mapcar (lambda (x) (* x 2)) sorted)))
                 (setq pipeline-log (cons (list 'stage3-doubled (length doubled)) pipeline-log))
                 ;; Filter: keep only values > 10
                 (let ((filtered nil))
                   (dolist (x doubled)
                     (when (> x 10)
                       (setq filtered (cons x filtered))))
                   (setq pipeline-log (cons 'stage3-done pipeline-log))
                   (nreverse filtered))))))
        ;; Stage 4: combine all with prog1 to return final but log intermediate
        (let ((final-result
               (prog1
                   (list
                    'original original
                    'sorted sorted
                    'transformed transformed
                    'count (length transformed))
                 (setq pipeline-log (cons 'complete pipeline-log)))))
          (list final-result (nreverse pipeline-log)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((original (5 3 8 1 9 2 7 4 6) sorted (1 2 3 4 5 6 7 8 9) transformed (12 14 16 18) count 4) ((stage1-input 9) stage2-start stage2-done stage3-start (stage3-doubled 9) stage3-done complete))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
