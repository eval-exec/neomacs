mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct PositionCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_window_position_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping window position semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        PositionCase {
            name: "set_window_buffer_uses_buffer_point_not_old_window_point",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w (selected-window))
         (b1 (get-buffer-create " *compat-pos-a*"))
         (b2 (get-buffer-create " *compat-pos-b*")))
    (unwind-protect
        (progn
          (with-current-buffer b1
            (erase-buffer)
            (insert (make-string 300 ?a))
            (goto-char 120))
          (with-current-buffer b2
            (erase-buffer)
            (insert (make-string 300 ?b))
            (goto-char 150))
          (set-window-buffer w b1)
          (set-window-start w 110)
          (set-window-point w 120)
          (set-window-buffer w b2)
          (with-current-buffer b1
            (goto-char 33))
          (set-window-buffer w b1)
          (list (window-start w)
                (window-point w)
                (with-current-buffer b1 (point))))
      (when (buffer-live-p b1) (kill-buffer b1))
      (when (buffer-live-p b2) (kill-buffer b2)))))"#,
        },
        PositionCase {
            name: "set_window_buffer_uses_last_window_start_from_other_window",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w1 (selected-window))
         (w2 (split-window nil nil 'right))
         (b1 (get-buffer-create " *compat-pos-c*"))
         (b2 (get-buffer-create " *compat-pos-d*")))
    (unwind-protect
        (progn
          (with-current-buffer b1
            (erase-buffer)
            (insert (make-string 300 ?a))
            (goto-char 120))
          (with-current-buffer b2
            (erase-buffer)
            (insert (make-string 300 ?b))
            (goto-char 150))
          (set-window-buffer w1 b1)
          (set-window-start w1 110)
          (set-window-point w1 120)
          (set-window-buffer w1 b2)
          (set-window-buffer w2 b1)
          (set-window-start w2 70)
          (set-window-buffer w2 b2)
          (set-window-buffer w1 b1)
          (list (window-start w1)
                (window-point w1)
                (window-start w2)
                (window-point w2)))
      (when (buffer-live-p b1) (kill-buffer b1))
      (when (buffer-live-p b2) (kill-buffer b2)))))"#,
        },
    ];

    for case in cases {
        let expected = run_oracle_eval(case.form)
            .unwrap_or_else(|err| panic!("oracle failed for {}: {err}", case.name));
        let actual = run_neovm_eval(case.form)
            .unwrap_or_else(|err| panic!("neovm failed for {}: {err}", case.name));
        assert_eq!(
            actual, expected,
            "position semantics mismatch for {}\nexpected: {}\nactual: {}",
            case.name, expected, actual
        );
    }
}
