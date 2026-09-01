//! Oracle parity tests for syntax-table and related syntax primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_syntax_table_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list (syntax-table-p (standard-syntax-table)) (syntax-table-p (copy-syntax-table)) (syntax-table-p (make-syntax-table)) (eq (char-table-subtype (standard-syntax-table)) 'syntax-table))";
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_make_syntax_table_parent_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((p (make-syntax-table)) (c (make-syntax-table p))) (eq (char-table-parent c) p))",
        expect,
    );
}

#[test]
fn oracle_prop_copy_syntax_table_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument syntax-table-p 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(copy-syntax-table 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_set_syntax_table_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument syntax-table-p 1)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(set-syntax-table 1)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_char_syntax_after_set_syntax_table_custom_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(with-temp-buffer (let ((st (copy-syntax-table (standard-syntax-table)))) (modify-syntax-entry ?A \".\" st) (set-syntax-table st) (char-syntax ?A)))";
    let expect = expect_test::expect![[r#""OK 46""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_syntax_after_observes_set_syntax_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(with-temp-buffer (insert \"A\") (goto-char (point-min)) (let ((st (copy-syntax-table (standard-syntax-table)))) (modify-syntax-entry ?A \".\" st) (set-syntax-table st) (syntax-after (point))))";
    let expect = expect_test::expect![[r#""OK (1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_to_syntax_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list (string-to-syntax \"w\") (string-to-syntax \"_\") (string-to-syntax \"@\"))";
    let expect = expect_test::expect![[r#""OK ((2) (3) nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("((2) (3) nil)", &oracle, &neovm);
}

#[test]
fn oracle_prop_syntax_class_to_char_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list (syntax-class-to-char 0) (syntax-class-to-char 2) (syntax-class-to-char 15))";
    let expect = expect_test::expect![[r#""OK (32 119 124)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(32 119 124)", &oracle, &neovm);
}

#[test]
fn oracle_prop_syntax_class_to_char_out_of_range_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 15 16)""#]];
    let (oracle, neovm) =
        crate::common::eval_oracle_and_neovm_expect("(syntax-class-to-char 16)", expect);
    assert_err_kind(&oracle, &neovm, "args-out-of-range");
}

#[test]
fn oracle_prop_matching_paren_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (41 91 nil)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(list (matching-paren ?\\() (matching-paren ?\\]) (matching-paren ?x))",
        expect,
    );
    assert_ok_eq("(41 91 nil)", &oracle, &neovm);
}

#[test]
fn oracle_prop_forward_comment_whitespace_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 4)""#]];
    crate::common::assert_oracle_parity_expect(
        "(with-temp-buffer (insert \"   x\") (goto-char 1) (list (forward-comment 1) (point)))",
        expect,
    );
    let expect = expect_test::expect![[r#""OK (nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        "(with-temp-buffer (insert \"x   \") (goto-char (point-max)) (list (forward-comment -1) (point)))",
        expect,
    );
}
