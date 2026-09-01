//! Complex combo batch 201 — `widget` deep: editable-field, checkbox,
//! radio-button-choice, menu-choice, tree-widget, item with validation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx201_widget_create_editable_field_with_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((default :convert-widget widget-value-convert-widget :keymap (keymap (5 . widget-end-of-line) (11 . widget-kill-line) (13 . widget-field-activate) (touchscreen-begin . widget-button-click) (down-mouse-1 . widget-button-click) (down-mouse-2 . widget-button-click) (backtab . widget-backward) (S-tab . widget-backward) (27 keymap (9 . widget-complete)) (9 . widget-forward)) :format \"%v\" :help-echo \"M-TAB: complete field; RET: enter value\" :value \"\" :prompt-internal widget-field-prompt-internal :prompt-history widget-field-history :prompt-value widget-field-prompt-value :action widget-field-action :validate widget-field-validate :valid-regexp \"\" :error \"Field's value doesn't match allowed forms\" :value-create widget-field-value-create :value-set widget-field-value-set :value-delete widget-field-value-delete :value-get widget-field-value-get :match widget-field-match) \"initial\" 30 \"^[a-z]+$\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((w (widget-create 'editable-field
                               :value "initial"
                               :size 30
                               :format "Prompt: %v"
                               :valid-regexp "^[a-z]+$"
                               :help-echo "Enter lowercase")))
        (list (widgetp w)
              (widget-value w)
              (widget-get w :size)
              (widget-get w :valid-regexp))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_checkbox_toggle_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((chk (widget-create 'checkbox)))
        (let ((v1 (widget-value chk)))
          (widget-apply chk :toggle)
          (let ((v2 (widget-value chk)))
            (widget-apply chk :toggle)
            (let ((v3 (widget-value chk)))
              (list (widgetp chk) v1 v2 v3))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_radio_button_choice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((rb (widget-create 'radio-button-choice
                                :value :b
                                :help-echo "Choose one"
                                '(:a) '(:b) '(:c))))
        (list (widgetp rb)
              (widget-value rb)
              (widget-apply rb :complete))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_menu_choice_with_items() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((default :convert-widget widget-types-convert-widget :copy widget-types-copy :format \"%[%t%]: %v\" :case-fold t :tag \"choice\" :void (item :format \"invalid (%t)\\n\") :value-create widget-choice-value-create :value-get widget-child-value-get :value-inline widget-child-value-inline :default-get widget-choice-default-get :mouse-down-action widget-choice-mouse-down-action :action widget-choice-action :error \"Make a choice\" :validate widget-choice-validate :match widget-choice-match :match-inline widget-choice-match-inline) :b)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((mc (widget-create 'menu-choice
                                :value :b
                                :help-echo "Select"
                                '(item :a) '(item :b) '(item :c))))
        (list (widgetp mc)
              (widget-value mc))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_default_format_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'widget-push-button)
          (boundp 'widget-editable-list)
          (boundp 'widget-image)
          (boundp 'widget-menu-max-short))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_field_constraints_and_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((w1 (widget-create 'editable-field :value "field1"))
            (w2 (widget-create 'editable-field :value "field2")))
        (widget-forward 1)
        (let ((at-w2 (eq (widget-at) w2)))
          (widget-backward 1)
          (let ((at-w1 (eq (widget-at) w1)))
            (list at-w1 at-w2)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_item_with_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((default :convert-widget widget-value-convert-widget :value-create widget-item-value-create :value-delete ignore :value-get widget-value-value-get :match widget-item-match :match-inline widget-item-match-inline :action widget-item-action :format \"%t\\n\") \"Static label text\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((it (widget-create 'item :value "Static label text")))
        (list (widgetp it)
              (widget-value it))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_text_create_with_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((editable-field :format \"%{%t%}: %v\" :keymap (keymap (5 . widget-end-of-line) (13 . widget-button-press) (touchscreen-begin . widget-button-click) (down-mouse-1 . widget-button-click) (down-mouse-2 . widget-button-click) (backtab . widget-backward) (S-tab . widget-backward) (27 keymap (9 . widget-backward)) (9 . widget-forward))) \"line1\\nline2\\nline3\" 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((txt (widget-create 'text
                                 :value "line1\nline2\nline3")))
        (list (widgetp txt)
              (widget-value txt)
              (length (split-string (widget-value txt) "\n")))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_apply_set_get_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((w (widget-create 'editable-field :value "before")))
        (widget-value-set w "after")
        (list (widget-value w)
              (widget-apply w :complete))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx201_widget_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'widget)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Widget mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((w (widget-create 'editable-field :value "test")))
            (let ((state (list (widgetp w)
                               (widget-value w)
                               (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
              (undo)
              (widen)
              (list state (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
