mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

fn assert_case(name: &str, form: &str) {
    if !oracle_enabled() {
        eprintln!(
            "skipping eval-after-load semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let gnu = run_oracle_eval(form)
        .unwrap_or_else(|err| panic!("GNU Emacs evaluation failed for {}: {err}", name));
    let neovm = run_neovm_eval(form)
        .unwrap_or_else(|err| panic!("NeoVM evaluation failed for {}: {err}", name));
    assert_eq!(
        neovm, gnu,
        "eval-after-load mismatch for {}:\nGNU: {}\nNeoVM: {}",
        name, gnu, neovm
    );
}

#[test]
fn compat_eval_after_load_runtime_lambda_immediate() {
    assert_case(
        "runtime_lambda_immediate",
        r#"(progn
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
    );
}

#[test]
fn compat_eval_after_load_immediate() {
    assert_case(
        "eval_after_load_immediate",
        r#"(progn
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
    );
}

#[test]
fn compat_eval_after_load_registry_shape() {
    assert_case(
        "eval_after_load_registry_shape",
        r#"(progn
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
    );
}

#[test]
fn compat_eval_after_load_deferred() {
    assert_case(
        "eval_after_load_deferred",
        r#"(progn
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
          (list before
                neovm--test-deferred-log
                (length neovm--test-deferred-log)
                (let ((snapshot neovm--test-deferred-log))
                  (provide 'neovm--test-deferred-feat)
                  (equal snapshot neovm--test-deferred-log)))))
    (setq features (delq 'neovm--test-deferred-feat features))
    (makunbound 'neovm--test-deferred-log)))"#,
    );
}
