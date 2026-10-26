//! Strict combo oracle probes, batch 331: window scroll / recenter / window-
//! start/end deep. set-window-start, recenter, window-start/end, pos-visible-in-
//! window-p, and window-end update with NO-UPDATE flag.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_recenter_screen_rows_preserve_point_with_box_drawing_hidden_and_wrapped_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Batch frames exercise the character-unit fallback. GUI pixel-budget
    // placement is tested separately; these exact starts protect display-row
    // traversal, including skipped invisible rows and continuation columns.
    let form = r#"(save-window-excursion
     (let ((cases nil))
      (dolist (kind '(ascii box hidden wrapped))
        (let ((buffer (generate-new-buffer " *recenter-control*")))
          (unwind-protect
              (progn
                (switch-to-buffer buffer)
                (setq-local truncate-lines nil)
                (dotimes (line 80)
                  (insert (format "%s line %02d %s\n"
                                  (if (eq kind 'ascii) "+" "├")
                                  (1+ line)
                                  (cond ((eq kind 'wrapped) (make-string 100 ?x))
                                        ((eq kind 'ascii) "|+")
                                        (t "│└")))))
                (when (eq kind 'hidden)
                  (goto-char (point-min))
                  (forward-line 32)
                  (let ((start (point)))
                    (forward-line 3)
                    (put-text-property start (point) 'invisible t)))
                (goto-char (point-min))
                (forward-line 39)
                (forward-char 5)
                (let ((saved-point (point)) (placements nil))
                  (dolist (arg '(nil 0 -1))
                    (recenter arg)
                    (push (list (save-excursion
                                  (goto-char (window-start))
                                  (list (line-number-at-pos) (current-column)))
                                (= (point) saved-point))
                          placements))
                  (push (list kind (nreverse placements)) cases)))
            (kill-buffer buffer))))
      (nreverse cases)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ascii (((29 0) t) ((40 0) t) ((18 0) t))) (box (((29 0) t) ((40 0) t) ((18 0) t))) (hidden (((26 0) t) ((40 0) t) ((15 0) t))) (wrapped (((34 79) t) ((40 0) t) ((29 0) t))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_start_set_recenter_dynamics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-scroll*")))
  (unwind-protect
      (with-current-buffer b
        (erase-buffer)
        (dotimes (i 100) (insert (format "line %d\n" i)))
        (let ((w (selected-window)))
          (set-window-buffer w b)
          (set-window-start w 1 t)
          (let ((start1 (window-start w)))
            (set-window-start w 500 t)
            (let ((start2 (window-start w)))
              (recenter 0)
              (list start1
                    start2
                    (window-start w)
                    (>= (window-end w) (window-start w)))))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (1 500 791 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_pos_visible_in_window_p_window_end_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-vis*")))
  (unwind-protect
      (with-current-buffer b
        (erase-buffer)
        (dotimes (i 50) (insert (format "row %d\n" i)))
        (let ((w (selected-window)))
          (set-window-buffer w b)
          (set-window-start w 1 t)
          (list (pos-visible-in-window-p 1 w)
                (integerp (window-end w t))
                (>= (window-end w t) (window-start w))
                (goto-char (window-start w))
                (integerp (window-end w)))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
