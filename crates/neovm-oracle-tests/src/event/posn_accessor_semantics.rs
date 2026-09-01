//! Oracle parity tests for GNU `subr.el' event and posn accessors.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_event_start_end_click_drag_and_touchscreen_shapes() {
    let form = r#"
(let* ((w (selected-window))
       (start (list w 11 '(12 . 34) 99 nil 11 '(3 . 4)))
       (end (list w 22 '(56 . 78) 100 nil 22 '(5 . 6)))
       (click (list 'mouse-1 start))
       (drag (list 'drag-mouse-1 start end))
       (touch (list 'touchscreen-begin (cons 7 start))))
  (list
   (eq (event-start click) start)
   (eq (event-end click) start)
   (eq (event-start drag) start)
   (eq (event-end drag) end)
   (eq (event-start touch) start)
   (eq (event-end touch) start)
   (event-click-count click)
   (event-click-count (list 'double-mouse-1 start 2))
   (event-line-count (list 'wheel-up start nil 4))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t 1 2 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_event_accessors_ignore_touchscreen_update_posn_payloads() {
    let form = r#"
(let* ((w (selected-window))
       (start (list w 11 '(12 . 34) 99 nil 11 '(3 . 4)))
       (end (list w 22 '(56 . 78) 100 nil 22 '(5 . 6)))
       (update (list 'touchscreen-update
                     (list (cons 1 start) (cons 2 end))))
       (nonpos (list 'mouse-1 nil 'not-a-posn))
       (drag-with-count (list 'drag-mouse-1 start 3 end))
       (wheel-with-bad-count (list 'wheel-up start nil 'four))
       (wheel-with-count (list 'wheel-up start nil 4)))
  (list
   ;; GNU `event-start' and `event-end' intentionally ignore
   ;; touchscreen-update payloads and fall back to point.
   (posnp (event-start update))
   (posnp (event-end update))
   (eq (event-end nonpos) nil)
   (posn-point (event-start nil))
   (posn-x-y (event-start nil))
   (event-click-count nil)
   (event-click-count drag-with-count)
   (event-line-count nil)
   (event-line-count wheel-with-bad-count)
   (event-line-count wheel-with-count)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil 1 (0 . 0) 1 3 1 1 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_posn_accessors_prefer_documented_slots() {
    let form = r#"
(let* ((w (selected-window))
       (string-pos (cons "abc" 1))
       (pos (list w '(mode-line . 7) '(12 . 34) 99 string-pos 42 '(3 . 4) '(0 . 0))))
  (list
   (windowp (posn-window pos))
   (eq (posn-window pos) w)
   (posn-area pos)
   (posn-point pos)
   (posn-x-y pos)
   (posn-timestamp pos)
   (posn-string pos)
   (posn-actual-col-row pos)
   (posn-image pos)
   (posn-object pos)))"#;
    let expect = expect_test::expect![[
        r#""OK (t t mode-line 42 (12 . 34) 99 (\"abc\" . 1) (3 . 4) (0 . 0) (0 . 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_window_print_includes_live_buffer_name() {
    let form = r#"
(prin1-to-string (selected-window))"#;
    let expect = expect_test::expect![[r##""OK \"#<window 1 on *scratch*>\"""##]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_window_print_in_nested_structures_includes_live_buffer_name() {
    let form = r#"
(let ((w (selected-window)))
  (list
   (prin1-to-string (list w))
   (prin1-to-string (vector w))
   (prin1-to-string (cons w w))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"(#<window 1 on *scratch*>)\" \"[#<window 1 on *scratch*>]\" \"(#<window 1 on *scratch*> . #<window 1 on *scratch*>)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_posnp_recognizes_only_current_window_posn_shape() {
    let form = r#"
(let* ((w (selected-window))
       (text-pos (list w 7 '(12 . 34) 99))
       (area-pos (list w '(mode-line . 7) '(12 . 34) 99))
       (missing-timestamp (list w 7 '(12 . 34)))
       (bad-window (list 'not-window 7 '(12 . 34) 99)))
  (list
   (posnp text-pos)
   (posnp area-pos)
   (posnp missing-timestamp)
   (posnp bad-window)
   (posnp nil)
   (posn-point (list w '(mode-line . 17) '(1 . 2) 3))
   (posn-point (list w 'vertical-scroll-bar '(1 . 2) 3))))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil nil nil mode-line nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
