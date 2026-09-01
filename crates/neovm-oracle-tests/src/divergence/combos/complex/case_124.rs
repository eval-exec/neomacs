//! Complex combo batch 124 — `event` / `mouse` / `drag` / `mouse-wheel`
//! / `menu-item` / `tool-bar` / tab-bar interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx124_event_basic_type_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (eventp ?a)
      (eventp 'C-a)
      (eventp '(control ?a))
      (eventp 'mouse-1)
      (eventp '(mouse-1))
      (eventp 'wrong-event-xyz))
"##,
        expect,
    );
}

#[test]
fn div_cx124_event_modifiers_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 nil) ((control 97) nil) ((meta 97) nil) ((control meta 97) nil) ((shift control 97) nil) ((hyper super 97) nil) (C-return (control)) (M-return (meta)) (C-M-return (meta control)) (mouse-1 (click)) (M-mouse-1 (meta click)) (C-down-mouse-1 (control down)) ((mouse-1) (click)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (e) (list e (event-modifiers e)))
        '(?a
          (control ?a)
          (meta ?a)
          (control meta ?a)
          (shift control ?a)
          (hyper super ?a)
          C-return
          M-return
          C-M-return
          mouse-1
          M-mouse-1
          C-down-mouse-1
          (mouse-1)))
"##,
        expect,
    );
}

#[test]
fn div_cx124_event_basic_type_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 97) (C-a nil) ((control 97) nil) (M-a nil) (C-M-a nil) (return return) (C-return nil) (M-return return) (mouse-1 mouse-1) (M-mouse-1 mouse-1) (C-down-mouse-1 mouse-1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (e) (list e (event-basic-type e)))
        '(?a
          C-a
          (control ?a)
          M-a
          C-M-a
          return
          C-return
          M-return
          mouse-1
          M-mouse-1
          C-down-mouse-1))
"##,
        expect,
    );
}

#[test]
fn div_cx124_event_convert_to_lost_focus() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 134217729 33554433 197132289)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (event-convert-list '(control ?a))
          (event-convert-list '(control meta ?a))
          (event-convert-list '(shift control ?a))
          (event-convert-list '(control meta shift hyper super alt ?a)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx124_mouse_event_structure_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((click-event (list 'mouse-1
                             (posn-make (selected-window)
                                        '(0 . 0)
                                        (selected-window)
                                        1))))
      (list (event-start click-event)
            (posn-window (event-start click-event))
            (posn-point (event-start click-event))
            (posn-col-row (event-start click-event))
            (posn-actual-col-row (event-start click-event))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx124_drag_event_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((drag-event (list 'drag-mouse-1
                            (posn-make (selected-window) '(0 . 0) (selected-window) 1)
                            (posn-make (selected-window) '(50 . 50) (selected-window) 25))))
      (list (event-start drag-event)
            (event-end drag-event)
            (posn-point (event-start drag-event))
            (posn-point (event-end drag-event))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx124_mouse_position_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((mp (mouse-position)))
      (list (consp mp)
            (framep (car mp))
            (frame-live-p (car mp))
            (or (null (cadr mp)) (integerp (cadr mp)))
            (or (null (cddr mp)) (integerp (cddr mp)))
            (consp (cddr mp))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx124_menu_item_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'define-key-after)
          (fboundp 'lookup-key)
          (fboundp 'menu-item)
          (fboundp 'easy-menu-define)
          (fboundp 'easy-menu-do-define))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx124_tool_bar_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tool-bar-add-item)
          (fboundp 'tool-bar-add-item-from-menu)
          (boundp 'tool-bar-map)
          (boundp 'tool-bar-mode))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx124_tab_bar_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tab-bar-mode)
          (fboundp 'tab-new)
          (boundp 'tab-bar-show)
          (boundp 'tab-bar-tab-name))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx124_mouse_wheel_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'mwheel-scroll)
          (boundp 'mouse-wheel-scroll-amount)
          (boundp 'mouse-wheel-progressive-speed)
          (boundp 'mouse-wheel-follow-mouse))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx124_event_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function posn-make)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((event (list 'mouse-1
                   (posn-make (selected-window)
                              '(0 . 0)
                              (selected-window)
                              1))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Event mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (event-basic-type event)
                         (event-modifiers event)
                         (posn-point (event-start event))
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
