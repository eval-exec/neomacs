//! Divergence tests: overlay creation, movement, priority, eviction.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_overlay_create_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (bold 10 1 6 (#<overlay from 1 to 6 in *scratch*>))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'priority 10)
    (list (overlay-get ov 'face)
          (overlay-get ov 'priority)
          (overlay-start ov)
          (overlay-end ov)
          (overlays-at 3)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (7 12 nil (#<overlay from 7 to 12 in *scratch*>))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov (make-overlay 1 6)))
    (move-overlay ov 7 12)
    (list (overlay-start ov)
          (overlay-end ov)
          (overlays-at 3)
          (overlays-at 9)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 2 5 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov1 (make-overlay 1 6))
        (ov2 (make-overlay 3 8)))
    (overlay-put ov1 'priority 5)
    (overlay-put ov2 'priority 10)
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (list (length (overlays-in 1 6))
          (length (overlays-in 3 8))
          (overlay-get ov1 'priority)
          (overlay-get ov2 'priority)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'face 'bold)
    (list (length (overlays-in 1 6))
          (progn (delete-overlay ov)
                 (length (overlays-in 1 6)))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 1 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World Foo Bar")
  (make-overlay 1 6)
  (make-overlay 7 12)
  (make-overlay 13 16)
  (list (length (overlays-in 1 20))
        (length (overlays-in 1 6))
        (length (overlays-in 6 7))
        (overlays-in 1 1))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_before_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"[\" \"]\" 6 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov (make-overlay 6 6)))
    (overlay-put ov 'before-string "[")
    (overlay-put ov 'after-string "]")
    (list (overlay-get ov 'before-string)
          (overlay-get ov 'after-string)
          (overlay-start ov)
          (overlay-end ov)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \"Hello World\" \"Hello World\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'invisible t)
    (list (overlay-get ov 'invisible)
          (buffer-substring 1 12)
          (buffer-substring-no-properties 1 12)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_intangible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (intangible t))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'intangible t)
    (list (overlay-get ov 'intangible)
          (overlay-properties ov)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((test-hook-fn) (test-hook-fn2) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'modification-hooks '(test-hook-fn))
    (overlay-put ov 'insert-behind-hooks '(test-hook-fn2))
    (list (overlay-get ov 'modification-hooks)
          (overlay-get ov 'insert-behind-hooks)
          (fboundp 'overlay-recenter)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_next_prev() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 7 7 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (let ((ov1 (make-overlay 1 6))
        (ov2 (make-overlay 7 12)))
    (list (next-overlay-change 1)
          (next-overlay-change 6)
          (previous-overlay-change 12)
          (previous-overlay-change 6)))) "#,
        expect,
    );
}
