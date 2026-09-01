mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct BufferLocalsCase {
    name: &'static str,
    form: &'static str,
}

fn run_case(case: BufferLocalsCase) {
    if !oracle_enabled() {
        eprintln!(
            "skipping buffer locals audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
    let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
    assert_eq!(
        neovm, gnu,
        "buffer locals semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
        case.name, gnu, neovm
    );
}

#[test]
fn compat_buffer_local_value_and_void_binding_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "buffer_local_value_and_void_binding",
        form: r#"(let ((buf (get-buffer-create " *compat-buffer-local-value*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (make-local-variable 'compat-void-local)
        (make-local-variable 'compat-bound-local)
        (setq compat-bound-local 42)
        (set-default 'compat-default-local 9)
        (list
         (condition-case err
         (buffer-local-value 'compat-void-local buf)
           (error (car err)))
         (buffer-local-value 'compat-bound-local buf)
         (buffer-local-value 'compat-default-local buf)))
    (kill-buffer buf)))"#,
    });
}

#[test]
fn compat_buffer_local_variables_shape_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "buffer_local_variables_shape",
        form: r#"(let ((buf (get-buffer-create " *compat-buffer-local-vars*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (make-local-variable 'compat-void-local)
        (make-local-variable 'compat-bound-local)
        (setq compat-bound-local 42)
        (let ((locals (buffer-local-variables buf)))
          (list
           (assq 'compat-bound-local locals)
           (memq 'compat-void-local locals)
           (assq 'major-mode locals)
           (assq 'buffer-read-only locals)
           (assq 'buffer-undo-list locals))))
    (kill-buffer buf)))"#,
    });
}

#[test]
fn compat_kill_all_local_variables_resets_current_buffer_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "kill_all_local_variables_resets_current_buffer",
        form: r#"(let ((buf (get-buffer-create " *compat-kill-all-locals*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (make-local-variable 'compat-bound-local)
        (setq compat-bound-local 42)
        (setq default-directory "/tmp/compat-kill-all-locals/")
        (setq buffer-read-only t)
        (setq-local truncate-lines t)
        (setq mode-name "Manual")
        (setq major-mode 'compat-major)
        (kill-all-local-variables)
        (list
         (condition-case err compat-bound-local (error (car err)))
         major-mode
         mode-name
         default-directory
         buffer-read-only
         truncate-lines
         (local-variable-p 'compat-bound-local buf)
         (local-variable-p 'default-directory buf)
         (local-variable-p 'buffer-read-only buf)
         (local-variable-p 'truncate-lines buf)))
    (kill-buffer buf)))"#,
    });
}

#[test]
fn compat_dynamic_lambda_parameter_preserves_forwarded_buffer_slot_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "dynamic_lambda_parameter_preserves_forwarded_buffer_slot",
        form: r#"(with-temp-buffer
  (setq mode-name "Outer")
  (let ((during (funcall '(lambda (mode-name) mode-name) "Inner")))
    (list during
          (condition-case err mode-name (error (car err)))
          (boundp 'mode-name)
          (local-variable-p 'mode-name))))"#,
    });
}

#[test]
fn compat_dynamic_bytecode_parameter_preserves_forwarded_buffer_slot_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "dynamic_bytecode_parameter_preserves_forwarded_buffer_slot",
        form: r#"(with-temp-buffer
  (setq mode-name "Outer")
  (let* ((fn (byte-compile '(lambda (mode-name) mode-name)))
         (during (funcall fn "Inner")))
    (list during
          (condition-case err mode-name (error (car err)))
          (boundp 'mode-name)
          (local-variable-p 'mode-name))))"#,
    });
}

#[test]
fn compat_kill_all_local_variables_preserves_partial_permanent_local_hooks_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "kill_all_local_variables_preserves_partial_permanent_local_hooks",
        form: r#"(let ((buf (get-buffer-create " *compat-kill-local-hook*")))
  (unwind-protect
      (with-current-buffer buf
        (fset 'compat--keep-local-hook (lambda (&rest _) nil))
        (fset 'compat--drop-local-hook (lambda (&rest _) nil))
        (put 'compat--keep-local-hook 'permanent-local-hook t)
        (put 'compat--mixed-hook 'permanent-local 'permanent-local-hook)
        (setq-local compat--mixed-hook '(compat--drop-local-hook compat--keep-local-hook t))
        (kill-all-local-variables)
        (list
         (condition-case err compat--mixed-hook (error (car err)))
         (local-variable-p 'compat--mixed-hook buf)
         (get 'compat--mixed-hook 'permanent-local)))
    (kill-buffer buf)))"#,
    });
}

#[test]
fn compat_buffer_slot_locality_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "buffer_slot_locality",
        form: r#"(let ((parent (get-buffer-create " *compat-buffer-slot-parent*"))
      (child nil))
  (unwind-protect
      (progn
        (with-current-buffer parent
          (setq default-directory "/tmp/compat-buffer-slot-parent/")
          (setq child (get-buffer-create " *compat-buffer-slot-child*")))
        (with-current-buffer child
          (list
           (local-variable-p 'default-directory child)
           (assq 'default-directory (buffer-local-variables child))
           default-directory
           (local-variable-p 'left-margin-width child)
           (assq 'left-margin-width (buffer-local-variables child))
           (progn
             (setq-local left-margin-width 7)
             (list
              (local-variable-p 'left-margin-width child)
              (assq 'left-margin-width (buffer-local-variables child)))))))
    (when child
      (kill-buffer child))
    (kill-buffer parent)))"#,
    });
}

#[test]
fn compat_make_indirect_buffer_clone_metadata_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "make_indirect_buffer_clone_metadata",
        form: r#"(let ((base (get-buffer-create " *compat-indirect-base*")))
  (unwind-protect
      (progn
        (set-buffer base)
        (insert "hello")
        (setq mode-name "Base")
        (setq-local compat-local 7)
        (let ((indirect (make-indirect-buffer base " *compat-indirect-copy*" t)))
          (unwind-protect
              (progn
                (list
                (buffer-base-buffer indirect)
                 (buffer-local-value 'mode-name indirect)
                 (buffer-local-value 'compat-local indirect)
                 (with-current-buffer indirect (buffer-string))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
    });
}

#[test]
fn compat_save_current_buffer_restores_current_buffer_locals_matches_gnu_emacs() {
    run_case(BufferLocalsCase {
        name: "save_current_buffer_restores_current_buffer_locals",
        form: r#"(let ((a (get-buffer-create " *compat-current-a*"))
      (b (get-buffer-create " *compat-current-b*")))
  (unwind-protect
      (progn
        (with-current-buffer a
          (setq default-directory "/tmp/compat-current-a/")
          (erase-buffer)
          (insert "A"))
        (with-current-buffer b
          (setq default-directory "/tmp/compat-current-b/")
          (erase-buffer)
          (insert "B"))
        (set-buffer a)
        (list
         (buffer-name (current-buffer))
         default-directory
         (save-current-buffer
           (set-buffer b)
           (list
            (buffer-name (current-buffer))
            default-directory
            (buffer-string)))
         (buffer-name (current-buffer))
         default-directory
         (buffer-string)))
    (kill-buffer b)
    (kill-buffer a)))"#,
    });
}
