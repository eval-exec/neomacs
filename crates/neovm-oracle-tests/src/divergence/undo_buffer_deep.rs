//! Divergence tests: undo, buffer-undo-list, primitive-undo deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_undo_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello World Foo\" nil t nil \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((start-undo buffer-undo-list))
    (insert " Foo")
    (list (buffer-string)
          (not (eq start-undo buffer-undo-list))
          (listp buffer-undo-list)
          (primitive-undo 1 buffer-undo-list)
          (buffer-string)))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Hello World\" ((6 . 12) nil (1 . 6) (t . 0)) (nil (6 . 12) nil (1 . 6) (t . 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (undo-boundary)
  (insert " World")
  (list (buffer-string)
        buffer-undo-list
        (progn (undo-boundary) buffer-undo-list))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"\" 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((m (point-marker)))
    (goto-char 6)
    (insert "Beautiful ")
    (primitive-undo 1 buffer-undo-list)
    (list (buffer-string) (marker-position m)))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello\" nil \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (delete-region 6 12)
  (list (buffer-string)
        (primitive-undo 1 buffer-undo-list)
        (buffer-string))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_prop_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'face 'bold)
  (let ((undo-list buffer-undo-list))
    (primitive-undo 1 undo-list)
    (list (get-text-property 1 'face)
          (get-text-property 3 'face)))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_undo_list_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'undo-limit)
  (integerp undo-limit)
  (boundp 'undo-strong-limit)
  (integerp undo-strong-limit)
  (boundp 'undo-outer-limit)
  (fboundp 'undo-amalgamate)) "#,
        expect,
    );
}

#[test]
fn divergence_undo_in_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'undo-only)
  (fboundp 'undo-in-region)
  (fboundp 'buffer-disable-undo)
  (fboundp 'buffer-enable-undo))"#,
        expect,
    );
}

#[test]
fn divergence_undo_auto() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'undo-auto-current-boundary-timer)
  (fboundp 'undo-auto--boundary-ensure-delayed)
  (fboundp 'undo-auto--add-boundary)
  (fboundp 'undo-amalgamate)) "#,
        expect,
    );
}

#[test]
fn divergence_undo_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"AAAABBBB\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA")
  (undo-boundary)
  (insert "BBBB")
  (undo-boundary)
  (insert "CCCC")
  (let ((len1 (length buffer-undo-list)))
    (primitive-undo 1 buffer-undo-list)
    (let ((len2 (length buffer-undo-list)))
      (list (buffer-string)
            (> len1 0)
            (> len2 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_nil_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello World\" ((6 . 12)) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (let ((ul buffer-undo-list))
    (setq buffer-undo-list nil)
    (insert " World")
    (list (buffer-string)
          buffer-undo-list
          (not (null buffer-undo-list))))) "#,
        expect,
    );
}
