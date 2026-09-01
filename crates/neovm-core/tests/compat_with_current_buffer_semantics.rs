mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

#[test]
fn compat_with_current_buffer_macro_shape_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping with-current-buffer macro audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((fn (symbol-function 'with-current-buffer)))
  (list
   (fboundp 'with-current-buffer)
   (macrop 'with-current-buffer)
   (special-form-p 'with-current-buffer)
   (subrp fn)
   (macroexpand-1
    '(with-current-buffer "wcb-shape"
       (list 1 2)
       'done))))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "with-current-buffer macro shape mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_with_current_buffer_runtime_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping with-current-buffer runtime audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let* ((a (get-buffer-create "wcb-a"))
         (b (get-buffer-create "wcb-b")))
  (save-current-buffer
    (set-buffer a)
    (erase-buffer)
    (insert "A"))
  (save-current-buffer
    (set-buffer b)
    (erase-buffer)
    (insert "B"))
  (set-buffer a)
  (list
   (buffer-name (current-buffer))
   (with-current-buffer b
     (list (buffer-name (current-buffer))
           (buffer-string)
           (eq (current-buffer) b)))
   (buffer-name (current-buffer))
   (buffer-string)))"#;

    let gnu = run_oracle_eval(form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "with-current-buffer runtime semantics mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
