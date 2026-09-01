mod common;

use common::{oracle_enabled, run_neovm_eval, run_oracle_eval};

struct WindowCase {
    name: &'static str,
    form: &'static str,
}

#[test]
fn compat_window_semantics_matches_gnu_emacs() {
    if !oracle_enabled() {
        eprintln!(
            "skipping window semantics audit: set NEOVM_FORCE_ORACLE_PATH or place GNU Emacs mirror alongside the repo"
        );
        return;
    }

    let cases = [
        WindowCase {
            name: "window_tree_navigation_and_normal_size",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((left (selected-window))
         (right (split-window nil nil 'right))
         (bottom (split-window right nil 'below))
         (root (frame-root-window))
         (vparent (window-parent right)))
    (list (window-valid-p root)
          (window-live-p root)
          (eq (window-parent left) root)
          (eq (window-next-sibling left) vparent)
          (eq (window-left-child root) left)
          (window-top-child root)
          (eq (window-parent right) vparent)
          (eq (window-parent bottom) vparent)
          (eq (window-top-child vparent) right)
          (window-left-child vparent)
          (eq (window-next-sibling right) bottom)
          (eq (window-prev-sibling bottom) right)
          (window-normal-size left)
          (window-normal-size left t)
          (window-normal-size right)
          (window-normal-size right t)
          (window-normal-size vparent)
          (window-normal-size vparent t))))"#,
        },
        WindowCase {
            name: "window_parameter_storage_matches_gnu_lifecycle",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((left (selected-window))
         (_ (set-window-parameter left 'foo 'left))
         (right (split-window nil nil 'right))
         (root (frame-root-window)))
    (list
     (window-parameter left 'foo)
     (window-parameter right 'foo)
     (progn
       (set-window-parameter right 'foo 'right)
       (window-parameter right 'foo))
     (progn
       (set-window-parameter root 'foo 'root)
       (window-parameter root 'foo))
     (progn
       (delete-window right)
       (list
        (window-parameter left 'foo)
        (window-parameter right 'foo)
        (condition-case err
            (window-parameters right)
          (error (car err))))))))"#,
        },
        WindowCase {
            name: "set_window_buffer_restores_saved_window_state",
            form: r#"(save-window-excursion
  (let* ((w (selected-window))
         (b1 (get-buffer-create " *compat-swb-a*"))
         (b2 (get-buffer-create " *compat-swb-b*")))
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
          (set-window-margins w 3 4)
          (list
           (list (window-start w)
                 (window-point w)
                 (window-margins w))
           (progn
             (set-window-buffer w b2)
             (list (window-start w)
                   (window-point w)
                   (window-margins w)))
           (progn
             (set-window-margins w 7 8)
             (set-window-buffer w b1 t)
             (list (window-start w)
                   (window-point w)
                   (window-margins w)))
           (progn
             (set-window-margins w 9 10)
             (set-window-buffer w b2 t)
             (list (window-start w)
                   (window-point w)
                   (window-margins w)))
           (progn
             (set-window-margins w 11 12)
             (set-window-buffer w b1 nil)
             (list (window-start w)
                   (window-point w)
                   (window-margins w)))))
      (when (buffer-live-p b1) (kill-buffer b1))
      (when (buffer-live-p b2) (kill-buffer b2)))))"#,
        },
        WindowCase {
            name: "window_old_point_tracks_gnu_set_window_buffer_resets",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w (selected-window))
         (b (get-buffer-create " *compat-window-old-point*")))
    (unwind-protect
        (progn
          (with-current-buffer b
            (erase-buffer)
            (insert (make-string 40 ?x))
            (goto-char 7))
          (set-window-buffer w b)
          (list
           (window-point w)
           (window-old-point w)
           (progn
             (set-window-point w 13)
             (list (window-point w)
                   (window-old-point w)))
           (progn
             (set-window-buffer w b t)
             (list (window-point w)
                   (window-old-point w)))
           (progn
             (with-current-buffer b
               (goto-char 21))
             (set-window-buffer w b nil)
             (list (window-point w)
                   (window-old-point w)))))
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_updates_history_lists",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w (selected-window))
         (b1 (get-buffer-create " *compat-swb-hist-a*"))
         (b2 (get-buffer-create " *compat-swb-hist-b*"))
         (n '((foo 1 2))))
    (unwind-protect
        (progn
          (with-current-buffer b1
            (erase-buffer)
            (insert (make-string 300 ?a)))
          (with-current-buffer b2
            (erase-buffer)
            (insert (make-string 300 ?b)))
          (set-window-prev-buffers w nil)
          (set-window-next-buffers w nil)
          (set-window-buffer w b1)
          (set-window-start w 7)
          (set-window-point w 11)
          (set-window-next-buffers w n)
          (set-window-buffer w b2)
          (list
           (null (window-next-buffers w))
           (mapcar (lambda (e) (buffer-name (car e))) (window-prev-buffers w))
           (mapcar (lambda (e)
                     (list (markerp (nth 1 e))
                           (marker-position (nth 1 e))
                           (markerp (nth 2 e))
                           (marker-position (nth 2 e))))
                   (window-prev-buffers w))))
      (when (buffer-live-p b1) (kill-buffer b1))
      (when (buffer-live-p b2) (kill-buffer b2)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_discards_current_buffer_from_history",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w (selected-window))
         (current (window-buffer w))
         (other (get-buffer-create "swb-current-clean")))
    (unwind-protect
        (progn
          (with-current-buffer current
            (erase-buffer)
            (insert "current-history"))
          (with-current-buffer other
            (erase-buffer)
            (insert "other-history"))
          (set-window-prev-buffers
           w
           (list
            (list current
                  (with-current-buffer current (copy-marker 3))
                  (with-current-buffer current (copy-marker 5)))
            (list other
                  (with-current-buffer other (copy-marker 2))
                  (with-current-buffer other (copy-marker 4)))))
          (set-window-next-buffers w (list current other))
          (set-window-buffer w current)
          (list
           (mapcar (lambda (e) (buffer-name (car e))) (window-prev-buffers w))
           (mapcar #'buffer-name (window-next-buffers w))))
      (when (buffer-live-p other) (kill-buffer other)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_respects_strong_dedication",
            form: r#"(save-window-excursion
  (let* ((w (selected-window))
         (b (get-buffer-create "swb-dedicated")))
    (unwind-protect
        (progn
          (set-window-dedicated-p w t)
          (condition-case err
              (progn
                (set-window-buffer w b)
                'no-error)
            (error (list (car err) (cadr err)))))
      (set-window-dedicated-p w nil)
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
        WindowCase {
            name: "window_discard_buffer_from_window_updates_history",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w (selected-window))
         (b1 (get-buffer-create "wdbfw-a"))
         (b2 (get-buffer-create "wdbfw-b")))
    (unwind-protect
        (progn
          (with-current-buffer b1
            (erase-buffer)
            (insert "aaaa"))
          (with-current-buffer b2
            (erase-buffer)
            (insert "bbbb"))
          (set-window-prev-buffers
           w
           (list
            (list b1
                  (with-current-buffer b1 (copy-marker 2))
                  (with-current-buffer b1 (copy-marker 3)))
            (list b2
                  (with-current-buffer b2 (copy-marker 2))
                  (with-current-buffer b2 (copy-marker 4)))))
          (set-window-next-buffers w (list b1 b2))
          (window-discard-buffer-from-window b1 w)
          (list
           (mapcar (lambda (e) (buffer-name (car e))) (window-prev-buffers w))
           (mapcar #'buffer-name (window-next-buffers w))))
      (when (buffer-live-p b1) (kill-buffer b1))
      (when (buffer-live-p b2) (kill-buffer b2)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_applies_buffer_local_margins",
            form: r#"(save-window-excursion
  (let* ((w (selected-window))
         (b (get-buffer-create "swb-margins")))
    (unwind-protect
        (progn
          (with-current-buffer b
            (setq-local left-margin-width 5)
            (setq-local right-margin-width 6))
          (set-window-margins w 1 2)
          (set-window-buffer w b)
          (window-margins w))
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_same_buffer_resets_selected_window_state",
            form: r#"(save-window-excursion
  (let* ((w (selected-window))
         (b (get-buffer-create "swb-same-reset")))
    (unwind-protect
        (progn
          (with-current-buffer b
            (erase-buffer)
            (insert (make-string 300 ?a))
            (goto-char 40))
          (set-window-buffer w b)
          (set-window-start w 110)
          (set-window-point w 120)
          (with-current-buffer b
            (goto-char 33))
          (set-window-buffer w b)
          (list (window-start w)
                (window-point w)
                (point)))
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_same_buffer_resets_nonselected_window_state",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w (split-window nil nil 'right))
         (b (get-buffer-create "swb-other-reset")))
    (unwind-protect
        (progn
          (with-current-buffer b
            (erase-buffer)
            (insert (make-string 300 ?a))
            (goto-char 40))
          (set-window-buffer w b)
          (set-window-start w 110)
          (set-window-point w 120)
          (with-current-buffer b
            (goto-char 33))
          (set-window-buffer w b)
          (list (window-start w)
                (window-point w)
                (with-current-buffer b (point))))
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_resets_hscroll_except_keep_margins_same_buffer",
            form: r#"(save-window-excursion
  (let* ((w (selected-window))
         (b (get-buffer-create "swb-hscroll-reset")))
    (unwind-protect
        (progn
          (set-window-buffer w b)
          (set-window-hscroll w 7)
          (list
           (window-hscroll w)
           (progn
             (set-window-buffer w b)
             (window-hscroll w))
           (progn
             (set-window-hscroll w 7)
             (set-window-buffer w b t)
             (window-hscroll w))))
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_updates_buffer_display_bookkeeping",
            form: r#"(save-window-excursion
  (let* ((w (selected-window))
         (b (get-buffer-create "swb-display-meta")))
    (unwind-protect
        (progn
          (with-current-buffer b
            (setq-local buffer-display-count 0)
            (setq-local buffer-display-time nil))
          (list
           (with-current-buffer b
             (list buffer-display-count buffer-display-time))
           (progn
             (set-window-buffer w b)
             (with-current-buffer b
               (list buffer-display-count
                     (consp buffer-display-time)
                     (= (length buffer-display-time) 4)
                     (mapcar #'integerp buffer-display-time))))
           (progn
             (set-window-buffer w b)
             (with-current-buffer b
               (list buffer-display-count
                     (consp buffer-display-time)
                     (= (length buffer-display-time) 4)
                     (mapcar #'integerp buffer-display-time))))))
      (when (buffer-live-p b) (kill-buffer b)))))"#,
        },
        WindowCase {
            name: "set_window_buffer_preserves_point_of_other_last_selected_window",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w1 (selected-window))
         (w2 (split-window nil nil 'right))
         (w3 (split-window w2 nil 'below))
         (b (get-buffer-create "swb-last-selected"))
         (c (get-buffer-create "swb-last-selected-other"))
         (d (get-buffer-create "swb-last-selected-replace")))
    (unwind-protect
        (progn
          (with-current-buffer b
            (erase-buffer)
            (insert (make-string 400 ?x))
            (goto-char 10))
          (with-current-buffer c
            (erase-buffer)
            (insert (make-string 40 ?c)))
          (with-current-buffer d
            (erase-buffer)
            (insert (make-string 40 ?d)))
          (set-window-buffer w1 b)
          (set-window-buffer w2 b)
          (set-window-buffer w3 c)
          (set-window-point w1 17)
          (set-window-point w2 91)
          (select-window w2)
          (select-window w3)
          (set-window-buffer w1 d)
          (list
           (with-current-buffer b (point))
           (window-point w2)
           (buffer-name (window-buffer w1))
           (buffer-name (window-buffer w2))
           (buffer-name (window-buffer w3))))
      (when (buffer-live-p b) (kill-buffer b))
      (when (buffer-live-p c) (kill-buffer c))
      (when (buffer-live-p d) (kill-buffer d)))))"#,
        },
        WindowCase {
            name: "other_window_cycles_across_split_windows",
            form: r#"(save-window-excursion
  (delete-other-windows)
  (let* ((w1 (selected-window))
         (w2 (split-window nil nil 'right)))
    (list
     (progn (other-window 1) (eq (selected-window) w2))
     (progn (other-window 1) (eq (selected-window) w1))
     (progn (other-window -1) (eq (selected-window) w2))
     (progn (select-window w1) (other-window 0.4) (eq (selected-window) w1))
     (progn (select-window w1) (other-window -0.4) (eq (selected-window) w2)))))"#,
        },
    ];

    for case in cases {
        let gnu = run_oracle_eval(case.form).expect("GNU Emacs evaluation");
        let neovm = run_neovm_eval(case.form).expect("NeoVM evaluation");
        assert_eq!(
            neovm, gnu,
            "window semantics mismatch for {}:\nGNU: {}\nNeoVM: {}",
            case.name, gnu, neovm
        );
    }
}
