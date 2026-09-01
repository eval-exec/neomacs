//! Complex combo batch 298 — `window` scrolling/recenter/pos-visible/
//! set-window-margins/set-window-fringes/set-window-hscroll round trips,
//! `other-window` navigation, `split-window` with size+side arguments.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx298_set_window_margins_fringes_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((nil) (0 0 nil nil) (12 . 4) (0 0 nil nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((m0 (window-margins win))
        (f0 (window-fringes win)))
    (set-window-margins win 12 4)
    (set-window-fringes win 16 8 t)
    (let ((m1 (window-margins win))
          (f1 (window-fringes win)))
      (set-window-margins win (car m0) (cdr m0))
      (set-window-fringes win (car f0) (cadr f0) (car (cddr f0)))
      (list m0 f0 m1 f1))))
"##,
        expect,
    )
}

#[test]
fn div_cx298_window_hscroll_set_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (50 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx298-hs*")))
  (with-current-buffer buf
    (insert (make-string 200 ?x)))
  (set-window-buffer (selected-window) buf)
  (set-window-hscroll (selected-window) 50)
  (let ((h1 (window-hscroll)))
    (set-window-hscroll (selected-window) 0)
    (let ((h2 (window-hscroll)))
      (prog1 (list h1 h2)
        (kill-buffer buf)))))
"##,
        expect,
    )
}

#[test]
fn div_cx298_pos_visible_in_window_p_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx298-pv*")))
  (with-current-buffer buf
    (insert (mapconcat #'identity (make-list 50 "content line") "\n")))
  (set-window-buffer (selected-window) buf)
  (set-window-start (selected-window) 1)
  (let ((vis1 (pos-visible-in-window-p 1))
        (vis-end (pos-visible-in-window-p (point-max))))
    (prog1 (list vis1 vis-end)
      (kill-buffer buf))))
"##,
        expect,
    )
}

#[test]
fn div_cx298_split_window_with_size_and_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((n-before (length (window-list))))
  (let ((win (split-window nil 10 'below)))
    (let ((n-after-split (length (window-list))))
      (delete-window win)
      (let ((n-after-delete (length (window-list))))
        (list n-before n-after-split n-after-delete
              (>= n-after-split (1+ n-before))
              (= n-after-delete n-before)))))
"##,
        expect,
    )
}

#[test]
fn div_cx298_window_vscroll_set_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (set-window-vscroll win 10 t)
  (let ((v1 (window-vscroll win t)))
    (set-window-vscroll win 0 t)
    (let ((v2 (window-vscroll win t)))
      (list v1 v2))))
"##,
        expect,
    )
}

#[test]
fn div_cx298_window_dedicated_p_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((before (window-dedicated-p win)))
    (set-window-dedicated-p win t)
    (let ((after-set (window-dedicated-p win)))
      (set-window-dedicated-p win nil)
      (let ((after-clear (window-dedicated-p win)))
        (list before after-set after-clear))))
"##,
        expect,
    )
}

#[test]
fn div_cx298_window_parameters_get_set_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val1 42 \"string\" t (neo-cx298-p2 . 42) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (set-window-parameter win 'neo-cx298-p1 :val1)
  (set-window-parameter win 'neo-cx298-p2 42)
  (set-window-parameter win 'neo-cx298-p3 "string")
  (let ((v1 (window-parameter win 'neo-cx298-p1))
        (v2 (window-parameter win 'neo-cx298-p2))
        (v3 (window-parameter win 'neo-cx298-p3))
        (all (window-parameters win)))
    (set-window-parameter win 'neo-cx298-p1 nil)
    (list v1 v2 v3 (consp all)
          (assq 'neo-cx298-p2 all)
          (window-parameter win 'neo-cx298-p1))))
"##,
        expect,
    )
}

#[test]
fn div_cx298_fit_window_to_buffer_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'fit-window-to-buffer)
      (fboundp 'shrink-window-if-larger-than-buffer)
      (fboundp 'balance-windows)
      (fboundp 'balance-windows-area))
"##,
        expect,
    )
}

#[test]
fn div_cx298_window_scroll_bar_width_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (integerp (window-scroll-bar-width win))
          (consp (window-scroll-bars win))))
"##,
        expect,
    )
}

#[test]
fn div_cx298_window_ops_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (set-window-parameter win 'neo-cx298-mega :win-val)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Window scrolling mega test buffer content here for testing")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 15))
          (ov (make-overlay 5 25)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (set-window-start (selected-window) 1)
      (narrow-to-region 2 40)
      (let ((state (list (window-parameter win 'neo-cx298-mega)
                         (window-hscroll)
                         (window-start)
                         (window-dedicated-p win)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (set-window-parameter win 'neo-cx298-mega nil)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
