//! Strict combo oracle probes, batch 9: print options (print-length/level/
//! circle/escape-newlines), prin1 multibyte/escape rules, text-property
//! set/add/remove return values, keymap inheritance via parent, frame/window
//! tree traversal, multi-coding encode lengths, and current-active-maps.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_e4_print_length_level_circle_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(a b c ...)\" \"(a (b ...))\" \"(#1=(1 2) #1#)\" \"\\\"a\\\\nb\tc\\\"\" \"(...)\" \"(... ... ...)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (let ((print-length 3)) (prin1-to-string '(a b c d e f)))
      (let ((print-level 2)) (prin1-to-string '(a (b (c (d (e)))))))
      (let ((print-circle t) (shared (list 1 2)))
        (prin1-to-string (list shared shared)))
      (let ((print-escape-newlines t)) (prin1-to-string "a\nb\tc"))
      (let ((print-length 0)) (prin1-to-string '(1 2 3)))
      (let ((print-level 1)) (prin1-to-string '((a) (b) (c)))))
"##,
        expect,
    );
}

#[test]
fn div_e4_prin1_multibyte_and_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"plain\\\"\" \"\\\"with \\\\\\\"quote\\\\\\\"\\\"\" \"\\\"tab\there\\\"\" \"\\\"café\\\"\" \"\\\"日本\\\"\" \"\\\"a\\\\nb\\\"\" \"\\\"caf\\\\x00e9\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string "plain")
      (prin1-to-string "with \"quote\"")
      (prin1-to-string "tab\there")
      (prin1-to-string "café")
      (prin1-to-string "日本")
      (let ((print-escape-newlines t)) (prin1-to-string "a\nb"))
      (let ((print-escape-multibyte t)) (prin1-to-string "café")))
"##,
        expect,
    );
}

#[test]
fn div_e4_text_property_set_remove_returns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t (face bold) (face bold) t nil t (foo bar baz qux) t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (list (set-text-properties 1 3 '(face bold))
        (text-properties-at 1)
        (text-properties-at 2)
        (remove-text-properties 1 3 '(face))
        (text-properties-at 1)
        (set-text-properties 4 6 '(foo bar baz qux))
        (text-properties-at 4)
        (remove-list-of-text-properties 4 6 '(foo baz))
        (text-properties-at 4)))
"##,
        expect,
    );
}

#[test]
fn div_e4_add_text_properties_multi_prop_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((c 3 b 2 a 1) (weight heavy face bold) (face bold weight heavy) (baz qux foo bar))""#
    ]];
    // Parity lock: add-text-properties adds every property in the plist,
    // regardless of order or whether `face' is among them.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer (insert "abcdef")
        (add-text-properties 1 4 '(a 1 b 2 c 3)) (text-properties-at 1))
      (with-temp-buffer (insert "abcdef")
        (add-text-properties 1 4 '(face bold weight heavy)) (text-properties-at 1))
      (with-temp-buffer (insert "abcdef")
        (add-text-properties 1 4 '(weight heavy face bold)) (text-properties-at 1))
      (with-temp-buffer (insert "abcdef")
        (add-text-properties 1 4 '(foo bar baz qux)) (text-properties-at 1)))
"##,
        expect,
    );
}

#[test]
fn div_e4_add_text_prop_after_disjoint_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((face bold) (face italic weight heavy) (face italic weight heavy) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (set-text-properties 1 3 '(face bold))
  (add-text-properties 4 6 '(weight heavy face italic))
  (list (text-properties-at 1)
        (text-properties-at 4)
        (text-properties-at 5)
        (text-properties-at 6)))
"##,
        expect,
    );
}

#[test]
fn div_e4_keymap_inheritance_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (parent-a child-b t parent-a [97] t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((parent (make-keymap))
      (child (make-sparse-keymap)))
  (define-key parent "a" 'parent-a)
  (define-key child "b" 'child-b)
  (set-keymap-parent child parent)
  (list (lookup-key child "a")
        (lookup-key child "b")
        (eq (keymap-parent child) parent)
        (lookup-key parent "a")
        (where-is-internal 'parent-a child t)
        (keymapp parent)))
"##,
        expect,
    );
}

#[test]
fn div_e4_frame_window_tree_traversal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-root)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (windowp (frame-root-window))
      (windowp (frame-first-window))
      (windowp (frame-selected-window))
      (eq (frame-selected-window) (selected-window))
      (length (window-list nil 'nomini))
      (eq (frame-root-window) (window-root (frame-root-window))))
"##,
        expect,
    );
}

#[test]
fn div_e4_encode_coding_various_lengths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 10 4 \"ABC\" 3 6 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (length (encode-coding-string "日本" 'shift_jis))
      (length (encode-coding-string "日本" 'iso-2022-jp))
      (length (encode-coding-string "café" 'iso-8859-1))
      (encode-coding-string "ABC" 'iso-2022-jp)
      (length (encode-coding-string "ΑΒΓ" 'iso-8859-7))
      (length (encode-coding-string "abc" 'utf-16be))
      (length (encode-coding-string "abc" 'utf-7)))
"##,
        expect,
    );
}

#[test]
fn div_e4_current_active_maps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 2 t Control-X-prefix find-file)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (keymapp (current-local-map))
      (keymapp (current-global-map))
      (length (current-active-maps t))
      (keymapp (car (current-active-maps nil)))
      (lookup-key (current-global-map) "\C-x")
      (lookup-key (current-global-map) [?\C-x ?\C-f]))
"##,
        expect,
    );
}
