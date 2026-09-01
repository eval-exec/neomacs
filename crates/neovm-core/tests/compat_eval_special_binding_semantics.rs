mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_eval_special_binding_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping eval special binding audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(list
  (list 'quit-flag
        (boundp 'quit-flag)
        (special-variable-p 'quit-flag))
  (let ((symbols '(inhibit-quit
                        inhibit-debugger
                        debug-on-error
                        debug-ignored-errors
                        debugger
                        signal-hook-function
                        debug-on-signal
                        internal-make-interpreted-closure-function)))
    (mapcar
     (lambda (sym)
       (let ((marker (list 'marker sym)))
         (list sym
               (boundp sym)
               (special-variable-p sym)
               (funcall
                (eval
                 `(lambda ()
                    (let ((,sym ',marker))
                      (equal (symbol-value ',sym) ',marker)))
                 t)))))
     symbols)))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "eval special binding mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
