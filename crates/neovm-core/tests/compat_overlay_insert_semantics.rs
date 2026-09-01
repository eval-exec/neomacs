mod common;

use common::{oracle_enabled, run_oracle_eval};
use neovm_core::emacs_core::{Context, format_eval_result_with_eval};

fn run_neovm_eval_minimal(form: &str) -> Result<String, String> {
    let mut eval = Context::new();
    let result = eval.eval_str(form);
    Ok(format_eval_result_with_eval(&eval, &result))
}

#[test]
fn compat_overlay_insert_before_markers_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping overlay insert semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((buf (get-buffer-create " *compat-overlay-before-markers*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (insert "abcd")
        (let ((m (copy-marker 2))
              (left (make-overlay 1 2))
              (right (make-overlay 2 4))
              (empty (make-overlay 2 2 nil t)))
          (overlay-put left 'face 'italic)
          (overlay-put right 'face 'bold)
          (goto-char 2)
          (insert-before-markers-and-inherit "X")
          (list
           (buffer-string)
           (marker-position m)
           (overlay-start left)
           (overlay-end left)
           (overlay-start right)
           (overlay-end right)
           (get-char-property 2 'face)
           (get-char-property 3 'face)
           (overlay-start empty)
           (overlay-end empty)
           (length (overlays-at 2 t))
           (length (overlays-at 3 t)))))
    (kill-buffer buf)))"#;

    let gnu =
        run_oracle_eval(form).unwrap_or_else(|err| panic!("GNU Emacs evaluation failed: {err}"));
    let neovm =
        run_neovm_eval_minimal(form).unwrap_or_else(|err| panic!("NeoVM evaluation failed: {err}"));
    assert_eq!(
        neovm, gnu,
        "overlay insert-before-markers semantics mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_overlay_print_and_deleted_identity_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping overlay print semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((buf (get-buffer-create " *compat-overlay-deleted*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (erase-buffer)
        (insert "abcd")
        (let ((ov (make-overlay 1 2)))
          (overlay-put ov 'face 'bold)
          (let ((live (prin1-to-string ov)))
            (delete-overlay ov)
            (overlay-put ov 'face 'italic)
            (list
             (overlayp ov)
             (eq ov ov)
             live
             (prin1-to-string ov)
             (overlay-buffer ov)
             (overlay-start ov)
             (overlay-end ov)
             (overlay-get ov 'face)
             (overlay-properties ov)))))
    (kill-buffer buf)))"#;

    let gnu =
        run_oracle_eval(form).unwrap_or_else(|err| panic!("GNU Emacs evaluation failed: {err}"));
    let neovm =
        run_neovm_eval_minimal(form).unwrap_or_else(|err| panic!("NeoVM evaluation failed: {err}"));
    assert_eq!(
        neovm, gnu,
        "overlay print/deletion semantics mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}

#[test]
fn compat_empty_front_advance_overlay_insert_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping empty overlay insert audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let form = r#"(let ((buf (get-buffer-create " *compat-overlay-empty-front*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (insert "abcd")
        (let ((ov (make-overlay 2 2 nil t)))
          (goto-char 2)
          (insert "X")
          (list
           (buffer-string)
           (overlay-start ov)
           (overlay-end ov)
           (length (overlays-at 2 t))
           (length (overlays-at 3 t)))))
    (kill-buffer buf)))"#;

    let gnu =
        run_oracle_eval(form).unwrap_or_else(|err| panic!("GNU Emacs evaluation failed: {err}"));
    let neovm =
        run_neovm_eval_minimal(form).unwrap_or_else(|err| panic!("NeoVM evaluation failed: {err}"));
    assert_eq!(
        neovm, gnu,
        "empty front-advance overlay insert semantics mismatch:\nGNU: {}\nNeoVM: {}",
        gnu, neovm
    );
}
