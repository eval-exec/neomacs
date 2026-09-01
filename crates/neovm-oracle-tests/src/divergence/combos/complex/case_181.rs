//! Complex combo batch 181 — `window` scrolling / recentering /
//! `set-window-start` / `set-window-hscroll` / `window-start` /
//! `pos-visible-in-window-p` queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx181_window_start_end_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx181-ws*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert (mapconcat #'identity (make-list 50 "line of content") "\n")))
  (set-window-buffer (selected-window) buf)
  (set-window-start (selected-window) 100)
  (let ((start (window-start))
        (end (window-end nil t)))
    (prog1 (list (integerp start) (integerp end) (> end start))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx181_set_window_hscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (50 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx181-hs*")))
  (with-current-buffer buf
    (insert (make-string 200 ?x)))
  (set-window-buffer (selected-window) buf)
  (set-window-hscroll (selected-window) 50)
  (let ((h (window-hscroll)))
    (set-window-hscroll (selected-window) 0)
    (prog1 (list h (window-hscroll))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx181_recenter_top_bottom_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"‘recenter’ing a window that does not display current-buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx181-rc*")))
  (with-current-buffer buf
    (insert (mapconcat #'identity (make-list 100 "content line") "\n")))
  (set-window-buffer (selected-window) buf)
  (goto-char 500)
  (recenter 0)
  (let ((at-top (window-start)))
    (recenter -1)
    (let ((at-bottom (window-start)))
      (recenter)
      (list (integerp at-top) (integerp at-bottom) (>= at-bottom at-top)
            (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx181_pos_visible_in_window_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx181-pv*")))
  (with-current-buffer buf
    (insert (mapconcat #'identity (make-list 50 "content") "\n")))
  (set-window-buffer (selected-window) buf)
  (set-window-start (selected-window) 1)
  (let ((vis1 (pos-visible-in-window-p 1))
        (vis50 (pos-visible-in-window-p (point-max))))
    (prog1 (list vis1 vis50)
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx181_window_text_pixel_size_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-text-pixel-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (integerp (window-text-pixel-width win))
        (integerp (window-text-pixel-height win))
        (integerp (window-pixel-width win))
        (integerp (window-pixel-height win))))
"##,
        expect,
    );
}

#[test]
fn div_cx181_window_vscroll() {
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
    );
}

#[test]
fn div_cx181_window_dedicated_p_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((before (window-dedicated-p win)))
    (set-window-dedicated-p win t)
    (let ((after-set (window-dedicated-p win)))
      (set-window-dedicated-p win nil)
      (let ((after-unset (window-dedicated-p win)))
        (list before after-set after-unset)))))
"##,
        expect,
    );
}

#[test]
fn div_cx181_scroll_up_down_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf (get-buffer-create " *neo-cx181-sc*")))
      (with-current-buffer buf
        (insert (mapconcat #'identity (make-list 100 "content line") "\n")))
      (set-window-buffer (selected-window) buf)
      (goto-char 1)
      (set-window-start (selected-window) 1)
      (let ((before (window-start)))
        (condition-case err (scroll-up 5) (error :err))
        (let ((after-up (window-start)))
          (condition-case err (scroll-down 5) (error :err))
          (let ((after-down (window-start)))
            (prog1 (list before after-up after-down)
              (kill-buffer buf)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx181_window_scroll_functions_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (let ((hook (lambda (win start) (push (cons (window-start win) start) fired))))
    (add-hook 'window-scroll-functions hook nil t)
    (let ((buf (get-buffer-create " *neo-cx181-sf*")))
      (with-current-buffer buf
        (insert (mapconcat #'identity (make-list 100 "x") "\n")))
      (set-window-buffer (selected-window) buf)
      (set-window-start (selected-window) 1)
      (sit-for 0)
      (set-window-start (selected-window) 50)
      (sit-for 0)
      (kill-buffer buf))
    (remove-hook 'window-scroll-functions hook t)
    (length fired)))
"##,
        expect,
    );
}

#[test]
fn div_cx181_window_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx181-mega*")))
  (set-window-buffer (selected-window) buf)
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "Window scroll mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (set-window-start (selected-window) 1)
      (narrow-to-region 2 18)
      (let ((state (list (window-start)
                         (window-hscroll)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (kill-buffer buf)
        (list state (buffer-live-p buf)))))
"##,
        expect,
    );
}
