//! Divergence tests: text property stickiness, rear-nonsticky, front-sticky deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_sticky_insert_between_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold italic italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "AAA" 'face 'bold 'rear-nonsticky t))
  (insert (propertize "BBB" 'face 'italic 'front-sticky t))
  (list (get-text-property 3 'face)
        (get-text-property 4 'face)
        (get-text-property 5 'face)))"#,
        expect,
    );
}

#[test]
fn divergence_sticky_delete_at_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (bold italic #(\"AAABBBB\" 0 3 (face bold) 3 7 (face italic)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "AAAA" 'face 'bold))
  (insert (propertize "BBBB" 'face 'italic))
  (delete-region 4 5)
  (list (get-text-property 3 'face)
        (get-text-property 4 'face)
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_sticky_insert_with_default_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil bold nil nil #(\"_XX__\" 1 3 (face bold front-sticky t rear-nonsticky nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "___")
  (goto-char 2)
  (insert (propertize "XX" 'face 'bold 'front-sticky t 'rear-nonsticky nil))
  (list (get-text-property 1 'face)
        (get-text-property 2 'face)
        (get-text-property 4 'face)
        (get-text-property 5 'face)
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_sticky_propagate_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil nil bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "XXXX" 'face 'bold 'rear-nonsticky '(face)))
  (goto-char 4)
  (insert "YY")
  (list (get-text-property 3 'face)
        (get-text-property 4 'face)
        (get-text-property 5 'face)
        (get-text-property 6 'face)))"#,
        expect,
    );
}

#[test]
fn divergence_text_props_after_replace_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (bold nil bold #(\"foo QUX baz\" 0 4 (face bold) 7 11 (face bold)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "foo bar baz" 'face 'bold))
  (goto-char 1)
  (re-search-forward "bar")
  (replace-match "QUX")
  (list (get-text-property 1 'face)
        (get-text-property 5 'face)
        (get-text-property 9 'face)
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_text_props_after_kill_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (bold italic #(\"Hello World\" 0 6 (face bold) 6 10 (face italic) 10 11 (rear-nonsticky t face italic)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "Hello " 'face 'bold))
  (insert (propertize "World" 'face 'italic))
  (kill-region 1 12)
  (goto-char 1)
  (yank)
  (list (get-text-property 1 'face)
        (get-text-property 7 'face)
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_text_props_substring_no_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil \"ell\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((s (propertize "Hello" 'face 'bold))
         (sub (substring-no-properties s 1 4)))
  (list (get-text-property 0 'face s)
        (get-text-property 0 'face sub)
        sub))"#,
        expect,
    );
}

#[test]
fn divergence_sticky_multi_prop_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold highlight nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "XX" 'face 'bold 'mouse-face 'highlight))
  (insert "YY")
  (list (get-text-property 1 'face)
        (get-text-property 1 'mouse-face)
        (get-text-property 3 'face)
        (get-text-property 3 'mouse-face)))"#,
        expect,
    );
}

#[test]
fn divergence_text_props_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (f1 nil nil 1 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "field1" 'field 'f1))
  (insert "nofield")
  (insert (propertize "field2" 'field 'f2))
  (list (get-text-property 1 'field)
        (get-text-property 7 'field)
        (get-text-property 13 'field)
        (field-beginning 7)
        (field-end 7)))"#,
        expect,
    );
}

#[test]
fn divergence_text_props_intangibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t nil #(\"beforeintangibleafter\" 6 16 (intangible t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "before")
  (insert (propertize "intangible" 'intangible t))
  (insert "after")
  (list (get-text-property 7 'intangible)
        (get-text-property 10 'intangible)
        (get-text-property 18 'intangible)
        (buffer-string)))"#,
        expect,
    );
}
