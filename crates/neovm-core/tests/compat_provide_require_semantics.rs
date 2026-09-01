mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct FeatureCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_provide_require_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping provide/require semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        FeatureCase {
            name: "basic_feature_lifecycle",
            form: r#"(progn
  (unwind-protect
      (progn
        (provide 'neovm--test-feat-alpha)
        (list
         (featurep 'neovm--test-feat-alpha)
         (featurep 'neovm--test-feat-nonexistent-xyz)
         (not (null (memq 'neovm--test-feat-alpha features)))
         (let ((count-before (length (memq 'neovm--test-feat-alpha features))))
           (provide 'neovm--test-feat-alpha)
           (= count-before (length (memq 'neovm--test-feat-alpha features))))
         (provide 'neovm--test-feat-beta)))
    (setq features (delq 'neovm--test-feat-alpha features))
    (setq features (delq 'neovm--test-feat-beta features))))"#,
        },
        FeatureCase {
            name: "featurep_subfeature",
            form: r#"(progn
  (unwind-protect
      (progn
        (provide 'neovm--test-feat-versioned
                 '(neovm--test-sub-v1 neovm--test-sub-v2))
        (list
         (featurep 'neovm--test-feat-versioned)
         (featurep 'neovm--test-feat-versioned 'neovm--test-sub-v1)
         (featurep 'neovm--test-feat-versioned 'neovm--test-sub-v2)
         (featurep 'neovm--test-feat-versioned 'neovm--test-sub-v99)
         (featurep 'neovm--test-feat-nonexistent 'neovm--test-sub-v1)))
    (setq features (delq 'neovm--test-feat-versioned features))))"#,
        },
        FeatureCase {
            name: "conditional_patterns",
            form: r#"(progn
  (unwind-protect
      (progn
        (provide 'neovm--test-cond-present)
        (list
         (when (featurep 'neovm--test-cond-present)
           'feature-available)
         (if (featurep 'neovm--test-cond-absent)
             'should-not-reach
           'correctly-absent)
         (require 'neovm--test-cond-nonexistent nil t)
         (require 'neovm--test-cond-present nil t)
         (and (featurep 'neovm--test-cond-present)
              (not (featurep 'neovm--test-cond-absent))
              'both-conditions-met)
         (or (featurep 'neovm--test-cond-absent)
             (featurep 'neovm--test-cond-also-absent)
             (featurep 'neovm--test-cond-present)
             'fallback)
         (cond
          ((featurep 'neovm--test-cond-absent) 'path-a)
          ((featurep 'neovm--test-cond-present) 'path-b)
         (t 'path-default))))
    (setq features (delq 'neovm--test-cond-present features))))"#,
        },
        FeatureCase {
            name: "eval_runtime_lambda_immediate",
            form: r#"(progn
  (defvar neovm--test-direct-eval-log nil)
  (unwind-protect
      (progn
        (let ((form1 '(setq neovm--test-direct-eval-log
                            (cons 'fired-1 neovm--test-direct-eval-log)))
              (form2 '(setq neovm--test-direct-eval-log
                            (cons 'fired-2 neovm--test-direct-eval-log))))
          (funcall (eval `(lambda () ,form1) lexical-binding))
          (funcall (eval `(lambda () ,form2) lexical-binding))
          (list neovm--test-direct-eval-log
                (length neovm--test-direct-eval-log))))
    (makunbound 'neovm--test-direct-eval-log)))"#,
        },
        FeatureCase {
            name: "eval_after_load_immediate",
            form: r#"(progn
  (defvar neovm--test-eal-log nil)
  (unwind-protect
      (progn
        (provide 'neovm--test-feat-eal)
        (eval-after-load 'neovm--test-feat-eal
          '(setq neovm--test-eal-log (cons 'fired-1 neovm--test-eal-log)))
        (eval-after-load 'neovm--test-feat-eal
          '(setq neovm--test-eal-log (cons 'fired-2 neovm--test-eal-log)))
        (list neovm--test-eal-log
              (length neovm--test-eal-log)))
    (setq features (delq 'neovm--test-feat-eal features))
    (makunbound 'neovm--test-eal-log)))"#,
        },
        FeatureCase {
            name: "with_eval_after_load_immediate",
            form: r#"(progn
  (defvar neovm--test-weal-result nil)
  (unwind-protect
      (progn
        (provide 'neovm--test-feat-weal)
        (with-eval-after-load 'neovm--test-feat-weal
          (setq neovm--test-weal-result 'executed))
        (list
         neovm--test-weal-result
         (progn
           (with-eval-after-load 'neovm--test-feat-weal
             (setq neovm--test-weal-result 'first)
             (setq neovm--test-weal-result (cons neovm--test-weal-result 'second)))
           neovm--test-weal-result)))
    (setq features (delq 'neovm--test-feat-weal features))
    (makunbound 'neovm--test-weal-result)))"#,
        },
        FeatureCase {
            name: "eval_after_load_registry_shape",
            form: r#"(progn
  (let ((after-load-alist nil))
    (eval-after-load 'neovm--test-registry-feat
      '(setq neovm--test-registry-log
             (cons 'registry-1 neovm--test-registry-log)))
    (eval-after-load 'neovm--test-registry-feat
      '(setq neovm--test-registry-log
             (cons 'registry-2 neovm--test-registry-log)))
    (let* ((entry (assq 'neovm--test-registry-feat after-load-alist))
           (callbacks (cdr entry))
           (first (car callbacks))
           (second (car (cdr callbacks))))
      (list (length callbacks)
            (equal first second)
            (eq first second)))))"#,
        },
        FeatureCase {
            name: "eval_after_load_deferred",
            form: r#"(progn
  (defvar neovm--test-deferred-log nil)
  (unwind-protect
      (progn
        (eval-after-load 'neovm--test-deferred-feat
          '(setq neovm--test-deferred-log
                 (cons 'deferred-1 neovm--test-deferred-log)))
        (eval-after-load 'neovm--test-deferred-feat
          '(setq neovm--test-deferred-log
                 (cons 'deferred-2 neovm--test-deferred-log)))
        (let ((before neovm--test-deferred-log))
          (provide 'neovm--test-deferred-feat)
          (list
           before
           neovm--test-deferred-log
           (length neovm--test-deferred-log)
           (let ((snapshot neovm--test-deferred-log))
             (provide 'neovm--test-deferred-feat)
             (equal snapshot neovm--test-deferred-log)))))
    (setq features (delq 'neovm--test-deferred-feat features))
    (makunbound 'neovm--test-deferred-log)))"#,
        },
        FeatureCase {
            name: "features_list_manipulation",
            form: r#"(progn
  (unwind-protect
      (progn
        (provide 'neovm--test-removable)
        (let ((step1 (featurep 'neovm--test-removable)))
          (setq features (delq 'neovm--test-removable features))
          (let ((step2 (featurep 'neovm--test-removable)))
            (provide 'neovm--test-removable)
            (let ((step3 (featurep 'neovm--test-removable)))
              (list
               step1
               step2
               step3
               (eq (require 'neovm--test-removable)
                   'neovm--test-removable))))))
    (setq features (delq 'neovm--test-removable features))))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form)
            .unwrap_or_else(|err| panic!("GNU Emacs evaluation failed for {}: {err}", case.name));
        let neovm = run_neovm_eval(case.form)
            .unwrap_or_else(|err| panic!("NeoVM evaluation failed for {}: {err}", case.name));
        assert_eq!(
            neovm, gnu,
            "provide/require mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
