//! Oracle parity tests for GNU `subr.el' event predicate semantics.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_eventp_accepts_integers_and_non_keyword_symbols() {
    let form = r#"
(list
 (eventp ?a)
 (eventp -1)
 (eventp 'mouse-1)
 (eventp '(mouse-1 ignored))
 (eventp nil)
 (eventp t)
 (eventp :keyword)
 (eventp '(:keyword ignored))
 (eventp "mouse-1")
 (eventp '(\"mouse-1\" ignored)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t nil t nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mouse_event_predicates_follow_basic_type() {
    let form = r#"
(let* ((w (selected-window))
       (pos (list w 7 '(12 . 34) 99))
       (events (list 'mouse-1
                     'down-mouse-1
                     'drag-mouse-2
                     'double-mouse-3
                     'mouse-movement
                     (list 'C-M-drag-mouse-1 pos pos)
                     (list 'wheel-up pos)
                     'wheel-up
                     'f1
                     ?a)))
  (mapcar
   (lambda (event)
     (list (if (consp event) (car event) event)
           (mouse-event-p event)
           (mouse-movement-p event)
           (event-basic-type event)))
   events))"#;
    let expect = expect_test::expect![[
        r#""OK ((mouse-1 (mouse-1 mouse-2 mouse-3 mouse-movement) nil mouse-1) (down-mouse-1 (mouse-1 mouse-2 mouse-3 mouse-movement) nil mouse-1) (drag-mouse-2 (mouse-2 mouse-3 mouse-movement) nil mouse-2) (double-mouse-3 nil nil nil) (mouse-movement (mouse-movement) nil mouse-movement) (C-M-drag-mouse-1 (mouse-1 mouse-2 mouse-3 mouse-movement) nil mouse-1) (wheel-up nil nil wheel-up) (wheel-up nil nil wheel-up) (f1 nil nil f1) (97 nil nil 97))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_mouse_movement_p_only_checks_event_car() {
    let form = r#"
(list
 (mouse-movement-p '(mouse-movement))
 (mouse-movement-p '(mouse-movement nil))
 (mouse-movement-p 'mouse-movement)
 (mouse-movement-p nil)
 (mouse-movement-p '(mouse-1))
 (mouse-movement-p '(drag-mouse-1 nil nil)))"#;
    let expect = expect_test::expect![[r#""OK (t t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
