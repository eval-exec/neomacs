//! Oracle parity tests for a design-by-contract system implemented in Elisp.
//!
//! Implements preconditions, postconditions, invariants, contract-wrapped
//! functions, contract violation detection, contract inheritance (composition),
//! and contract-checked data structures.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Basic contract wrappers: precondition and postcondition
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_contract_basic_pre_post() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a contract system that wraps functions with pre/post conditions.
    // Violations produce (violation . message), successes return (ok . value).
    let form = r#"(progn
  ;; make-contract: wrap a function with precondition and postcondition checks
  ;; pre-fn: takes args, returns nil if ok, or error-message string
  ;; post-fn: takes (result . args), returns nil if ok, or error-message string
  (fset 'neovm--ct-make-contract
    (lambda (name impl pre-fn post-fn)
      (lambda (&rest args)
        (let ((pre-err (apply pre-fn args)))
          (if pre-err
              (list 'violation 'precondition name pre-err)
            (let ((result (apply impl args)))
              (let ((post-err (funcall post-fn (cons result args))))
                (if post-err
                    (list 'violation 'postcondition name post-err)
                  (list 'ok result)))))))))

  (unwind-protect
      (let* (;; Contract for integer division: divisor != 0, result * divisor <= dividend
             (safe-div
              (funcall 'neovm--ct-make-contract
                       "safe-div"
                       (lambda (a b) (/ a b))
                       ;; Precondition: b must not be zero, both must be integers
                       (lambda (a b)
                         (cond
                           ((not (integerp a)) "dividend must be integer")
                           ((not (integerp b)) "divisor must be integer")
                           ((= b 0) "division by zero")
                           (t nil)))
                       ;; Postcondition: result * b + remainder = a
                       (lambda (result-and-args)
                         (let ((result (car result-and-args))
                               (a (cadr result-and-args))
                               (b (caddr result-and-args)))
                           (if (<= (abs (* result b)) (abs a))
                               nil
                             "result magnitude exceeds dividend")))))
             ;; Contract for bounded-add: sum must be in [-1000, 1000]
             (bounded-add
              (funcall 'neovm--ct-make-contract
                       "bounded-add"
                       (lambda (a b) (+ a b))
                       ;; Precondition: both numbers
                       (lambda (a b)
                         (if (and (numberp a) (numberp b))
                             nil
                           "both arguments must be numbers"))
                       ;; Postcondition: result in range
                       (lambda (res-args)
                         (let ((result (car res-args)))
                           (if (and (>= result -1000) (<= result 1000))
                               nil
                             (format "result %d out of range [-1000, 1000]" result)))))))
        (list
          ;; Successful division
          (funcall safe-div 10 3)
          (funcall safe-div 100 7)
          (funcall safe-div -15 4)
          ;; Precondition violations
          (funcall safe-div 10 0)
          (funcall safe-div "x" 3)
          ;; Bounded add: success
          (funcall bounded-add 100 200)
          (funcall bounded-add -500 300)
          ;; Bounded add: postcondition violation
          (funcall bounded-add 800 500)
          (funcall bounded-add -600 -500)))
    (fmakunbound 'neovm--ct-make-contract)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 3) (ok 14) (ok -3) (violation precondition \"safe-div\" \"division by zero\") (violation precondition \"safe-div\" \"dividend must be integer\") (ok 300) (ok -200) (violation postcondition \"bounded-add\" \"result 1300 out of range [-1000, 1000]\") (violation postcondition \"bounded-add\" \"result -1100 out of range [-1000, 1000]\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Invariant-checked data structure: sorted list container
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_contract_invariant_sorted_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A sorted list container with an invariant that elements are always
    // in non-decreasing order. All mutations check the invariant.
    let form = r#"(let ((make-sorted-list nil)
                        (sorted-insert nil)
                        (sorted-remove nil)
                        (sorted-check-invariant nil)
                        (sorted-to-list nil))
  ;; Container is an alist: ((data . elements) (invariant-ok . t/nil))
  (setq sorted-check-invariant
        (lambda (container)
          (let ((elts (cdr (assq 'data container)))
                (ok t))
            (when (cdr elts)
              (let ((prev (car elts))
                    (rest (cdr elts)))
                (while (and rest ok)
                  (when (> prev (car rest))
                    (setq ok nil))
                  (setq prev (car rest))
                  (setq rest (cdr rest)))))
            ok)))
  (setq make-sorted-list
        (lambda ()
          (list (cons 'data nil) (cons 'invariant-ok t))))
  (setq sorted-to-list
        (lambda (container)
          (cdr (assq 'data container))))
  (setq sorted-insert
        (lambda (container val)
          (let* ((elts (cdr (assq 'data container)))
                 (new-elts nil)
                 (inserted nil))
            ;; Insert in sorted position
            (let ((rest elts))
              (while rest
                (if (and (not inserted) (<= val (car rest)))
                    (progn
                      (setq new-elts (cons val new-elts))
                      (setq inserted t))
                  nil)
                (setq new-elts (cons (car rest) new-elts))
                (setq rest (cdr rest))))
            (unless inserted
              (setq new-elts (cons val new-elts)))
            (setq new-elts (nreverse new-elts))
            (let ((result (list (cons 'data new-elts)
                                (cons 'invariant-ok nil))))
              ;; Check invariant
              (let ((ok (funcall sorted-check-invariant result)))
                (setcdr (assq 'invariant-ok result) ok)
                (if ok
                    (list 'ok result)
                  (list 'invariant-violation
                        (format "sort order broken after inserting %d" val))))))))
  (setq sorted-remove
        (lambda (container val)
          (let* ((elts (cdr (assq 'data container)))
                 (new-elts (delq val (copy-sequence elts))))
            (let ((result (list (cons 'data new-elts)
                                (cons 'invariant-ok nil))))
              (let ((ok (funcall sorted-check-invariant result)))
                (setcdr (assq 'invariant-ok result) ok)
                (list 'ok result))))))
  ;; Build up a sorted list
  (let* ((s0 (funcall make-sorted-list))
         (r1 (funcall sorted-insert s0 5))
         (s1 (cadr r1))
         (r2 (funcall sorted-insert s1 3))
         (s2 (cadr r2))
         (r3 (funcall sorted-insert s2 7))
         (s3 (cadr r3))
         (r4 (funcall sorted-insert s3 1))
         (s4 (cadr r4))
         (r5 (funcall sorted-insert s4 4))
         (s5 (cadr r5)))
    (let ((contents-after-inserts (funcall sorted-to-list s5)))
      ;; Remove an element
      (let* ((r6 (funcall sorted-remove s5 3))
             (s6 (cadr r6))
             (contents-after-remove (funcall sorted-to-list s6)))
        ;; Insert duplicate
        (let* ((r7 (funcall sorted-insert s6 4))
               (s7 (cadr r7))
               (contents-with-dup (funcall sorted-to-list s7)))
          (list
            ;; All operations succeeded
            (car r1) (car r2) (car r3) (car r4) (car r5)
            contents-after-inserts
            (car r6)
            contents-after-remove
            (car r7)
            contents-with-dup
            ;; Invariants all ok
            (funcall sorted-check-invariant s5)
            (funcall sorted-check-invariant s6)
            (funcall sorted-check-invariant s7)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (ok ok ok ok ok (1 3 4 5 7) ok (1 4 5 7) ok (1 4 4 5 7) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Contract inheritance: composing contracts on derived operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_contract_inheritance_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Base contracts can be composed: a derived contract inherits all
    // preconditions and postconditions from parent contracts plus its own.
    let form = r#"(progn
  ;; Contract combiner: merges multiple pre/post conditions
  (fset 'neovm--ct-combine-contracts
    (lambda (name impl pre-fns post-fns)
      (lambda (&rest args)
        ;; Check all preconditions
        (let ((pre-err nil)
              (pres pre-fns))
          (while (and pres (not pre-err))
            (setq pre-err (apply (car pres) args))
            (setq pres (cdr pres)))
          (if pre-err
              (list 'violation 'precondition name pre-err)
            (let ((result (apply impl args)))
              ;; Check all postconditions
              (let ((post-err nil)
                    (posts post-fns)
                    (ctx (cons result args)))
                (while (and posts (not post-err))
                  (setq post-err (funcall (car posts) ctx))
                  (setq posts (cdr posts)))
                (if post-err
                    (list 'violation 'postcondition name post-err)
                  (list 'ok result)))))))))

  (unwind-protect
      (let* (;; Base contract preconditions
             (check-numbers
              (lambda (&rest args)
                (let ((err nil))
                  (dolist (a args)
                    (unless (or err (numberp a))
                      (setq err (format "non-number argument: %S" a))))
                  err)))
             (check-positive
              (lambda (&rest args)
                (let ((err nil))
                  (dolist (a args)
                    (unless (or err (> a 0))
                      (setq err (format "non-positive argument: %S" a))))
                  err)))
             ;; Base postcondition: result is a number
             (check-result-number
              (lambda (ctx) (if (numberp (car ctx)) nil "result is not a number")))
             ;; Derived postcondition: result is positive
             (check-result-positive
              (lambda (ctx) (if (> (car ctx) 0) nil
                             (format "result %d is not positive" (car ctx)))))
             ;; sqrt-contract: inherits number-check + positive-check + result-positive
             (safe-sqrt
              (funcall 'neovm--ct-combine-contracts
                       "safe-sqrt"
                       (lambda (x) (truncate (sqrt x)))
                       (list check-numbers check-positive)
                       (list check-result-number check-result-positive)))
             ;; geometric-mean: inherits all base contracts
             (safe-geo-mean
              (funcall 'neovm--ct-combine-contracts
                       "safe-geo-mean"
                       (lambda (a b) (truncate (sqrt (* a b))))
                       (list check-numbers check-positive)
                       (list check-result-number)))
             ;; harmonic-mean with additional postcondition
             (safe-harmonic
              (funcall 'neovm--ct-combine-contracts
                       "safe-harmonic"
                       (lambda (a b) (/ (* 2 a b) (+ a b)))
                       (list check-numbers check-positive)
                       (list check-result-number check-result-positive))))
        (list
          ;; sqrt successes
          (funcall safe-sqrt 25)
          (funcall safe-sqrt 100)
          (funcall safe-sqrt 2)
          ;; sqrt failures
          (funcall safe-sqrt -4)
          (funcall safe-sqrt "x")
          ;; geometric mean
          (funcall safe-geo-mean 4 9)
          (funcall safe-geo-mean 16 25)
          (funcall safe-geo-mean -1 4)
          ;; harmonic mean
          (funcall safe-harmonic 3 6)
          (funcall safe-harmonic 10 10)
          (funcall safe-harmonic -1 5)))
    (fmakunbound 'neovm--ct-combine-contracts)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 5) (ok 10) (ok 1) (violation precondition \"safe-sqrt\" \"non-positive argument: -4\") (violation precondition \"safe-sqrt\" \"non-number argument: \\\"x\\\"\") (ok 6) (ok 20) (violation precondition \"safe-geo-mean\" \"non-positive argument: -1\") (ok 4) (ok 10) (violation precondition \"safe-harmonic\" \"non-positive argument: -1\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Contract-wrapped stack data structure with full invariant checking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_contract_checked_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A stack with contracts: push requires non-nil value, pop requires non-empty,
    // size invariant checked after every operation.
    let form = r#"(let ((make-stack nil)
                        (stack-push nil)
                        (stack-pop nil)
                        (stack-peek nil)
                        (stack-size nil)
                        (stack-to-list nil))
  (setq make-stack (lambda () (list (cons 'elts nil) (cons 'size 0))))
  (setq stack-size (lambda (s) (cdr (assq 'size s))))
  (setq stack-to-list (lambda (s) (cdr (assq 'elts s))))
  (setq stack-push
        (lambda (s val)
          ;; Precondition: val must not be nil
          (if (null val)
              (list 'violation "push: value must not be nil")
            (let* ((elts (cdr (assq 'elts s)))
                   (sz (cdr (assq 'size s)))
                   (new-elts (cons val elts))
                   (new-sz (1+ sz))
                   (result (list (cons 'elts new-elts) (cons 'size new-sz))))
              ;; Invariant: size must equal length of elements
              (if (/= new-sz (length new-elts))
                  (list 'violation "push: size mismatch after push")
                (list 'ok result))))))
  (setq stack-pop
        (lambda (s)
          ;; Precondition: stack must not be empty
          (let ((sz (cdr (assq 'size s))))
            (if (= sz 0)
                (list 'violation "pop: stack is empty")
              (let* ((elts (cdr (assq 'elts s)))
                     (val (car elts))
                     (new-elts (cdr elts))
                     (new-sz (1- sz))
                     (new-stack (list (cons 'elts new-elts) (cons 'size new-sz))))
                ;; Invariant check
                (if (/= new-sz (length new-elts))
                    (list 'violation "pop: size mismatch after pop")
                  (list 'ok val new-stack)))))))
  (setq stack-peek
        (lambda (s)
          (let ((sz (cdr (assq 'size s))))
            (if (= sz 0)
                (list 'violation "peek: stack is empty")
              (list 'ok (car (cdr (assq 'elts s))))))))
  ;; Exercise the stack
  (let* ((s0 (funcall make-stack))
         (r1 (funcall stack-push s0 'alpha))
         (s1 (cadr r1))
         (r2 (funcall stack-push s1 'bravo))
         (s2 (cadr r2))
         (r3 (funcall stack-push s2 'charlie))
         (s3 (cadr r3))
         ;; Peek should see charlie
         (pk (funcall stack-peek s3))
         ;; Pop charlie
         (r4 (funcall stack-pop s3))
         (s4 (caddr r4))
         ;; Pop bravo
         (r5 (funcall stack-pop s4))
         (s5 (caddr r5))
         ;; Pop alpha
         (r6 (funcall stack-pop s5))
         (s6 (caddr r6))
         ;; Pop on empty
         (r7 (funcall stack-pop s6))
         ;; Push nil (violation)
         (r8 (funcall stack-push s6 nil)))
    (list
      ;; All pushes ok
      (car r1) (car r2) (car r3)
      ;; Peek
      pk
      ;; Pop results
      (car r4) (cadr r4)   ;; ok, charlie
      (car r5) (cadr r5)   ;; ok, bravo
      (car r6) (cadr r6)   ;; ok, alpha
      ;; Violations
      r7 r8
      ;; Sizes at each stage
      (funcall stack-size s0)
      (funcall stack-size s1)
      (funcall stack-size s2)
      (funcall stack-size s3)
      ;; Contents at s3
      (funcall stack-to-list s3)
      ;; Empty after all pops
      (funcall stack-size s6)
      (funcall stack-to-list s6))))"#;
    let expect = expect_test::expect![[
        r#""OK (ok ok ok (ok charlie) ok charlie ok bravo ok alpha (violation \"pop: stack is empty\") (violation \"push: value must not be nil\") 0 1 2 3 (charlie bravo alpha) 0 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Contract system with named contract registries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_contract_registry_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A registry that stores named contracts and can apply them.
    // Supports registering, looking up, and composing contracts by name.
    let form = r#"(progn
  (defvar neovm--ct-registry nil)

  (fset 'neovm--ct-register
    (lambda (name pre-fn post-fn)
      (setq neovm--ct-registry
            (cons (list name pre-fn post-fn) neovm--ct-registry))))

  (fset 'neovm--ct-lookup
    (lambda (name)
      (let ((entry nil) (rest neovm--ct-registry))
        (while (and rest (not entry))
          (when (eq (car (car rest)) name)
            (setq entry (car rest)))
          (setq rest (cdr rest)))
        entry)))

  (fset 'neovm--ct-apply
    (lambda (contract-name impl &rest args)
      (let ((contract (funcall 'neovm--ct-lookup contract-name)))
        (if (not contract)
            (list 'error (format "no contract: %s" contract-name))
          (let* ((pre-fn (nth 1 contract))
                 (post-fn (nth 2 contract))
                 (pre-err (apply pre-fn args)))
            (if pre-err
                (list 'violation 'pre contract-name pre-err)
              (let ((result (apply impl args)))
                (let ((post-err (funcall post-fn (cons result args))))
                  (if post-err
                      (list 'violation 'post contract-name post-err)
                    (list 'ok result))))))))))

  (unwind-protect
      (progn
        ;; Register contracts
        (funcall 'neovm--ct-register
                 'non-negative
                 (lambda (&rest args)
                   (let ((err nil))
                     (dolist (a args)
                       (when (and (not err) (or (not (numberp a)) (< a 0)))
                         (setq err (format "non-negative required: %S" a))))
                     err))
                 (lambda (ctx)
                   (if (and (numberp (car ctx)) (>= (car ctx) 0)) nil
                     "result must be non-negative")))
        (funcall 'neovm--ct-register
                 'bounded-100
                 (lambda (&rest args)
                   (let ((err nil))
                     (dolist (a args)
                       (when (and (not err)
                                  (or (not (numberp a))
                                      (> (abs a) 100)))
                         (setq err (format "value out of [-100,100]: %S" a))))
                     err))
                 (lambda (ctx)
                   (if (and (numberp (car ctx)) (<= (abs (car ctx)) 10000)) nil
                     "result exceeds bounds")))
        (funcall 'neovm--ct-register
                 'string-args
                 (lambda (&rest args)
                   (let ((err nil))
                     (dolist (a args)
                       (when (and (not err) (not (stringp a)))
                         (setq err (format "string required: %S" a))))
                     err))
                 (lambda (ctx)
                   (if (stringp (car ctx)) nil "result must be string")))
        ;; Use registered contracts
        (list
          ;; Non-negative contract on addition
          (funcall 'neovm--ct-apply 'non-negative #'+ 5 10)
          (funcall 'neovm--ct-apply 'non-negative #'+ -1 10)
          ;; Bounded-100 contract on multiplication
          (funcall 'neovm--ct-apply 'bounded-100 #'* 50 50)
          (funcall 'neovm--ct-apply 'bounded-100 #'* 200 1)
          ;; String contract on concat
          (funcall 'neovm--ct-apply 'string-args #'concat "hello" " " "world")
          (funcall 'neovm--ct-apply 'string-args #'concat "a" 42)
          ;; Unknown contract
          (funcall 'neovm--ct-apply 'nonexistent #'+ 1 2)
          ;; Verify registry has 3 entries
          (length neovm--ct-registry)))
    (fmakunbound 'neovm--ct-register)
    (fmakunbound 'neovm--ct-lookup)
    (fmakunbound 'neovm--ct-apply)
    (makunbound 'neovm--ct-registry)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ok 15) (violation pre non-negative \"non-negative required: -1\") (ok 2500) (violation pre bounded-100 \"value out of [-100,100]: 200\") (ok \"hello world\") (violation pre string-args \"string required: 42\") (error \"no contract: nonexistent\") 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-field record with per-field validation contracts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_contract_validated_record() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A record system where each field has a type/validation contract.
    // Setting a field checks the contract. Building a complete record
    // checks all required fields are present.
    let form = r#"(let ((make-schema nil)
                        (schema-validate-field nil)
                        (schema-build-record nil))
  ;; Schema: list of (field-name validator required)
  (setq make-schema
        (lambda (field-specs)
          field-specs))
  (setq schema-validate-field
        (lambda (schema field value)
          (let ((spec nil) (rest schema))
            (while (and rest (not spec))
              (when (eq (car (car rest)) field)
                (setq spec (car rest)))
              (setq rest (cdr rest)))
            (if (not spec)
                (list 'error (format "unknown field: %s" field))
              (let ((validator (nth 1 spec)))
                (if (funcall validator value)
                    (list 'ok value)
                  (list 'violation
                        (format "field %s: invalid value %S" field value))))))))
  (setq schema-build-record
        (lambda (schema field-values)
          (let ((errors nil)
                (record nil))
            ;; Validate each provided field
            (let ((rest field-values))
              (while rest
                (let* ((field (car rest))
                       (value (cadr rest))
                       (result (funcall schema-validate-field schema field value)))
                  (if (eq (car result) 'ok)
                      (setq record (cons (cons field value) record))
                    (setq errors (cons (cadr result) errors))))
                (setq rest (cddr rest))))
            ;; Check required fields
            (dolist (spec schema)
              (when (nth 2 spec)
                (unless (assq (car spec) record)
                  (setq errors
                        (cons (format "missing required field: %s" (car spec))
                              errors)))))
            (if errors
                (list 'invalid (nreverse errors))
              (list 'valid (nreverse record))))))
  ;; Define a person schema
  (let ((person-schema
         (funcall make-schema
                  (list
                   (list 'name (lambda (v) (and (stringp v) (> (length v) 0))) t)
                   (list 'age (lambda (v) (and (integerp v) (>= v 0) (<= v 150))) t)
                   (list 'email (lambda (v) (and (stringp v)
                                                  (string-match-p "@" v))) nil)
                   (list 'score (lambda (v) (and (numberp v)
                                                  (>= v 0) (<= v 100))) nil)))))
    (list
      ;; Valid complete record
      (funcall schema-build-record person-schema
               '(name "Alice" age 30 email "alice@example.com" score 95))
      ;; Valid with only required fields
      (funcall schema-build-record person-schema
               '(name "Bob" age 25))
      ;; Missing required field
      (funcall schema-build-record person-schema
               '(name "Charlie"))
      ;; Invalid field value
      (funcall schema-build-record person-schema
               '(name "" age 30))
      ;; Multiple errors
      (funcall schema-build-record person-schema
               '(name "" age -5 email "bademail" score 200))
      ;; Individual field validation
      (funcall schema-validate-field person-schema 'name "Alice")
      (funcall schema-validate-field person-schema 'name "")
      (funcall schema-validate-field person-schema 'age 25)
      (funcall schema-validate-field person-schema 'age 200)
      (funcall schema-validate-field person-schema 'unknown 42))))"#;
    let expect = expect_test::expect![[
        r#""OK ((valid ((name . \"Alice\") (age . 30) (email . \"alice@example.com\") (score . 95))) (valid ((name . \"Bob\") (age . 25))) (invalid (\"missing required field: age\")) (invalid (\"field name: invalid value \\\"\\\"\" \"missing required field: name\")) (invalid (\"field name: invalid value \\\"\\\"\" \"field age: invalid value -5\" \"field email: invalid value \\\"bademail\\\"\" \"field score: invalid value 200\" \"missing required field: name\" \"missing required field: age\")) (ok \"Alice\") (violation \"field name: invalid value \\\"\\\"\") (ok 25) (violation \"field age: invalid value 200\") (error \"unknown field: unknown\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Function pipeline with per-stage contracts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_contract_pipeline_stages() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A data processing pipeline where each stage has input/output contracts.
    // Data flows through stages; any contract violation halts the pipeline.
    let form = r#"(let ((make-stage nil)
                        (run-pipeline nil))
  (setq make-stage
        (lambda (name transform in-contract out-contract)
          (list name transform in-contract out-contract)))
  (setq run-pipeline
        (lambda (stages input)
          (let ((current input)
                (trace nil)
                (error nil))
            (dolist (stage stages)
              (unless error
                (let ((name (nth 0 stage))
                      (transform (nth 1 stage))
                      (in-check (nth 2 stage))
                      (out-check (nth 3 stage)))
                  ;; Check input contract
                  (let ((in-err (funcall in-check current)))
                    (if in-err
                        (setq error (list 'input-violation name in-err))
                      ;; Transform
                      (let ((output (funcall transform current)))
                        ;; Check output contract
                        (let ((out-err (funcall out-check output)))
                          (if out-err
                              (setq error (list 'output-violation name out-err))
                            (setq trace (cons (list name current output) trace))
                            (setq current output)))))))))
            (if error
                (list 'pipeline-error error (nreverse trace))
              (list 'pipeline-ok current (nreverse trace))))))
  ;; Define a number processing pipeline
  (let* ((parse-stage
          (funcall make-stage "parse"
                   (lambda (s) (string-to-number s))
                   (lambda (v) (if (stringp v) nil "input must be string"))
                   (lambda (v) (if (numberp v) nil "output must be number"))))
         (double-stage
          (funcall make-stage "double"
                   (lambda (n) (* n 2))
                   (lambda (v) (if (numberp v) nil "input must be number"))
                   (lambda (v) (if (and (numberp v) (<= v 1000)) nil
                                 (format "output %d exceeds 1000" v)))))
         (negate-stage
          (funcall make-stage "negate"
                   (lambda (n) (- n))
                   (lambda (v) (if (numberp v) nil "input must be number"))
                   (lambda (v) (if (numberp v) nil "output must be number"))))
         (pipeline (list parse-stage double-stage negate-stage)))
    (list
      ;; Successful pipeline: "42" -> 42 -> 84 -> -84
      (funcall run-pipeline pipeline "42")
      ;; Successful with small number
      (funcall run-pipeline pipeline "5")
      ;; Input contract violation at parse stage (non-string)
      (funcall run-pipeline pipeline 123)
      ;; Output contract violation at double stage (too large)
      (funcall run-pipeline pipeline "600")
      ;; Two-stage pipeline only
      (funcall run-pipeline (list parse-stage double-stage) "25"))))"#;
    let expect = expect_test::expect![[
        r#""OK ((pipeline-ok -84 ((\"parse\" \"42\" 42) (\"double\" 42 84) (\"negate\" 84 -84))) (pipeline-ok -10 ((\"parse\" \"5\" 5) (\"double\" 5 10) (\"negate\" 10 -10))) (pipeline-error (input-violation \"parse\" \"input must be string\") nil) (pipeline-error (output-violation \"double\" \"output 1200 exceeds 1000\") ((\"parse\" \"600\" 600))) (pipeline-ok 50 ((\"parse\" \"25\" 25) (\"double\" 25 50))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
