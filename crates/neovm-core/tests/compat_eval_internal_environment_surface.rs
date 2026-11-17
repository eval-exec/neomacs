use crate::common;

use common::{run_neovm_eval, run_oracle_eval};

#[test]
fn compat_eval_internal_environment_surface_matches_gnu_emacs() {
    // Reader-safe: every reference to the spelling is `(intern "...")`, never
    // a quoted symbol. A quoted symbol is interned by the READER before the
    // form runs (in GNU and in this runtime's file reader), which is why the
    // old shape saw a symbol at startup. GNU's startup obarray does not carry
    // it: `syms_of_eval` creates `Qinternal_interpreter_environment` and
    // immediately `Funintern`s it (`src/eval.c:4597`), so a fresh
    // `(intern-soft "internal-interpreter-environment")` is nil; the rest
    // then describes the symbol that explicit interning creates.
    let form = r#"(let ((sym (intern-soft "internal-interpreter-environment")))
  (list
   sym
   (progn (intern "internal-interpreter-environment")
          (intern-soft "internal-interpreter-environment"))
   (boundp (intern "internal-interpreter-environment"))
   (special-variable-p (intern "internal-interpreter-environment"))
   (condition-case err
       (symbol-value (intern "internal-interpreter-environment"))
     (error (list 'error (car err))))))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "internal interpreter environment surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
