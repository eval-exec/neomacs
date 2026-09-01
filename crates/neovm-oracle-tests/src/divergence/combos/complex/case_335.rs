//! Complex combo batch 335 — `buffer`/`window`/`frame` ultimate:
//! indirect-buffer text sharing, buffer-list reordering, window-configuration
//! save/restore, frame-parameter round-trip, display-pixel-dimensions query.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx335_indirect_buffer_shares_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"shared text content\" \"shared text content\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((base (get-buffer-create " *neo-cx335-base*"))
       (ind (make-indirect-buffer base " *neo-cx335-ind*")))
  (with-current-buffer base
    (insert "shared text content"))
  (let ((base-str (with-current-buffer base (buffer-string)))
        (ind-str (with-current-buffer ind (buffer-string))))
    (prog1 (list base-str ind-str (string= base-str ind-str)
                 (eq (buffer-base-buffer ind) base))
      (kill-buffer ind)
      (kill-buffer base))))
"##,
        expect,
    )
}

#[test]
fn div_cx335_buffer_list_reorder_bury_unbury() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx335-bury-a*"))
      (buf-b (get-buffer-create " *neo-cx335-bury-b*")))
  (let ((before-a (cl-position buf-a (buffer-list))))
    (bury-buffer buf-a)
    (let ((after-bury-a (cl-position buf-a (buffer-list))))
      (kill-buffer buf-a)
      (kill-buffer buf-b)
      (list before-a after-bury-a (> after-bury-a before-a)))))
"##,
        expect,
    )
}

#[test]
fn div_cx335_window_configuration_save_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((config (current-window-configuration))
      (n-before (length (window-list))))
  (split-window)
  (let ((n-split (length (window-list))))
    (set-window-configuration config)
    (let ((n-restored (length (window-list))))
      (list n-before n-split n-restored (= n-before n-restored)))))
"##,
        expect,
    )
}

#[test]
fn div_cx335_frame_parameter_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (let ((before (frame-parameter frame 'neo-cx335-param)))
    (modify-frame-parameters frame '((neo-cx335-param . "value-1")))
    (let ((v1 (frame-parameter frame 'neo-cx335-param)))
      (modify-frame-parameters frame '((neo-cx335-param . "value-2")))
      (let ((v2 (frame-parameter frame 'neo-cx335-param)))
        (modify-frame-parameters frame '((neo-cx335-param)))
        (list before v1 v2 (frame-parameter frame 'neo-cx335-param)))))
"##,
        expect,
    )
}

#[test]
fn div_cx335_display_info_full_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil \"F1\" dark)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (integerp (display-pixel-width))
        (integerp (display-pixel-height))
        (integerp (display-color-cells))
        (integerp (display-planes))
        (display-graphic-p)
        (frame-parameter frame 'name)
        (frame-parameter frame 'background-mode)))
"##,
        expect,
    )
}

#[test]
fn div_cx335_save_window_excursion_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((n-before (length (window-list))))
  (save-window-excursion
    (split-window)
    (let ((n-inside (length (window-list))))
      (split-window)
      (list n-before n-inside (length (window-list)))))
  (length (window-list)))
"##,
        expect,
    )
}

#[test]
fn div_cx335_window_parameter_set_get_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val1 42 \"string\" t (neo-cx335-wp2 . 42) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (set-window-parameter win 'neo-cx335-wp1 :val1)
  (set-window-parameter win 'neo-cx335-wp2 42)
  (set-window-parameter win 'neo-cx335-wp3 "string")
  (let ((v1 (window-parameter win 'neo-cx335-wp1))
        (v2 (window-parameter win 'neo-cx335-wp2))
        (v3 (window-parameter win 'neo-cx335-wp3))
        (all (window-parameters win)))
    (set-window-parameter win 'neo-cx335-wp1 nil)
    (list v1 v2 v3 (consp all)
          (assq 'neo-cx335-wp2 all)
          (window-parameter win 'neo-cx335-wp1))))
"##,
        expect,
    )
}

#[test]
fn div_cx335_generate_new_buffer_name_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"neo-cx335-buf<3>\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create "neo-cx335-buf"))
      (buf-b (get-buffer-create "neo-cx335-buf<2>")))
  (let ((next (generate-new-buffer-name "neo-cx335-buf")))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    next))
"##,
        expect,
    )
}

#[test]
fn div_cx335_buffer_modified_p_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx335-mod*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert "content"))
  (let ((mod-1 (buffer-modified-p buf)))
    (with-current-buffer buf (set-buffer-modified-p nil))
    (let ((mod-2 (buffer-modified-p buf)))
      (prog1 (list mod-1 mod-2)
        (kill-buffer buf)))))
"##,
        expect,
    )
}

#[test]
fn div_cx335_buffer_window_frame_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame))
      (win (selected-window))
      (config (current-window-configuration)))
  (modify-frame-parameters frame '((neo-cx335-mega . "val")))
  (set-window-parameter win 'neo-cx335-mega-wp :wval)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Buffer/window/frame mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (window-configuration-to-register ?c)
      (narrow-to-region 2 18)
      (let ((state (list (frame-parameter frame 'neo-cx335-mega)
                         (window-parameter win 'neo-cx335-mega-wp)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (jump-to-register ?c)
        (modify-frame-parameters frame '((neo-cx335-mega)))
        (set-window-parameter win 'neo-cx335-mega-wp nil)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}
