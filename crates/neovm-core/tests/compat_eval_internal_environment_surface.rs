use crate::common;

use common::{run_neovm_eval, run_oracle_eval};

#[test]
fn compat_eval_internal_environment_surface_matches_gnu_emacs() {
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
