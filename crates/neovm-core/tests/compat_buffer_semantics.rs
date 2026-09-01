mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct BufferCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_buffer_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping buffer semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        BufferCase {
            name: "modified_and_restore_transitions",
            form: r#"(let ((buf (get-buffer-create " *compat-buffer-state*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (list :initial
              (buffer-modified-p)
              (buffer-modified-tick)
              (buffer-chars-modified-tick)
              (recent-auto-save-p)
              :after-set-t
              (progn
                (set-buffer-modified-p t)
                (list (buffer-modified-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)
                      (recent-auto-save-p)))
              :after-restore-nil
              (progn
                (restore-buffer-modified-p nil)
                (list (buffer-modified-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)
                      (recent-auto-save-p)))
              :after-restore-autosaved
              (progn
                (restore-buffer-modified-p 'autosaved)
                (list (buffer-modified-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)
                      (recent-auto-save-p)))))
    (kill-buffer buf)))"#,
        },
        BufferCase {
            name: "autosave_state_transitions",
            form: r#"(let ((buf (get-buffer-create " *compat-buffer-auto*")))
  (unwind-protect
      (progn
        (set-buffer buf)
        (insert "x")
        (list :before-auto
              (buffer-modified-p)
              (recent-auto-save-p)
              (buffer-modified-tick)
              (buffer-chars-modified-tick)
              :after-auto
              (progn
                (set-buffer-auto-saved)
                (list (buffer-modified-p)
                      (recent-auto-save-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)))
              :after-set-t
              (progn
                (set-buffer-modified-p t)
                (list (buffer-modified-p)
                      (recent-auto-save-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)))
              :after-insert
              (progn
                (insert "y")
                (list (buffer-modified-p)
                      (recent-auto-save-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)))
              :after-clear
              (progn
                (set-buffer-modified-p nil)
                (list (buffer-modified-p)
                      (recent-auto-save-p)
                      (buffer-modified-tick)
                      (buffer-chars-modified-tick)))))
    (kill-buffer buf)))"#,
        },
        BufferCase {
            name: "indirect_buffer_text_properties_follow_shared_text_edits",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-text-props-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "abcdef")
          (put-text-property 2 5 'face 'bold))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-text-props-indirect*" nil)))
          (unwind-protect
              (progn
                (with-current-buffer indirect
                  (delete-region 3 4))
                (list
                 (with-current-buffer base
                   (list
                    (buffer-string)
                    (get-text-property 2 'face)
                    (get-text-property 3 'face)
                    (get-text-property 4 'face)
                    (get-text-property 5 'face)))
                 (with-current-buffer indirect
                   (list
                    (buffer-string)
                    (get-text-property 2 'face)
                    (get-text-property 3 'face)
                    (get-text-property 4 'face)
                    (get-text-property 5 'face)))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "indirect_buffer_undo_list_follows_shared_text_history",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-undo-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (setq buffer-undo-list nil)
          (insert "abc"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-undo-indirect*" nil)))
          (unwind-protect
              (list
               (with-current-buffer base
                 (prin1-to-string buffer-undo-list))
               (with-current-buffer indirect
                 (prin1-to-string buffer-undo-list))
               (with-current-buffer indirect
                 (let ((buffer-undo-list buffer-undo-list))
                   (primitive-undo 1 buffer-undo-list)
                   (buffer-string)))
               (with-current-buffer base
                 (buffer-string)))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "set_buffer_modified_p_returns_nil_and_updates_indirect_base_state",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-modified-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "x"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-modified-indirect*" nil)))
          (unwind-protect
              (list
               (with-current-buffer indirect
                 (set-buffer-modified-p nil))
               (with-current-buffer base
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p)))
               (with-current-buffer indirect
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "indirect_buffer_autosave_state_is_buffer_local",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-autosave-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "xy"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-autosave-indirect*" nil)))
          (unwind-protect
              (list
               (with-current-buffer indirect
                 (set-buffer-auto-saved))
               (with-current-buffer base
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p)))
               (with-current-buffer indirect
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "restore_buffer_modified_p_autosaved_targets_indirect_base",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-restore-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "xy"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-restore-indirect*" nil)))
          (unwind-protect
              (list
               (with-current-buffer indirect
                 (restore-buffer-modified-p 'autosaved))
               (with-current-buffer base
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p)))
               (with-current-buffer indirect
                 (list (buffer-modified-p)
                       (buffer-modified-tick)
                       (recent-auto-save-p))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "internal_set_buffer_modified_tick_shares_modiff_not_autosave",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-modiff-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "xy"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-modiff-indirect*" nil)))
          (unwind-protect
              (progn
                (with-current-buffer indirect
                  (set-buffer-auto-saved)
                  (internal--set-buffer-modified-tick 77))
                (list
                 (with-current-buffer base
                   (list (buffer-modified-p)
                         (buffer-modified-tick)
                         (recent-auto-save-p)))
                 (with-current-buffer indirect
                   (list (buffer-modified-p)
                         (buffer-modified-tick)
                         (recent-auto-save-p)))))
            (kill-buffer indirect))))
    (kill-buffer base)))"#,
        },
        BufferCase {
            name: "killing_base_buffer_kills_indirect_buffers",
            form: r#"(let ((base (get-buffer-create " *compat-buffer-kill-base*")))
  (unwind-protect
      (progn
        (with-current-buffer base
          (erase-buffer)
          (insert "abc"))
        (let ((indirect
               (make-indirect-buffer base " *compat-buffer-kill-indirect*" nil)))
          (list (buffer-live-p base)
                (buffer-live-p indirect)
                (kill-buffer base)
                (buffer-live-p base)
                (buffer-live-p indirect)
                (get-buffer " *compat-buffer-kill-base*")
                (get-buffer " *compat-buffer-kill-indirect*"))))
    (when (get-buffer " *compat-buffer-kill-base*")
      (kill-buffer " *compat-buffer-kill-base*"))
    (when (get-buffer " *compat-buffer-kill-indirect*")
      (kill-buffer " *compat-buffer-kill-indirect*"))))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "buffer semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
