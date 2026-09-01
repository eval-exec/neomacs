//! widget / custom coverage (currently faithful).
//!
//! Probes defcustom/custom-variable-p/custom-type/standard-value,
//! widget-create (editable-field, item) + widget-get/value/apply/delete,
//! define-widget + widget-put/get on a type, built-in widget-type predicates,
//! and custom-facep. All run under --batch (widget-create works headless here).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

fn _u() {}

#[test]
fn div_wc_defcustom_custom_variable_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    _u();
    let expect = expect_test::expect![[r#""OK (((funcall #'(closure (t) nil 5))) nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (defcustom neo-wc-dc 5 "doc")
           (list (custom-variable-p 'neo-wc-dc)
                 (get 'neo-wc-dc 'custom-type)
                 (consp (get 'neo-wc-dc 'standard-value))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_wc_widget_create_editable_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((default :convert-widget widget-value-convert-widget :keymap (keymap (5 . widget-end-of-line) (11 . widget-kill-line) (13 . widget-field-activate) (touchscreen-begin . widget-button-click) (down-mouse-1 . widget-button-click) (down-mouse-2 . widget-button-click) (backtab . widget-backward) (S-tab . widget-backward) (27 keymap (9 . widget-complete)) (9 . widget-forward)) :format \"%v\" :help-echo \"M-TAB: complete field; RET: enter value\" :value \"\" :prompt-internal widget-field-prompt-internal :prompt-history widget-field-history :prompt-value widget-field-prompt-value :action widget-field-action :validate widget-field-validate :valid-regexp \"\" :error \"Field's value doesn't match allowed forms\" :value-create widget-field-value-create :value-set widget-field-value-set :value-delete widget-field-value-delete :value-get widget-field-value-get :match widget-field-match) \"hi\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((w (widget-create 'editable-field :value "hi")))
        (list (widgetp w) (widget-value w))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_wc_widget_get_put_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"T\" 99 \"xval\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((w (widget-create 'item :value "xval" :tag "T")))
        (widget-put w :neo-prop 99)
        (list (widget-get w :tag) (widget-get w :neo-prop) (widget-value w))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_wc_widget_apply_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :done""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((w (widget-create 'item :value "y")))
        (widget-apply w :delete)
        :done))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_wc_define_widget_type_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-widget 'neo-wc-item 'item "custom widget")
      (widget-put 'neo-wc-item :neo-prop 99)
      (list (widget-get 'neo-wc-item :neo-prop)
            (widget-get 'neo-wc-item :tag)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_wc_widget_type_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        // `widgetp` returns the raw `widget-type` plist (GNU wid-edit.el
        // widgetp), whose printed form embeds byte-compile state (an .elc
        // docstring pair inside a byte-code object) — environment-bound and
        // unnormalizable. The predicate CONTRACT is non-nil/nil, so lock the
        // boolean projection instead.
        r##"
(list (and (widgetp 'editable-field) t) (and (widgetp 'item) t)
      (and (widgetp 'button) t) (and (widgetp 'menu-choice) t)
      (and (widgetp 'checkbox) t) (and (widgetp 'toggle) t))
"##,
        expect,
    );
}

#[test]
fn div_wc_custom_facep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] nil [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (custom-facep 'default) (custom-facep 'bold)
      (custom-facep 'nonexistent-face) (facep 'default))
"##,
        expect,
    );
}

#[test]
fn div_wc_widget_child_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . invalid-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((w (widget-create 'editable-field :value "abc")))
        (list (length (widget-value w))
              (widget-apply w :value))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_wc_defface_custom_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((t :foreground \"red\")) [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] \"red\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defface neo-wc-face '((t :foreground "red")) "doc")
  (list (get 'neo-wc-face 'face-defface-spec)
        (facep 'neo-wc-face)
        (face-attribute 'neo-wc-face :foreground)))
"##,
        expect,
    );
}
