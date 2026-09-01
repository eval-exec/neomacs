//! Strict combo oracle probes, batch 129: window-state-get/put with
//! dedicated + parameters, marker tracking through replace-match with
//! case conversion, kbd macro execution state, and cl-defmethod
//! combination ordering. Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u3_window_state_with_dedicated_and_parameters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b1 (get-buffer-create " *probe-ws-a*"))
      (b2 (get-buffer-create " *probe-ws-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (set-window-parameter nil 'probe-param 'value)
        (set-window-dedicated-p nil 'test)
        (let ((state (window-state-get nil t)))
          (let ((w2 (split-window nil nil 'right)))
            (set-window-buffer w2 b2)
            (set-window-parameter w2 'other 'param2)
            (select-window w2))
          (window-state-put state nil 'safe)
          (list (count-windows)
                (buffer-name (window-buffer (selected-window)))
                (window-parameter nil 'probe-param)
                (window-dedicated-p))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (1 \" *probe-ws-a*\" nil test)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u3_marker_through_replace_match_case_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "The QUICK brown FOX")
  (let ((m (set-marker (make-marker) 5)))
    (goto-char 1)
    (while (re-search-forward "[A-Z]+" nil t)
      (replace-match (downcase (match-string 0)))
      (undo-boundary))
    (list (buffer-string)
          (marker-position m)
          (eq (marker-buffer m) (current-buffer))
          (buffer-substring 1 4)
          (buffer-substring 5 10))))
"##;
    let expect = expect_test::expect![[r#""OK (\"The QUICK brown FOX\" 5 t \"The\" \"QUICK\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u3_kbd_macro_execution_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((executing-kbd-macro nil)
      (last-kbd-macro nil))
  (with-temp-buffer
    (setq last-kbd-macro (kbd "a b c"))
    (execute-kbd-macro last-kbd-macro)
    (list (buffer-string)
          executing-kbd-macro
          (stringp last-kbd-macro)
          (vectorp last-kbd-macro)
          (length last-kbd-macro))))
"##;
    let expect = expect_test::expect![[r#""OK (\"abc\" nil t nil 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u3_cl_defmethod_combination_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (cl-defgeneric probe-mc (x))
  (cl-defmethod probe-mc :around ((x integer))
    (push 'around-in log)
    (prog1 (cl-call-next-method)
      (push 'around-out log)))
  (cl-defmethod probe-mc :before ((x integer))
    (push 'before log))
  (cl-defmethod probe-mc :after ((x integer))
    (push 'after log))
  (cl-defmethod probe-mc ((x integer))
    (push 'primary log)
    (* x 2))
  (list (probe-mc 5)
        (nreverse log)))
"##;
    let expect = expect_test::expect![[r#""OK (10 (around-in before primary after around-out))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u3_overlay_priority_face_merge_at_exact_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((o1 (make-overlay 3 7))
        (o2 (make-overlay 5 7))
        (o3 (make-overlay 5 9)))
    (overlay-put o1 'priority 1)
    (overlay-put o2 'priority 3)
    (overlay-put o3 'priority 2)
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o3 'face 'underline)
    (list (get-char-property 3 'face)
          (get-char-property 4 'face)
          (get-char-property 5 'face)
          (get-char-property 6 'face)
          (get-char-property 7 'face)
          (get-char-property 8 'face)
          (get-char-property 9 'face)
          (mapcar (lambda (o) (overlay-get o 'priority))
                  (sort (overlays-at 5)
                        (lambda (a b) (< (overlay-get a 'priority)
                                         (overlay-get b 'priority))))))))
"##;
    let expect =
        expect_test::expect![[r#""OK (bold bold italic italic underline underline nil (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
