//! Complex combo batch 155 — `frame` / `window` parameter persistence
//! across `with-selected-frame` / `with-selected-window`,
//! `window-state-get`/`window-state-put` round-trips.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx155_with_selected_window_preserves_origin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((origin (selected-window)))
  (with-selected-window origin
    (list (eq (selected-window) origin))))
"##,
        expect,
    );
}

#[test]
fn div_cx155_with_selected_frame_preserves_origin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((origin (selected-frame)))
  (with-selected-frame origin
    (list (eq (selected-frame) origin))))
"##,
        expect,
    );
}

#[test]
fn div_cx155_window_state_get_put_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((state (window-state-get)))
      (list (consp state)
            (window-state-p state)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx155_window_parameters_get_set_per_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (set-window-parameter win 'neo-cx155-param :val)
  (let ((got (window-parameter win 'neo-cx155-param)))
    (set-window-parameter win 'neo-cx155-param nil)
    (list got (window-parameter win 'neo-cx155-param))))
"##,
        expect,
    );
}

#[test]
fn div_cx155_frame_parameters_get_set_per_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"v1\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (modify-frame-parameters frame '((neo-cx155-frame-param . "v1")))
  (let ((got (frame-parameter frame 'neo-cx155-frame-param)))
    (modify-frame-parameters frame '((neo-cx155-frame-param)))
    (list got (frame-parameter frame 'neo-cx155-frame-param))))
"##,
        expect,
    );
}

#[test]
fn div_cx155_window_live_p_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (window-live-p win)
        (window-valid-p win)
        (windowp win)))
"##,
        expect,
    );
}

#[test]
fn div_cx155_frame_live_p_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments framep 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (frame-live-p frame)
        (framep frame)
        (framep frame t)))
"##,
        expect,
    );
}

#[test]
fn div_cx155_window_total_size_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (integerp (window-total-width win))
        (integerp (window-total-height win))
        (integerp (window-body-width win))
        (integerp (window-body-height win))))
"##,
        expect,
    );
}

#[test]
fn div_cx155_window_scroll_bar_width_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (integerp (window-scroll-bar-width win))
        (consp (window-fringes win))
        (consp (window-margins win))))
"##,
        expect,
    );
}

#[test]
fn div_cx155_window_combination_resize_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (fboundp 'window-resize)
        (fboundp 'window-resize-no-error)
        (fboundp 'adjust-window-trailing-edge)))
"##,
        expect,
    );
}

#[test]
fn div_cx155_window_buffer_swap_via_set_window_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx155-a*"))
      (buf-b (get-buffer-create " *neo-cx155-b*"))
      (win (selected-window)))
  (with-current-buffer buf-a (insert "AAA"))
  (with-current-buffer buf-b (insert "BBB"))
  (set-window-buffer win buf-a)
  (let ((a-in-win (eq (window-buffer win) buf-a)))
    (set-window-buffer win buf-b)
    (let ((b-in-win (eq (window-buffer win) buf-b)))
      (prog1 (list a-in-win b-in-win)
        (kill-buffer buf-a)
        (kill-buffer buf-b))))
"##,
        expect,
    );
}

#[test]
fn div_cx155_window_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window))
      (frame (selected-frame)))
  (modify-frame-parameters frame '((neo-cx155-mega-param . "value")))
  (set-window-parameter win 'neo-cx155-mega-win :wval)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Window/frame mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (frame-parameter frame 'neo-cx155-mega-param)
                         (window-parameter win 'neo-cx155-mega-win)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (modify-frame-parameters frame '((neo-cx155-mega-param)))
        (set-window-parameter win 'neo-cx155-mega-win nil)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
