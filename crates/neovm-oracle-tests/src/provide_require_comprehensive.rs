//! Oracle parity tests for the Emacs feature system.
//!
//! Covers: `provide`, `require`, `featurep`, the `features` list,
//! requiring already-provided features, `eval-after-load`,
//! `with-eval-after-load`, feature dependency chains, conditional
//! feature loading patterns, and interaction with `unload-feature`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Basic provide/featurep/features interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_basic_feature_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; provide adds to features, featurep checks membership
  (unwind-protect
      (progn
        (provide 'neovm--test-feat-alpha)
        (list
         ;; featurep returns t for provided feature
         (featurep 'neovm--test-feat-alpha)
         ;; featurep returns nil for unknown feature
         (featurep 'neovm--test-feat-nonexistent-xyz)
         ;; features list contains the symbol
         (not (null (memq 'neovm--test-feat-alpha features)))
         ;; provide is idempotent: providing again doesn't duplicate
         (let ((count-before (length (memq 'neovm--test-feat-alpha features))))
           (provide 'neovm--test-feat-alpha)
           (= count-before (length (memq 'neovm--test-feat-alpha features))))
         ;; provide returns the feature symbol
         (provide 'neovm--test-feat-beta)))
    (setq features (delq 'neovm--test-feat-alpha features))
    (setq features (delq 'neovm--test-feat-beta features))))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t neovm--test-feat-beta)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// require with already-provided feature (no-op)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_already_provided() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; When a feature is already provided, require is a no-op
  (unwind-protect
      (progn
        (provide 'neovm--test-feat-gamma)
        (list
         ;; require should not error when feature is already provided
         (require 'neovm--test-feat-gamma)
         ;; featurep still true after require
         (featurep 'neovm--test-feat-gamma)
         ;; require returns the feature symbol
         (eq (require 'neovm--test-feat-gamma) 'neovm--test-feat-gamma)))
    (setq features (delq 'neovm--test-feat-gamma features))))"#;
    let expect = expect_test::expect![[r#""OK (neovm--test-feat-gamma t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple features and ordering in features list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_multiple_features_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; provide multiple features, check they are all present and ordered
  (unwind-protect
      (let ((saved-features features))
        (provide 'neovm--test-feat-one)
        (provide 'neovm--test-feat-two)
        (provide 'neovm--test-feat-three)
        (list
         ;; all three are provided
         (featurep 'neovm--test-feat-one)
         (featurep 'neovm--test-feat-two)
         (featurep 'neovm--test-feat-three)
         ;; most recent provide is at front of features list
         ;; (provide pushes to front if not already present)
         (let ((pos-one (length (memq 'neovm--test-feat-one features)))
               (pos-three (length (memq 'neovm--test-feat-three features))))
           ;; three was provided last, so its memq tail is longer (closer to front)
           (> pos-three pos-one))
         ;; featurep on a non-symbol-like value
         (featurep 'neovm--test-feat-one)
         ;; Count how many neovm test features are in the list
         (let ((count 0))
           (dolist (f features)
             (when (memq f '(neovm--test-feat-one neovm--test-feat-two neovm--test-feat-three))
               (setq count (1+ count))))
           count)))
    (setq features (delq 'neovm--test-feat-one features))
    (setq features (delq 'neovm--test-feat-two features))
    (setq features (delq 'neovm--test-feat-three features))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// featurep with subfeature (version) argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_featurep_subfeature() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; featurep can take an optional subfeature argument
  ;; (featurep FEATURE SUBFEATURE) checks if SUBFEATURE is in the
  ;; subfeature list of FEATURE.
  ;; GNU provide takes a LIST of subfeatures as the second argument.
  (unwind-protect
      (progn
        (provide 'neovm--test-feat-versioned
                 '(neovm--test-sub-v1 neovm--test-sub-v2))
        (list
         ;; Feature itself is provided
         (featurep 'neovm--test-feat-versioned)
         ;; Subfeatures are provided
         (featurep 'neovm--test-feat-versioned 'neovm--test-sub-v1)
         (featurep 'neovm--test-feat-versioned 'neovm--test-sub-v2)
         ;; Non-existent subfeature
         (featurep 'neovm--test-feat-versioned 'neovm--test-sub-v99)
         ;; Subfeature of non-existent feature
         (featurep 'neovm--test-feat-nonexistent 'neovm--test-sub-v1)))
    (setq features (delq 'neovm--test-feat-versioned features))))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_provide_reprovide_subfeature_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fprovide only writes the `subfeatures' property when the
    // second argument is non-nil.  Re-providing an already-present feature with
    // nil subfeatures therefore preserves the previous subfeature list, while
    // a later non-nil list replaces it.
    let form = r#"(let ((features (delq 'neovm--test-feat-reprovide-sub features)))
  (unwind-protect
      (progn
        (put 'neovm--test-feat-reprovide-sub 'subfeatures nil)
        (provide 'neovm--test-feat-reprovide-sub
                 '(neovm--test-sub-a neovm--test-sub-b))
        (let ((after-first
               (list
                (featurep 'neovm--test-feat-reprovide-sub)
                (featurep 'neovm--test-feat-reprovide-sub
                          'neovm--test-sub-a)
                (get 'neovm--test-feat-reprovide-sub 'subfeatures))))
          (provide 'neovm--test-feat-reprovide-sub nil)
          (let ((after-nil
                 (list
                  (featurep 'neovm--test-feat-reprovide-sub
                            'neovm--test-sub-a)
                  (get 'neovm--test-feat-reprovide-sub 'subfeatures))))
            (provide 'neovm--test-feat-reprovide-sub
                     '(neovm--test-sub-c))
            (list
             after-first
             after-nil
             (featurep 'neovm--test-feat-reprovide-sub
                       'neovm--test-sub-a)
             (featurep 'neovm--test-feat-reprovide-sub
                       'neovm--test-sub-c)
             (get 'neovm--test-feat-reprovide-sub 'subfeatures)))))
    (setq features (delq 'neovm--test-feat-reprovide-sub features))
    (put 'neovm--test-feat-reprovide-sub 'subfeatures nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((t t (neovm--test-sub-a neovm--test-sub-b)) (t (neovm--test-sub-a neovm--test-sub-b)) nil t (neovm--test-sub-c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_featurep_subfeatures_use_equal_member() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Ffeaturep checks subfeatures with `member', not `memq'.
    // The subfeature argument can therefore match non-symbol objects by
    // structural `equal' even though `provide' documents symbol subfeatures.
    let form = r#"(let ((features (delq 'neovm--test-feat-equal-sub features)))
  (unwind-protect
      (progn
        (put 'neovm--test-feat-equal-sub 'subfeatures nil)
        (provide 'neovm--test-feat-equal-sub
                 (list (copy-sequence "sub")
                       (list 1 2)
                       [neovm--test-a neovm--test-b]))
        (list
         (featurep 'neovm--test-feat-equal-sub
                   (copy-sequence "sub"))
         (featurep 'neovm--test-feat-equal-sub
                   (list 1 2))
         (featurep 'neovm--test-feat-equal-sub
                   [neovm--test-a neovm--test-b])
         (featurep 'neovm--test-feat-equal-sub
                   (copy-sequence "missing"))))
    (setq features (delq 'neovm--test-feat-equal-sub features))
    (put 'neovm--test-feat-equal-sub 'subfeatures nil)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval-after-load with already-loaded feature
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_eval_after_load_immediate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; eval-after-load runs the form immediately if the file/feature
  ;; is already loaded. We simulate by providing first.
  (defvar neovm--test-eal-log nil)

  (unwind-protect
      (progn
        (provide 'neovm--test-feat-eal)
        ;; This should fire immediately since feature is already provided
        (eval-after-load 'neovm--test-feat-eal
          '(setq neovm--test-eal-log (cons 'fired-1 neovm--test-eal-log)))
        ;; Multiple eval-after-load on same feature
        (eval-after-load 'neovm--test-feat-eal
          '(setq neovm--test-eal-log (cons 'fired-2 neovm--test-eal-log)))
        (list
         ;; Both should have fired immediately (in order)
         neovm--test-eal-log
         ;; Log should contain both entries
         (length neovm--test-eal-log)))
    (setq features (delq 'neovm--test-feat-eal features))
    (makunbound 'neovm--test-eal-log)))"#;
    let expect = expect_test::expect![[r#""OK ((fired-2 fired-1) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-eval-after-load macro (lexical body)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_with_eval_after_load() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; with-eval-after-load is the macro form that wraps body in a lambda
  (defvar neovm--test-weal-result nil)

  (unwind-protect
      (progn
        (provide 'neovm--test-feat-weal)
        ;; Should fire immediately since feature already provided
        (with-eval-after-load 'neovm--test-feat-weal
          (setq neovm--test-weal-result 'executed))
        (list
         neovm--test-weal-result
         ;; with-eval-after-load with multiple body forms
         (progn
           (with-eval-after-load 'neovm--test-feat-weal
             (setq neovm--test-weal-result 'first)
             (setq neovm--test-weal-result (cons neovm--test-weal-result 'second)))
           neovm--test-weal-result)))
    (setq features (delq 'neovm--test-feat-weal features))
    (makunbound 'neovm--test-weal-result)))"#;
    let expect = expect_test::expect![[r#""OK (executed (first . second))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Feature dependency chain simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_dependency_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Simulate a dependency chain: C depends on B, B depends on A.
  ;; We provide them in order and verify correct resolution.
  (defvar neovm--test-dep-load-order nil)

  (unwind-protect
      (progn
        ;; Simulate loading module A
        (setq neovm--test-dep-load-order
              (cons 'loading-a neovm--test-dep-load-order))
        (provide 'neovm--test-dep-a)

        ;; Simulate loading module B (depends on A)
        (require 'neovm--test-dep-a)  ;; already provided, no-op
        (setq neovm--test-dep-load-order
              (cons 'loading-b neovm--test-dep-load-order))
        (provide 'neovm--test-dep-b)

        ;; Simulate loading module C (depends on both A and B)
        (require 'neovm--test-dep-a)
        (require 'neovm--test-dep-b)
        (setq neovm--test-dep-load-order
              (cons 'loading-c neovm--test-dep-load-order))
        (provide 'neovm--test-dep-c)

        (list
         ;; Load order (reversed since we push)
         (nreverse neovm--test-dep-load-order)
         ;; All features are provided
         (featurep 'neovm--test-dep-a)
         (featurep 'neovm--test-dep-b)
         (featurep 'neovm--test-dep-c)
         ;; Requiring already-provided features returns the symbol
         (eq (require 'neovm--test-dep-c) 'neovm--test-dep-c)))
    (setq features (delq 'neovm--test-dep-a features))
    (setq features (delq 'neovm--test-dep-b features))
    (setq features (delq 'neovm--test-dep-c features))
    (makunbound 'neovm--test-dep-load-order)))"#;
    let expect = expect_test::expect![[r#""OK ((loading-a loading-b loading-c) t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Conditional feature loading patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_conditional_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Common Elisp patterns: (when (featurep 'x) ...), (require 'x nil t)
  (unwind-protect
      (progn
        (provide 'neovm--test-cond-present)
        (list
         ;; Pattern: (when (featurep ...) ...) for conditional execution
         (when (featurep 'neovm--test-cond-present)
           'feature-available)
         ;; Pattern: conditional on absent feature
         (if (featurep 'neovm--test-cond-absent)
             'should-not-reach
           'correctly-absent)
         ;; Pattern: require with NOERROR=t for optional dependency
         ;; Returns nil when feature not found (instead of error)
         (require 'neovm--test-cond-nonexistent nil t)
         ;; Pattern: require with NOERROR=t for present feature works
         (require 'neovm--test-cond-present nil t)
         ;; Pattern: and-chain with featurep guard
         (and (featurep 'neovm--test-cond-present)
              (not (featurep 'neovm--test-cond-absent))
              'both-conditions-met)
         ;; Pattern: or-chain for fallback features
         (or (featurep 'neovm--test-cond-absent)
             (featurep 'neovm--test-cond-also-absent)
             (featurep 'neovm--test-cond-present)
             'fallback)
         ;; Pattern: cond-based feature dispatch
         (cond
          ((featurep 'neovm--test-cond-absent) 'path-a)
          ((featurep 'neovm--test-cond-present) 'path-b)
          (t 'path-default))))
    (setq features (delq 'neovm--test-cond-present features))))"#;
    let expect = expect_test::expect![[
        r#""OK (feature-available correctly-absent nil neovm--test-cond-present both-conditions-met t path-b)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// eval-after-load with deferred evaluation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_eval_after_load_deferred() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; eval-after-load registered BEFORE the feature is provided
  ;; should fire when provide is eventually called
  (defvar neovm--test-deferred-log nil)

  (unwind-protect
      (progn
        ;; Register eval-after-load before providing
        (eval-after-load 'neovm--test-deferred-feat
          '(setq neovm--test-deferred-log
                 (cons 'deferred-1 neovm--test-deferred-log)))
        (eval-after-load 'neovm--test-deferred-feat
          '(setq neovm--test-deferred-log
                 (cons 'deferred-2 neovm--test-deferred-log)))
        ;; Not yet fired
        (let ((before neovm--test-deferred-log))
          ;; Now provide the feature — triggers deferred forms
          (provide 'neovm--test-deferred-feat)
          (list
           ;; Before provide: log was empty
           before
           ;; After provide: both forms fired
           neovm--test-deferred-log
           (length neovm--test-deferred-log)
           ;; Providing again should NOT re-fire
           (let ((snapshot neovm--test-deferred-log))
             (provide 'neovm--test-deferred-feat)
             (equal snapshot neovm--test-deferred-log)))))
    (setq features (delq 'neovm--test-deferred-feat features))
    (makunbound 'neovm--test-deferred-log)))"#;
    let expect = expect_test::expect![[r#""OK (nil (deferred-2 deferred-1) 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Feature manipulation: removing from features list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_provide_require_features_list_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Directly manipulating the features list (delq) to "unload"
  (unwind-protect
      (progn
        (provide 'neovm--test-removable)
        (let ((step1 (featurep 'neovm--test-removable)))
          ;; Remove from features list
          (setq features (delq 'neovm--test-removable features))
          (let ((step2 (featurep 'neovm--test-removable)))
            ;; Re-provide
            (provide 'neovm--test-removable)
            (let ((step3 (featurep 'neovm--test-removable)))
              (list
               step1   ;; t: was provided
               step2   ;; nil: was removed
               step3   ;; t: re-provided
               ;; Require after re-provide works
               (eq (require 'neovm--test-removable) 'neovm--test-removable))))))
    (setq features (delq 'neovm--test-removable features))))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
