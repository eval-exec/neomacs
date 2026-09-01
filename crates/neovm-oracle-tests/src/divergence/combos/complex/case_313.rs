//! Complex combo batch 313 — `overlay` lifecycle ultimate: create/move/
//! delete/evaporate, priority chains with invisible+display+face,
//! before-string/after-string, window-local filtering, modification-hooks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx313_overlay_create_move_delete_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 3 7 #<overlay in no buffer> 5 10 nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'face 'bold)
    (list (overlayp ov)
          (overlay-start ov) (overlay-end ov)
          (move-overlay ov 5 10)
          (overlay-start ov) (overlay-end ov)
          (delete-overlay ov)
          (overlayp ov)
          (overlays-in 1 16))))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_evaporate_when_region_emptied() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 1 t 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'evaporate t)
    (overlay-put ov 'face 'region)
    (let ((before (overlayp ov))
          (count-before (length (overlays-in 1 10))))
      (delete-region 3 7)
      (list before count-before
            (overlayp ov)
            (length (overlays-in 1 6))))))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_priority_chain_with_invisible_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil bold italic italic italic italic neo-cx313-h \"0123456789ABCDEF\" (t) \"0123456789ABCDEF\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (add-to-invisibility-spec 'neo-cx313-h)
  (let ((lo (make-overlay 2 6))
        (hi (make-overlay 4 10)))
    (overlay-put lo 'priority 0)
    (overlay-put hi 'priority 10)
    (overlay-put lo 'face 'bold)
    (overlay-put hi 'face 'italic)
    (overlay-put lo 'invisible 'neo-cx313-h)
    (list (get-char-property 1 'face)
          (get-char-property 2 'face)
          (get-char-property 4 'face)
          (get-char-property 5 'face)
          (get-char-property 6 'face)
          (get-char-property 8 'face)
          (get-char-property 3 'invisible)
          (buffer-substring 1 17)
          (remove-from-invisibility-spec 'neo-cx313-h)
          (buffer-substring 1 17))))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_before_after_string_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"middl\" #(\"[BEFORE]\" 0 8 (face bold)) #(\"[AFTER]\" 0 7 (face italic)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "middle")
  (let ((ov (make-overlay 3 5)))
    (overlay-put ov 'before-string (propertize "[BEFORE]" 'face 'bold))
    (overlay-put ov 'after-string (propertize "[AFTER]" 'face 'italic))
    (list (buffer-substring 1 6)
          (overlay-get ov 'before-string)
          (overlay-get ov 'after-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_window_local_filtering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (region nil #<window 1 on *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 1 5 nil t nil)))
    (overlay-put ov 'window (selected-window))
    (overlay-put ov 'face 'region)
    (list (get-char-property 1 'face)
          (get-char-property 5 'face)
          (overlay-get ov 'window))))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_modification_hooks_invocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:before 4 6 nil) (:after 4 4 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (calls)
  (with-temp-buffer
    (insert "0123456789")
    (let ((ov (make-overlay 3 7)))
      (overlay-put ov 'modification-hooks
                   (list (lambda (ov after-p beg end &optional len)
                           (push (list (if after-p :after :before) beg end len) calls))))
      (delete-region 4 6)
      (insert "XY")))
  (nreverse calls))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_insert_in_front_behind_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 7 \"01X234Y56789\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 3 6 nil nil nil)))
    (overlay-put ov 'insert-in-front-hooks
                 (list (lambda (&rest _) nil)))
    (overlay-put ov 'insert-behind-hooks
                 (list (lambda (&rest _) nil)))
    (goto-char 3)
    (insert "X")
    (goto-char 7)
    (insert "Y")
    (list (overlay-start ov) (overlay-end ov) (buffer-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_invisible_buffer_substring_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 1 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "beforehiddenafter")
  (add-to-invisibility-spec 'neo-cx313-h)
  (let ((ov (make-overlay 6 12)))
    (overlay-put ov 'invisible 'neo-cx313-h)
    (let ((visible (buffer-substring 1 19))
          (no-props (buffer-substring-no-properties 1 19))
          (filtered (filter-buffer-substring 1 19)))
      (remove-from-invisibility-spec 'neo-cx313-h)
      (list visible no-props filtered (length visible) (length filtered)))))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_category_properties_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-cx313-cat bold 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 1 5)))
    (overlay-put ov 'category 'neo-cx313-cat)
    (put 'neo-cx313-cat 'face 'bold)
    (put 'neo-cx313-cat 'priority 99)
    (list (overlay-get ov 'category)
          (get 'neo-cx313-cat 'face)
          (get 'neo-cx313-cat 'priority))))
"##,
        expect,
    )
}

#[test]
fn div_cx313_overlay_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Overlay lifecycle mega test buffer content")
  (put-text-property 1 6 'face 'bold)
  (let ((m (set-marker (make-marker) 10))
        (ov1 (make-overlay 4 12))
        (ov2 (make-overlay 15 25)))
    (overlay-put ov1 'face 'italic)
    (overlay-put ov1 'evaporate t)
    (overlay-put ov1 'priority 5)
    (overlay-put ov2 'face 'region)
    (overlay-put ov2 'invisible 'neo-cx313-mega-h)
    (add-to-invisibility-spec 'neo-cx313-mega-h)
    (narrow-to-region 3 30)
    (move-overlay ov2 8 20)
    (delete-region 10 15)
    (let ((state (list (buffer-string)
                       (marker-position m)
                       (overlay-start ov1) (overlay-end ov1)
                       (overlay-start ov2) (overlay-end ov2)
                       (overlayp ov1)
                       (text-properties-at 1)
                       (length (overlays-in 1 30)))))
      (undo) (undo)
      (widen)
      (remove-from-invisibility-spec 'neo-cx313-mega-h)
      (list state (buffer-string)
            (overlay-start ov1) (overlay-end ov1)
            (overlay-start ov2) (overlay-end ov2)
            (text-properties-at 1)))))
"##,
        expect,
    )
}
