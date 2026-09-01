mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_bootstrap_runtime_core_surface_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping bootstrap runtime core surface audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(list
  (list 'frame
        (featurep 'frame)
        (fboundp 'frame-set-background-mode)
        (autoloadp (symbol-function 'frame-set-background-mode)))
  (list 'simple
        (featurep 'simple)
        (fboundp 'shell-command)
        (autoloadp (symbol-function 'shell-command)))
  (list 'core
        (fboundp 'cons)
        (subrp (symbol-function 'cons))
        (fboundp 'intern)
        (subrp (symbol-function 'intern))
        (fboundp 'format)
        (subrp (symbol-function 'format)))
  (list 'faces
        (featurep 'faces)
        (fboundp 'face-spec-recalc)
        (autoloadp (symbol-function 'face-spec-recalc))
        (fboundp 'face-list)
        (autoloadp (symbol-function 'face-list))
        (fboundp 'face-user-default-spec)
        (autoloadp (symbol-function 'face-user-default-spec))))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "bootstrap runtime core surface mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
