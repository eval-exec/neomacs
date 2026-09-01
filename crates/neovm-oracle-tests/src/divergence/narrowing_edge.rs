//! Divergence tests: narrowing, widening, restriction edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_narrow_to_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 11 \"Worl\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World Foo Bar")
  (narrow-to-region 7 11)
  (list (point-min) (point-max)
        (buffer-string))) "#,
        expect,
    );
}

#[test]
fn divergence_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Worl\" 1 20 \"Hello World Foo Bar\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World Foo Bar")
  (narrow-to-region 7 11)
  (let ((narrowed (buffer-string)))
    (widen)
    (list narrowed (point-min) (point-max)
          (buffer-string)))) "#,
        expect,
    );
}

#[test]
fn divergence_narrow_and_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"The mark is not set now, so there is no region\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (narrow-to-region 3 7)
  (list (buffer-narrowed-p)
        (point-min) (point-max)
        (region-beginning) (region-end))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_narrowed_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (buffer-narrowed-p)
  (progn (insert "Hello") (buffer-narrowed-p))
  (progn (narrow-to-region 1 3) (buffer-narrowed-p))
  (progn (widen) (buffer-narrowed-p))) "#,
        expect,
    );
}

#[test]
fn divergence_save_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 5 \"Hell\") 1 12 \"Hello World\" (1 12))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (narrow-to-region 1 5)
  (let ((inner (list (point-min) (point-max) (buffer-string))))
    (save-restriction
      (widen)
      (list inner
            (point-min) (point-max) (buffer-string)
            (progn
              (save-restriction)
              (list (point-min) (point-max))))))) "#,
        expect,
    );
}

#[test]
fn divergence_save_excursion_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 1 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (narrow-to-region 1 5)
  (let ((pos (point)))
    (save-excursion
      (widen)
      (goto-char 10))
    (list (point) (point-min) (point-max)))) "#,
        expect,
    );
}

#[test]
fn divergence_narrow_with_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((#<overlay from 1 to 6 in *scratch*>) 1 \"llo W\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (make-overlay 1 6)
  (narrow-to-region 3 8)
  (list (overlays-in (point-min) (point-max))
        (length (overlays-in 1 12))
        (buffer-string))) "#,
        expect,
    );
}

#[test]
fn divergence_narrow_with_text_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold #(\"llo W\" 0 3 (face bold)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'face 'bold)
  (narrow-to-region 3 8)
  (list (get-text-property (point-min) 'face)
        (get-text-property (1+ (point-min)) 'face)
        (buffer-string))) "#,
        expect,
    );
}

#[test]
fn divergence_narrow_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ABXCDEFGHIJ\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (narrow-to-region 3 7)
  (goto-char (point-min))
  (insert "X")
  (list (buffer-string) (point) (point-min) (point-max))
  (widen)
  (list (buffer-string))) "#,
        expect,
    );
}

#[test]
fn divergence_narrow_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ABEFGHIJ\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (narrow-to-region 3 7)
  (delete-region (point-min) (+ (point-min) 2))
  (list (buffer-string) (point-min) (point-max))
  (widen)
  (list (buffer-string))) "#,
        expect,
    );
}
