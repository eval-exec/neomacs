//! Complex combo batch 91 — overlay rendering edge cases: invisible
//! overlays with priority, display property with strings, `before-string`
//! / `after-string`, and evaporate behavior at point.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx91_overlay_invisible_with_priority_and_window_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (add-to-invisibility-spec '(hide-mid . t))
  (let ((ov1 (make-overlay 3 6))
        (ov2 (make-overlay 7 10)))
    (overlay-put ov1 'invisible 'hide-mid)
    (overlay-put ov1 'priority 5)
    (overlay-put ov2 'invisible 'hide-mid)
    (overlay-put ov2 'priority 10)
    (let ((v1 (buffer-substring 1 16))
          (v2 (buffer-substring-no-properties 1 16)))
      (remove-from-invisibility-spec 'hide-mid)
      (list v1 v2 (length v1) (length (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_display_string_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"AAA BBB CC\" \"[XX]\" \"[XX]\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAA BBB CCC")
  (let ((ov (make-overlay 5 7)))
    (overlay-put ov 'display "[XX]")
    (list (buffer-substring 1 11)
          (get-char-property 5 'display)
          (get-char-property 6 'display)
          (get-char-property 7 'display)
          (get-char-property 8 'display))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_before_string_and_after_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"middl\" \"[BEFORE]\" \"[AFTER]\" 3 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "middle")
  (let ((ov (make-overlay 3 5)))
    (overlay-put ov 'before-string "[BEFORE]")
    (overlay-put ov 'after-string "[AFTER]")
    (list (buffer-substring 1 6)
          (overlay-get ov 'before-string)
          (overlay-get ov 'after-string)
          (overlay-start ov) (overlay-end ov))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_evaporate_when_emptied() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 3 7 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'evaporate t)
    (overlay-put ov 'face 'region)
    (let ((alive-before (overlayp ov))
          (start-before (overlay-start ov))
          (end-before (overlay-end ov)))
      (delete-region 3 7)
      (let ((alive-after (overlayp ov)))
        (list alive-before start-before end-before alive-after)))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_priority_and_face_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (bold italic italic italic nil (#<overlay in no buffer> #<overlay in no buffer>))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((lo (make-overlay 2 5))
        (hi (make-overlay 3 7)))
    (overlay-put lo 'priority 0)
    (overlay-put hi 'priority 10)
    (overlay-put lo 'face 'bold)
    (overlay-put hi 'face 'italic)
    (let ((at-2 (get-char-property 2 'face))
          (at-3 (get-char-property 3 'face))
          (at-4 (get-char-property 4 'face))
          (at-6 (get-char-property 6 'face))
          (at-7 (get-char-property 7 'face)))
      (prog1 (list at-2 at-3 at-4 at-6 at-7
                   (overlays-at 4))
        (delete-overlay lo)
        (delete-overlay hi)))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_window_local_only_in_selected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (region region #<window 1 on *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 1 5 nil t nil)))
    (overlay-put ov 'window (selected-window))
    (overlay-put ov 'face 'region)
    (let ((at-1-in-win (get-char-property 1 'face))
          (at-1-default (get-char-property 1 'face)))
      (list at-1-in-win at-1-default
            (overlay-get ov 'window)))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_invisible_buffer_substring_sees_through() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 1 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "beforehiddenafter")
  (add-to-invisibility-spec 'hide-it)
  (let ((ov (make-overlay 6 12)))
    (overlay-put ov 'invisible 'hide-it)
    (let ((visible (buffer-substring 1 19))
          (no-props (buffer-substring-no-properties 1 19))
          (full (buffer-string)))
      (remove-from-invisibility-spec 'hide-it)
      (list visible no-props full
            (length visible) (length full)))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_modification_hooks_invocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:before 4 6 nil) (:after 4 4 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (with-temp-buffer
    (insert "0123456789")
    (let ((ov (make-overlay 3 7)))
      (overlay-put ov 'modification-hooks
                   (list (lambda (ov after-p beg end &optional length)
                           (push (list (if after-p :after :before) beg end length) calls))))
      (delete-region 4 6)
      (insert "X" "Y" "Z")))
  (nreverse calls))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_insert_in_hooks_with_insert_in_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 7 \"01X234Y56789\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 3 6 nil nil nil)))
    (overlay-put ov 'insert-in-front-hooks
                 (list (lambda (ov after-p beg end &optional length) nil)))
    (overlay-put ov 'insert-behind-hooks
                 (list (lambda (ov after-p beg end &optional length) nil)))
    (goto-char 3)
    (insert "X")
    (goto-char 7)
    (insert "Y")
    (list (overlay-start ov) (overlay-end ov) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_move_and_redisplay_preempt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 10 t 8 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((ov (make-overlay 2 6)))
    (overlay-put ov 'face 'region)
    (move-overlay ov 5 10)
    (let ((after-move-start (overlay-start ov))
          (after-move-end (overlay-end ov)))
      (move-overlay ov 8 8 (current-buffer))
      (let ((zero-width (and (= (overlay-start ov) (overlay-end ov))
                             (overlayp ov))))
        (list after-move-start after-move-end zero-width
              (overlay-start ov) (overlay-end ov))))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_nested_categories_priority_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 1 29)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF0123456789")
  (add-to-invisibility-spec '(outer . t))
  (add-to-invisibility-spec '(inner . nil))
  (let ((outer-ov (make-overlay 5 25))
        (inner-ov (make-overlay 10 20)))
    (overlay-put outer-ov 'invisible 'outer)
    (overlay-put inner-ov 'invisible 'inner)
    (let ((at-8-invis (get-char-property 8 'invisible))
          (at-15-invis (get-char-property 15 'invisible))
          (vis-1 (buffer-substring 1 29)))
      (remove-from-invisibility-spec 'outer)
      (let ((vis-2 (buffer-substring 1 29)))
        (list at-8-invis at-15-invis vis-1 vis-2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx91_overlay_marker_point_undo_narrow_textprop_display_invis_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Visible text and more content here for tests")
  (put-text-property 1 7 'face 'bold)
  (put-text-property 9 14 'display "XX")
  (add-to-invisibility-spec 'neo-cx91-hide)
  (let ((m (set-marker (make-marker) 20))
        (invis-ov (make-overlay 5 10))
        (disp-ov (make-overlay 25 35))
        (face-ov (make-overlay 15 25)))
    (overlay-put invis-ov 'invisible 'neo-cx91-hide)
    (overlay-put disp-ov 'display "[DISPLAY]")
    (overlay-put face-ov 'face 'italic)
    (overlay-put face-ov 'priority 5)
    (narrow-to-region 3 40)
    (let ((state (list (buffer-string)
                       (marker-position m)
                       (overlay-start invis-ov) (overlay-end invis-ov)
                       (overlay-start disp-ov) (overlay-end disp-ov)
                       (overlay-start face-ov) (overlay-end face-ov)
                       (text-properties-at 1)
                       (get-char-property 16 'face)
                       (get-char-property 28 'display))))
      (undo)
      (widen)
      (list state
            (buffer-string)
            (marker-position m)
            (overlayp invis-ov) (overlay-start invis-ov)
            (overlayp disp-ov) (overlay-start disp-ov)
            (overlayp face-ov) (overlay-start face-ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
