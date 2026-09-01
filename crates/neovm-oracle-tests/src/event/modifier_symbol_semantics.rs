//! Oracle parity tests for GNU `subr.el' event modifier decomposition.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_event_modifiers_and_basic_type_for_symbolic_mouse_events() {
    let form = r#"
(let ((events '(mouse-1
                down-mouse-1
                drag-mouse-1
                double-mouse-1
                triple-mouse-1
                double-drag-mouse-1
                C-M-down-mouse-2
                S-double-mouse-3
                wheel-up
                C-S-wheel-down)))
  (mapcar
   (lambda (event)
     (list event
           (event-modifiers event)
           (event-basic-type event)
           (get event 'event-symbol-elements)))
   events))"#;
    let expect = expect_test::expect![[
        r#""OK ((mouse-1 (click) mouse-1 (mouse-1 click)) (down-mouse-1 (down) mouse-1 (mouse-1 down)) (drag-mouse-1 (drag) mouse-1 (mouse-1 drag)) (double-mouse-1 (double) mouse-1 (mouse-1 double)) (triple-mouse-1 (triple) mouse-1 (mouse-1 triple)) (double-drag-mouse-1 (double drag) mouse-1 (mouse-1 double drag)) (C-M-down-mouse-2 (meta control down) mouse-2 (mouse-2 meta control down)) (S-double-mouse-3 (shift double) mouse-3 (mouse-3 shift double)) (wheel-up (click) wheel-up (wheel-up click)) (C-S-wheel-down (control shift click) wheel-down (wheel-down control shift click)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_event_modifiers_accept_full_event_lists() {
    let form = r#"
(let* ((w (selected-window))
       (pos (list w 7 '(12 . 34) 99))
       (events (list (list 'double-mouse-1 pos)
                     (list 'triple-down-mouse-2 pos)
                     (list 'drag-mouse-3 pos pos)
                     (list 'C-M-drag-mouse-1 pos pos))))
  (mapcar
   (lambda (event)
     (list (car event)
           (event-modifiers event)
           (event-basic-type event)))
   events))"#;
    let expect = expect_test::expect![[
        r#""OK ((double-mouse-1 (double) mouse-1) (triple-down-mouse-2 (triple down) mouse-2) (drag-mouse-3 (drag) mouse-3) (C-M-drag-mouse-1 (meta control drag) mouse-1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_event_modifiers_ignore_string_events() {
    let form = r#"
(list (event-modifiers "mouse-1")
      (event-basic-type "mouse-1")
      (event-modifiers "")
      (event-basic-type ""))"#;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
