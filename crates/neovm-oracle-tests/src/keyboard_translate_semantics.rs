//! Oracle parity tests for GNU `subr.el` `keyboard-translate`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_keyboard_translate_creates_table_and_sets_character_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((keyboard-translate-table nil))
   (list (keyboard-translate ?a ?b)
         (char-table-p keyboard-translate-table)
         (char-table-subtype keyboard-translate-table)
         (aref keyboard-translate-table ?a)
         (aref keyboard-translate-table ?b)
         (char-table-range keyboard-translate-table ?a)
         (char-table-range keyboard-translate-table ?b)))
 (let ((keyboard-translate-table 'stale))
   (list (keyboard-translate ?x ?y)
         (char-table-p keyboard-translate-table)
         (char-table-subtype keyboard-translate-table)
         (aref keyboard-translate-table ?x)
         (condition-case e
             (keyboard-translate "x" ?z)
           (error (list (car e) (cadr e)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((98 t keyboard-translate-table 98 nil 98 nil) (121 t keyboard-translate-table 121 (wrong-type-argument fixnump)))""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "((98 t keyboard-translate-table 98 nil 98 nil) (121 t keyboard-translate-table 121 (wrong-type-argument fixnump)))",
        &oracle,
        &neovm,
    );
}

#[test]
fn oracle_key_translate_parses_keys_mutates_table_and_reports_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((keyboard-translate-table nil))
   (list (key-translate "a" "b")
         (char-table-p keyboard-translate-table)
         (char-table-subtype keyboard-translate-table)
         (aref keyboard-translate-table ?a)
         (aref keyboard-translate-table ?b)))
 (let ((keyboard-translate-table nil))
   (key-translate "a" nil)
   (list (char-table-p keyboard-translate-table)
         (aref keyboard-translate-table ?a)))
 (condition-case e
     (key-translate "" "a")
   (error (list (car e) (cadr e))))
 (condition-case e
     (key-translate "a b" "c")
   (error (list (car e) (cadr e))))
 (condition-case e
     (key-translate "a" "b c")
   (error (list (car e) (cadr e))))
 (condition-case e
     (key-translate 42 "a")
   (error (list (car e) (cadr e) (caddr e)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((98 t keyboard-translate-table 98 nil) (t nil) (error \"\\\"\\\" is not a valid key definition; see ‘key-valid-p’\") (error \"FROM key a b is not a single key\") (error \"TO key b c is not a single key\") (error \"42 is not a valid key definition; see ‘key-valid-p’\" nil))""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        r#"((98 t keyboard-translate-table 98 nil) (t nil) (error "\"\" is not a valid key definition; see ‘key-valid-p’") (error "FROM key a b is not a single key") (error "TO key b c is not a single key") (error "42 is not a valid key definition; see ‘key-valid-p’" nil))"#,
        &oracle,
        &neovm,
    );
}

/// GNU prefixes a non-text event by its posn area only when the event
/// head's `event-kind` is `mouse-click` or `touchscreen`
/// (`src/keyboard.c:11403-11406`).  Movement heads have kind
/// `mouse-movement` (`src/keyboard.c:13299-13300`), and the area comes from
/// `EVENT_START` (`src/keyboard.c:11409`): a drag is prefixed by the press
/// area, not the release area.  Menu-bar, tool-bar and tab-bar events take
/// GNU's second branch (`src/keyboard.c:11445-11482`), which does not consult
/// the kind.  Prefixed movement events made `[left-fringe mouse-movement]`
/// undefined (issue 621).
#[test]
fn mouse_area_prefix_is_gated_by_event_kind_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((win (selected-window))
       (xy (cons 10 5))
       (fringe (list win 'left-fringe xy 0 nil 1 xy 8 5))
       (text (list win (point-min) xy 0 nil 1 xy 8 5))
       (mode-line (list win 'mode-line xy 0 nil 1 xy 8 5))
       (menu-bar (list nil 'menu-bar xy 0 nil 1 xy 8 5))
       (tool-bar (list (selected-frame) 'tool-bar xy 0 nil 1 xy 8 5))
       (tab-bar (list (selected-frame) 'tab-bar xy 0 nil 1 xy 8 5))
       (shape (lambda (events)
                (mapcar (lambda (event)
                          (if (symbolp event) event
                            (cons (car event)
                                  (make-list (1- (length event)) 'posn))))
                        (let ((unread-command-events events))
                          (append (read-key-sequence nil) nil))))))
  ;; A fresh batch session never translated a real mouse event, so the
  ;; mouse symbols have no event-kind yet.  Give them GNU's kinds to
  ;; test the gate itself.
  (put 'mouse-movement 'event-kind 'mouse-movement)
  (put 'drag-mouse-1 'event-kind 'mouse-click)
  (list
   (funcall shape (list (list 'mouse-movement fringe)))
   (funcall shape (list (list 'mouse-movement text)))
   (funcall shape (list (list 'drag-mouse-1 text fringe) ?x))
   (funcall shape (list (list 'drag-mouse-1 fringe text) ?x))
   (funcall shape (list (list 'drag-mouse-1 text mode-line) ?x))
   (funcall shape (list (list 'mouse-1 menu-bar) ?x))
   (funcall shape (list (list 'tool-bar-item tool-bar) ?x))
   (funcall shape (list (list 'down-mouse-1 tab-bar) ?x))))"#;
    let (gnu, neomacs) = eval_oracle_and_neovm(form);
    assert_ok_eq(
        "(((mouse-movement posn)) ((mouse-movement posn)) ((drag-mouse-1 posn posn)) (left-fringe (drag-mouse-1 posn posn)) ((drag-mouse-1 posn posn)) (menu-bar (mouse-1 posn)) (tool-bar (tool-bar-item posn)) (tab-bar (down-mouse-1 posn)))",
        &gnu,
        &neomacs,
    );
}
