use crate::common;

use common::{run_neovm_eval, run_oracle_eval};

#[test]
fn compat_interpreted_closure_runtime_surface_matches_gnu_emacs() {
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
