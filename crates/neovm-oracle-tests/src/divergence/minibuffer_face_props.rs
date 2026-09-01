//! Minibuffer × face/overlay/propertize cross-cutting coverage.
//!
//! Verifies that the face/text-property application across the completion and
//! prompt machinery matches GNU: propertization of completion strings
//! (completions-common-part / completions-first-difference / completion--string
//! / completions-highlight / mouse-face), format-prompt (no properties on the
//! returned string — face applied at display via minibuffer-prompt-properties),
//! completion face default attributes, and minibuffer-message.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_mfp_all_completions_string_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((face completions-common-part) (face completions-first-difference) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((cs (let ((completion-styles '(basic)))
             (completion-all-completions "a" '("apple" "apricot" "banana") nil 1)))
       (s (car cs)))
  (list (text-properties-at 0 s)
        (text-properties-at 1 s)
        (text-properties-at 2 s)))
"##,
        expect,
    );
}

#[test]
fn div_mfp_all_completions_base_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"apple\" 0 1 (face (completions-common-part completions-first-difference))) #(\"apricot\" 0 1 (face (completions-common-part completions-first-difference))) . 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((completion-styles '(basic)))
  (completion-all-completions "a" '("apple" "apricot") nil 0))
"##,
        expect,
    );
}

#[test]
fn div_mfp_insert_strings_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"apple\\napricot\" (completion--string \"apple\" cursor-face completions-highlight mouse-face highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (completion--insert-strings '("apple" "apricot"))
      (list (buffer-substring-no-properties 1 (point-max))
            (text-properties-at 1)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_mfp_completion_face_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified unspecified)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (facep 'completions-common-part)
      (facep 'completions-first-difference)
      (facep 'completions-annotations)
      (facep 'completions-highlight)
      (face-attribute 'completions-common-part :foreground)
      (face-attribute 'completions-first-difference :foreground))
"##,
        expect,
    );
}

#[test]
fn div_mfp_format_prompt_no_inline_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil \"Prompt (default d): \")""#]];
    // The prompt face is applied at display time via minibuffer-prompt-properties,
    // not stored on the format-prompt string itself.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (format-prompt "Prompt" "d")))
  (list (text-properties-at 0 p)
        (text-properties-at 5 p)
        (text-properties-at (- (length p) 1) p)
        p))
"##,
        expect,
    );
}

#[test]
fn div_mfp_minibuffer_message_no_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"msg x\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e (minibuffer-message "msg %s" "x") (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_mfp_display_completion_list_error_parity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""3 possible completions:\napple\napricot\nbananaOK (errored . wrong-type-argument)""#
    ]];
    // display-completion-list in batch errors identically (wrong-type-argument
    // on a buffer position) — no divergence in the error path.
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-current-buffer (get-buffer-create "*mfp-comp*")
      (completion-list-mode)
      (display-completion-list '("apple" "apricot" "banana") "a")
      (text-properties-at 1))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}
