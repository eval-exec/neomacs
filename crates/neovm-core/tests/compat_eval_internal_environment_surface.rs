mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_eval_internal_environment_surface_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping internal interpreter environment audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((sym (intern-soft "internal-interpreter-environment")))
  (list
   sym
   (eq sym (intern "internal-interpreter-environment"))
   (boundp 'internal-interpreter-environment)
   (special-variable-p 'internal-interpreter-environment)
   (condition-case err
       (symbol-value 'internal-interpreter-environment)
     (error (list 'error (car err))))))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "internal interpreter environment surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
