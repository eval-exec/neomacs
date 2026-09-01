mod common;

use std::fs;

use common::{elisp_string, oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_default_toplevel_value_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping default lexical-binding audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((orig (default-toplevel-value 'debug-on-error)))
  (unwind-protect
      (progn
        (set-default-toplevel-value 'debug-on-error 'vm-top-default)
        (list
         (let ((debug-on-error 'vm-dynamic))
           (list debug-on-error
                 (default-toplevel-value 'debug-on-error)
                 (progn
                   (set-default-toplevel-value 'debug-on-error 'vm-updated)
                   (list debug-on-error
                         (default-toplevel-value 'debug-on-error)))))
         (default-toplevel-value 'debug-on-error)))
    (set-default-toplevel-value 'debug-on-error orig)))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "default-toplevel-value mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_load_default_lexical_binding_hook_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping default lexical-binding load audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let fixture_dir = tempfile::tempdir().expect("default lexical binding tempdir");
    let source_path = fixture_dir.path().join("default-lexical-load.el");
    fs::write(
        &source_path,
        "(setq vm-default-lexical-load-probe lexical-binding)\n\
         (setq vm-default-lexical-load-fn (let ((x 41)) (lambda () (+ x 1))))\n",
    )
    .expect("write default lexical load fixture");

    let form = format!(
        r#"(progn
  (setq vm-default-lexical-load-from nil
        vm-default-lexical-load-probe nil
        vm-default-lexical-load-fn nil)
  (let ((lexical-binding nil)
        (internal--get-default-lexical-binding-function
         (lambda (from)
           (setq vm-default-lexical-load-from from)
           t)))
    (load {path} nil nil t))
  (list vm-default-lexical-load-from
        vm-default-lexical-load-probe
        (condition-case err
            (funcall vm-default-lexical-load-fn)
          (error (car err)))))"#,
        path = elisp_string(&source_path)
    );

    let gnu = run_oracle_eval(&form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(&form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "default lexical-binding load mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_eval_buffer_default_lexical_binding_hook_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping default lexical-binding eval-buffer audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (setq vm-default-lexical-buffer-from nil
        vm-default-lexical-buffer-probe nil
        vm-default-lexical-buffer-fn nil)
  (let ((lexical-binding nil)
        (internal--get-default-lexical-binding-function
         (lambda (from)
           (setq vm-default-lexical-buffer-from from)
           t)))
    (with-temp-buffer
      (rename-buffer " *vm-default-lexical-buffer*" t)
      (insert "(setq vm-default-lexical-buffer-probe lexical-binding)\n"
              "(setq vm-default-lexical-buffer-fn (let ((x 41)) (lambda () (+ x 1))))\n")
      (eval-buffer)))
  (list vm-default-lexical-buffer-from
        vm-default-lexical-buffer-probe
        (condition-case err
            (funcall vm-default-lexical-buffer-fn)
          (error (car err)))))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "default lexical-binding eval-buffer mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
