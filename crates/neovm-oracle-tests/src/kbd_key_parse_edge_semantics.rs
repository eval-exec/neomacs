//! Oracle parity tests for GNU `key-parse' and `kbd' edge semantics.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_kbd_repeats_and_historical_macro_delimiters() {
    let form = r#"
(list
 (kbd "3*a")
 (key-description (kbd "3*a"))
 (kbd "2*C-x")
 (key-description (kbd "2*C-x"))
 (kbd "C-x ( C-d C-x )")
 (key-description (kbd "C-x ( C-d C-x )")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"aaa\" \"a a a\" \"\u{18}\u{18}\" \"C-x C-x\" \"\u{4}\" \"C-d\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_key_parse_comments_and_line_boundaries() {
    let form = r#"
(list
 (kbd "a ;; ignored to end of line
b")
 (key-description (kbd "a ;; ignored to end of line
b"))
 (kbd "a REM ignored to end of line
b")
 (key-description (kbd "a REM ignored to end of line
b")))"#;
    let expect = expect_test::expect![[r#""OK (\"ab\" \"a b\" \"ab\" \"a b\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_key_parse_octal_events() {
    let form = r#"
(list
 (kbd "\\101")
 (key-description (kbd "\\101"))
 (kbd "\\377")
 (key-description (kbd "\\377"))
 (listify-key-sequence (kbd "\\377")))"#;
    let expect = expect_test::expect![[r#""OK (\"A\" \"A\" [255] \"ÿ\" (255))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_key_parse_angle_tokens_with_embedded_spaces() {
    let form = r#"
(list
 (key-parse "<mouse-1>")
 (key-parse "<mouse-1> a")
 (condition-case err (key-parse "<mouse 1>") (error (car err)))
 (condition-case err (kbd "<mouse 1>") (error (car err))))"#;
    let expect = expect_test::expect![[r#""OK ([mouse-1] [mouse-1 97] [mouse\\ 1] [mouse\\ 1])""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_key_valid_p_strict_textual_syntax_contract() {
    let form = r#"
(list
 (key-valid-p "a")
 (key-valid-p "C-c o")
 (key-valid-p "H-<left>")
 (key-valid-p "C-M-<space>")
 (key-valid-p "RET")
 (key-valid-p "M-RET")
 ;; key-valid-p enforces the documented modifier order and single-space
 ;; tokenization.  key-parse is more permissive for some historical forms.
 (key-valid-p "<M-C-down>")
 (key-valid-p "C-M-a")
 (key-valid-p "M-C-a")
 (key-valid-p "3*a")
 (key-valid-p "a  b")
 (key-valid-p "")
 (key-valid-p 42)
 (condition-case e
     (key-parse "C-M-foo")
   (error (list (car e) (cadr e))))
 (condition-case e
     (key-parse "M-C-a")
   (error (list (car e) (cadr e)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t t t t nil t nil nil nil nil nil (error \"C-M- must prefix a single character, not foo\") [134217729])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
