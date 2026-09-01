//! Oracle parity tests for GNU text property plist ordering.
//!
//! Text-property plist order is observable through `text-properties-at` and
//! through printed propertized string syntax.  GNU `textprop.c` has distinct
//! observable order semantics for adding properties and replacing all
//! properties, so these tests pin both paths.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_add_text_properties_preserves_supplied_plist_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (copy-sequence "abc")))
  (add-text-properties
   0 3 '(face help-key-binding font-lock-face help-key-binding) s)
  (list s
        (text-properties-at 0 s)))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"abc\" 0 3 (font-lock-face help-key-binding face help-key-binding)) (font-lock-face help-key-binding face help-key-binding))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_propertize_preserves_supplied_plist_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (propertize "abc"
                     'face 'bold
                     'font-lock-face 'italic
                     'help-echo "tip")))
  (list s
        (text-properties-at 0 s)))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"abc\" 0 3 (face bold font-lock-face italic help-echo \"tip\")) (face bold font-lock-face italic help-echo \"tip\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_format_reverses_copied_format_and_argument_plists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU styled_format copies both property sources through
    // text_property_list -> add_text_properties_from_list ->
    // Fadd_text_properties.  Each pair is prepended to the destination
    // interval, so the copied order is observably reversed even though the
    // original propertized strings retain their supplied plist order.
    let form = r#"
(let* ((arg (propertize "arg" 'a 1 'b 2 'c 3))
       (fmt (propertize "[%s]" 'f1 1 'f2 2 'f3 3))
       (out (format fmt arg)))
  (list
   (text-properties-at 0 arg)
   (text-properties-at 0 fmt)
   (text-properties-at 0 out)
   (text-properties-at 1 out)
   (text-properties-at 4 out)))
"#;

    assert_oracle_parity(form);
}

#[test]
fn oracle_prop_describe_key_briefly_preserves_message_and_insert_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((g (make-sparse-keymap)))
  (define-key g (kbd "C-f") #'forward-char)
  (use-global-map g)
  (list
   (describe-key-briefly (kbd "C-f"))
   (with-temp-buffer
     (describe-key-briefly (kbd "C-f") t)
     (buffer-string))))
"#;

    assert_oracle_parity(form);
}

#[test]
fn oracle_prop_add_text_properties_replacement_keeps_existing_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (propertize "abc" 'a 1 'b 2 'c 3)))
  (add-text-properties 0 3 '(b 20 d 4) s)
  (list s
        (text-properties-at 0 s)))
"#;

    let expect =
        expect_test::expect![[r#""OK (#(\"abc\" 0 3 (d 4 a 1 b 20 c 3)) (d 4 a 1 b 20 c 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_set_text_properties_uses_replacement_plist_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (propertize "abc" 'old 1 'stale 2)))
  (set-text-properties 0 3 '(x 1 y 2 z 3) s)
  (list s
        (text-properties-at 0 s)))
"#;

    let expect = expect_test::expect![[r#""OK (#(\"abc\" 0 3 (x 1 y 2 z 3)) (x 1 y 2 z 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
