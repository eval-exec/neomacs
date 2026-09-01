//! Strict combo oracle probes, batch 187: char accessor functions under motion.
//! char-after / char-before at explicit positions, following-char /
//! preceding-char relative to point, and char accessors at buffer edges.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_char_after_before_at_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "abcdef")
  (list (char-after 1)
        (char-after 3)
        (char-after 6)
        (char-after 7)
        (char-before 1)
        (char-before 2)
        (char-before 6)
        (char-before 7)
        (char-to-string (char-after 1))))
"##;
    let expect = expect_test::expect![[r#""OK (97 99 102 nil nil 97 101 102 \"a\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_following_preceding_char_under_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "abcdef")
  (goto-char 3)
  (list (following-char)
        (preceding-char)
        (progn (forward-char) (following-char))
        (progn (forward-char) (preceding-char))
        (progn (goto-char 1) (following-char))
        (progn (goto-char 1) (preceding-char))
        (progn (goto-char (point-max)) (following-char))
        (progn (goto-char (point-max)) (preceding-char))))
"##;
    let expect = expect_test::expect![[r#""OK (99 98 100 100 97 0 0 102)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_accessors_multibyte_and_end_of_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "line1\n日本\nline3")
  (list (char-after 1)
        (char-after 7)
        (char-after 8)
        (char-after 9)
        (char-after 12)
        (char-before 8)
        (char-before 10)
        (progn (goto-char 6) (following-char))
        (progn (goto-char 6) (preceding-char))
        (char-to-string (char-after 7))))
"##;
    let expect = expect_test::expect![[r#""OK (108 26085 26412 10 110 26085 10 10 49 \"日\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
