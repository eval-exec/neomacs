//! Complex combo batch 61 — window / display geometry, line-prefix / wrap-prefix,
//! fill-prefix interplay, scroll, line-spacing, cursor-sensor / intangible text,
//! and display margins / fringes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx61_line_prefix_wrap_prefix_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"L1> \" \"  \" \"L2> \" nil 8 #(\"line one\\nline two\\nline three\\n\" 0 7 (wrap-prefix \"  \" line-prefix \"L1> \") 9 16 (line-prefix \"L2> \")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line one\nline two\nline three\n")
  (put-text-property 1 8 'line-prefix "L1> ")
  (put-text-property 1 8 'wrap-prefix "  ")
  (put-text-property 10 17 'line-prefix "L2> ")
  (list (get-text-property 1 'line-prefix)
        (get-text-property 1 'wrap-prefix)
        (get-text-property 10 'line-prefix)
        (get-text-property 10 'wrap-prefix)
        (next-single-property-change 1 'line-prefix)
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx61_intangible_text_and_constrain_to_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 nil nil t 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAAA----BBBB")
  (put-text-property 5 9 'intangible t)
  (goto-char 1)
  (let ((adv1 (condition-case e (forward-char 4) (error :err)))
        (adv2 (condition-case e (forward-char 1) (error :err))))
    (list (point) adv1 adv2
          (get-text-property (point) 'intangible)
          (skip-syntax-forward "w"))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_cursor_intangible_overlay_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'cursor-sensor)
      (with-temp-buffer
        (insert "tangible1 intangible tangible2")
        (let ((ov (make-overlay 11 21)))
          (overlay-put ov 'cursor-intangible t))
        (list (overlay-get (car (overlays-at 15)) 'cursor-intangible)
              (overlay-get (car (overlays-at 15)) 'cursor-sensor-functions)
              (length (overlays-in 1 30)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_display_property_inline_image_placeholder() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil (image :type xpm :file \"pic.xpm\") nil \"IMG\" 8 21)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before PIC after PIC end")
  (put-text-property 8 11 'display '(image :type xpm :file "pic.xpm"))
  (put-text-property 18 21 'display "IMG")
  (list (get-text-property 1 'display)
        (get-text-property 9 'display)
        (get-text-property 12 'display)
        (get-text-property 19 'display)
        (next-single-property-change 1 'display)
        (previous-single-property-change 25 'display)))
"##,
        expect,
    );
}

#[test]
fn div_cx61_window_text_height_and_pixel_dimensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 24 80 23 80 23 1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (window-pixel-width win)
        (window-pixel-height win)
        (window-body-width win)
        (window-body-height win 'pixels)
        (window-text-width)
        (window-text-height)
        (window-mode-line-height win)
        (window-header-line-height win)))
"##,
        expect,
    );
}

#[test]
fn div_cx61_window_edges_and_pixel_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (window-edges win)
        (window-edges win t t)
        (window-inside-edges win)
        (window-inside-pixel-edges win)
        (window-absolute-pixel-edges win)
        (window-scroll-bar-width win)
        (window-fringes win)
        (window-margins win)))
"##,
        expect,
    );
}

#[test]
fn div_cx61_set_window_margins_fringes_scroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil) (0 0 nil nil) (12 . 4) (0 0 nil nil) 5 (nil) (0 0 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((m0 (window-margins win))
        (f0 (window-fringes win)))
    (set-window-margins win 12 4)
    (set-window-fringes win 16 8 t)
    (set-window-hscroll win 5)
    (let ((m1 (window-margins win))
          (f1 (window-fringes win))
          (h1 (window-hscroll win)))
      (set-window-margins win (car m0) (cdr m0))
      (set-window-fringes win (car f0) (cadr f0) (car (cddr f0)))
      (set-window-hscroll win 0)
      (list m0 f0 m1 f1 h1
            (window-margins win)
            (window-fringes win)))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_line_spacing_and_line_height_text_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 nil nil 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\n")
  (let ((default (default-value 'line-spacing)))
    (setq line-spacing 4)
    (let ((sp1 (buffer-local-value 'line-spacing (current-buffer))))
      (setq line-spacing nil)
      (list sp1
            (buffer-local-value 'line-spacing (current-buffer))
            default
            (line-pixel-height)
            (default-line-height)))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_posn_at_point_and_pixel_col_row() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (0 . 0) nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "first line\nsecond line\nthird line\n")
  (goto-char 15)
  (let ((posn (posn-at-point (point))))
    (list (posn-point posn)
          (posn-col-row posn)
          (posn-actual-col-row posn)
          (posn-window posn)
          (posn-area posn)
          (posn-string posn)
          (posn-object posn)
          (posn-image posn))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_window_scroll_functions_and_redisplay_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (let ((hook (lambda (win start) (push (list (window-start win) start) fired))))
    (add-hook 'window-scroll-functions hook nil t)
    (with-temp-buffer
      (insert (mapconcat #'identity (make-list 200 "line of text") "\n"))
      (goto-char (point-min))
      (set-window-start (selected-window) (point-min))
      (sit-for 0)
      (forward-line 50)
      (set-window-start (selected-window) (point))
      (sit-for 0))
    (remove-hook 'window-scroll-functions hook t)
    (nreverse fired)))
"##,
        expect,
    );
}

#[test]
fn div_cx61_temp_buffer_resize_and_window_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((config (current-window-configuration)))
      (let ((buf (with-temp-buffer
                   (insert "x")
                   (set-window-buffer (split-window) (current-buffer))
                   (current-buffer))))
        (let ((n-before (length (window-list)))
              (n-after 0))
          (delete-other-windows)
          (setq n-after (length (window-list)))
          (list n-before n-after
                (eq (current-buffer) (window-buffer (selected-window)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_window_point_marker_and_special_window_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 1 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx61-wp*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ"))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 10)
  (let ((m (make-marker)))
    (set-marker m 20 buf)
    (set-window-point (selected-window) m)
    (let ((wp (window-point (selected-window)))
          (start (window-start (selected-window))))
      (prog1 (list wp start (marker-position m))
        (set-window-buffer (selected-window) (other-buffer))
        (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_window_display_table_buffer_display_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([88] [124 10] t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((wdt (make-display-table)))
      (aset wdt ?A [?X])
      (aset wdt ?\n [?| ?\n])
      (with-temp-buffer
        (insert "A\nBC\n")
        (setq buffer-display-table wdt)
        (let ((got (buffer-local-value 'buffer-display-table (current-buffer))))
          (list (if got (aref got ?A))
                (if got (aref got ?\n))
                (eq got wdt)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_selective_display_and_invisible_text_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"visible\thidden\\nvisib\" \"visible\thidden\\nvisib\" \"visible\thidden\\nvisible2\thidden2\\n\" 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "visible\thidden\nvisible2\thidden2\n")
  (let ((ss1 (buffer-substring 1 21)))
    (setq selective-display 4)
    (let ((ss2 (buffer-substring 1 21)))
      (setq selective-display nil)
      (list ss1 ss2 (buffer-string) (current-column)))))
"##,
        expect,
    );
}

#[test]
fn div_cx61_overlay_invisibility_spec_buffer_local_combination_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "01234567890123456789")
  (let ((spec0 buffer-invisibility-spec))
    (add-to-invisibility-spec 'hide)
    (add-to-invisibility-spec '(border . t))
    (let ((ov1 (make-overlay 2 6)))
      (overlay-put ov1 'invisible 'hide))
    (let ((ov2 (make-overlay 10 14)))
      (overlay-put ov2 'invisible 'border))
    (let ((v1 (buffer-substring 1 21))
          (v2 (buffer-substring-no-properties 1 21)))
      (remove-from-invisibility-spec 'hide)
      (let ((v3 (buffer-substring 1 21)))
        (list v1 v2 v3 spec0 buffer-invisibility-spec
              (length (overlays-in 1 21))
              (get-char-property 3 'invisible)
              (get-char-property 11 'invisible)))))
"##,
        expect,
    );
}
