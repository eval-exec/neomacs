mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct SwitchCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_switch_to_buffer_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping switch-to-buffer audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        SwitchCase {
            name: "switch_to_buffer_updates_selected_window_and_display_count",
            form: r#"(save-window-excursion
  (let* ((w (selected-window))
         (b (get-buffer-create "stb-basic")))
    (unwind-protect
        (progn
          (with-current-buffer b
            (setq-local buffer-display-count 0))
          (switch-to-buffer b)
          (list
           (eq (current-buffer) b)
           (eq (window-buffer w) b)
           (eq (selected-window) w)
           (with-current-buffer b buffer-display-count)))
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
        SwitchCase {
            name: "switch_to_buffer_preserves_window_state_when_configured",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w (selected-window))
         (b1 (get-buffer-create "stb-point-a"))
         (b2 (get-buffer-create "stb-point-b")))
    (unwind-protect
        (progn
          (with-current-buffer b1
            (erase-buffer)
            (insert (make-string 300 ?a))
            (setq-local switch-to-buffer-preserve-window-point t))
          (with-current-buffer b2
            (erase-buffer)
            (insert (make-string 300 ?b)))
          (switch-to-buffer b1)
          (set-window-start w 40)
          (set-window-point w 50)
          (switch-to-buffer b2)
          (switch-to-buffer b1)
          (list
           (window-start w)
           (window-point w)
           (with-current-buffer b1 (point))))
      (when (buffer-live-p b1) (kill-buffer b1))
      (when (buffer-live-p b2) (kill-buffer b2)))))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "switch-to-buffer mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
