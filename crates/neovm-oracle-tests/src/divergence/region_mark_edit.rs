//! Divergence tests: region, mark, transient-mark-mode deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_mark_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 6 1 6 \"Hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (push-mark 1)
  (goto-char 6)
  (list (mark) (point) (region-beginning) (region-end)
        (buffer-substring (region-beginning) (region-end)))) "#,
        expect,
    );
}

#[test]
fn divergence_use_region_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'use-region-p)
  (boundp 'transient-mark-mode)
  (booleanp transient-mark-mode))"#,
        expect,
    );
}

#[test]
fn divergence_mark_ring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'mark-ring)
  (listp mark-ring)
  (fboundp 'set-mark-command)
  (fboundp 'pop-to-mark-command)
  (fboundp 'pop-global-mark))"#,
        expect,
    );
}

#[test]
fn divergence_exchange_point_and_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((11 1) 1 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (push-mark 1)
  (goto-char 11)
  (let ((before (list (point) (mark))))
    (exchange-point-and-mark)
    (list before (point) (mark)))) "#,
        expect,
    );
}

#[test]
fn divergence_kill_region_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (push-mark 1)
  (goto-char 6)
  (list (fboundp 'kill-region)
        (fboundp 'kill-ring-save)
        (fboundp 'yank)
        (fboundp 'yank-pop)
        (boundp 'kill-ring)
        (listp kill-ring))) "#,
        expect,
    );
}

#[test]
fn divergence_rectangle_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'kill-rectangle)
  (fboundp 'yank-rectangle)
  (fboundp 'open-rectangle)
  (fboundp 'clear-rectangle)
  (fboundp 'delete-rectangle)
  (fboundp 'string-rectangle)
  (fboundp 'extract-rectangle))"#,
        expect,
    );
}

#[test]
fn divergence_delete_extract_rectangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"lin\") 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\nline2\nline3\n")
  (list (extract-rectangle 1 4)
        (length (extract-rectangle 1 4)))) "#,
        expect,
    );
}

#[test]
fn divergence_indent_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'indent-region)
  (fboundp 'indent-relative)
  (fboundp 'indent-for-tab-command)
  (fboundp 'indent-to))"#,
        expect,
    );
}

#[test]
fn divergence_comment_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'comment-region)
  (fboundp 'uncomment-region)
  (fboundp 'comment-or-uncomment-region)
  (boundp 'comment-start)
  (boundp 'comment-end))"#,
        expect,
    );
}

#[test]
fn divergence_fill_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'fill-region)
  (fboundp 'fill-paragraph)
  (fboundp 'fill-region-as-paragraph)
  (boundp 'fill-column)
  (integerp fill-column))"#,
        expect,
    );
}
