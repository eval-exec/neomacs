//! Complex combo batch 367 — `widget`/`button`/`browse-url` ultimate:
//! widget-create editable-field/checkbox/radio/menu/item/text,
//! make-button/insert-button/next-previous-button, browse-url/goto-address/ffap.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx367_widget_create_editable_field_with_validation() {
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
    )
}

#[test]
fn div_cx367_widget_checkbox_toggle_cycle() {
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
    )
}

#[test]
fn div_cx367_widget_radio_button_choice() {
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
    )
}

#[test]
fn div_cx367_widget_menu_choice_with_items() {
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
    )
}

#[test]
fn div_cx367_widget_field_navigation() {
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
    )
}

#[test]
fn div_cx367_button_make_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "Some text content here")
      (make-button 6 10 'action (lambda (b) (message "clicked"))
                   'help-echo "Click"
                   'face 'link
                   'mouse-face 'highlight)
      (let ((btn (button-at 7)))
        (list (buttonp btn)
              (when btn (button-start btn))
              (when btn (button-end btn))
              (when btn (button-get btn 'help-echo))
              (when btn (button-get btn 'face))
              (length (overlays-in 1 20)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx367_button_next_previous_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 16 26 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "text one text two text three text four")
      (make-button 6 9)
      (make-button 16 19)
      (make-button 26 31)
      (goto-char 1)
      (let ((b1 (next-button (point))))
        (let ((b2 (when b1 (next-button (button-start b1)))))
          (let ((b3 (when b2 (next-button (button-start b2)))))
            (let ((back (when b3 (previous-button (button-start b3)))))
              (list (and b1 (button-start b1))
                    (and b2 (button-start b2))
                    (and b3 (button-start b3))
                    (and back (button-start back))))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx367_browse_url_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'browse-url)
      (list (fboundp 'browse-url)
            (fboundp 'browse-url-at-point)
            (boundp 'browse-url-browser-function)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx367_thing_at_point_url_email_filename() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"https://example.com/path\" \"user@example.com\" \"/home/user/file.txt\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer
   (insert "see https://example.com/path for details")
   (goto-char 5)
   (thing-at-point 'url))
 (with-temp-buffer
   (insert "contact user@example.com for info")
   (goto-char 10)
   (thing-at-point 'email))
 (with-temp-buffer
   (insert "edit /home/user/file.txt for changes")
   (goto-char 6)
   (thing-at-point 'filename)))
"##,
        expect,
    )
}

#[test]
fn div_cx367_widget_button_browse_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'widget)
      (require 'button)
      (require 'browse-url)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Widget/button/browse-url mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (make-button 7 13 'action (lambda (_) :clicked) 'face 'link)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((btn (button-at 8)))
            (let ((state (list (buttonp btn)
                               (when btn (button-start btn))
                               (when btn (button-end btn))
                               (fboundp 'browse-url)
                               (fboundp 'widget-create)
                               (buffer-string)
                               (marker-position m)
                               (overlay-start ov) (overlay-end ov)
                               (text-properties-at 1))))
              (undo)
              (widen()
              (list state (buffer-string) (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (text-properties-at 1)))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}
