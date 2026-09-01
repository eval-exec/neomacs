mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_interpreted_closure_runtime_surface_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping interpreted closure runtime audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(progn
  (setq neovm--hook-count 0)
  (fset 'neovm--counting-make-interpreted-closure
        (lambda (args body env docstring iform)
          (setq neovm--hook-count (1+ neovm--hook-count))
          (make-interpreted-closure args body env docstring iform)))
  (unwind-protect
      (list
       (special-variable-p 'internal-make-interpreted-closure-function)
       internal-make-interpreted-closure-function
       (funcall (let ((x 1)) (lambda () x)))
       (let ((internal-make-interpreted-closure-function
              'neovm--counting-make-interpreted-closure))
         (funcall (let ((x 2)) (lambda () x))))
       neovm--hook-count)
    (fmakunbound 'neovm--counting-make-interpreted-closure)
    (makunbound 'neovm--hook-count)))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "interpreted closure runtime surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
