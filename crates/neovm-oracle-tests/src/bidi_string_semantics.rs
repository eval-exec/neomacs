//! Oracle parity tests for GNU `subr.el` bidi string helpers.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_bidi_string_mark_left_to_right_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:bidi-string-mark-left-to-right returns the original object
    // when no RTL character is present.  For an RTL string it appends an
    // invisible U+200E LEFT-TO-RIGHT MARK.  Non-strings signal
    // `wrong-type-argument'.
    let form = r#"
(let* ((plain "abc!?")
       (rtl (string (decode-char 'ucs #x05D0)))
       (mixed (concat "x" rtl "."))
       (plain-result (bidi-string-mark-left-to-right plain))
       (rtl-result (bidi-string-mark-left-to-right rtl))
       (mixed-result (bidi-string-mark-left-to-right mixed)))
  (list
   (eq plain-result plain)
   (equal plain-result "abc!?")
   (length rtl-result)
   (= (aref rtl-result 0) (decode-char 'ucs #x05D0))
   (= (aref rtl-result 1) #x200e)
   (get-text-property 1 'invisible rtl-result)
   (substring-no-properties rtl-result)
   (length mixed-result)
   (= (aref mixed-result (1- (length mixed-result))) #x200e)
   (get-text-property (1- (length mixed-result)) 'invisible mixed-result)
   (substring-no-properties mixed-result)
   (condition-case err
       (bidi-string-mark-left-to-right 42)
     (error (car err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t 2 t t t \"א\u{200e}\" 4 t t \"xא.\u{200e}\" wrong-type-argument)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
