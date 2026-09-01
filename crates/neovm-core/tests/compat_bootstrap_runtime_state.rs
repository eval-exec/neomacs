mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_bootstrap_runtime_state_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping bootstrap runtime state audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((features '(cl-lib cl-macs cl-seq cl-extra gv icons pcase))
      (vars '(gensym-counter
              macroexp--pending-eager-loads
              pcase--memoize
              pcase--dontwarn-upats
              pcase--find-macro-def-regexp
              icon-preference
              icon
              icon-button)))
  (list
   (mapcar (lambda (feature)
             (list feature (featurep feature)))
           features)
   (mapcar (lambda (sym)
             (list sym
                   (boundp sym)
                   (and (boundp sym)
                        (if (memq sym '(gensym-counter macroexp--pending-eager-loads))
                            (symbol-value sym)
                          t))))
           vars)))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "bootstrap runtime state mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
