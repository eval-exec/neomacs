//! Complex combo batch 214 — `window` configuration / `window-state` /
//! `window-combination` / `split-window` / `delete-window` /
//! `balance-windows` operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx214_split_window_and_delete_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((n-before (length (window-list))))
  (let ((win (split-window)))
    (let ((n-after-split (length (window-list))))
      (delete-window win)
      (let ((n-after-delete (length (window-list))))
        (list n-before n-after-split n-after-delete
              (>= n-after-split (1+ n-before))
              (= n-after-delete n-before)))))
"##,
        expect,
    );
}

#[test]
fn div_cx214_window_configuration_save_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((config (current-window-configuration))
      (n-before (length (window-list))))
  (split-window)
  (split-window)
  (let ((n-split (length (window-list))))
    (set-window-configuration config)
    (let ((n-restored (length (window-list))))
      (list n-before n-split n-restored
            (= n-before n-restored)))))
"##,
        expect,
    );
}

#[test]
fn div_cx214_window_state_get_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (((min-height . 4) (min-width . 10) (min-height-ignore . 2) (min-width-ignore . 2) (min-height-safe . 1) (min-width-safe . 2) (min-pixel-height . 4) (min-pixel-width . 10) (min-pixel-height-ignore . 2) (min-pixel-width-ignore . 2) (min-pixel-height-safe . 1) (min-pixel-width-safe . 2)) leaf (pixel-width . 80) (pixel-height . 24) (total-width . 80) (total-height . 24) (normal-height . 1.0) (normal-width . 1.0) (parameters (clone-of . #<window 1 on *scratch*>)) (buffer #<buffer *scratch*> (selected . t) (hscroll . 0) (fringes 0 0 nil nil) (margins nil) (scroll-bars nil 0 t nil 0 t nil) (vscroll . 0) (dedicated) (point . #<marker at 1 in *scratch*>) (start . #<marker at 1 in *scratch*>))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((state (window-state-get)))
      (list (consp state)
            (window-state-get)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx214_save_window_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((n-before (length (window-list))))
  (save-window-excursion
    (split-window)
    (let ((n-inside (length (window-list))))
      (split-window)
      (let ((n-inside-2 (length (window-list))))
        (list n-before n-inside n-inside-2))))
  (let ((n-after (length (window-list))))
    n-after))
"##,
        expect,
    );
}

#[test]
fn div_cx214_window_edges_pixel_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (window-edges win)
        (window-inside-edges win)
        (window-pixel-edges win)
        (window-inside-pixel-edges win)
        (window-absolute-pixel-edges win)))
"##,
        expect,
    );
}

#[test]
fn div_cx214_window_margins_fringes_scroll_bar_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((nil) (0 0 nil nil) 0 (nil 0 t nil 0 t nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (window-margins win)
        (window-fringes win)
        (window-scroll-bar-width win)
        (window-scroll-bars win)))
"##,
        expect,
    );
}

#[test]
fn div_cx214_balance_windows_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'balance-windows)
      (fboundp 'balance-windows-area)
      (fboundp 'window-tree)
      (fboundp 'window-combined-p)
      (fboundp 'window-parent))
"##,
        expect,
    );
}

#[test]
fn div_cx214_window_tree_structure_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t #<window 1 on *scratch*> t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tree (window-tree)))
  (list (consp tree)
        (car tree)
        (>= (length tree) 2)))
"##,
        expect,
    );
}

#[test]
fn div_cx214_get_buffer_window_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (current-buffer)))
  (list (consp (get-buffer-window-list buf))
        (windowp (get-buffer-window buf))
        (eq (get-buffer-window buf) (selected-window))))
"##,
        expect,
    );
}

#[test]
fn div_cx214_window_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((n-before (length (window-list)))
      (config (current-window-configuration)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Window config mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (window-configuration-to-register ?c)
      (narrow-to-region 2 18)
      (split-window)
      (let ((n-split (length (window-list))))
        (let ((state (list n-before n-split
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (delete-other-windows)
          (jump-to-register ?c)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))))
"##,
        expect,
    );
}
