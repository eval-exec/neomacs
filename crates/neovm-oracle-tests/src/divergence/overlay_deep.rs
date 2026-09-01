//! Divergence tests: overlay deep - priority, lazy highlighting, invisible.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_overlay_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (10 5 (#<overlay from 1 to 6 in *scratch*> #<overlay from 4 to 12 in *scratch*>) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World!")
  (let ((ov1 (make-overlay 1 6))
        (ov2 (make-overlay 4 12)))
    (overlay-put ov1 'priority 10)
    (overlay-put ov2 'priority 5)
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (list (overlay-get ov1 'priority)
          (overlay-get ov2 'priority)
          (overlays-at 5)
          (length (overlays-at 5)))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_start_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 7 #<buffer *scratch*> t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 3 7)))
    (list (overlay-start ov)
          (overlay-end ov)
          (overlay-buffer ov)
          (overlayp ov))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_move() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 3 7)))
    (move-overlay ov 1 4)
    (list (overlay-start ov)
          (overlay-end ov))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World!")
  (let ((ov (make-overlay 6 12)))
    (overlay-put ov 'invisible t)
    (list (overlay-get ov 'invisible)
          (get-char-property 7 'invisible)
          (get-char-property 3 'invisible))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (after before)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (let ((ov (make-overlay 3 6))
        (result nil))
    (overlay-put ov 'modification-hooks
                 (list (lambda (ov after beg end &optional len)
                         (push (if after 'after 'before) result))))
    (goto-char 4)
    (insert "X")
    result))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_insert_in_front() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 8 \"ABXXCDEFGH\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (let ((ov (make-overlay 3 6 nil t nil)))
    (goto-char 3)
    (insert "XX")
    (list (overlay-start ov)
          (overlay-end ov)
          (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_insert_behind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 8 \"ABCDEYYFGH\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (let ((ov (make-overlay 3 6 nil nil t)))
    (goto-char 6)
    (insert "YY")
    (list (overlay-start ov)
          (overlay-end ov)
          (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_overlay_list_in_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 1 1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (make-overlay 1 4)
  (make-overlay 3 7)
  (make-overlay 6 10)
  (list (length (overlays-in 1 10))
        (length (overlays-at 5))
        (length (overlays-at 1))
        (length (overlays-at 10))))"#,
        expect,
    );
}

#[test]
fn divergence_delete_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil bold nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (let ((ov (make-overlay 1 5)))
    (overlay-put ov 'face 'bold)
    (delete-overlay ov)
    (list (overlay-start ov)
          (overlay-end ov)
          (overlay-get ov 'face)
          (overlays-at 3))))"#,
        expect,
    );
}

#[test]
fn divergence_next_overlay_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 7 9 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (make-overlay 2 5)
  (make-overlay 7 9)
  (list (next-overlay-change 1)
        (next-overlay-change 5)
        (previous-overlay-change 10)
        (previous-overlay-change 8)))"#,
        expect,
    );
}
