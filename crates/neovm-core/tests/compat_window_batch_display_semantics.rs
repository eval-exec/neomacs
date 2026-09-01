mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct WindowDisplayCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_window_batch_display_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping window batch display audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        WindowDisplayCase {
            name: "window_scroll_bars_batch_round_trip",
            form: r#"(let ((w (selected-window)))
  (list
   (window-scroll-bars w)
   (set-window-scroll-bars w 13 'left 9 'bottom t)
   (window-scroll-bars w)
   (window-scroll-bar-width w)
   (window-scroll-bar-height w)
   (window-vscroll w)
   (set-window-vscroll w 7)
   (window-vscroll w)))"#,
        },
        WindowDisplayCase {
            name: "set_window_buffer_applies_batch_display_defaults",
            form: r#"(save-window-excursion
  (let* ((w (selected-window))
         (b (get-buffer-create "swb-display-batch")))
    (unwind-protect
        (progn
          (with-current-buffer b
            (setq-local left-fringe-width 3)
            (setq-local right-fringe-width 5)
            (setq-local fringes-outside-margins t)
            (setq-local scroll-bar-width 11)
            (setq-local vertical-scroll-bar 'left)
            (setq-local scroll-bar-height 7)
            (setq-local horizontal-scroll-bar 'bottom))
          (set-window-buffer w b)
          (list
           (window-fringes w)
           (window-scroll-bars w)
           (window-scroll-bar-width w)
           (window-scroll-bar-height w)))
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "window batch display mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
