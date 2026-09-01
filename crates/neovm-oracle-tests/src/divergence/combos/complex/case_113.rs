//! Complex combo batch 113 — kbd macro / keyboard input / read-key /
//! read-event / read-char-exclusive / unread-command-events interplay.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx113_unread_command_events_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (97 98 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((unread-command-events (list ?a ?b ?c)))
      (list (read-char)
            (read-char)
            (read-char)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_this_command_and_last_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list this-command
      last-command
      (eq this-original-command this-command)
      (boundp 'real-this-command)
      (boundp 'real-last-command))
"##,
        expect,
    );
}

#[test]
fn div_cx113_read_key_sequence_with_kbd() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\u{18}\u{6}\" \"C-x C-f\" 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((unread-command-events (listify-key-sequence (kbd "C-x C-f"))))
      (let ((keys (read-key-sequence nil)))
        (list keys
              (key-description keys)
              (length keys))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_event_start_and_posn_decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((event (list 'mouse-1
                       (posn-make (selected-window)
                                  '(0 . 0)
                                  (selected-window)
                                  1))))
      (list (event-basic-type event)
            (event-modifiers event)
            (event-start event)
            (posn-point (event-start event))
            (posn-window (event-start event))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_kbd_parse_special_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"RET\" \"RET\" 1) (\"TAB\" \"TAB\" 1) (\"ESC\" \"ESC\" 1) (\"DEL\" \"DEL\" 1) (\"SPC\" \"SPC\" 1) (\"NUL\" \"C-@\" 1) (\"<f1>\" \"<f1>\" 1) (\"<f12>\" \"<f12>\" 1) (\"<home>\" \"<home>\" 1) (\"<end>\" \"<end>\" 1) (\"<prior>\" \"<prior>\" 1) (\"<next>\" \"<next>\" 1) (\"<up>\" \"<up>\" 1) (\"<down>\" \"<down>\" 1) (\"<left>\" \"<left>\" 1) (\"<right>\" \"<right>\" 1) (\"C-M-a\" \"C-M-a\" 1) (\"M-x\" \"M-x\" 1) (\"C-c C-c\" \"C-c C-c\" 2) (\"C-u M-x\" \"C-u M-x\" 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (s)
              (list s (key-description (kbd s)) (length (kbd s))))
            '("RET" "TAB" "ESC" "DEL" "SPC" "NUL"
              "<f1>" "<f12>" "<home>" "<end>" "<prior>" "<next>"
              "<up>" "<down>" "<left>" "<right>"
              "C-M-a" "M-x" "C-c C-c" "C-u M-x"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_recent_keys_and_key_history() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((rk (recent-keys)))
      (list (vectorp rk)
            (arrayp rk)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_input_method_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'current-input-method)
          (boundp 'current-input-method)
          (boundp 'default-input-method)
          (boundp 'input-method-verbose-flag))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_translate_key_via_key_translation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([24] nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((tbl (make-sparse-keymap)))
      (define-key tbl [?\C-a] [?\C-x])
      (list (lookup-key tbl [?\C-a])
            (lookup-key tbl [?\C-x])))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_recursive_minibuffer_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (minibuffer-depth)
          (>= (minibuffer-depth) 0))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_input_method_activate_deactivate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'activate-input-method)
          (fboundp 'deactivate-input-method)
          (fboundp 'toggle-input-method)
          (boundp 'input-method-function))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_mouse_position_query() {
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
fn div_cx113_set_transient_map_and_overriding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-cx113-cmd t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((map (make-sparse-keymap)))
      (define-key map "x" 'neo-cx113-cmd)
      (set-transient-map map t (lambda () (message "exited")))
      (list (lookup-key map "x")
            (keymapp map)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx113_input_event_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "Input event mega test buffer content")
      (put-text-property 1 6 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (let ((unread-command-events (list ?X)))
          (let ((state (list (length unread-command-events)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
